//! Bounded local feedback storage. Ordinary edits never become reviewed training targets.
use crate::*;
use vesper_core::{
    automatic_spelling_pair, memory_phrase_occurs, CandidateState, CorrectionPair, Language,
    LearningContext, LearningOutcome, PersonalizationProfile, RetryRelation, VocabularyEntry,
    LEARNING_POLICY_VERSION,
};

const MAX_CANDIDATES: usize = 256;
const MAX_EVENTS: usize = 1024;
const EXPIRY_MS: i64 = 30 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditSupport {
    pub source_key: String,
    pub episode_id: String,
    pub event_id: String,
    /// Local lexical cues from this independent edit. Missing in schema-1 support.
    #[serde(default)]
    pub teaching_terms: Vec<String>,
    /// Recognizer family that produced the heard form; empty in older support.
    #[serde(default)]
    pub recognizer: String,
}

/// A user rejection of an applied automatic rule; feeds the policy breaker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleRevert {
    pub rule: String,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningCandidate {
    pub id: String,
    pub phrase: String,
    pub alias: String,
    pub language: Language,
    pub project: String,
    pub context: String,
    pub state: CandidateState,
    pub support: Vec<EditSupport>,
    pub observed_count: u32,
    pub last_seen_ms: i64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningEventKind {
    UserEdit,
    RawObservation,
    MemoryUndo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningEvent {
    pub id: String,
    pub context: LearningContext,
    pub kind: LearningEventKind,
    pub pair: Option<CorrectionPair>,
    pub retry: Option<RetryRelation>,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AutomaticLearningState {
    pub schema_version: u32,
    pub policy_version: u32,
    pub candidates: Vec<LearningCandidate>,
    pub events: Vec<LearningEvent>,
    pub expired_candidates: u64,
    pub reverts: Vec<RuleRevert>,
    /// Set when distinct rules were rejected in a short window. Unconfirmed
    /// automatic rules stay in shadow until the user resumes them.
    pub breaker_tripped_ms: Option<i64>,
}
impl Default for AutomaticLearningState {
    fn default() -> Self {
        Self {
            schema_version: 2,
            policy_version: LEARNING_POLICY_VERSION,
            candidates: vec![],
            events: vec![],
            expired_candidates: 0,
            reverts: vec![],
            breaker_tripped_ms: None,
        }
    }
}

/// Profile and learning evidence read in one SQLite snapshot for one operation.
/// Pre-ASR hints use `profile`; both post-ASR stages use `post_asr_profile`.
#[derive(Debug, Clone, Default)]
pub struct DictationPersonalizationSnapshot {
    pub profile: PersonalizationProfile,
    pub learning: AutomaticLearningState,
    pub project_selection: vesper_core::ProjectSelection,
    /// Resolved at capture start by `resolve_context`; empty for imports.
    pub workspace: vesper_core::WorkspaceContext,
    pub(crate) workspaces: crate::workspaces::WorkspaceSnapshot,
    // Term inventory (see term_inventory.rs): the usable inventory rows and the
    // per-recording selection made after workspace resolution.
    pub inventory: Vec<vesper_core::InventoryTerm>,
    pub working_set: vesper_core::WorkingSet,
    /// Opt-in field words read at capture start; references for this recording only.
    pub field_terms: Vec<String>,
    /// Set when stored personalization could not be read (see `unavailable`).
    pub load_warning: Option<String>,
}

impl DictationPersonalizationSnapshot {
    /// Learned words and preferences never block recording (issue #362). When
    /// they cannot be read, the recording uses default settings with learning
    /// off, so nothing is written over the unreadable data, and the reason is
    /// reported as a warning on the result.
    pub fn unavailable(reason: &str) -> Self {
        Self {
            load_warning: Some(format!(
                "Your learned words could not be loaded, so this recording used default settings: {reason}"
            )),
            ..Self::default()
        }
    }
}

impl DictationPersonalizationSnapshot {
    pub fn resolve_project(&mut self, application: Option<&str>, document: Option<&str>) {
        self.project_selection = vesper_core::resolve_project(
            self.profile.project_mode,
            &self.profile.active_project,
            &self.profile.project_rules,
            application,
            document,
        );
        self.profile.active_project = self.project_selection.project.clone();
    }

    pub fn post_asr_profile(&self, raw: &str, language: Language) -> PersonalizationProfile {
        self.post_asr_profile_for(raw, language, "")
    }

    /// Unconfirmed automatic aliases apply only for the recognizer family that
    /// produced their evidence (legacy support without a family applies to all),
    /// and not at all while the policy breaker is tripped.
    pub fn post_asr_profile_for(
        &self,
        raw: &str,
        language: Language,
        model: &str,
    ) -> PersonalizationProfile {
        let family = vesper_core::recognizer_family(model);
        let paused = self.learning.breaker_tripped_ms.is_some();
        let mut selected = self.profile.clone();
        for entry in &mut selected.vocabulary {
            let Some(source) = &entry.automatic_source else {
                continue;
            };
            if entry.confirmed {
                // User-confirmed: trusted like a manual entry, no per-utterance
                // evidence gate. Unticked automatic entries keep the gate below.
                if entry.enabled {
                    entry.context.clear();
                }
                continue;
            }
            let eligible = !paused
                && self.learning.candidates.iter().any(|candidate| {
                    candidate.id == *source
                        && candidate.state == CandidateState::Active
                        && (model.is_empty()
                            || candidate.support.iter().all(|s| s.recognizer.is_empty())
                            || candidate.support.iter().any(|s| s.recognizer == family))
                        && candidate.phrase == entry.phrase
                        && entry.aliases.contains(&candidate.alias)
                        && candidate.language == entry.language
                        && candidate.project == entry.project
                        && language != Language::Auto
                        && vesper_core::language_matches(entry.language, language)
                        && vesper_core::project_matches(&entry.project, &selected.active_project)
                        && (memory_phrase_occurs(raw, &candidate.alias)
                            || memory_phrase_occurs(raw, &candidate.phrase))
                        && vesper_core::teaching_context_matches(
                            candidate.support.iter().flat_map(|s| &s.teaching_terms),
                            raw,
                            &CorrectionPair {
                                heard: candidate.alias.clone(),
                                intended: candidate.phrase.clone(),
                            },
                            &selected.active_project,
                        )
                });
            entry.enabled &= eligible;
            if entry.enabled {
                // The raw-text evidence guard already established this operation's
                // context. First-edit tags must not veto later independent cues in
                // cleanup retrieval or final application. Persisted/pre-ASR scope
                // and manual entries remain unchanged.
                entry.context.clear();
            }
        }
        selected
    }
}

#[cfg(test)]
mod project_snapshot_tests {
    use super::*;
    #[test]
    fn resolving_an_operation_does_not_rewrite_the_saved_manual_scope_or_follow_new_rules() {
        let dir = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(dir.path().into()).unwrap();
        let mut profile = store.dictation_personalization().unwrap();
        profile.project_mode = vesper_core::ProjectMode::Automatic;
        profile.project_rules.push(vesper_core::ProjectRule {
            id: "editor".into(),
            application_id: "test.editor".into(),
            application_name: "Editor".into(),
            project: "First".into(),
            folder: None,
            enabled: true,
        });
        store.save_personalization(profile).unwrap();
        let mut operation = store.dictation_personalization_snapshot().unwrap();
        operation.resolve_project(Some("test.editor"), None);
        let mut saved = store.personalization().unwrap();
        assert!(saved.active_project.is_empty());
        saved.project_rules[0].project = "Second".into();
        store.save_personalization(saved).unwrap();
        assert_eq!(operation.profile.active_project, "First");
        assert_eq!(
            operation.project_selection.rule_id.as_deref(),
            Some("editor")
        );
        let import = store.dictation_personalization_snapshot().unwrap();
        assert!(import.profile.active_project.is_empty());
        assert_eq!(
            import.project_selection.source,
            vesper_core::ProjectSource::Shared
        );
    }
}

fn stamp() -> i64 {
    Utc::now().timestamp_millis()
}

/// Stable across installed models and reruns of identical resampled audio.
pub fn learning_audio_key(samples: &[f32]) -> String {
    let mut hash = Sha256::new();
    // Persisted identity domain, not product branding. Changing it would assign
    // a second identity to existing audio and break cross-install deduplication.
    hash.update(b"transcribe-learning-pcm16k-v1");
    for sample in samples {
        hash.update(sample.to_le_bytes());
    }
    format!("{:x}", hash.finalize())
}

impl DatasetStore {
    /// Authenticate the frozen scope against an application-owned observation.
    /// Frontend callers never supply this context. Expired/forgotten operations
    /// abstain; a mutable settings project is not evidence about an older result.
    pub(crate) fn learning_operation_current(&self, context: &LearningContext) -> Result<bool> {
        if !permitted(&self.personalization()?, context)? {
            return Ok(false);
        }
        Ok(self.automatic_learning_state()?.events.iter().any(|event| {
            matches!(event.kind, LearningEventKind::RawObservation)
                && event.context == *context
                && stamp().saturating_sub(event.timestamp_ms) <= EXPIRY_MS
                && event.id == format!("{}:observed", context.result_id)
        }))
    }

    pub fn dictation_personalization_snapshot(&self) -> Result<DictationPersonalizationSnapshot> {
        // Persist the existing desktop enable migration before the read transaction.
        self.dictation_personalization()?;
        self.ensure_workspace_store()?;
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Deferred,
        )?;
        let profile = self.personalization()?;
        let project_selection = vesper_core::resolve_project(
            vesper_core::ProjectMode::Manual,
            &profile.active_project,
            &[],
            None,
            None,
        );
        let snapshot = DictationPersonalizationSnapshot {
            profile,
            project_selection,
            learning: self.automatic_learning_state()?,
            workspace: Default::default(),
            workspaces: self.workspace_snapshot()?,
            // A damaged inventory never blocks dictation; it only loses hints.
            inventory: self.inventory_terms().unwrap_or_default(),
            working_set: vesper_core::WorkingSet::default(),
            field_terms: vec![],
            load_warning: None,
        };
        tx.commit()?;
        Ok(snapshot)
    }

    pub fn automatic_learning_state(&self) -> Result<AutomaticLearningState> {
        let state: Option<String> = self
            .connection
            .query_row(
                "SELECT state_json FROM automatic_learning WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let mut state: AutomaticLearningState = state
            .map(|s| stored::decode(&s))
            .transpose()?
            .unwrap_or_default();
        if !(1..=2).contains(&state.schema_version)
            || !(1..=LEARNING_POLICY_VERSION).contains(&state.policy_version)
            || state.candidates.len() > MAX_CANDIDATES + 128
            || state.events.len() > MAX_EVENTS
            || state.candidates.iter().any(|c| {
                c.support.len() > 16
                    || c.support.iter().any(|s| {
                        s.teaching_terms.len() > vesper_core::MAX_TEACHING_TERMS
                            || s.teaching_terms.iter().any(|t| {
                                t.is_empty()
                                    || t.len() > vesper_core::MAX_TEACHING_TERM_BYTES
                                    || t.chars().any(char::is_control)
                            })
                            || s.teaching_terms
                                .iter()
                                .collect::<std::collections::BTreeSet<_>>()
                                .len()
                                != s.teaching_terms.len()
                    })
            })
        {
            return Err(DatasetError::InvalidConfig(
                "Unsupported or oversized automatic-learning store.".into(),
            ));
        }
        if state.policy_version < LEARNING_POLICY_VERSION {
            // A new policy version is the documented way out of a tripped breaker.
            state.breaker_tripped_ms = None;
            state.reverts.clear();
        }
        // Read compatibility is non-writing; the next mutation persists schema 2.
        state.schema_version = 2;
        state.policy_version = LEARNING_POLICY_VERSION;
        Ok(state)
    }

    /// Caller supplies authoritative result context and before-text, never a frontend baseline.
    pub fn learn_from_edit(
        &self,
        context: &LearningContext,
        revision: u64,
        before: &str,
        after: &str,
    ) -> Result<LearningOutcome> {
        let started = std::time::Instant::now();
        let tx = self.connection.unchecked_transaction()?;
        let mut outcome = self.learn_edit_in_transaction(&tx, context, revision, before, after)?;
        tx.commit()?;
        outcome.elapsed_micros = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
        Ok(outcome)
    }

    fn learn_edit_in_transaction(
        &self,
        tx: &Connection,
        context: &LearningContext,
        revision: u64,
        before: &str,
        after: &str,
    ) -> Result<LearningOutcome> {
        let started = std::time::Instant::now();
        let mut profile = self.personalization()?;
        let mut outcome = LearningOutcome {
            profile_revision: profile.revision,
            ..Default::default()
        };
        if !permitted(&profile, context)? {
            outcome.status = "Learning is off for this recording.".into();
            return Ok(outcome);
        }
        let mut state = self.automatic_learning_state()?;
        prune(&mut state, stamp());
        let mut teaching_changed = false;
        let mut pending_reason = None;
        let event_id = format!("{}:{revision}", context.result_id);
        if state.events.iter().any(|e| e.id == event_id) {
            outcome.status = "This edit was already processed.".into();
            return Ok(outcome);
        }
        let pair = automatic_spelling_pair(before, after);
        let result_prefix = format!("{}:", context.result_id);
        let mut withdrawn = Vec::new();
        for candidate in &mut state.candidates {
            if after.len() <= vesper_core::MAX_LEARNING_TEXT_BYTES
                && scope(candidate.language, &candidate.project, context)
                && !memory_phrase_occurs(after, &candidate.phrase)
            {
                let previous = candidate.support.len();
                candidate
                    .support
                    .retain(|support| !support.event_id.starts_with(&result_prefix));
                if previous != candidate.support.len() {
                    withdrawn.push(candidate.id.clone());
                    candidate.reason =
                        "Support withdrawn after the user revised this draft.".into();
                }
            }
        }
        // Rejected automatic output is negative evidence, including a third
        // spelling. Do not turn a generated intermediate word into an audio alias.
        // Withdrawn automatic spellings leave the vocabulary entirely: a listed
        // word is always used, so the system removes what it added instead of
        // parking a disabled entry. The candidate is suspended as negative
        // evidence so the spelling does not reactivate.
        let withdrawn_entries: Vec<(String, String)> = profile
            .vocabulary
            .iter()
            .filter(|entry| {
                entry.automatic_source.is_some()
                    && entry.enabled
                    && scope(entry.language, &entry.project, context)
                    && (withdrawn.contains(entry.automatic_source.as_ref().unwrap())
                        || (memory_phrase_occurs(before, &entry.phrase)
                            && !memory_phrase_occurs(after, &entry.phrase)
                            && (entry.aliases.iter().any(|a| memory_phrase_occurs(after, a))
                                || pair
                                    .as_ref()
                                    .is_some_and(|p| p.heard.eq_ignore_ascii_case(&entry.phrase)))))
            })
            .map(|entry| (entry.id.clone(), entry.automatic_source.clone().unwrap()))
            .collect();
        for (_, source) in &withdrawn_entries {
            suspend(&mut state, source);
            outcome.suspended += 1;
        }
        record_reverts(
            &mut state,
            withdrawn_entries.iter().map(|(_, s)| s.as_str()),
        );
        if !withdrawn_entries.is_empty() {
            profile
                .vocabulary
                .retain(|entry| !withdrawn_entries.iter().any(|(id, _)| *id == entry.id));
        }
        if let Some(pair) = pair
            .as_ref()
            .filter(|_| outcome.suspended == 0 && withdrawn.is_empty())
        {
            let position = state.candidates.iter().position(|c| {
                c.phrase == pair.intended
                    && c.alias == pair.heard
                    && c.language == context.language
                    && c.project == context.project
            });
            let position = position.unwrap_or_else(|| {
                let excluded = vesper_core::memory_words(&pair.heard);
                let tags = vesper_core::memory_words(before)
                    .difference(&excluded)
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" ");
                state.candidates.push(LearningCandidate {
                    id: Uuid::new_v4().to_string(),
                    phrase: pair.intended.clone(),
                    alias: pair.heard.clone(),
                    language: context.language,
                    project: context.project.clone(),
                    context: if context.project.is_empty() {
                        tags
                    } else {
                        String::new()
                    },
                    state: CandidateState::Provisional,
                    support: vec![],
                    observed_count: 0,
                    last_seen_ms: stamp(),
                    reason: "Waiting for independent spelling evidence.".into(),
                });
                state.candidates.len() - 1
            });
            let paused = state.breaker_tripped_ms.is_some();
            let candidate = &mut state.candidates[position];
            candidate.last_seen_ms = stamp();
            if candidate.state != CandidateState::Suspended {
                if !candidate.support.iter().any(|s| {
                    s.source_key == context.source_key || s.episode_id == context.episode_id
                }) {
                    let support = EditSupport {
                        source_key: context.source_key.clone(),
                        episode_id: context.episode_id.clone(),
                        event_id: event_id.clone(),
                        teaching_terms: vesper_core::teaching_context_terms(
                            before,
                            pair,
                            &context.project,
                        ),
                        recognizer: vesper_core::recognizer_family(&context.model_id),
                    };
                    if candidate.support.len() < 16 {
                        teaching_changed |= candidate.state == CandidateState::Active
                            && !support.teaching_terms.is_empty();
                        candidate.support.push(support);
                    } else if !support.teaching_terms.is_empty() {
                        // Full legacy candidates can acquire context without increasing support.
                        if let Some(old) = candidate
                            .support
                            .iter_mut()
                            .find(|s| s.teaching_terms.is_empty())
                        {
                            teaching_changed |= candidate.state == CandidateState::Active;
                            *old = support;
                        }
                    }
                }
                // Asymmetric promotion: see `vesper_core::promotion_policy`.
                let required = vesper_core::promotion_policy(pair, !context.project.is_empty())
                    .required_support();
                let conflict = profile.vocabulary.iter().any(|v| {
                    (v.enabled || v.automatic_source.is_none())
                        && (v.project.is_empty()
                            || candidate.project.is_empty()
                            || v.project.eq_ignore_ascii_case(&candidate.project))
                        && (v.language == Language::Auto
                            || candidate.language == Language::Auto
                            || v.language == candidate.language)
                        && v.aliases
                            .iter()
                            .any(|a| a.eq_ignore_ascii_case(&candidate.alias))
                        && v.phrase != candidate.phrase
                });
                if conflict {
                    candidate.state = CandidateState::Suspended;
                    candidate.reason =
                        "Conflicts with an existing spelling; no automatic replacement.".into();
                } else if required.is_some_and(|required| candidate.support.len() >= required)
                    && !paused
                    && candidate.language != Language::Auto
                    && (!candidate.context.is_empty() || !candidate.project.is_empty())
                    && candidate.state != CandidateState::Active
                    && profile.vocabulary.len() < 128
                {
                    // Never alter or reactivate a manually managed equivalent entry.
                    if !profile.vocabulary.iter().any(|v| {
                        v.phrase == candidate.phrase
                            && v.project == candidate.project
                            && v.language == candidate.language
                            && v.aliases.contains(&candidate.alias)
                    }) {
                        profile.vocabulary.push(VocabularyEntry {
                            id: Uuid::new_v4().to_string(),
                            phrase: candidate.phrase.clone(),
                            aliases: vec![candidate.alias.clone()],
                            language: candidate.language,
                            project: candidate.project.clone(),
                            context: candidate.context.clone(),
                            enabled: true,
                            automatic_source: Some(candidate.id.clone()),
                            learned_at: now(),
                            ..Default::default()
                        });
                        candidate.state = CandidateState::Active;
                        candidate.reason =
                            "Learned from ordinary user edits; not an audio review.".into();
                        outcome.activated.push(candidate.phrase.clone());
                    }
                }
                if candidate.state == CandidateState::Provisional {
                    candidate.reason = if candidate.language == Language::Auto {
                        "Select a recording language in Settings, then correct a new recording; automatic language mode cannot activate learned spellings.".into()
                    } else if required.is_none() {
                        "A single common word is never replaced automatically. Add the spelling in Vocabulary to use it.".into()
                    } else if paused {
                        "Automatic spellings are paused after recent rejections. Resume them in Vocabulary.".into()
                    } else if let Some(required) = required.filter(|r| candidate.support.len() < *r)
                    {
                        format!(
                            "Waiting for independent spelling evidence ({}/{required} recordings). Replaying the same audio does not count again.",
                            candidate.support.len()
                        )
                    } else if candidate.context.is_empty() && candidate.project.is_empty() {
                        "Waiting for project or transcript context before using this spelling."
                            .into()
                    } else if profile.vocabulary.len() >= 128 {
                        "Vocabulary is full (128 entries). Remove an unused entry, then correct a new recording.".into()
                    } else {
                        "An equivalent saved entry is managed manually. Check its enabled state in Vocabulary.".into()
                    };
                    pending_reason = Some(candidate.reason.clone());
                    outcome.provisional += 1;
                }
            }
        }
        // Events keep changed spans; independent support keeps bounded teaching cues.
        // A rewrite without a spelling pair keeps neither transcript nor new cues.
        state.events.push(LearningEvent {
            id: event_id,
            context: context.clone(),
            kind: LearningEventKind::UserEdit,
            pair,
            retry: None,
            timestamp_ms: stamp(),
        });
        prune(&mut state, stamp());
        put_state(tx, &state)?;
        if !outcome.activated.is_empty() || outcome.suspended > 0 || teaching_changed {
            put_profile(tx, &mut profile)?;
        }
        outcome.profile_revision = profile.revision;
        outcome.elapsed_micros = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
        outcome.status = if outcome.suspended > 0 {
            "Rejected learned spelling paused."
        } else if !withdrawn.is_empty() {
            "Draft updated; earlier spelling evidence withdrawn."
        } else if !outcome.activated.is_empty() {
            "Spelling remembered for relevant future dictation."
        } else if outcome.provisional > 0 {
            pending_reason
                .as_deref()
                .unwrap_or("Spelling evidence saved; waiting for independent support.")
        } else {
            "Draft updated; no automatic spelling learned."
        }
        .into();
        Ok(outcome)
    }

    pub fn edit_session_draft(
        &self,
        id: &str,
        expected_revision: u64,
        before: &str,
        after: &str,
        context: &LearningContext,
    ) -> Result<(SessionRecord, LearningOutcome)> {
        validate_text(after, false)?;
        let started = std::time::Instant::now();
        let tx = self.connection.unchecked_transaction()?;
        let mut record = read_record(&tx, id)?;
        let baseline = record
            .draft_transcript
            .as_deref()
            .or_else(|| {
                (!record.personalization_rejected)
                    .then_some(record.personalized_transcript.as_deref())
                    .flatten()
            })
            .or(record.cleaned_transcript.as_deref())
            .or(record.raw_transcript.as_deref())
            .unwrap_or("");
        if record.draft_revision != expected_revision || baseline != before {
            return Err(DatasetError::InvalidConfig(
                "The saved draft changed; refresh before editing.".into(),
            ));
        }
        let mut outcome =
            self.learn_edit_in_transaction(&tx, context, expected_revision, before, after)?;
        record.draft_revision = record
            .draft_revision
            .checked_add(1)
            .ok_or_else(|| DatasetError::InvalidConfig("Draft revision exhausted.".into()))?;
        record.draft_transcript = Some(after.into());
        write_record(&tx, &record)?;
        tx.commit()?;
        outcome.elapsed_micros = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
        Ok((record, outcome))
    }

    /// Observations can rank credible entries and collect candidates, never activate aliases.
    pub fn observe_learning(
        &self,
        context: &LearningContext,
        raw: &str,
        retry: Option<RetryRelation>,
    ) -> Result<()> {
        let tx = self.connection.unchecked_transaction()?;
        let mut profile = self.personalization()?;
        if !permitted(&profile, context)? || raw.len() > vesper_core::MAX_LEARNING_TEXT_BYTES {
            return Ok(());
        }
        let mut state = self.automatic_learning_state()?;
        prune(&mut state, stamp());
        if state
            .events
            .iter()
            .any(|e| e.id == format!("{}:observed", context.result_id))
        {
            return Ok(());
        }
        let independent = !state.events.iter().any(|e| {
            matches!(e.kind, LearningEventKind::RawObservation)
                && (e.context.source_key == context.source_key
                    || e.context.episode_id == context.episode_id)
        });
        let mut changed = false;
        for word in &mut profile.vocabulary {
            if independent
                && word.enabled
                && scope(word.language, &word.project, context)
                && vesper_core::context_matches(
                    &word.context,
                    &format!("{} {raw}", context.project),
                )
                && (memory_phrase_occurs(raw, &word.phrase)
                    || word.aliases.iter().any(|a| memory_phrase_occurs(raw, a)))
                // A phrase this decode was biased toward may appear because of
                // the bias; counting it would make the hint list confirm itself.
                && !vesper_core::hinted(&word.phrase, &context.hinted_phrases)
            {
                word.usage_count = word.usage_count.saturating_add(1);
                changed = true;
            }
        }
        // Only conspicuous term forms are collected; no NER model or external dictionary.
        for word in raw
            .split_whitespace()
            .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric() && c != '_'))
            .filter(|s| {
                independent
                    && s.len() <= 128
                    && vesper_core::technical_spelling(s)
                    && memory_phrase_occurs(raw, s)
                    && !vesper_core::hinted(s, &context.hinted_phrases)
            })
            .take(8)
        {
            if let Some(c) = state.candidates.iter_mut().find(|c| {
                c.phrase == word
                    && c.alias.is_empty()
                    && c.language == context.language
                    && c.project == context.project
            }) {
                c.observed_count = c.observed_count.saturating_add(1);
                c.last_seen_ms = stamp();
            } else {
                state.candidates.push(LearningCandidate {
                    id: Uuid::new_v4().to_string(),
                    phrase: word.into(),
                    alias: String::new(),
                    language: context.language,
                    project: context.project.clone(),
                    context: String::new(),
                    state: CandidateState::Observed,
                    support: vec![],
                    observed_count: 1,
                    last_seen_ms: stamp(),
                    reason: "Observed in ASR output; spelling is unverified.".into(),
                });
            }
        }
        state.events.push(LearningEvent {
            id: format!("{}:observed", context.result_id),
            context: context.clone(),
            kind: LearningEventKind::RawObservation,
            pair: None,
            retry,
            timestamp_ms: stamp(),
        });
        prune(&mut state, stamp());
        put_state(&tx, &state)?;
        if changed {
            put_profile(&tx, &mut profile)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn suspend_automatic_memories(&self, ids: &[String]) -> Result<usize> {
        let tx = self.connection.unchecked_transaction()?;
        let count = self.suspend_automatic_in_transaction(&tx, ids)?;
        tx.commit()?;
        Ok(count)
    }

    /// Suspend one candidate by id, with or without a vocabulary entry.
    /// Dismissing a proposal keeps it suspended so future edits never
    /// re-learn it; any linked automatic entry leaves the vocabulary, because
    /// a listed word is always used.
    pub fn suspend_automatic_candidate(&self, candidate_id: &str) -> Result<usize> {
        let tx = self.connection.unchecked_transaction()?;
        let mut profile = self.personalization()?;
        let mut state = self.automatic_learning_state()?;
        if !state.candidates.iter().any(|c| c.id == candidate_id) {
            return Ok(0);
        }
        suspend(&mut state, candidate_id);
        profile
            .vocabulary
            .retain(|entry| entry.automatic_source.as_deref() != Some(candidate_id));
        put_profile(&tx, &mut profile)?;
        put_state(&tx, &state)?;
        tx.commit()?;
        Ok(1)
    }

    /// Promote a proposal candidate (observed or provisional) to a manual
    /// vocabulary entry. The entry is user-owned and used while listed; the
    /// candidate is removed so it cannot activate a duplicate.
    pub fn approve_automatic_candidate(
        &self,
        candidate_id: &str,
    ) -> Result<PersonalizationProfile> {
        let tx = self.connection.unchecked_transaction()?;
        let mut profile = self.personalization()?;
        let mut state = self.automatic_learning_state()?;
        let position = state
            .candidates
            .iter()
            .position(|c| c.id == candidate_id)
            .ok_or_else(|| DatasetError::SessionNotFound(candidate_id.into()))?;
        let candidate = state.candidates[position].clone();
        if candidate.state == CandidateState::Suspended {
            return Err(DatasetError::InvalidConfig(
                "That suggestion was removed; use Forget automatic learning to reset it.".into(),
            ));
        }
        let exists = profile.vocabulary.iter().any(|entry| {
            entry.phrase == candidate.phrase
                && entry.project == candidate.project
                && entry.language == candidate.language
                && (candidate.alias.is_empty() || entry.aliases.contains(&candidate.alias))
        });
        if !exists {
            if profile.vocabulary.len() >= 128 {
                return Err(DatasetError::InvalidConfig(
                    "Vocabulary is full (128 entries). Remove an unused entry first.".into(),
                ));
            }
            let mut aliases = Vec::new();
            if !candidate.alias.is_empty() {
                aliases.push(candidate.alias.clone());
            }
            profile.vocabulary.push(VocabularyEntry {
                id: Uuid::new_v4().to_string(),
                phrase: candidate.phrase.clone(),
                aliases,
                language: candidate.language,
                project: candidate.project.clone(),
                context: candidate.context.clone(),
                enabled: true,
                automatic_source: None,
                confirmed: true,
                learned_at: now(),
                ..Default::default()
            });
        }
        state.candidates.remove(position);
        put_profile(&tx, &mut profile)?;
        put_state(&tx, &state)?;
        tx.commit()?;
        Ok(profile)
    }

    pub fn undo_learning_for_result(&self, result_id: &str) -> Result<usize> {
        let tx = self.connection.unchecked_transaction()?;
        self.invalidate_semantic_result(result_id)?;
        self.invalidate_policy_result(result_id)?;
        let state = self.automatic_learning_state()?;
        let prefix = format!("{result_id}:");
        let candidate_ids: Vec<_> = state
            .candidates
            .iter()
            .filter(|c| c.support.iter().any(|s| s.event_id.starts_with(&prefix)))
            .map(|c| &c.id)
            .collect();
        let ids: Vec<_> = self
            .personalization()?
            .vocabulary
            .into_iter()
            .filter(|v| {
                v.automatic_source
                    .as_ref()
                    .is_some_and(|s| candidate_ids.contains(&s))
            })
            .map(|v| v.id)
            .collect();
        let count = self.suspend_automatic_in_transaction(&tx, &ids)?;
        tx.commit()?;
        Ok(count)
    }

    pub(crate) fn suspend_automatic_in_transaction(
        &self,
        tx: &Connection,
        ids: &[String],
    ) -> Result<usize> {
        self.suspend_automatic_with(tx, ids, false)
    }

    /// The user undid applied output: suspend and count the rejection for the breaker.
    pub(crate) fn reject_automatic_in_transaction(
        &self,
        tx: &Connection,
        ids: &[String],
    ) -> Result<usize> {
        self.suspend_automatic_with(tx, ids, true)
    }

    /// A saved draft that drops a gate-written spelling and restores what was heard
    /// (or picks a third spelling) rejects that term's automatic use. Returns the
    /// rejected term ids; each counts toward the policy breaker.
    pub fn observe_gate_reverts(
        &self,
        context: &LearningContext,
        before: &str,
        after: &str,
        changes: &[vesper_core::GateChange],
    ) -> Result<Vec<String>> {
        if changes.is_empty() || !permitted(&self.personalization()?, context)? {
            return Ok(vec![]);
        }
        let rejected = changes
            .iter()
            .filter(|c| {
                memory_phrase_occurs(before, &c.spelling)
                    && !memory_phrase_occurs(after, &c.spelling)
            })
            .map(|c| c.term_id.clone())
            .collect::<Vec<_>>();
        // Exact normalizations name their inventory row; near-miss repairs name
        // the reference spelling, which suspends every inventory row with it.
        let mut suspend = rejected
            .iter()
            .filter_map(|id| id.strip_prefix("inv:")?.parse::<i64>().ok())
            .collect::<Vec<_>>();
        let references = rejected
            .iter()
            .filter_map(|id| id.strip_prefix("reference:"))
            .collect::<Vec<_>>();
        if !references.is_empty() {
            suspend.extend(
                self.inventory_terms()?
                    .into_iter()
                    .filter(|t| references.contains(&t.surface.as_str()))
                    .filter_map(|t| t.id.strip_prefix("inv:")?.parse::<i64>().ok()),
            );
        }
        for id in suspend {
            self.suspend_inventory_term(id)?;
        }
        if !rejected.is_empty() {
            let tx = self.connection.unchecked_transaction()?;
            let mut state = self.automatic_learning_state()?;
            record_reverts(&mut state, rejected.iter().map(|id| id.as_str()));
            put_state(&tx, &state)?;
            tx.commit()?;
        }
        Ok(rejected)
    }

    /// Clear a tripped breaker after the user reviews their automatic spellings.
    pub fn resume_automatic_spellings(&self) -> Result<()> {
        let tx = self.connection.unchecked_transaction()?;
        let mut state = self.automatic_learning_state()?;
        state.breaker_tripped_ms = None;
        state.reverts.clear();
        put_state(&tx, &state)?;
        tx.commit()?;
        Ok(())
    }

    fn suspend_automatic_with(
        &self,
        tx: &Connection,
        ids: &[String],
        reverted: bool,
    ) -> Result<usize> {
        let mut profile = self.personalization()?;
        let mut state = self.automatic_learning_state()?;
        let mut count = 0;
        // Suspended memories leave the vocabulary: a listed word is always
        // used. The candidate suspension is kept as negative evidence.
        let removed: Vec<String> = profile
            .vocabulary
            .iter()
            .filter(|entry| ids.contains(&entry.id) && entry.automatic_source.is_some())
            .map(|entry| entry.automatic_source.clone().unwrap())
            .collect();
        for source in &removed {
            suspend(&mut state, source);
            count += 1;
        }
        if reverted {
            record_reverts(&mut state, removed.iter().map(String::as_str));
        }
        if count > 0 {
            let sources = removed;
            profile.vocabulary.retain(|entry| {
                !entry
                    .automatic_source
                    .as_ref()
                    .is_some_and(|s| sources.contains(s))
            });
        }
        if count > 0 {
            put_profile(tx, &mut profile)?;
            put_state(tx, &state)?;
        }
        Ok(count)
    }

    pub fn forget_automatic_learning(
        &self,
        expected_revision: u64,
    ) -> Result<PersonalizationProfile> {
        let tx = self.connection.unchecked_transaction()?;
        let mut profile = self.personalization()?;
        if profile.revision != expected_revision {
            return Err(DatasetError::InvalidConfig(
                "Preferences changed; refresh before forgetting.".into(),
            ));
        }
        profile.vocabulary.retain(|v| v.automatic_source.is_none());
        for word in &mut profile.vocabulary {
            word.usage_count = 0;
        }
        profile.learning_epoch = profile.learning_epoch.saturating_add(1);
        tx.execute("DELETE FROM semantic_shadow", [])?;
        tx.execute("DELETE FROM policy_shadow", [])?;
        tx.execute("DELETE FROM policy_shadow_job", [])?;
        put_profile(&tx, &mut profile)?;
        put_state(&tx, &AutomaticLearningState::default())?;
        tx.commit()?;
        Ok(profile)
    }

    /// Runs inside the normal profile-save transaction. Clients cannot forge provenance/ranks.
    pub(crate) fn reconcile_automatic_profile(
        &self,
        current: &PersonalizationProfile,
        next: &mut PersonalizationProfile,
    ) -> Result<()> {
        let mut state = self.automatic_learning_state()?;
        let mut invalidate = current.automatic_learning != next.automatic_learning;
        next.learning_epoch = current.learning_epoch;
        let mut disabled_automatic: Vec<String> = Vec::new();
        for entry in &mut next.vocabulary {
            let previous = current.vocabulary.iter().find(|p| p.id == entry.id);
            entry.usage_count = previous.map_or(0, |p| p.usage_count);
            if let Some(source) = &entry.automatic_source {
                let valid = previous.is_some_and(|p| {
                    p.automatic_source.as_ref() == Some(source)
                        && p.phrase == entry.phrase
                        && p.aliases == entry.aliases
                        && p.language == entry.language
                        && p.project == entry.project
                        && p.context == entry.context
                        && entry.sources.is_empty()
                });
                if !valid {
                    return Err(DatasetError::InvalidConfig("Automatic source cannot be edited. Convert this entry to a manual spelling first.".into()));
                }
                // Removal is the only off-switch: a saved automatic entry that
                // arrives disabled is dropped below and its candidate is
                // suspended, so a listed word is never silently unused.
                if !entry.enabled {
                    disabled_automatic.push(source.clone());
                }
                // An explicit inspector enable is a manual decision, not implicit admission.
                if entry.enabled && previous.is_some_and(|p| !p.enabled) {
                    entry.automatic_source = None;
                    invalidate = true;
                }
            }
            // User-added words are always confirmed; the tick only gates
            // automatic entries.
            if entry.automatic_source.is_none() {
                entry.confirmed = true;
            }
        }
        if !disabled_automatic.is_empty() {
            next.vocabulary.retain(|entry| {
                !entry
                    .automatic_source
                    .as_ref()
                    .is_some_and(|s| disabled_automatic.contains(s))
            });
            for source in &disabled_automatic {
                suspend(&mut state, source);
            }
            invalidate = true;
        }
        for previous in &current.vocabulary {
            if let Some(source) = &previous.automatic_source {
                match next.vocabulary.iter().find(|v| v.id == previous.id) {
                    Some(entry) if entry.automatic_source.is_none() => {
                        // Adopted as a manual entry on review: the automatic
                        // evidence is consumed, not rejected, so the candidate
                        // and its support leave with it.
                        let support: Vec<_> = state
                            .candidates
                            .iter()
                            .filter(|c| &c.id == source)
                            .flat_map(|c| c.support.iter().map(|s| s.event_id.clone()))
                            .collect();
                        state.events.retain(|e| {
                            !support.contains(&e.id)
                                && !e.pair.as_ref().is_some_and(|p| {
                                    p.intended == previous.phrase
                                        && previous.aliases.contains(&p.heard)
                                })
                        });
                        state.candidates.retain(|c| {
                            &c.id != source && !(c.phrase == previous.phrase && c.alias.is_empty())
                        });
                        invalidate = true;
                    }
                    None => {
                        // A removed automatic entry suspends its candidate
                        // instead of deleting it: the rejection is negative
                        // evidence, so future edits do not re-learn a spelling
                        // the user removed. Same-phrase observations are
                        // suspended too, or they would re-learn it. Past events
                        // are kept as an audit trail.
                        let mut to_suspend = vec![source.clone()];
                        to_suspend.extend(
                            state
                                .candidates
                                .iter()
                                .filter(|c| c.phrase == previous.phrase && c.alias.is_empty())
                                .map(|c| c.id.clone()),
                        );
                        for id in &to_suspend {
                            suspend(&mut state, id);
                        }
                        invalidate = true;
                    }
                    _ => {}
                }
            }
        }
        if invalidate {
            next.learning_epoch = next.learning_epoch.saturating_add(1);
            self.connection.execute("DELETE FROM semantic_shadow", [])?;
            self.connection.execute("DELETE FROM policy_shadow", [])?;
            self.connection
                .execute("DELETE FROM policy_shadow_job", [])?;
        }
        prune(&mut state, stamp());
        // The surrounding profile transaction commits this atomically.
        if !state.events.is_empty() || !state.candidates.is_empty() || invalidate {
            put_state(&self.connection, &state)?;
        }
        Ok(())
    }
}

fn permitted(profile: &PersonalizationProfile, context: &LearningContext) -> Result<bool> {
    if context.result_id.is_empty()
        || context.result_id.len() > 64
        || context.source_key.is_empty()
        || context.source_key.len() > 128
        || context.episode_id.is_empty()
        || context.episode_id.len() > 64
        || context.project.len() > 64
        || context.model_id.len() > 256
    {
        return Err(DatasetError::InvalidConfig(
            "Invalid learning result context.".into(),
        ));
    }
    Ok(profile.automatic_learning
        && context.learning_enabled
        && profile.learning_epoch == context.learning_epoch)
}
fn scope(language: Language, project: &str, context: &LearningContext) -> bool {
    vesper_core::language_matches(language, context.language)
        && vesper_core::project_matches(project, &context.project)
}
/// Rejections of applied rules count toward the breaker; list housekeeping does not.
fn record_reverts<'a>(
    state: &mut AutomaticLearningState,
    rules: impl IntoIterator<Item = &'a str>,
) {
    let now = stamp();
    state
        .reverts
        .extend(rules.into_iter().map(|rule| RuleRevert {
            rule: rule.to_owned(),
            at_ms: now,
        }));
    state
        .reverts
        .retain(|r| now.saturating_sub(r.at_ms) <= vesper_core::BREAKER_WINDOW_MS);
    if state.breaker_tripped_ms.is_none()
        && vesper_core::breaker_trips(
            state.reverts.iter().map(|r| (r.rule.as_str(), r.at_ms)),
            now,
        )
    {
        state.breaker_tripped_ms = Some(now);
    }
}

fn suspend(state: &mut AutomaticLearningState, id: &str) {
    if let Some(candidate) = state.candidates.iter_mut().find(|c| c.id == id) {
        candidate.state = CandidateState::Suspended;
        for support in &mut candidate.support {
            support.teaching_terms.clear();
        }
        candidate.reason = "Paused after a rejected or disabled application.".into();
    }
}
fn prune(state: &mut AutomaticLearningState, timestamp: i64) {
    for candidate in &mut state.candidates {
        if candidate.state == CandidateState::Suspended {
            for support in &mut candidate.support {
                support.teaching_terms.clear();
            }
        }
    }
    let before = state.candidates.len();
    state.candidates.retain(|c| {
        matches!(c.state, CandidateState::Active | CandidateState::Suspended)
            || timestamp.saturating_sub(c.last_seen_ms) <= EXPIRY_MS
    });
    state.expired_candidates = state
        .expired_candidates
        .saturating_add((before - state.candidates.len()) as u64);
    state
        .events
        .retain(|e| timestamp.saturating_sub(e.timestamp_ms) <= EXPIRY_MS);
    if state.events.len() > MAX_EVENTS {
        state.events.drain(..state.events.len() - MAX_EVENTS);
    }
    state.candidates.sort_by_key(|c| c.last_seen_ms);
    while state
        .candidates
        .iter()
        .filter(|c| c.state != CandidateState::Active)
        .count()
        > MAX_CANDIDATES
    {
        if let Some(index) = state
            .candidates
            .iter()
            .position(|c| c.state != CandidateState::Active)
        {
            state.candidates.remove(index);
        }
    }
}
pub(crate) fn put_state(tx: &Connection, state: &AutomaticLearningState) -> Result<()> {
    if state
        .events
        .iter()
        .any(|e| serde_json::to_vec(e).map_or(true, |s| s.len() > 4096))
    {
        return Err(DatasetError::InvalidConfig(
            "Learning evidence exceeds its byte budget.".into(),
        ));
    }
    tx.execute("INSERT INTO automatic_learning(id,state_json) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET state_json=excluded.state_json", [serde_json::to_string(state)?])?;
    Ok(())
}
pub(crate) fn put_profile(tx: &Connection, profile: &mut PersonalizationProfile) -> Result<()> {
    profile.schema_version = 4;
    profile.revision = profile
        .revision
        .checked_add(1)
        .ok_or_else(|| DatasetError::InvalidConfig("Profile revision exhausted.".into()))?;
    profile.validate().map_err(DatasetError::InvalidConfig)?;
    tx.execute("INSERT INTO personalization(id,profile_json) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET profile_json=excluded.profile_json", [serde_json::to_string(profile)?])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const CONTEXT_HEARD: &str = "Open readme for deployment";
    const CONTEXT_EDITED: &str = "Open README.md for deployment";

    #[test]
    fn audio_identity_is_unchanged_by_product_renaming() {
        assert_eq!(
            learning_audio_key(&[0.0, 0.25, -0.5, 1.0]),
            "f0775b4bdcb5dc3ba83340e1f1bca3da6bb5dfe268f0c35bddf5d2159f0119f1"
        );
    }

    #[test]
    fn draft_transaction_keeps_background_writes_from_invalidating_its_snapshot() {
        let (_dir, store, profile) = setup();
        let background = DatasetStore::open(store.root.clone()).unwrap();
        background
            .connection
            .busy_timeout(std::time::Duration::ZERO)
            .unwrap();
        let tx = store.connection.unchecked_transaction().unwrap();
        assert_eq!(store.personalization().unwrap(), profile);

        // A shadow write between the foreground read and write must yield,
        // rather than commit a new WAL snapshot and break the user's edit.
        let error = background
            .connection
            .execute(
                "INSERT INTO policy_shadow_job(id, request_json) VALUES(1, '{}')",
                [],
            )
            .unwrap_err();
        assert_eq!(
            error.sqlite_error_code(),
            Some(rusqlite::ErrorCode::DatabaseBusy)
        );
        store
            .learn_edit_in_transaction(
                &tx,
                &context(&profile, "concurrent-edit"),
                0,
                CONTEXT_HEARD,
                CONTEXT_EDITED,
            )
            .unwrap();
        tx.commit().unwrap();
        assert_eq!(
            store.automatic_learning_state().unwrap().candidates.len(),
            1
        );
        background
            .connection
            .execute(
                "INSERT INTO policy_shadow_job(id, request_json) VALUES(1, '{}')",
                [],
            )
            .unwrap();
    }

    #[test]
    fn post_asr_context_guard_requires_observable_cues_and_preserves_manual_entries() {
        let (_dir, store, p) = setup();
        store
            .learn_from_edit(&context(&p, "context"), 0, CONTEXT_HEARD, CONTEXT_EDITED)
            .unwrap();
        let mut snapshot = store.dictation_personalization_snapshot().unwrap();
        assert_eq!(
            snapshot.learning.candidates[0].support[0].teaching_terms,
            ["deployment"]
        );
        let selected = snapshot.post_asr_profile(CONTEXT_HEARD, Language::English);
        assert_eq!(
            selected.personalize(CONTEXT_HEARD, Language::English).text,
            CONTEXT_EDITED
        );
        for raw in [
            "Open readme for shopping",
            "Open readme in repo",
            "Open 'readme' for deployment",
            "Open readme for 'deployment'",
            "Discuss deployment without a filename",
        ] {
            assert!(
                !snapshot.post_asr_profile(raw, Language::English).vocabulary[0].enabled,
                "{raw}"
            );
        }
        assert!(
            !snapshot
                .post_asr_profile(CONTEXT_HEARD, Language::Hindi)
                .vocabulary[0]
                .enabled
        );
        assert!(
            !snapshot
                .post_asr_profile(CONTEXT_HEARD, Language::Auto)
                .vocabulary[0]
                .enabled
        );
        let mut manual = snapshot.profile.vocabulary[0].clone();
        manual.id = "manual".into();
        manual.automatic_source = None;
        snapshot.profile.vocabulary.push(manual.clone());
        snapshot.profile.active_project = "other".into();
        let selected = snapshot.post_asr_profile(CONTEXT_HEARD, Language::English);
        assert!(!selected.vocabulary[0].enabled);
        assert_eq!(selected.vocabulary[1], manual);
        // Selection is operation-local; it never disables the persisted entry.
        assert!(store.personalization().unwrap().vocabulary[0].enabled);
    }

    #[test]
    fn prior_policy_evidence_upgrades_without_rewriting_or_adding_support_on_read() {
        for version in [2, 3] {
            let (_dir, store, p) = setup();
            store
                .learn_from_edit(&context(&p, "original"), 0, CONTEXT_HEARD, CONTEXT_EDITED)
                .unwrap();
            let mut previous =
                serde_json::to_value(store.automatic_learning_state().unwrap()).unwrap();
            previous["policyVersion"] = json!(version);
            let encoded = serde_json::to_string(&previous).unwrap();
            store
                .connection
                .execute("UPDATE automatic_learning SET state_json=?1", [&encoded])
                .unwrap();
            let current = store.automatic_learning_state().unwrap();
            assert_eq!(current.policy_version, LEARNING_POLICY_VERSION);
            assert_eq!(current.candidates[0].support.len(), 1);
            let read_stored = || {
                store
                    .connection
                    .query_row("SELECT state_json FROM automatic_learning", [], |r| {
                        r.get::<_, String>(0)
                    })
                    .unwrap()
            };
            assert_eq!(read_stored(), encoded);
            store
                .observe_learning(&context(&p, "observation"), CONTEXT_HEARD, None)
                .unwrap();
            let written: serde_json::Value = serde_json::from_str(&read_stored()).unwrap();
            assert_eq!(written["policyVersion"], LEARNING_POLICY_VERSION);
            assert_eq!(
                store.automatic_learning_state().unwrap().candidates[0]
                    .support
                    .len(),
                1
            );
        }
    }

    #[test]
    fn legacy_context_is_not_fabricated_and_new_independent_edits_upgrade_atomically() {
        let (_dir, store, p) = setup();
        store
            .learn_from_edit(&context(&p, "legacy"), 0, CONTEXT_HEARD, CONTEXT_EDITED)
            .unwrap();
        let mut legacy = serde_json::to_value(store.automatic_learning_state().unwrap()).unwrap();
        legacy["schemaVersion"] = json!(1);
        legacy["policyVersion"] = json!(1);
        legacy["candidates"][0]["support"][0]
            .as_object_mut()
            .unwrap()
            .remove("teachingTerms");
        let encoded = serde_json::to_string(&legacy).unwrap();
        store
            .connection
            .execute("UPDATE automatic_learning SET state_json=?1", [&encoded])
            .unwrap();
        let old = store.dictation_personalization_snapshot().unwrap();
        assert!(
            !old.post_asr_profile(CONTEXT_HEARD, Language::English)
                .vocabulary[0]
                .enabled
        );
        let stored: String = store
            .connection
            .query_row("SELECT state_json FROM automatic_learning", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(stored, encoded, "read must not rewrite old evidence");
        store
            .learn_from_edit(&context(&p, "fresh"), 0, CONTEXT_HEARD, CONTEXT_EDITED)
            .unwrap();
        let fresh = store.dictation_personalization_snapshot().unwrap();
        assert!(fresh.profile.revision > old.profile.revision);
        assert!(
            fresh
                .post_asr_profile(CONTEXT_HEARD, Language::English)
                .vocabulary[0]
                .enabled
        );
        assert!(
            !old.post_asr_profile(CONTEXT_HEARD, Language::English)
                .vocabulary[0]
                .enabled
        );
        let stored: String = store
            .connection
            .query_row("SELECT state_json FROM automatic_learning", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&stored).unwrap()["schemaVersion"],
            2
        );
    }

    #[test]
    fn full_legacy_support_can_acquire_context_without_inflating_votes_or_duplicate_cues() {
        let (_dir, store, p) = setup();
        for i in 0..16 {
            store
                .learn_from_edit(
                    &context(&p, &format!("old-{i}")),
                    0,
                    "Open readme",
                    "Open README.md",
                )
                .unwrap();
        }
        let mut duplicate = context(&p, "duplicate");
        duplicate.source_key = "audio-old-0".into();
        store
            .learn_from_edit(&duplicate, 0, CONTEXT_HEARD, CONTEXT_EDITED)
            .unwrap();
        assert!(
            !store
                .dictation_personalization_snapshot()
                .unwrap()
                .post_asr_profile(CONTEXT_HEARD, Language::English)
                .vocabulary[0]
                .enabled
        );
        store
            .learn_from_edit(
                &context(&p, "independent"),
                0,
                CONTEXT_HEARD,
                CONTEXT_EDITED,
            )
            .unwrap();
        let snapshot = store.dictation_personalization_snapshot().unwrap();
        assert_eq!(snapshot.learning.candidates[0].support.len(), 16);
        assert!(
            snapshot
                .post_asr_profile(CONTEXT_HEARD, Language::English)
                .vocabulary[0]
                .enabled
        );
    }

    #[test]
    fn context_support_is_rolled_back_cleared_by_undo_and_retained_only_with_active_memory() {
        let (_dir, store, p) = setup();
        store
            .learn_from_edit(&context(&p, "old"), 0, "Open readme", "Open README.md")
            .unwrap();
        store.connection.execute_batch("CREATE TRIGGER fail_context BEFORE UPDATE ON personalization BEGIN SELECT RAISE(ABORT, 'injected context write failure'); END;").unwrap();
        assert!(store
            .learn_from_edit(&context(&p, "new"), 0, CONTEXT_HEARD, CONTEXT_EDITED)
            .is_err());
        assert_eq!(
            store.automatic_learning_state().unwrap().candidates[0]
                .support
                .len(),
            1
        );
        store
            .connection
            .execute_batch("DROP TRIGGER fail_context")
            .unwrap();
        store
            .learn_from_edit(&context(&p, "new"), 0, CONTEXT_HEARD, CONTEXT_EDITED)
            .unwrap();
        let mut retained = store.automatic_learning_state().unwrap();
        prune(&mut retained, stamp() + EXPIRY_MS + 1);
        assert!(retained.events.is_empty());
        assert!(retained.candidates[0]
            .support
            .iter()
            .any(|s| !s.teaching_terms.is_empty()));
        store.undo_learning_for_result("new").unwrap();
        let suspended = store.dictation_personalization_snapshot().unwrap();
        assert!(suspended.learning.candidates[0]
            .support
            .iter()
            .all(|s| s.teaching_terms.is_empty()));
        // Undone automatic memories leave the vocabulary entirely; the
        // candidate stays suspended so the spelling cannot reactivate.
        assert!(
            suspended.profile.vocabulary.is_empty(),
            "undone automatic memories leave the vocabulary"
        );
        assert_eq!(
            suspended.learning.candidates[0].state,
            CandidateState::Suspended
        );
        store
            .forget_automatic_learning(suspended.profile.revision)
            .unwrap();
        assert!(store
            .automatic_learning_state()
            .unwrap()
            .candidates
            .is_empty());
    }

    #[test]
    fn context_metadata_rejects_oversized_duplicate_and_future_evidence() {
        let (_dir, store, p) = setup();
        store
            .learn_from_edit(&context(&p, "context"), 0, CONTEXT_HEARD, CONTEXT_EDITED)
            .unwrap();
        let original = serde_json::to_value(store.automatic_learning_state().unwrap()).unwrap();
        for bad_terms in [
            json!(["x".repeat(65)]),
            json!(["duplicate", "duplicate"]),
            json!((0..17).map(|n| format!("term{n}")).collect::<Vec<_>>()),
            json!(["bad\nterm"]),
        ] {
            let mut invalid = original.clone();
            invalid["candidates"][0]["support"][0]["teachingTerms"] = bad_terms;
            store
                .connection
                .execute(
                    "UPDATE automatic_learning SET state_json=?1",
                    [invalid.to_string()],
                )
                .unwrap();
            assert!(store.automatic_learning_state().is_err());
        }
        let mut future = original;
        future["policyVersion"] = json!(999);
        store
            .connection
            .execute(
                "UPDATE automatic_learning SET state_json=?1",
                [future.to_string()],
            )
            .unwrap();
        assert!(store.automatic_learning_state().is_err());
    }
    fn setup() -> (tempfile::TempDir, DatasetStore, PersonalizationProfile) {
        let dir = tempfile::tempdir().unwrap();
        let store = DatasetStore::open(dir.path().join("data")).unwrap();
        let profile = store
            .save_personalization(PersonalizationProfile {
                automatic_learning: true,
                active_project: "repo".into(),
                ..Default::default()
            })
            .unwrap();
        (dir, store, profile)
    }
    fn context(p: &PersonalizationProfile, id: &str) -> LearningContext {
        LearningContext {
            result_id: id.into(),
            source_key: format!("audio-{id}"),
            episode_id: id.into(),
            language: Language::English,
            project: "repo".into(),
            model_id: "model-a".into(),
            learning_epoch: p.learning_epoch,
            learning_enabled: true,
            ..Default::default()
        }
    }
    fn teach(store: &DatasetStore, p: &PersonalizationProfile, id: &str) -> LearningOutcome {
        store
            .learn_from_edit(
                &context(p, id),
                0,
                "Open readme in the repo",
                "Open README.md in the repo",
            )
            .unwrap()
    }
    #[test]
    fn technical_edit_is_automatic_persistent_scoped_and_not_a_review() {
        let (dir, store, p) = setup();
        assert_eq!(teach(&store, &p, "one").activated, ["README.md"]);
        let learned = store.personalization().unwrap();
        assert!(learned.vocabulary[0].sources.is_empty());
        assert!(learned.vocabulary[0].automatic_source.is_some());
        assert_eq!(
            learned.personalize("Open readme", Language::English).text,
            "Open README.md"
        );
        let mut other = learned.clone();
        other.active_project = "personal".into();
        assert_eq!(
            other.personalize("Open readme", Language::English).text,
            "Open readme"
        );
        assert_eq!(
            learned.personalize("Open readme", Language::Hindi).text,
            "Open readme"
        );
        assert!(store.list_sessions().unwrap().is_empty());
        assert_eq!(
            fs::read_dir(dir.path().join("data/audio")).unwrap().count(),
            0
        );
        drop(store);
        let reopened = DatasetStore::open(dir.path().join("data")).unwrap();
        assert_eq!(reopened.personalization().unwrap(), learned);
        let mut switched = context(&p, "two");
        switched.model_id = "different-recognizer".into();
        reopened
            .observe_learning(&switched, "Open readme", None)
            .unwrap();
        assert_eq!(
            reopened.personalization().unwrap().vocabulary[0].usage_count,
            1
        );
    }
    #[test]
    fn independent_corrections_pool_across_models_but_audio_replays_do_not() {
        let (_dir, store, profile) = setup();
        let mut first = context(&profile, "first");
        first.model_id = vesper_core::AsrModel::CohereTranscribe.id().into();
        let edit = |context: &LearningContext| {
            store
                .learn_from_edit(
                    context,
                    0,
                    "deploy kubctl service",
                    "deploy kubectl service",
                )
                .unwrap()
        };
        assert_eq!(edit(&first).provisional, 1);
        for (index, model) in vesper_core::AsrModel::ALL.into_iter().enumerate() {
            let mut replay = context(&profile, &format!("replay-{index}"));
            replay.source_key = first.source_key.clone();
            replay.model_id = model.id().into();
            assert!(edit(&replay).activated.is_empty());
        }
        assert!(store.personalization().unwrap().vocabulary.is_empty());
        let mut independent = context(&profile, "independent");
        independent.model_id = vesper_core::AsrModel::WhisperSmall.id().into();
        assert_eq!(edit(&independent).activated, ["kubectl"]);
        let snapshot = store.dictation_personalization_snapshot().unwrap();
        assert_eq!(snapshot.learning.candidates.len(), 1);
        assert_eq!(snapshot.learning.candidates[0].support.len(), 2);
        assert_eq!(
            snapshot
                .post_asr_profile("deploy kubctl service", Language::English)
                .personalize("deploy kubctl service", Language::English)
                .text,
            "deploy kubectl service"
        );
    }

    #[test]
    fn repeated_audio_and_retry_chains_do_not_supply_independent_support() {
        let (_dir, store, p) = setup();
        let one = context(&p, "one");
        let edit = |c: &LearningContext, rev| {
            store
                .learn_from_edit(c, rev, "Meet Martellis tomorrow", "Meet Martellys tomorrow")
                .unwrap()
        };
        assert_eq!(edit(&one, 0).provisional, 1);
        assert!(edit(&one, 0).activated.is_empty()); // Exact replay.
        let mut same_audio = context(&p, "two");
        same_audio.source_key = one.source_key.clone();
        edit(&same_audio, 0);
        let mut retry = context(&p, "three");
        retry.episode_id = one.episode_id.clone();
        edit(&retry, 0);
        assert!(store.personalization().unwrap().vocabulary.is_empty());
        assert_eq!(
            store.automatic_learning_state().unwrap().candidates[0]
                .support
                .len(),
            1
        );
        assert_eq!(edit(&context(&p, "later"), 0).activated, ["Martellys"]);
    }
    #[test]
    fn revised_automatic_target_pauses_old_mapping_and_allows_fresh_support() {
        let (_dir, store, p) = setup();
        for id in ["first", "second"] {
            store
                .learn_from_edit(
                    &context(&p, id),
                    0,
                    "Meet Jonathon tomorrow",
                    "Meet Jonathan tomorrow",
                )
                .unwrap();
        }
        let outcome = store
            .learn_from_edit(
                &context(&p, "revision"),
                0,
                "Meet Jonathan tomorrow",
                "Meet Jonatan tomorrow",
            )
            .unwrap();
        assert_eq!(outcome.suspended, 1);
        assert!(outcome.activated.is_empty());
        assert_eq!(
            store
                .dictation_personalization_snapshot()
                .unwrap()
                .post_asr_profile("Meet Jonathon tomorrow", Language::English)
                .personalize("Meet Jonathon tomorrow", Language::English)
                .text,
            "Meet Jonathon tomorrow"
        );
        // The displayed intermediate spelling is not a new audio error label.
        assert!(!store
            .automatic_learning_state()
            .unwrap()
            .candidates
            .iter()
            .any(|c| c.alias == "Jonathan"));
        for (id, activated) in [("fresh-one", false), ("fresh-two", true)] {
            let outcome = store
                .learn_from_edit(
                    &context(&p, id),
                    0,
                    "Meet Jonathon tomorrow",
                    "Meet Jonatan tomorrow",
                )
                .unwrap();
            assert_eq!(!outcome.activated.is_empty(), activated);
        }
        assert_eq!(
            store
                .dictation_personalization_snapshot()
                .unwrap()
                .post_asr_profile("Meet Jonathon tomorrow", Language::English)
                .personalize("Meet Jonathon tomorrow", Language::English)
                .text,
            "Meet Jonatan tomorrow"
        );
        // A withdrawn automatic spelling leaves the vocabulary entirely:
        // listed words are always used. Its candidate stays suspended so the
        // spelling cannot reactivate.
        assert!(store
            .personalization()
            .unwrap()
            .vocabulary
            .iter()
            .find(|v| v.phrase == "Jonathan")
            .is_none());
        assert!(store
            .automatic_learning_state()
            .unwrap()
            .candidates
            .iter()
            .filter(|c| c.phrase == "Jonathan")
            .all(|c| c.state == CandidateState::Suspended));
    }

    #[test]
    fn retracting_provisional_edit_withdraws_only_that_results_support() {
        let (_dir, store, p) = setup();
        let before = "Meet Martellis tomorrow";
        let after = "Meet Martellys tomorrow";
        store
            .learn_from_edit(&context(&p, "first"), 0, before, after)
            .unwrap();
        store
            .learn_from_edit(&context(&p, "first"), 1, after, before)
            .unwrap();
        let state = store.automatic_learning_state().unwrap();
        assert_eq!(
            state.candidates.len(),
            1,
            "retraction must not teach a reverse alias"
        );
        assert!(state.candidates[0].support.is_empty());
        let next = store
            .learn_from_edit(&context(&p, "second"), 0, before, after)
            .unwrap();
        assert!(next.activated.is_empty());
        assert_eq!(next.provisional, 1);
        // Duplicate callback from the retracted result cannot restore its support.
        store
            .learn_from_edit(&context(&p, "first"), 0, before, after)
            .unwrap();
        assert_eq!(
            store.automatic_learning_state().unwrap().candidates[0]
                .support
                .len(),
            1
        );
        assert_eq!(
            store
                .learn_from_edit(&context(&p, "third"), 0, before, after)
                .unwrap()
                .activated,
            ["Martellys"]
        );
    }

    #[test]
    fn later_teaching_context_reaches_application_without_changing_stored_hint_scope() {
        let (_dir, store, mut p) = setup();
        p.active_project.clear();
        let p = store.save_personalization(p).unwrap();
        for (id, before, after) in [
            ("first", "Deploy kubernets alpha", "Deploy kubernetes alpha"),
            ("second", "Manage kubernets beta", "Manage kubernetes beta"),
        ] {
            let mut c = context(&p, id);
            c.project.clear();
            store.learn_from_edit(&c, 0, before, after).unwrap();
        }
        let snapshot = store.dictation_personalization_snapshot().unwrap();
        let stored = snapshot.profile.clone();
        for raw in ["Deploy kubernets alpha", "Manage kubernets beta"] {
            assert_eq!(
                snapshot
                    .post_asr_profile(raw, Language::English)
                    .personalize(raw, Language::English)
                    .text,
                raw.replace("kubernets", "kubernetes")
            );
        }
        for raw in [
            "Discuss kubernets gamma",
            "Manage 'kubernets' beta",
            "Inspect kubernets 'beta'",
        ] {
            assert_eq!(
                snapshot
                    .post_asr_profile(raw, Language::English)
                    .personalize(raw, Language::English)
                    .text,
                raw
            );
        }
        assert_eq!(
            snapshot
                .post_asr_profile("Manage kubernets beta", Language::Hindi)
                .personalize("Manage kubernets beta", Language::Hindi)
                .text,
            "Manage kubernets beta"
        );
        assert_eq!(store.personalization().unwrap(), stored);
        assert!(!stored.vocabulary[0].context.is_empty());
    }

    #[test]
    fn rejected_target_keeps_manual_conflicts_and_other_results_support() {
        for enabled in [false, true] {
            let (_dir, store, mut p) = setup();
            p.vocabulary.push(VocabularyEntry {
                id: "manual".into(),
                phrase: "Jonathan".into(),
                aliases: vec!["Jonathon".into()],
                language: Language::English,
                project: "repo".into(),
                enabled,
                ..Default::default()
            });
            let p = store.save_personalization(p).unwrap();
            for id in ["first", "second"] {
                let outcome = store
                    .learn_from_edit(
                        &context(&p, id),
                        0,
                        "Meet Jonathon tomorrow",
                        "Meet Jonatan tomorrow",
                    )
                    .unwrap();
                assert!(outcome.activated.is_empty());
            }
            assert_eq!(store.personalization().unwrap().vocabulary, p.vocabulary);
        }
        let (_dir, store, p) = setup();
        for id in ["first", "second"] {
            store
                .learn_from_edit(
                    &context(&p, id),
                    0,
                    "Meet Jonathon tomorrow",
                    "Meet Jonathan tomorrow",
                )
                .unwrap();
        }
        let outcome = store
            .learn_from_edit(
                &context(&p, "second"),
                1,
                "Meet Jonathan tomorrow",
                "Meet Jonathon tomorrow",
            )
            .unwrap();
        assert_eq!(outcome.suspended, 1);
        let state = store.automatic_learning_state().unwrap();
        assert_eq!(state.candidates[0].support.len(), 1);
        assert_eq!(state.candidates[0].support[0].event_id, "first:0");
        assert!(state.candidates[0].support[0].teaching_terms.is_empty());
    }

    #[test]
    fn quote_and_intent_rewrites_do_not_suspend_unapplied_automatic_targets() {
        let (_dir, store, p) = setup();
        for id in ["first", "second"] {
            store
                .learn_from_edit(
                    &context(&p, id),
                    0,
                    "Meet Jonathon tomorrow",
                    "Meet Jonathan tomorrow",
                )
                .unwrap();
        }
        for (id, before, after) in [
            (
                "quoted",
                "Meet 'Jonathan' tomorrow",
                "Meet Jonatan tomorrow",
            ),
            ("intent", "Meet Jonathan tomorrow", "Cancel tomorrow"),
        ] {
            assert_eq!(
                store
                    .learn_from_edit(&context(&p, id), 0, before, after)
                    .unwrap()
                    .suspended,
                0
            );
        }
        assert!(store.personalization().unwrap().vocabulary[0].enabled);
    }

    #[test]
    fn oversized_draft_is_unknown_context_not_evidence_of_a_retracted_target() {
        let (_dir, store, p) = setup();
        let c = context(&p, "first");
        store
            .learn_from_edit(&c, 0, "Meet Martellis tomorrow", "Meet Martellys tomorrow")
            .unwrap();
        let after = format!("Meet Martellys tomorrow {}", "context ".repeat(1100));
        store
            .learn_from_edit(&c, 1, "Meet Martellys tomorrow", &after)
            .unwrap();
        let state = store.automatic_learning_state().unwrap();
        assert_eq!(state.candidates[0].support.len(), 1);
        assert_eq!(state.candidates[0].state, CandidateState::Provisional);
    }
    #[test]
    fn observations_and_retry_signals_never_activate_or_duplicate_usage() {
        let (_dir, store, p) = setup();
        teach(&store, &p, "teach");
        let first = context(&p, "first");
        store
            .observe_learning(&first, "Open readme with NewTool", None)
            .unwrap();
        let mut repeat = context(&p, "repeat");
        repeat.episode_id = first.episode_id;
        store
            .observe_learning(
                &repeat,
                "I said open readme with NewTool",
                Some(RetryRelation::PossibleRepair),
            )
            .unwrap();
        let profile = store.personalization().unwrap();
        assert_eq!(profile.vocabulary.len(), 1);
        assert_eq!(profile.vocabulary[0].usage_count, 1);
        let state = store.automatic_learning_state().unwrap();
        assert!(state
            .events
            .iter()
            .any(|e| e.retry == Some(RetryRelation::PossibleRepair)));
        assert!(state.candidates.iter().any(|c| c.phrase == "NewTool"
            && c.state == CandidateState::Observed
            && c.support.is_empty()));
    }
    #[test]
    fn a_biased_decode_never_counts_its_own_hints_as_usage_or_observations() {
        let (_dir, store, p) = setup();
        teach(&store, &p, "teach");
        let phrase = store.personalization().unwrap().vocabulary[0]
            .phrase
            .clone();
        let raw = format!("Open {phrase} with NewTool");
        let hints = vec![phrase.clone(), "NewTool".to_owned()];
        for n in 0..3 {
            let mut biased = context(&p, &format!("biased-{n}"));
            biased.hint_hash = vesper_core::hint_hash(&hints);
            biased.hinted_phrases = hints.clone();
            store.observe_learning(&biased, &raw, None).unwrap();
        }
        assert_eq!(
            store.personalization().unwrap().vocabulary[0].usage_count,
            0
        );
        let state = store.automatic_learning_state().unwrap();
        assert!(!state.candidates.iter().any(|c| c.phrase == "NewTool"));
        // The observation is still recorded, with its bias provenance.
        assert_eq!(
            state
                .events
                .iter()
                .filter(|e| e.context.hint_hash == vesper_core::hint_hash(&hints))
                .count(),
            3
        );
        // An unbiased decode of the same text remains ordinary evidence.
        store
            .observe_learning(&context(&p, "plain"), &raw, None)
            .unwrap();
        assert_eq!(
            store.personalization().unwrap().vocabulary[0].usage_count,
            1
        );
        assert!(store
            .automatic_learning_state()
            .unwrap()
            .candidates
            .iter()
            .any(|c| c.phrase == "NewTool"));
    }
    #[test]
    fn changing_learning_permission_invalidates_old_callbacks_and_preserves_memory() {
        let (_dir, store, p) = setup();
        teach(&store, &p, "one");
        let mut paused = store.personalization().unwrap();
        paused.automatic_learning = false;
        let mut paused = store.save_personalization(paused).unwrap();
        assert_eq!(paused.vocabulary.len(), 1);
        paused.automatic_learning = true;
        let resumed = store.save_personalization(paused).unwrap();
        let before = serde_json::to_string(&store.automatic_learning_state().unwrap()).unwrap();
        assert!(teach(&store, &p, "late").activated.is_empty());
        assert_eq!(
            serde_json::to_string(&store.automatic_learning_state().unwrap()).unwrap(),
            before
        );
        assert!(resumed.learning_epoch > p.learning_epoch);
    }
    #[test]
    fn undo_and_reversed_edits_suspend_without_learning_a_reverse_rule() {
        let (_dir, store, p) = setup();
        teach(&store, &p, "one");
        let outcome = store
            .learn_from_edit(&context(&p, "undo"), 0, "Open README.md", "Open readme")
            .unwrap();
        assert_eq!(outcome.suspended, 1);
        // The rejected spelling leaves the vocabulary; the suspended
        // candidate keeps later edits from re-learning it.
        let profile = store.personalization().unwrap();
        assert!(profile.vocabulary.is_empty());
        assert_eq!(
            profile.personalize("Open readme", Language::English).text,
            "Open readme"
        );
        assert_eq!(
            store.automatic_learning_state().unwrap().candidates[0].state,
            CandidateState::Suspended
        );
        teach(&store, &p, "later");
        assert!(store.personalization().unwrap().vocabulary.is_empty());
    }

    #[test]
    fn dismissing_a_proposal_suspends_it_without_a_vocabulary_entry() {
        let (_dir, store, p) = setup();
        store
            .learn_from_edit(
                &context(&p, "first"),
                0,
                "Meet Jonathon tomorrow",
                "Meet Jonathan tomorrow",
            )
            .unwrap();
        let id = store.automatic_learning_state().unwrap().candidates[0]
            .id
            .clone();
        assert_eq!(store.suspend_automatic_candidate(&id).unwrap(), 1);
        assert_eq!(store.suspend_automatic_candidate("missing").unwrap(), 0);
        let state = store.automatic_learning_state().unwrap();
        assert_eq!(state.candidates[0].state, CandidateState::Suspended);
        assert!(store.personalization().unwrap().vocabulary.is_empty());
        // A dismissed spelling never reactivates from later edits.
        for id in ["second", "third"] {
            let outcome = store
                .learn_from_edit(
                    &context(&p, id),
                    0,
                    "Meet Jonathon tomorrow",
                    "Meet Jonathan tomorrow",
                )
                .unwrap();
            assert!(outcome.activated.is_empty());
        }
        assert!(store.personalization().unwrap().vocabulary.is_empty());
    }

    #[test]
    fn approving_a_proposal_promotes_it_to_a_manual_entry() {
        let (_dir, store, p) = setup();
        store
            .learn_from_edit(
                &context(&p, "first"),
                0,
                "Meet Jonathon tomorrow",
                "Meet Jonathan tomorrow",
            )
            .unwrap();
        let id = store.automatic_learning_state().unwrap().candidates[0]
            .id
            .clone();
        let profile = store.approve_automatic_candidate(&id).unwrap();
        assert_eq!(profile.vocabulary.len(), 1);
        let entry = &profile.vocabulary[0];
        assert_eq!(entry.phrase, "Jonathan");
        assert_eq!(entry.aliases, ["Jonathon"]);
        assert!(entry.automatic_source.is_none());
        assert!(entry.enabled);
        // Approving ticks the entry: user-owned words are always confirmed.
        assert!(entry.confirmed);
        assert!(store
            .automatic_learning_state()
            .unwrap()
            .candidates
            .is_empty());
        // The candidate is gone, so approving again fails.
        assert!(store.approve_automatic_candidate(&id).is_err());
    }

    #[test]
    fn ticking_an_automatic_entry_trusts_it_without_utterance_evidence() {
        let (_dir, store, p) = setup();
        teach(&store, &p, "one");
        assert!(!store.personalization().unwrap().vocabulary[0].confirmed);
        // Unticked: an unrelated utterance keeps the entry gated off.
        let snapshot = store.dictation_personalization_snapshot().unwrap();
        assert!(
            !snapshot
                .post_asr_profile("Hello world today", Language::English)
                .vocabulary[0]
                .enabled
        );
        // Tick it: the same unrelated utterance now keeps it enabled.
        let mut profile = store.personalization().unwrap();
        profile.vocabulary[0].confirmed = true;
        store.save_personalization(profile).unwrap();
        assert!(store.personalization().unwrap().vocabulary[0].confirmed);
        let snapshot = store.dictation_personalization_snapshot().unwrap();
        assert!(
            snapshot
                .post_asr_profile("Hello world today", Language::English)
                .vocabulary[0]
                .enabled
        );
        // Removing a ticked entry still suspends it; later edits never return it.
        let mut profile = store.personalization().unwrap();
        profile.vocabulary.clear();
        store.save_personalization(profile).unwrap();
        let current = store.personalization().unwrap();
        let outcome = teach(&store, &current, "two");
        assert!(outcome.activated.is_empty());
        assert!(store.personalization().unwrap().vocabulary.is_empty());
    }

    #[test]
    fn manual_entries_are_always_confirmed_on_save() {
        let (_dir, store, p) = setup();
        teach(&store, &p, "one");
        let mut profile = store.personalization().unwrap();
        profile.vocabulary[0].automatic_source = None;
        profile.vocabulary[0].confirmed = false;
        let saved = store.save_personalization(profile).unwrap();
        assert!(saved.vocabulary[0].confirmed);
    }

    #[test]
    fn approving_a_removed_proposal_is_rejected() {
        let (_dir, store, p) = setup();
        store
            .learn_from_edit(
                &context(&p, "first"),
                0,
                "Meet Jonathon tomorrow",
                "Meet Jonathan tomorrow",
            )
            .unwrap();
        let id = store.automatic_learning_state().unwrap().candidates[0]
            .id
            .clone();
        assert_eq!(store.suspend_automatic_candidate(&id).unwrap(), 1);
        assert!(store.approve_automatic_candidate(&id).is_err());
    }

    #[test]
    fn saving_a_disabled_automatic_entry_removes_it_and_suspends_support() {
        let (_dir, store, p) = setup();
        teach(&store, &p, "one");
        let mut profile = store.personalization().unwrap();
        assert_eq!(profile.vocabulary.len(), 1);
        profile.vocabulary[0].enabled = false;
        let saved = store.save_personalization(profile).unwrap();
        assert!(saved.vocabulary.is_empty());
        let state = store.automatic_learning_state().unwrap();
        assert_eq!(state.candidates.len(), 1);
        assert_eq!(state.candidates[0].state, CandidateState::Suspended);
    }

    #[test]
    fn removing_an_automatic_entry_suspends_its_candidate() {
        let (_dir, store, p) = setup();
        teach(&store, &p, "one");
        let mut profile = store.personalization().unwrap();
        profile.vocabulary.clear();
        store.save_personalization(profile).unwrap();
        let state = store.automatic_learning_state().unwrap();
        assert_eq!(state.candidates.len(), 1);
        assert_eq!(state.candidates[0].state, CandidateState::Suspended);
        // Future matching edits do not re-learn the removed spelling.
        let current = store.personalization().unwrap();
        let outcome = teach(&store, &current, "two");
        assert!(outcome.activated.is_empty());
        assert!(store.personalization().unwrap().vocabulary.is_empty());
    }
    #[test]
    fn forgetting_deletes_derived_evidence_and_rejects_old_sources() {
        let (_dir, store, p) = setup();
        teach(&store, &p, "one");
        let before = store.personalization().unwrap();
        assert!(store
            .forget_automatic_learning(before.revision - 1)
            .is_err());
        let forgotten = store.forget_automatic_learning(before.revision).unwrap();
        assert!(forgotten.vocabulary.is_empty());
        assert!(store.automatic_learning_state().unwrap().events.is_empty());
        assert!(teach(&store, &p, "late").activated.is_empty());
        assert!(store
            .automatic_learning_state()
            .unwrap()
            .candidates
            .is_empty());
        assert_eq!(teach(&store, &forgotten, "fresh").activated, ["README.md"]);
        let mut removed = store.personalization().unwrap();
        removed.vocabulary.clear();
        store.save_personalization(removed).unwrap();
        // Clearing an automatic entry suspends its candidate (negative
        // evidence) instead of deleting it, so the spelling is not re-learned.
        // Past events are kept as an audit trail.
        assert!(store
            .automatic_learning_state()
            .unwrap()
            .candidates
            .iter()
            .all(|c| c.state == CandidateState::Suspended));
    }
    #[test]
    fn cannot_forge_automatic_provenance_or_broaden_its_scope() {
        let (_dir, store, p) = setup();
        teach(&store, &p, "one");
        let current = store.personalization().unwrap();
        let mut forged = current.clone();
        forged.vocabulary[0].project.clear();
        assert!(store.save_personalization(forged).is_err());
        let mut forged = current.clone();
        forged.vocabulary[0].usage_count = 999;
        let saved = store.save_personalization(forged).unwrap();
        assert_eq!(saved.vocabulary[0].usage_count, 0);
        let mut manual = saved;
        manual.vocabulary[0].automatic_source = None;
        manual.vocabulary[0].project.clear();
        assert!(store.save_personalization(manual).unwrap().vocabulary[0]
            .automatic_source
            .is_none());
    }
    #[test]
    fn explicit_review_takes_ownership_without_resurrecting_automatic_evidence() {
        let (dir, store, p) = setup();
        teach(&store, &p, "ordinary-edit");
        let automatic = store.personalization().unwrap().vocabulary.remove(0);
        let audio = dir.path().join("fixture.wav");
        fs::write(&audio, b"synthetic fixture").unwrap();
        let config = AppConfig {
            language: Language::English,
            ..Default::default()
        };
        let record = store.import_audio(&audio, &config).unwrap();
        store
            .set_transcripts(
                &record.id,
                "Open readme in the repo".into(),
                None,
                json!({}),
            )
            .unwrap();
        store
            .approve_transcription(&record.id, "Open README.md in the repo".into())
            .unwrap();
        let suggestion = store.correction_suggestions(&record.id).unwrap().remove(0);
        let saved = store
            .remember_correction(crate::RememberCorrection {
                session_id: record.id,
                review_revision_id: suggestion.review_revision_id,
                pair: suggestion.pair,
                context: automatic.context,
                project: automatic.project,
                pronunciation: automatic.pronunciation,
                profile_revision: store.personalization().unwrap().revision,
            })
            .unwrap();
        assert_eq!(saved.vocabulary.len(), 1);
        assert!(saved.vocabulary[0].automatic_source.is_none());
        assert_eq!(saved.vocabulary[0].sources.len(), 1);
        assert!(store
            .automatic_learning_state()
            .unwrap()
            .candidates
            .is_empty());
        assert!(store.automatic_learning_state().unwrap().events.is_empty());
        let forgotten = store.forget_automatic_learning(saved.revision).unwrap();
        assert_eq!(forgotten.vocabulary[0].id, automatic.id);
        assert!(teach(&store, &p, "late-callback").activated.is_empty());
    }

    #[test]
    fn profile_write_failure_rolls_back_admission_and_evidence() {
        let (_dir, store, p) = setup();
        store.connection.execute_batch("CREATE TRIGGER fail_learning BEFORE UPDATE ON personalization BEGIN SELECT RAISE(ABORT, 'injected write failure'); END;").unwrap();
        assert!(store
            .learn_from_edit(&context(&p, "one"), 0, "Open readme", "Open README.md")
            .is_err());
        assert!(store.personalization().unwrap().vocabulary.is_empty());
        assert!(store.automatic_learning_state().unwrap().events.is_empty());
        assert!(store
            .automatic_learning_state()
            .unwrap()
            .candidates
            .is_empty());
    }
    #[test]
    fn saved_draft_and_learning_are_atomic_but_never_review_or_export_targets() {
        let (dir, store, p) = setup();
        let audio = dir.path().join("public-fixture.wav");
        fs::write(&audio, b"synthetic test audio bytes").unwrap();
        let record = store.import_audio(&audio, &AppConfig::default()).unwrap();
        store
            .set_transcripts(&record.id, "Open readme in repo".into(), None, json!({}))
            .unwrap();
        let ctx = context(&p, "draft");
        let (saved, outcome) = store
            .edit_session_draft(
                &record.id,
                0,
                "Open readme in repo",
                "Open README.md in repo",
                &ctx,
            )
            .unwrap();
        assert_eq!(outcome.activated, ["README.md"]);
        assert_eq!(saved.draft_revision, 1);
        assert_eq!(saved.raw_transcript.as_deref(), Some("Open readme in repo"));
        assert!(saved.review_history.is_empty());
        assert!(saved.verified_transcript.is_none());
        assert!(saved.corrected_transcript.is_none());
        assert!(store
            .edit_session_draft(&record.id, 0, "Open readme in repo", "changed", &ctx)
            .is_err());
        assert_eq!(
            store
                .export_dataset(&dir.path().join("training"), true)
                .unwrap()
                .session_count,
            0
        );
        let encoded = serde_json::to_string(&store.automatic_learning_state().unwrap()).unwrap();
        assert!(!encoded.contains("Open readme in repo"));
        assert!(!encoded.contains("Open README.md in repo"));
        assert!(encoded.contains("README.md"));
    }
    #[test]
    fn retention_is_bounded_and_keeps_compact_active_support() {
        let (_dir, store, p) = setup();
        teach(&store, &p, "one");
        let mut state = store.automatic_learning_state().unwrap();
        let now = stamp();
        state.candidates[0].last_seen_ms = now - EXPIRY_MS - 1;
        let mut expired = state.candidates[0].clone();
        expired.id = "expired".into();
        expired.state = CandidateState::Observed;
        state.candidates.push(expired);
        let event = state.events[0].clone();
        state.events = (0..1100)
            .map(|i| {
                let mut e = event.clone();
                e.id = i.to_string();
                e
            })
            .collect();
        prune(&mut state, now);
        assert_eq!(state.events.len(), MAX_EVENTS);
        assert_eq!(state.expired_candidates, 1);
        assert_eq!(state.candidates.len(), 1);
        assert!(!state.candidates[0].support.is_empty());
        prune(&mut state, now + EXPIRY_MS + 1);
        assert!(state.events.is_empty());
        assert_eq!(state.candidates.len(), 1);
    }
    #[test]
    fn pending_diagnostics_distinguish_duplicate_support_and_capacity() {
        let (_dir, store, profile) = setup();
        let first = context(&profile, "first");
        let pending = store
            .learn_from_edit(&first, 0, "deploy kubctl service", "deploy kubectl service")
            .unwrap();
        assert!(pending.status.contains("1/2 recordings"));
        let mut duplicate = context(&profile, "duplicate");
        duplicate.source_key = first.source_key;
        let pending = store
            .learn_from_edit(
                &duplicate,
                0,
                "deploy kubctl service",
                "deploy kubectl service",
            )
            .unwrap();
        assert!(pending.status.contains("1/2 recordings"));
        let mut full = store.personalization().unwrap();
        full.vocabulary = (0..128)
            .map(|n| VocabularyEntry {
                id: format!("manual-{n}"),
                phrase: format!("term{n}"),
                ..Default::default()
            })
            .collect();
        store.save_personalization(full).unwrap();
        let pending = store
            .learn_from_edit(
                &context(&profile, "second"),
                0,
                "deploy kubctl service",
                "deploy kubectl service",
            )
            .unwrap();
        assert!(pending.activated.is_empty());
        assert!(pending.status.contains("Vocabulary is full"));
        assert!(store.automatic_learning_state().unwrap().candidates[0]
            .reason
            .contains("Vocabulary is full"));
    }

    #[test]
    fn auto_language_and_conflicting_aliases_abstain() {
        let (_dir, store, p) = setup();
        let mut auto = context(&p, "auto");
        auto.language = Language::Auto;
        let outcome = store
            .learn_from_edit(&auto, 0, "Open readme", "Open README.md")
            .unwrap();
        assert!(outcome.activated.is_empty());
        assert!(outcome.status.contains("Select a recording language"));
        assert!(store.automatic_learning_state().unwrap().candidates[0]
            .reason
            .contains("Select a recording language"));
        teach(&store, &p, "one");
        assert!(store
            .learn_from_edit(&context(&p, "other"), 0, "Open readme", "Open README.txt")
            .unwrap()
            .activated
            .is_empty());
        assert_eq!(store.personalization().unwrap().vocabulary.len(), 1);
    }

    #[test]
    fn heard_class_sets_required_independent_support() {
        let (_dir, store, p) = setup();
        let teach = |id: &str, before: &str, after: &str| {
            store
                .learn_from_edit(&context(&p, id), 0, before, after)
                .unwrap()
        };
        // An ordinary phrase needs two independent edits, even for an identifier.
        assert!(teach(
            "a1",
            "Open read me in the repo",
            "Open README.md in the repo"
        )
        .activated
        .is_empty());
        assert_eq!(
            teach(
                "a2",
                "Open read me in the repo",
                "Open README.md in the repo"
            )
            .activated,
            ["README.md"]
        );
        // Policy 6: one ordinary word becomes a term only after three independent
        // edits in a project; earlier edits stay provisional.
        for (id, activated) in [("d1", false), ("d2", false), ("d3", true)] {
            let outcome = teach(
                id,
                "The data formatting page for Fermion",
                "The Detta formatting page for Fermion",
            );
            assert_eq!(outcome.activated == ["Detta"], activated, "{id}");
        }
        // Corrected to another lowercase word: never automatic, however often.
        for id in ["w1", "w2", "w3", "w4", "w5"] {
            let outcome = teach(
                id,
                "I will write the repo docs",
                "I will right the repo docs",
            );
            assert!(outcome.activated.is_empty(), "{id}");
        }
        // Word-to-word rewrites are not spelling evidence at all: no candidate.
        let state = store.automatic_learning_state().unwrap();
        assert!(state
            .candidates
            .iter()
            .all(|c| c.phrase != "right" || c.state != CandidateState::Active));
        let data = state
            .candidates
            .iter()
            .find(|c| c.phrase == "Detta")
            .unwrap();
        assert_eq!(
            (data.state, data.support.len()),
            (CandidateState::Active, 3)
        );
    }

    #[test]
    fn learned_aliases_stay_with_the_recognizer_family_that_needed_them() {
        let (_dir, store, p) = setup();
        let mut cohere = context(&p, "c1");
        cohere.model_id = "Cohere Transcribe".into();
        assert_eq!(
            store
                .learn_from_edit(
                    &cohere,
                    0,
                    "Open readme for deployment",
                    "Open README.md for deployment"
                )
                .unwrap()
                .activated,
            ["README.md"]
        );
        let snapshot = store.dictation_personalization_snapshot().unwrap();
        let raw = "Open readme for deployment";
        let apply = |model: &str| {
            snapshot
                .post_asr_profile_for(raw, Language::English, model)
                .personalize(raw, Language::English)
                .text
        };
        assert_eq!(apply("Cohere Transcribe"), "Open README.md for deployment");
        assert_eq!(apply("NVIDIA Parakeet TDT 0.6B v3"), raw);
        // Callers without a model keep the previous behavior.
        assert_eq!(
            snapshot
                .post_asr_profile(raw, Language::English)
                .personalize(raw, Language::English)
                .text,
            "Open README.md for deployment"
        );
    }

    #[test]
    fn two_distinct_rejected_rules_trip_the_breaker_until_resumed() {
        let (_dir, store, p) = setup();
        for (id, before, after) in [
            (
                "t1",
                "Open readme for deployment",
                "Open README.md for deployment",
            ),
            (
                "t2",
                "Run sherpa onnx for deployment",
                "Run sherpa_onnx for deployment",
            ),
            (
                "t3",
                "Use kube ctl for deployment",
                "Use kubectl.io for deployment",
            ),
        ] {
            assert_eq!(
                store
                    .learn_from_edit(&context(&p, id), 0, before, after)
                    .unwrap()
                    .activated
                    .len(),
                1,
                "{id}"
            );
        }
        // The user puts the heard forms back on two different applied rules.
        for (id, before, after) in [
            (
                "r1",
                "Open README.md for deployment",
                "Open readme for deployment",
            ),
            (
                "r2",
                "Run sherpa_onnx for deployment",
                "Run sherpa onnx for deployment",
            ),
        ] {
            assert_eq!(
                store
                    .learn_from_edit(&context(&p, id), 0, before, after)
                    .unwrap()
                    .suspended,
                1
            );
        }
        let snapshot = store.dictation_personalization_snapshot().unwrap();
        assert!(snapshot.learning.breaker_tripped_ms.is_some());
        let raw = "Use kube ctl for deployment";
        assert_eq!(
            snapshot
                .post_asr_profile_for(raw, Language::English, "model-a")
                .personalize(raw, Language::English)
                .text,
            raw
        );
        store.resume_automatic_spellings().unwrap();
        let resumed = store.dictation_personalization_snapshot().unwrap();
        assert_eq!(
            resumed
                .post_asr_profile_for(raw, Language::English, "model-a")
                .personalize(raw, Language::English)
                .text,
            "Use kubectl.io for deployment"
        );
    }

    #[test]
    fn rejecting_gate_spellings_counts_toward_the_breaker() {
        let (_dir, store, p) = setup();
        let ctx = context(&p, "gate");
        let change = |spelling: &str, id: &str| vesper_core::GateChange {
            start: 0,
            end: 0,
            heard: String::new(),
            spelling: spelling.into(),
            term_id: id.into(),
        };
        let changes = [change("sherpa_onnx", "t1"), change("Sortformer", "t2")];
        let before = "Link sherpa_onnx and Sortformer.";
        // Unrelated edits and kept spellings are not rejections.
        assert!(store
            .observe_gate_reverts(
                &ctx,
                before,
                "Link sherpa_onnx and Sortformer now.",
                &changes
            )
            .unwrap()
            .is_empty());
        assert_eq!(
            store
                .observe_gate_reverts(&ctx, before, "Link sherpa onnx and Sortformer.", &changes)
                .unwrap(),
            ["t1"]
        );
        assert!(store
            .automatic_learning_state()
            .unwrap()
            .breaker_tripped_ms
            .is_none());
        store
            .observe_gate_reverts(&ctx, before, "Link sherpa_onnx and sort former.", &changes)
            .unwrap();
        assert!(store
            .automatic_learning_state()
            .unwrap()
            .breaker_tripped_ms
            .is_some());
    }
}
