//! Bounded local feedback storage. Ordinary edits never become reviewed training targets.
use crate::*;
use transcribe_core::{
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
}
impl Default for AutomaticLearningState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            policy_version: LEARNING_POLICY_VERSION,
            candidates: vec![],
            events: vec![],
            expired_candidates: 0,
        }
    }
}

fn stamp() -> i64 {
    Utc::now().timestamp_millis()
}

/// Stable across installed models and reruns of identical resampled audio.
pub fn learning_audio_key(samples: &[f32]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"transcribe-learning-pcm16k-v1");
    for sample in samples {
        hash.update(sample.to_le_bytes());
    }
    format!("{:x}", hash.finalize())
}

impl DatasetStore {
    pub fn automatic_learning_state(&self) -> Result<AutomaticLearningState> {
        let state: Option<String> = self
            .connection
            .query_row(
                "SELECT state_json FROM automatic_learning WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let state: AutomaticLearningState = state
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or_default();
        if state.schema_version != 1
            || state.candidates.len() > MAX_CANDIDATES + 128
            || state.events.len() > MAX_EVENTS
        {
            return Err(DatasetError::InvalidConfig(
                "Unsupported or oversized automatic-learning store.".into(),
            ));
        }
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
        let event_id = format!("{}:{revision}", context.result_id);
        if state.events.iter().any(|e| e.id == event_id) {
            outcome.status = "This edit was already processed.".into();
            return Ok(outcome);
        }
        // A reversal is feedback against the inferred mapping, not a new reverse alias.
        for entry in &mut profile.vocabulary {
            if entry.automatic_source.is_some()
                && entry.enabled
                && scope(entry.language, &entry.project, context)
                && memory_phrase_occurs(before, &entry.phrase)
                && !memory_phrase_occurs(after, &entry.phrase)
                && entry.aliases.iter().any(|a| memory_phrase_occurs(after, a))
            {
                entry.enabled = false;
                suspend(&mut state, entry.automatic_source.as_deref().unwrap());
                outcome.suspended += 1;
            }
        }
        let pair = automatic_spelling_pair(before, after);
        if let Some(pair) = pair.as_ref().filter(|_| outcome.suspended == 0) {
            let position = state.candidates.iter().position(|c| {
                c.phrase == pair.intended
                    && c.alias == pair.heard
                    && c.language == context.language
                    && c.project == context.project
            });
            let position = position.unwrap_or_else(|| {
                let excluded = transcribe_core::memory_words(&pair.heard);
                let tags = transcribe_core::memory_words(before)
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
            let candidate = &mut state.candidates[position];
            candidate.last_seen_ms = stamp();
            if candidate.state != CandidateState::Suspended {
                if !candidate.support.iter().any(|s| {
                    s.source_key == context.source_key || s.episode_id == context.episode_id
                }) && candidate.support.len() < 16
                {
                    candidate.support.push(EditSupport {
                        source_key: context.source_key.clone(),
                        episode_id: context.episode_id.clone(),
                        event_id: event_id.clone(),
                    });
                }
                let required = if !context.project.is_empty()
                    && transcribe_core::technical_spelling(&pair.intended)
                {
                    1
                } else {
                    2
                };
                let conflict = profile.vocabulary.iter().any(|v| {
                    (v.project.is_empty()
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
                } else if candidate.support.len() >= required
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
                    outcome.provisional += 1;
                }
            }
        }
        // Store changed spans only. A rewrite without a spelling pair keeps no text.
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
        if !outcome.activated.is_empty() || outcome.suspended > 0 {
            put_profile(tx, &mut profile)?;
        }
        outcome.profile_revision = profile.revision;
        outcome.elapsed_micros = started.elapsed().as_micros().min(u64::MAX as u128) as u64;
        outcome.status = if outcome.suspended > 0 {
            "Reversed learned spelling paused."
        } else if !outcome.activated.is_empty() {
            "Spelling remembered for relevant future dictation."
        } else if outcome.provisional > 0 {
            "Spelling evidence saved; waiting for independent support."
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
        if !permitted(&profile, context)? || raw.len() > transcribe_core::MAX_LEARNING_TEXT_BYTES {
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
                && transcribe_core::context_matches(
                    &word.context,
                    &format!("{} {raw}", context.project),
                )
                && (memory_phrase_occurs(raw, &word.phrase)
                    || word.aliases.iter().any(|a| memory_phrase_occurs(raw, a)))
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
                    && transcribe_core::technical_spelling(s)
                    && memory_phrase_occurs(raw, s)
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

    pub fn undo_learning_for_result(&self, result_id: &str) -> Result<usize> {
        let tx = self.connection.unchecked_transaction()?;
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
        let mut profile = self.personalization()?;
        let mut state = self.automatic_learning_state()?;
        let mut count = 0;
        for entry in &mut profile.vocabulary {
            if entry.enabled && ids.contains(&entry.id) {
                if let Some(source) = &entry.automatic_source {
                    entry.enabled = false;
                    suspend(&mut state, source);
                    count += 1;
                }
            }
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
                if !entry.enabled {
                    suspend(&mut state, source);
                }
                // An explicit inspector enable is a manual decision, not implicit admission.
                if entry.enabled && previous.is_some_and(|p| !p.enabled) {
                    entry.automatic_source = None;
                    invalidate = true;
                }
            }
        }
        for previous in &current.vocabulary {
            if let Some(source) = &previous.automatic_source {
                if !next
                    .vocabulary
                    .iter()
                    .any(|v| v.id == previous.id && v.automatic_source.as_ref() == Some(source))
                {
                    let support: Vec<_> = state
                        .candidates
                        .iter()
                        .filter(|c| &c.id == source)
                        .flat_map(|c| c.support.iter().map(|s| s.event_id.clone()))
                        .collect();
                    state.events.retain(|e| {
                        !support.contains(&e.id)
                            && !e.pair.as_ref().is_some_and(|p| {
                                p.intended == previous.phrase && previous.aliases.contains(&p.heard)
                            })
                    });
                    state.candidates.retain(|c| {
                        &c.id != source && !(c.phrase == previous.phrase && c.alias.is_empty())
                    });
                    invalidate = true;
                }
            }
        }
        if invalidate {
            next.learning_epoch = next.learning_epoch.saturating_add(1);
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
    transcribe_core::language_matches(language, context.language)
        && transcribe_core::project_matches(project, &context.project)
}
fn suspend(state: &mut AutomaticLearningState, id: &str) {
    if let Some(candidate) = state.candidates.iter_mut().find(|c| c.id == id) {
        candidate.state = CandidateState::Suspended;
        candidate.reason = "Paused after a rejected or disabled application.".into();
    }
}
fn prune(state: &mut AutomaticLearningState, timestamp: i64) {
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
fn put_state(tx: &Connection, state: &AutomaticLearningState) -> Result<()> {
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
fn put_profile(tx: &Connection, profile: &mut PersonalizationProfile) -> Result<()> {
    profile.schema_version = 3;
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
        }
    }
    fn teach(store: &DatasetStore, p: &PersonalizationProfile, id: &str) -> LearningOutcome {
        store
            .learn_from_edit(
                &context(p, id),
                0,
                "Open read me in the repo",
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
            learned.personalize("Open read me", Language::English).text,
            "Open README.md"
        );
        let mut other = learned.clone();
        other.active_project = "personal".into();
        assert_eq!(
            other.personalize("Open read me", Language::English).text,
            "Open read me"
        );
        assert_eq!(
            learned.personalize("Open read me", Language::Hindi).text,
            "Open read me"
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
            .observe_learning(&switched, "Open read me", None)
            .unwrap();
        assert_eq!(
            reopened.personalization().unwrap().vocabulary[0].usage_count,
            1
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
    fn observations_and_retry_signals_never_activate_or_duplicate_usage() {
        let (_dir, store, p) = setup();
        teach(&store, &p, "teach");
        let first = context(&p, "first");
        store
            .observe_learning(&first, "Open read me with NewTool", None)
            .unwrap();
        let mut repeat = context(&p, "repeat");
        repeat.episode_id = first.episode_id;
        store
            .observe_learning(
                &repeat,
                "I said open read me with NewTool",
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
            .learn_from_edit(&context(&p, "undo"), 0, "Open README.md", "Open read me")
            .unwrap();
        assert_eq!(outcome.suspended, 1);
        let profile = store.personalization().unwrap();
        assert_eq!(profile.vocabulary.len(), 1);
        assert!(!profile.vocabulary[0].enabled);
        assert_eq!(
            profile.personalize("Open read me", Language::English).text,
            "Open read me"
        );
        assert_eq!(
            store.automatic_learning_state().unwrap().candidates[0].state,
            CandidateState::Suspended
        );
        teach(&store, &p, "later");
        assert!(!store.personalization().unwrap().vocabulary[0].enabled);
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
        assert!(store
            .automatic_learning_state()
            .unwrap()
            .candidates
            .is_empty());
        assert!(store.automatic_learning_state().unwrap().events.is_empty());
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
                "Open read me in the repo".into(),
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
            .learn_from_edit(&context(&p, "one"), 0, "Open read me", "Open README.md")
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
            .set_transcripts(&record.id, "Open read me in repo".into(), None, json!({}))
            .unwrap();
        let ctx = context(&p, "draft");
        let (saved, outcome) = store
            .edit_session_draft(
                &record.id,
                0,
                "Open read me in repo",
                "Open README.md in repo",
                &ctx,
            )
            .unwrap();
        assert_eq!(outcome.activated, ["README.md"]);
        assert_eq!(saved.draft_revision, 1);
        assert_eq!(
            saved.raw_transcript.as_deref(),
            Some("Open read me in repo")
        );
        assert!(saved.review_history.is_empty());
        assert!(saved.verified_transcript.is_none());
        assert!(saved.corrected_transcript.is_none());
        assert!(store
            .edit_session_draft(&record.id, 0, "Open read me in repo", "changed", &ctx)
            .is_err());
        assert_eq!(
            store
                .export_dataset(&dir.path().join("training"), true)
                .unwrap()
                .session_count,
            0
        );
        let encoded = serde_json::to_string(&store.automatic_learning_state().unwrap()).unwrap();
        assert!(!encoded.contains("Open read me in repo"));
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
    fn auto_language_and_conflicting_aliases_abstain() {
        let (_dir, store, p) = setup();
        let mut auto = context(&p, "auto");
        auto.language = Language::Auto;
        assert!(store
            .learn_from_edit(&auto, 0, "Open read me", "Open README.md")
            .unwrap()
            .activated
            .is_empty());
        teach(&store, &p, "one");
        assert!(store
            .learn_from_edit(&context(&p, "other"), 0, "Open read me", "Open README.txt")
            .unwrap()
            .activated
            .is_empty());
        assert_eq!(store.personalization().unwrap().vocabulary.len(), 1);
    }
}
