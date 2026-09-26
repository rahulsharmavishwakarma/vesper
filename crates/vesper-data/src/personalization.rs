use crate::*;
use vesper_core::{
    correction_suggestions, CleanupExample, CorrectionPair, Language, MemoryEvidence, MemorySource,
    OutputIntent, PersonalizationProfile, VocabularyEntry,
};

impl DatasetStore {
    /// Desktop policy: ordinary dictation always participates in local learning.
    /// The generic profile API still supports frozen/offline evaluation controls.
    /// Persist the retired opt-in transition through existing revision/epoch guards.
    pub fn dictation_personalization(&self) -> Result<PersonalizationProfile> {
        let profile = self.personalization()?;
        if profile.automatic_learning {
            Ok(profile)
        } else {
            self.save_dictation_personalization(profile)
        }
    }

    /// Older clients must not restore the retired desktop opt-in gate.
    pub fn save_dictation_personalization(
        &self,
        mut profile: PersonalizationProfile,
    ) -> Result<PersonalizationProfile> {
        profile.automatic_learning = true;
        self.save_personalization(profile)
    }

    pub fn personalization(&self) -> Result<PersonalizationProfile> {
        let serialized: Option<String> = self
            .connection
            .query_row(
                "SELECT profile_json FROM personalization WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let mut profile: PersonalizationProfile = serialized
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or_default();
        profile.validate().map_err(DatasetError::InvalidConfig)?;
        // Schema one has no aliases; migration adds defaults without activating new edits.
        profile.schema_version = 3;
        Ok(profile)
    }

    /// Optimistic revision guard prevents stale UI saves from resurrecting deleted
    /// entries. Example provenance is checked against authoritative human reviews.
    pub fn save_personalization(
        &self,
        mut profile: PersonalizationProfile,
    ) -> Result<PersonalizationProfile> {
        profile.validate().map_err(DatasetError::InvalidConfig)?;
        let tx = self.connection.unchecked_transaction()?;
        let current = self.personalization()?;
        if current.revision != profile.revision {
            return Err(DatasetError::InvalidConfig(
                "Preferences changed; refresh before saving.".into(),
            ));
        }
        profile.schema_version = 3;
        self.reconcile_automatic_profile(&current, &mut profile)?;
        for word in &mut profile.vocabulary {
            let previous = current.vocabulary.iter().find(|p| p.id == word.id);
            word.learned_at = previous.map(|p| p.learned_at.clone()).unwrap_or_default();
            if previous != Some(word) {
                word.learned_at = now();
            }
        }
        for word in &profile.vocabulary {
            // A stricter suggestion policy must not prevent pausing/removing old memories.
            // Revalidate provenance whenever the spelling, aliases, language or sources change.
            if current.vocabulary.iter().any(|previous| {
                previous.id == word.id
                    && previous.phrase == word.phrase
                    && previous.aliases == word.aliases
                    && previous.language == word.language
                    && previous.sources == word.sources
            }) {
                continue;
            }
            let mut witnessed_aliases = std::collections::HashSet::new();
            for source in &word.sources {
                let record = read_record(&tx, &source.session_id)?;
                let review = record
                    .review_history
                    .iter()
                    .find(|r| r.id == source.review_revision_id)
                    .ok_or_else(|| {
                        DatasetError::InvalidConfig(
                            "A correction source must refer to a saved review.".into(),
                        )
                    })?;
                let pairs = correction_suggestions(
                    record.raw_transcript.as_deref().unwrap_or(""),
                    &review.text,
                );
                if record.config_snapshot.language != word.language
                    || !pairs.iter().any(|pair| {
                        pair.intended == word.phrase && word.aliases.contains(&pair.heard)
                    })
                {
                    return Err(DatasetError::InvalidConfig("A remembered correction must match its reviewed source. Clear its source links when editing manually.".into()));
                }
                witnessed_aliases.extend(
                    pairs
                        .into_iter()
                        .filter(|p| p.intended == word.phrase)
                        .map(|p| p.heard),
                );
            }
            if !word.sources.is_empty()
                && word
                    .aliases
                    .iter()
                    .any(|alias| !witnessed_aliases.contains(alias))
            {
                return Err(DatasetError::InvalidConfig("Each reviewed alias must have a matching saved review. Clear source links for manually authored aliases.".into()));
            }
        }
        for example in &profile.examples {
            let record = read_record(&tx, &example.session_id)?;
            if !record.review_history.iter().any(|r| {
                r.id == example.review_revision_id
                    && r.target_kind == ReviewTargetKind::CleanupCorrection
                    && r.text == example.output
            }) || record.raw_transcript.as_deref() != Some(example.input.as_str())
                || record.config_snapshot.language != example.language
                || record.config_snapshot.cleanup_preset != example.preset
            {
                return Err(DatasetError::InvalidConfig("A remembered example must match an approved cleanup revision and its original input.".into()));
            }
        }
        profile.revision = profile
            .revision
            .checked_add(1)
            .ok_or_else(|| DatasetError::InvalidConfig("Profile revision exhausted.".into()))?;
        tx.execute("INSERT INTO personalization (id, profile_json) VALUES (1, ?1) ON CONFLICT(id) DO UPDATE SET profile_json = excluded.profile_json", [serde_json::to_string(&profile)?])?;
        tx.commit()?;
        Ok(profile)
    }

    /// Only an explicit UI action calls this, never ordinary review approval.
    pub fn remember_cleanup(
        &self,
        session_id: &str,
        intent: OutputIntent,
    ) -> Result<PersonalizationProfile> {
        let record = read_record(&self.connection, session_id)?;
        let review = record
            .review_history
            .iter()
            .rev()
            .find(|r| r.target_kind == ReviewTargetKind::CleanupCorrection)
            .ok_or(DatasetError::EmptyCorrection)?;
        let mut profile = self.personalization()?;
        if profile
            .examples
            .iter()
            .any(|e| e.review_revision_id == review.id)
        {
            return Ok(profile);
        }
        // A newly remembered review supersedes this session's earlier example.
        profile.examples.retain(|e| e.session_id != record.id);
        profile.examples.push(CleanupExample {
            id: Uuid::new_v4().to_string(),
            session_id: record.id.clone(),
            review_revision_id: review.id.clone(),
            input: record
                .raw_transcript
                .clone()
                .ok_or(DatasetError::MissingRawTranscript)?,
            output: review.text.clone(),
            language: record.config_snapshot.language,
            preset: record.config_snapshot.cleanup_preset,
            intent,
            enabled: true,
            project: profile.active_project.clone(),
        });
        self.save_personalization(profile)
    }

    /// Execution evidence is appended separately from immutable machine output.
    pub fn record_inference_evidence(&self, id: &str, evidence: Value) -> Result<SessionRecord> {
        if !evidence.is_object() {
            return Err(DatasetError::InvalidModelVersions);
        }
        let tx = self.connection.unchecked_transaction()?;
        let mut record = read_record(&tx, id)?;
        record.model_versions["execution"] = evidence;
        write_record(&tx, &record)?;
        tx.commit()?;
        Ok(record)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RememberCorrection {
    pub session_id: String,
    pub review_revision_id: String,
    pub pair: CorrectionPair,
    pub context: String,
    pub project: String,
    pub pronunciation: bool,
    pub profile_revision: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrectionSuggestion {
    pub review_revision_id: String,
    pub pair: CorrectionPair,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCandidate {
    pub session_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayGroup {
    pub model_id: String,
    pub evidence_key: String,
    pub language: Language,
    pub samples: usize,
    pub reference_words: usize,
    pub baseline_edits: usize,
    pub memory_edits: usize,
    pub improved: usize,
    pub worsened: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningDashboard {
    pub profile_revision: u64,
    pub inspected_sessions: usize,
    pub reviewed_sessions: usize,
    pub applied_sessions: usize,
    pub applied_occurrences: usize,
    pub undone_sessions: usize,
    pub replay_excluded: usize,
    pub replay: Vec<ReplayGroup>,
    pub review_queue: Vec<ReviewCandidate>,
}

impl DatasetStore {
    pub fn correction_suggestions(&self, id: &str) -> Result<Vec<CorrectionSuggestion>> {
        let record = read_record(&self.connection, id)?;
        let raw = record.raw_transcript.as_deref().unwrap_or("");
        let mut suggestions = Vec::new();
        // Separate latest review for each purpose; never mine an unreviewed prediction.
        for kind in [
            ReviewTargetKind::VerbatimTranscription,
            ReviewTargetKind::CleanupCorrection,
        ] {
            if let Some(review) = record
                .review_history
                .iter()
                .rev()
                .find(|r| r.target_kind == kind)
            {
                for pair in correction_suggestions(raw, &review.text) {
                    if !suggestions
                        .iter()
                        .any(|s: &CorrectionSuggestion| s.pair == pair)
                    {
                        suggestions.push(CorrectionSuggestion {
                            review_revision_id: review.id.clone(),
                            pair,
                        });
                    }
                }
            }
        }
        Ok(suggestions)
    }

    pub fn remember_correction(
        &self,
        request: RememberCorrection,
    ) -> Result<PersonalizationProfile> {
        let mut profile = self.personalization()?;
        if profile.revision != request.profile_revision {
            return Err(DatasetError::InvalidConfig(
                "Preferences changed; refresh before remembering this correction.".into(),
            ));
        }
        if !self
            .correction_suggestions(&request.session_id)?
            .iter()
            .any(|s| s.review_revision_id == request.review_revision_id && s.pair == request.pair)
        {
            return Err(DatasetError::InvalidConfig(
                "This suggestion no longer matches the latest reviewed text.".into(),
            ));
        }
        let record = read_record(&self.connection, &request.session_id)?;
        let source = MemorySource {
            session_id: request.session_id,
            review_revision_id: request.review_revision_id,
        };
        if let Some(entry) = profile.vocabulary.iter_mut().find(|e| {
            e.phrase == request.pair.intended
                && (e.automatic_source.is_none() || e.aliases.contains(&request.pair.heard))
                && e.language == record.config_snapshot.language
                && e.project == request.project.trim()
                && e.context == request.context.trim()
                && e.pronunciation == request.pronunciation
        }) {
            // An explicit reviewed confirmation can take ownership of the same alias.
            // Additional unreviewed aliases must not acquire forged review provenance.
            if entry.automatic_source.take().is_some() {
                entry.aliases.retain(|a| a == &request.pair.heard);
            }
            if !entry.aliases.contains(&request.pair.heard) {
                entry.aliases.push(request.pair.heard);
            }
            if !entry.sources.contains(&source) {
                entry.sources.push(source);
            }
            entry.enabled = true;
        } else {
            profile.vocabulary.push(VocabularyEntry {
                id: Uuid::new_v4().to_string(),
                phrase: request.pair.intended,
                aliases: vec![request.pair.heard],
                language: record.config_snapshot.language,
                project: request.project.trim().into(),
                context: request.context.trim().into(),
                pronunciation: request.pronunciation,
                enabled: true,
                sources: vec![source],
                ..VocabularyEntry::default()
            });
        }
        self.save_personalization(profile)
    }

    /// A separate result; neither native ASR nor model cleanup is overwritten.
    pub fn complete_personalization(
        &self,
        id: &str,
        text: String,
        evidence: MemoryEvidence,
        project: String,
    ) -> Result<SessionRecord> {
        validate_text(&text, false)?;
        let tx = self.connection.unchecked_transaction()?;
        let mut record = read_record(&tx, id)?;
        let baseline = record
            .cleaned_transcript
            .as_deref()
            .or(record.raw_transcript.as_deref())
            .ok_or(DatasetError::MissingRawTranscript)?;
        if record.personalization_evidence.is_some() {
            return Err(DatasetError::PredictionsAlreadyStored(id.into()));
        }
        record.personalized_transcript = (text != baseline).then_some(text);
        record.personalization_evidence = Some(evidence);
        record.personalization_project = project;
        write_record(&tx, &record)?;
        tx.commit()?;
        Ok(record)
    }

    pub fn undo_personalization(&self, id: &str) -> Result<SessionRecord> {
        let tx = self.connection.unchecked_transaction()?;
        let mut record = read_record(&tx, id)?;
        tx.execute(
            "DELETE FROM semantic_shadow WHERE json_extract(request_json, '$.sessionId') = ?1",
            [id],
        )?;
        self.invalidate_shadow_session(id)?;
        record.personalization_rejected = true;
        if let Some(evidence) = &record.personalization_evidence {
            self.suspend_automatic_in_transaction(
                &tx,
                &evidence
                    .applied
                    .iter()
                    .map(|a| a.entry_id.clone())
                    .collect::<Vec<_>>(),
            )?;
        }
        write_record(&tx, &record)?;
        tx.commit()?;
        Ok(record)
    }

    /// Bounded retrospective text replay. Never reruns ASR or calls a model/API.
    /// Source/activation times and duplicate audio groups gate each memory independently.
    pub fn learning_dashboard(&self) -> Result<LearningDashboard> {
        let profile = self.personalization()?;
        let mut statement = self
            .connection
            .prepare("SELECT record_json FROM sessions ORDER BY created_at DESC, id LIMIT 512")?;
        let records = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .map(|row| Ok(serde_json::from_str::<SessionRecord>(&row?)?))
            .collect::<Result<Vec<_>>>()?;
        let mut report = LearningDashboard {
            profile_revision: profile.revision,
            inspected_sessions: records.len(),
            ..LearningDashboard::default()
        };
        let mut seen_audio = std::collections::HashSet::new();
        let mut review_groups = Vec::new();
        let mut sources = std::collections::HashMap::new();
        for entry in &profile.vocabulary {
            for source in &entry.sources {
                if !sources.contains_key(&source.session_id) {
                    sources.insert(
                        source.session_id.clone(),
                        read_record(&self.connection, &source.session_id)?,
                    );
                }
            }
        }
        // Reserve one audit slot in addition to recurring-error/negative-feedback cases.
        let mut audit = None;
        for record in &records {
            let raw = record.raw_transcript.as_deref().unwrap_or("");
            if let Some(evidence) = &record.personalization_evidence {
                if !evidence.applied.is_empty() {
                    report.applied_sessions += 1;
                    report.applied_occurrences += evidence
                        .applied
                        .iter()
                        .map(|a| a.occurrences)
                        .sum::<usize>();
                }
            }
            report.undone_sessions += usize::from(record.personalization_rejected);
            report.reviewed_sessions += usize::from(!record.review_history.is_empty());
            if record.review_history.is_empty() && !raw.trim().is_empty() {
                let reason = if record.personalization_rejected {
                    Some("Remembered correction undone")
                } else if record.transcript_feedback == Some(TranscriptFeedback::Dislike) {
                    Some("You marked this result as unhelpful")
                } else if record
                    .personalization_evidence
                    .as_ref()
                    .is_some_and(|e| e.ambiguous_matches > 0)
                {
                    Some("Conflicting remembered spellings")
                } else {
                    let mut scoped = profile.clone();
                    scoped.active_project = record.personalization_project.clone();
                    let replay = scoped.personalize(raw, record.config_snapshot.language);
                    (!replay.evidence.applied.is_empty())
                        .then_some("Contains a recurring correction")
                };
                let group = (
                    record.config_snapshot.language,
                    record.created_at.get(..10).unwrap_or("").to_owned(),
                );
                if let Some(reason) = reason {
                    if report.review_queue.len() < 4 && !review_groups.contains(&group) {
                        review_groups.push(group);
                        report.review_queue.push(ReviewCandidate {
                            session_id: record.id.clone(),
                            reason: reason.into(),
                        });
                    }
                } else if audit.is_none() {
                    audit = Some(ReviewCandidate {
                        session_id: record.id.clone(),
                        reason: "Audit an otherwise unflagged recording".into(),
                    });
                }
            }
            let Some(reference) = record.verified_transcript.as_deref() else {
                continue;
            };
            if !seen_audio.insert(&record.audio_sha256) || raw.trim().is_empty() {
                report.replay_excluded += 1;
                continue;
            }
            let Some(baseline) = vesper_core::word_edits(reference, raw) else {
                report.replay_excluded += 1;
                continue;
            };
            let mut eligible = profile.clone();
            eligible.active_project = record.personalization_project.clone();
            eligible.examples.clear();
            eligible.vocabulary.retain(|entry| {
                entry.enabled
                    && !entry.sources.is_empty()
                    && before(&entry.learned_at, &record.created_at)
                    && entry.sources.iter().all(|source| {
                        sources.get(&source.session_id).is_some_and(|origin| {
                            origin.audio_sha256 != record.audio_sha256
                                && before(&origin.created_at, &record.created_at)
                                && origin.review_history.iter().any(|r| {
                                    r.id == source.review_revision_id
                                        && before(&r.timestamp, &record.created_at)
                                })
                        })
                    })
            });
            if eligible.vocabulary.is_empty() {
                report.replay_excluded += 1;
                continue;
            }
            let personalized = eligible.personalize(raw, record.config_snapshot.language);
            let Some(edits) = vesper_core::word_edits(reference, &personalized.text) else {
                report.replay_excluded += 1;
                continue;
            };
            let model_id = record
                .config_snapshot
                .models
                .custom_asr
                .as_ref()
                .map(|p| p.id.clone())
                .unwrap_or_else(|| record.config_snapshot.models.asr.id().into());
            let mut provenance = record.model_versions.clone();
            if let Some(object) = provenance.as_object_mut() {
                object.remove("execution");
            }
            // Do not pool records from different pinned artifacts/runtime/settings.
            // This remains text replay, not an acoustic or hardware benchmark.
            let evidence_key = format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(
                    &json!({"provenance": provenance, "config": record.config_snapshot})
                )?)
            );
            let index = report
                .replay
                .iter()
                .position(|g| {
                    g.model_id == model_id
                        && g.language == record.config_snapshot.language
                        && g.evidence_key == evidence_key
                })
                .unwrap_or_else(|| {
                    report.replay.push(ReplayGroup {
                        model_id: model_id.clone(),
                        evidence_key: evidence_key.clone(),
                        language: record.config_snapshot.language,
                        ..ReplayGroup::default()
                    });
                    report.replay.len() - 1
                });
            let group = &mut report.replay[index];
            group.samples += 1;
            group.reference_words += reference.split_whitespace().count();
            group.baseline_edits += baseline;
            group.memory_edits += edits;
            group.improved += usize::from(edits < baseline);
            group.worsened += usize::from(edits > baseline);
        }
        if let Some(audit) = audit {
            report.review_queue.push(audit);
        }
        Ok(report)
    }
}

fn before(earlier: &str, later: &str) -> bool {
    chrono::DateTime::parse_from_rfc3339(earlier)
        .ok()
        .zip(chrono::DateTime::parse_from_rfc3339(later).ok())
        .is_some_and(|(a, b)| a < b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_profiles_upgrade_legacy_learning_once_and_preserve_other_choices() {
        for schema in [1, 2, 3] {
            let dir = tempfile::tempdir().unwrap();
            let store = DatasetStore::open(dir.path().join("data")).unwrap();
            let mut legacy = serde_json::to_value(PersonalizationProfile {
                schema_version: schema,
                revision: 7,
                apply_corrections: false,
                active_project: "existing-project".into(),
                vocabulary: vec![VocabularyEntry {
                    id: "disabled-word".into(),
                    phrase: "README.md".into(),
                    enabled: false,
                    ..Default::default()
                }],
                ..Default::default()
            })
            .unwrap();
            if schema < 3 {
                legacy.as_object_mut().unwrap().remove("automaticLearning");
                legacy.as_object_mut().unwrap().remove("learningEpoch");
            }
            store
                .connection
                .execute(
                    "INSERT INTO personalization(id, profile_json) VALUES(1, ?1)",
                    [legacy.to_string()],
                )
                .unwrap();
            let previous = store.personalization().unwrap();
            let mut expected = previous.clone();
            expected.automatic_learning = true;
            expected.revision += 1;
            expected.learning_epoch += 1;
            assert_eq!(store.dictation_personalization().unwrap(), expected);
            assert_eq!(store.dictation_personalization().unwrap(), expected);
            assert!(store.save_dictation_personalization(previous).is_err());
            let reopened = DatasetStore::open(dir.path().join("data")).unwrap();
            assert_eq!(reopened.dictation_personalization().unwrap(), expected);
        }
    }

    #[test]
    fn desktop_learning_needs_no_opt_in_and_forget_blocks_old_feedback() {
        use vesper_core::LearningContext;
        let dir = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(dir.path().join("data")).unwrap();
        let profile = store.dictation_personalization().unwrap();
        assert!(profile.automatic_learning);
        let context = |id: &str, epoch| LearningContext {
            result_id: id.into(),
            source_key: format!("audio-{id}"),
            episode_id: id.into(),
            language: Language::English,
            learning_epoch: epoch,
            learning_enabled: true,
            ..Default::default()
        };
        let before = "Deploy kubernets to the cluster";
        let after = "Deploy kubernetes to the cluster";
        for id in ["one", "two"] {
            store
                .learn_from_edit(&context(id, profile.learning_epoch), 0, before, after)
                .unwrap();
        }
        let learned = store.dictation_personalization().unwrap();
        assert_eq!(learned.personalize(before, Language::English).text, after);
        assert!(store.list_sessions().unwrap().is_empty());
        let forgotten = store.forget_automatic_learning(learned.revision).unwrap();
        assert!(forgotten.automatic_learning);
        store
            .learn_from_edit(&context("late", profile.learning_epoch), 0, before, after)
            .unwrap();
        assert!(store.automatic_learning_state().unwrap().events.is_empty());
        assert!(store
            .dictation_personalization()
            .unwrap()
            .vocabulary
            .is_empty());
        for id in ["new-one", "new-two"] {
            store
                .learn_from_edit(&context(id, forgotten.learning_epoch), 0, before, after)
                .unwrap();
        }
        assert_eq!(
            store
                .dictation_personalization()
                .unwrap()
                .personalize(before, Language::English)
                .text,
            after
        );
    }

    #[test]
    fn desktop_save_cannot_restore_the_retired_opt_in_switch() {
        let dir = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(dir.path().join("data")).unwrap();
        let mut profile = store.dictation_personalization().unwrap();
        let epoch = profile.learning_epoch;
        profile.automatic_learning = false;
        profile.preserve_fillers = true;
        let saved = store.save_dictation_personalization(profile).unwrap();
        assert!(saved.automatic_learning);
        assert!(saved.preserve_fillers);
        assert_eq!(saved.learning_epoch, epoch);
    }

    /// Explicit native-data lane; public audio and references only, isolated stores.
    #[test]
    #[ignore = "requires VESPER_PUBLIC_MEMORY_CORPUS pointing to the cached calibration corpus"]
    fn public_corpus_reviews_remember_undo_reload_and_reject_stale_updates() {
        let root = std::path::PathBuf::from(
            vesper_core::environment::var_os("VESPER_PUBLIC_MEMORY_CORPUS").unwrap(),
        );
        let manifest: Value =
            serde_json::from_slice(&std::fs::read(root.join("calibration.json")).unwrap()).unwrap();
        let report: Value = serde_json::from_slice(
            &std::fs::read(root.join("results/calibration-nemotron.json")).unwrap(),
        )
        .unwrap();
        let mut checked = std::collections::HashMap::<String, usize>::new();
        for sample in manifest["samples"].as_array().unwrap() {
            let locale = sample["language"].as_str().unwrap();
            if *checked.get(locale).unwrap_or(&0) >= 2 {
                continue;
            }
            let run = report["runs"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["sampleId"] == sample["id"])
                .unwrap();
            let raw = run["rawHypothesis"].as_str().unwrap();
            let reference = sample["reference"].as_str().unwrap();
            if correction_suggestions(raw, reference).is_empty() {
                continue;
            }
            let temp = tempfile::tempdir().unwrap();
            let store = DatasetStore::open(temp.path().join("store")).unwrap();
            let mut config = AppConfig::default();
            config.models.asr = vesper_core::AsrModel::Nemotron35AsrStreaming06b;
            config.language = if locale == "hi-IN" {
                Language::Hindi
            } else {
                Language::English
            };
            let source = store
                .import_audio(&root.join(sample["audioPath"].as_str().unwrap()), &config)
                .unwrap();
            assert_eq!(source.audio_sha256, sample["sha256"].as_str().unwrap());
            store
                .set_transcripts(
                    &source.id,
                    raw.into(),
                    None,
                    json!({"test":"public-corpus-workflow"}),
                )
                .unwrap();
            assert!(store.correction_suggestions(&source.id).unwrap().is_empty());
            store
                .approve_transcription(&source.id, reference.into())
                .unwrap();
            assert!(store.personalization().unwrap().vocabulary.is_empty());
            let suggestion = store.correction_suggestions(&source.id).unwrap().remove(0);
            let saved = store
                .remember_correction(RememberCorrection {
                    session_id: source.id.clone(),
                    review_revision_id: suggestion.review_revision_id,
                    pair: suggestion.pair,
                    context: String::new(),
                    project: String::new(),
                    pronunciation: true,
                    profile_revision: 0,
                })
                .unwrap();
            assert!(saved.vocabulary[0].pronunciation);
            let result = saved.personalize(raw, config.language);
            assert!(!result.evidence.applied.is_empty());
            let complete = store
                .complete_personalization(&source.id, result.text, result.evidence, String::new())
                .unwrap();
            let undone = store.undo_personalization(&source.id).unwrap();
            assert_eq!(undone.raw_transcript.as_deref(), Some(raw));
            assert_eq!(undone.review_history, complete.review_history);
            assert!(undone.personalization_rejected);
            let dashboard = store.learning_dashboard().unwrap();
            assert_eq!(
                (
                    dashboard.applied_sessions,
                    dashboard.undone_sessions,
                    dashboard.reviewed_sessions
                ),
                (1, 1, 1)
            );
            assert!(dashboard.replay.is_empty()); // A source is not held-out improvement.
            store
                .approve_correction(&source.id, reference.into())
                .unwrap();
            let with_example = store
                .remember_cleanup(&source.id, OutputIntent::Note)
                .unwrap();
            assert_eq!(with_example.examples.len(), 1);
            assert_eq!(
                with_example.effective_intent(raw, config.language),
                OutputIntent::Verbatim
            );
            assert!(store.save_personalization(saved).is_err()); // Stale revision.
            drop(store);
            let reopened = DatasetStore::open(temp.path().join("store")).unwrap();
            assert_eq!(reopened.personalization().unwrap(), with_example);
            let mut paused = with_example.clone();
            paused.vocabulary[0].enabled = false;
            let mut paused = reopened.save_personalization(paused).unwrap();
            assert_eq!(paused.personalize(raw, config.language).text, raw);
            paused.vocabulary.clear();
            paused.examples.clear();
            reopened.save_personalization(paused).unwrap();
            assert!(reopened.save_personalization(with_example).is_err());
            *checked.entry(locale.into()).or_default() += 1;
        }
        assert_eq!(checked.values().sum::<usize>(), 6);
        eprintln!("Completed six real-corpus review/remember/undo/reload workflows: {checked:?}");
    }

    fn recording(store: &DatasetStore, audio: &Path, content: &[u8], at: &str) -> SessionRecord {
        std::fs::write(audio, content).unwrap();
        let record = store.import_audio(audio, &AppConfig::default()).unwrap();
        let mut record = store
            .set_transcripts(
                &record.id,
                "run cube control".into(),
                None,
                json!({"revision":"base"}),
            )
            .unwrap();
        record.created_at = at.into();
        write_record(&store.connection, &record).unwrap();
        record
    }

    fn reviewed(store: &DatasetStore, id: &str, text: &str, at: &str) -> SessionRecord {
        let mut record = store.approve_transcription(id, text.into()).unwrap();
        record.review_history.last_mut().unwrap().timestamp = at.into();
        write_record(&store.connection, &record).unwrap();
        record
    }

    fn remember(store: &DatasetStore, record: &SessionRecord) -> PersonalizationProfile {
        let suggestion = store.correction_suggestions(&record.id).unwrap().remove(0);
        store
            .remember_correction(RememberCorrection {
                session_id: record.id.clone(),
                review_revision_id: suggestion.review_revision_id,
                pair: suggestion.pair,
                context: String::new(),
                project: String::new(),
                pronunciation: true,
                profile_revision: store.personalization().unwrap().revision,
            })
            .unwrap()
    }

    #[test]
    fn stricter_suggestions_do_not_prevent_pausing_a_legacy_reviewed_memory() {
        let temp = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(temp.path().join("dataset")).unwrap();
        let mut record = recording(
            &store,
            &temp.path().join("fixture.wav"),
            b"legacy",
            "2000-01-01T00:00:00Z",
        );
        record.raw_transcript = Some("I".into());
        write_record(&store.connection, &record).unwrap();
        let record = reviewed(&store, &record.id, "Ama", "2000-01-02T00:00:00Z");
        assert!(store.correction_suggestions(&record.id).unwrap().is_empty());
        let mut legacy = PersonalizationProfile {
            revision: 1,
            vocabulary: vec![VocabularyEntry {
                id: "legacy".into(),
                phrase: "Ama".into(),
                aliases: vec!["I".into()],
                language: record.config_snapshot.language,
                enabled: true,
                sources: vec![MemorySource {
                    session_id: record.id.clone(),
                    review_revision_id: record.review_history[0].id.clone(),
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        store
            .connection
            .execute(
                "INSERT INTO personalization (id, profile_json) VALUES (1, ?1)",
                [serde_json::to_string(&legacy).unwrap()],
            )
            .unwrap();
        legacy.vocabulary[0].enabled = false;
        let saved = store.save_personalization(legacy).unwrap();
        assert!(!saved.vocabulary[0].enabled);
        let mut forged = saved;
        forged.vocabulary[0]
            .aliases
            .push("new unreviewed alias".into());
        assert!(store.save_personalization(forged).is_err());
    }

    #[test]
    fn correction_memory_requires_explicit_review_and_validates_each_alias() {
        let temp = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(temp.path().join("dataset")).unwrap();
        let record = recording(
            &store,
            &temp.path().join("fixture.wav"),
            b"source",
            "2000-01-01T00:00:00Z",
        );
        assert!(store.correction_suggestions(&record.id).unwrap().is_empty());
        reviewed(&store, &record.id, "run kubectl", "2000-01-02T00:00:00Z");
        assert!(store.personalization().unwrap().vocabulary.is_empty());
        let saved = remember(&store, &record);
        assert_eq!(saved.vocabulary[0].sources.len(), 1);
        assert_eq!(
            saved
                .personalize("run cube control", Language::English)
                .text,
            "run kubectl"
        );
        let mut forged = saved.clone();
        forged.vocabulary[0]
            .aliases
            .push("unreviewed spelling".into());
        assert!(store.save_personalization(forged).is_err());
        let mut forged_time = saved.clone();
        forged_time.vocabulary[0].learned_at = "1900-01-01T00:00:00Z".into();
        let unchanged = store.save_personalization(forged_time).unwrap();
        assert_eq!(
            unchanged.vocabulary[0].learned_at,
            saved.vocabulary[0].learned_at
        );
        let mut removed = unchanged.clone();
        removed.vocabulary.clear();
        store.save_personalization(removed).unwrap();
        assert!(store.save_personalization(unchanged).is_err());
    }

    #[test]
    fn paired_replay_excludes_sources_duplicates_and_past_sessions_and_keeps_regressions() {
        let temp = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(temp.path().join("dataset")).unwrap();
        let audio = temp.path().join("fixture.wav");
        let source = recording(&store, &audio, b"source", "2000-01-01T00:00:00Z");
        reviewed(&store, &source.id, "run kubectl", "2000-01-02T00:00:00Z");
        remember(&store, &source);
        for (bytes, at, target) in [
            (b"earlier".as_slice(), "2000-02-01T00:00:00Z", "run kubectl"),
            (b"source".as_slice(), "2999-01-01T00:00:00Z", "run kubectl"),
            (b"later".as_slice(), "2999-01-01T00:00:00Z", "run kubectl"),
            (b"later".as_slice(), "2999-01-02T00:00:00Z", "run kubectl"),
            (
                b"homophone".as_slice(),
                "2999-01-01T00:00:00Z",
                "run cube control",
            ),
        ] {
            let sample = recording(&store, &audio, bytes, at);
            reviewed(&store, &sample.id, target, "2999-02-01T00:00:00Z");
        }
        let report = store.learning_dashboard().unwrap();
        assert_eq!(report.replay.len(), 1);
        let group = &report.replay[0];
        assert_eq!((group.samples, group.improved, group.worsened), (2, 1, 1));
        assert_eq!(group.baseline_edits, group.memory_edits);
        assert_eq!(report.replay_excluded, 4);
        assert_eq!(report.applied_sessions, 0); // Replay is not real usage.
    }

    #[test]
    fn remembered_output_and_undo_preserve_machine_and_reviewed_targets() {
        let temp = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(temp.path().join("dataset")).unwrap();
        let record = recording(
            &store,
            &temp.path().join("fixture.wav"),
            b"source",
            "2000-01-01T00:00:00Z",
        );
        reviewed(&store, &record.id, "run kubectl", "2000-01-02T00:00:00Z");
        let profile = remember(&store, &record);
        let result =
            profile.personalize(record.raw_transcript.as_deref().unwrap(), Language::English);
        let applied = store
            .complete_personalization(
                &record.id,
                result.text.clone(),
                result.evidence.clone(),
                String::new(),
            )
            .unwrap();
        assert_eq!(applied.raw_transcript.as_deref(), Some("run cube control"));
        assert!(applied.cleaned_transcript.is_none()); // Works with cleanup disabled.
        assert_eq!(
            applied.personalized_transcript.as_deref(),
            Some("run kubectl")
        );
        assert!(store
            .complete_personalization(&record.id, result.text, result.evidence, String::new())
            .is_err());
        let undone = store.undo_personalization(&record.id).unwrap();
        assert!(undone.personalization_rejected);
        assert_eq!(undone.raw_transcript, applied.raw_transcript);
        assert_eq!(undone.review_history, applied.review_history);
        assert_eq!(
            undone.personalized_transcript,
            applied.personalized_transcript
        );
        let report = store.learning_dashboard().unwrap();
        assert_eq!((report.applied_sessions, report.undone_sessions), (1, 1));
        drop(store);
        let reopened = DatasetStore::open(temp.path().join("dataset")).unwrap();
        assert_eq!(reopened.personalization().unwrap(), profile);
    }
    #[test]
    fn approval_does_not_activate_and_forged_or_deleted_examples_cannot_reappear() {
        let temp = tempfile::tempdir().unwrap();
        let audio = temp.path().join("fixture.wav");
        std::fs::write(&audio, b"synthetic storage fixture").unwrap();
        let store = DatasetStore::open(temp.path().join("dataset")).unwrap();
        let session = store.import_audio(&audio, &AppConfig::default()).unwrap();
        store
            .set_transcripts(&session.id, "send atlas tomorrow".into(), None, json!({}))
            .unwrap();
        assert!(store
            .remember_cleanup(&session.id, OutputIntent::Verbatim)
            .is_err());
        store
            .approve_correction(&session.id, "Send Atlas tomorrow.".into())
            .unwrap();
        assert!(store.personalization().unwrap().examples.is_empty());
        let saved = store
            .remember_cleanup(&session.id, OutputIntent::Verbatim)
            .unwrap();
        assert_eq!(saved.examples.len(), 1);
        let mut forged = saved.clone();
        forged.examples[0].output = "Invent a fact.".into();
        assert!(store.save_personalization(forged).is_err());
        let mut removed = saved.clone();
        removed.examples.clear();
        store.save_personalization(removed).unwrap();
        assert!(store.save_personalization(saved).is_err());
        assert!(store.personalization().unwrap().examples.is_empty());
    }
    #[test]
    fn revisions_survive_reopen_and_stale_updates_fail() {
        let temp = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(temp.path().to_owned()).unwrap();
        let initial = store.personalization().unwrap();
        let mut edited = initial.clone();
        edited.output_intent = OutputIntent::Technical;
        let saved = store.save_personalization(edited).unwrap();
        assert_eq!(saved.revision, 1);
        assert!(store.save_personalization(initial).is_err());
        drop(store);
        assert_eq!(
            DatasetStore::open(temp.path().to_owned())
                .unwrap()
                .personalization()
                .unwrap(),
            saved
        );
    }
}
