//! Untrusted local-model proposals. These contracts never grant memory admission.
use crate::{LearningContext, OutputIntent};
use serde::{Deserialize, Serialize};

pub const SEMANTIC_SCHEMA_VERSION: u32 = 1;
pub const SEMANTIC_INPUT_TOKENS: usize = 768;
pub const SEMANTIC_OUTPUT_TOKENS: usize = 192;
pub const SEMANTIC_TEXT_BYTES: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticStatus {
    Pending,
    Completed,
    Unsupported,
    Cancelled,
    Deadline,
    InvalidOutput,
    InputLimit,
    OutputLimit,
    Failed,
    Superseded,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticAnalysisReport {
    pub status: SemanticStatus,
    pub proposal: Option<SemanticProposal>,
    pub elapsed_micros: u64,
    pub prompt_tokens: usize,
    pub generated_tokens: usize,
    /// Content-free artifact/runtime/provider identity; not evidence of correctness.
    pub provenance: serde_json::Value,
}

impl SemanticAnalysisReport {
    pub fn empty(status: SemanticStatus) -> Self {
        Self {
            status,
            proposal: None,
            elapsed_micros: 0,
            prompt_tokens: 0,
            generated_tokens: 0,
            provenance: serde_json::json!({}),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticMemory {
    pub id: String,
    pub phrase: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticOrigin {
    InAppDraftEdit,
    AuthoredEvaluation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticAnalysisRequest {
    pub schema_version: u32,
    pub origin: SemanticOrigin,
    pub session_id: Option<String>,
    pub context: LearningContext,
    /// Revision after committing the edit, not the frontend's old revision.
    pub result_revision: u64,
    pub before: String,
    pub after: String,
    pub earlier: Option<String>,
    /// Caller supplies only enabled memories in this exact project/language scope.
    pub memories: Vec<SemanticMemory>,
}

impl SemanticAnalysisRequest {
    pub fn validate(&self) -> Result<(), &'static str> {
        let c = &self.context;
        if self.schema_version != SEMANTIC_SCHEMA_VERSION
            || self.session_id.as_ref().is_some_and(|id| !bounded(id, 64))
            || self.result_revision == 0
            || !c.learning_enabled
            || !bounded(&c.result_id, 64)
            || !bounded(&c.source_key, 128)
            || !bounded(&c.episode_id, 64)
            || c.project.len() > 64
            || c.project.chars().any(char::is_control)
            || c.model_id.len() > 256
            || self.before.trim().is_empty()
            || self.after.trim().is_empty()
            || self.before == self.after
            || self.before.len() + self.after.len() + self.earlier.as_ref().map_or(0, String::len)
                > SEMANTIC_TEXT_BYTES
            || self.earlier.as_ref().is_some_and(|s| s.trim().is_empty())
            || self.memories.len() > 8
            || self
                .memories
                .iter()
                .any(|m| !bounded(&m.id, 64) || !bounded(&m.phrase, 128))
            || self
                .memories
                .iter()
                .map(|m| &m.id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.memories.len()
        {
            return Err("Invalid or oversized semantic request");
        }
        Ok(())
    }

    pub fn parse_proposal(&self, json: &str) -> Result<SemanticProposal, &'static str> {
        self.validate()?;
        if json.len() > 8192 {
            return Err("Oversized semantic response");
        }
        let proposal: SemanticProposal =
            serde_json::from_str(json).map_err(|_| "Invalid semantic schema")?;
        proposal.validate(self)?;
        Ok(proposal)
    }
}

fn bounded(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticRelation {
    Repair,
    Continuation,
    ChangedIntent,
    Unrelated,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticSource {
    Before,
    After,
    Earlier,
}

/// Byte offsets into the supplied UTF-8 text. Neither a quote nor a model
/// confidence score can substitute for referencing actual supplied evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSpan {
    pub source: SemanticSource,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticMemorySelection {
    pub id: String,
    pub evidence: Vec<SemanticSpan>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticProposal {
    pub schema_version: u32,
    pub relation: SemanticRelation,
    pub relation_evidence: Vec<SemanticSpan>,
    pub intent: Option<OutputIntent>,
    pub intent_evidence: Vec<SemanticSpan>,
    pub memories: Vec<SemanticMemorySelection>,
}

impl SemanticProposal {
    pub fn validate(&self, request: &SemanticAnalysisRequest) -> Result<(), &'static str> {
        request.validate()?;
        let valid_spans = |spans: &[SemanticSpan]| {
            spans.len() <= 4
                && spans.iter().all(|s| {
                    let text = match s.source {
                        SemanticSource::Before => Some(request.before.as_str()),
                        SemanticSource::After => Some(request.after.as_str()),
                        SemanticSource::Earlier => request.earlier.as_deref(),
                    };
                    text.is_some_and(|t| {
                        s.start < s.end
                            && t.get(s.start..s.end).is_some_and(|v| !v.trim().is_empty())
                    })
                })
        };
        let cites = |spans: &[SemanticSpan], source| spans.iter().any(|s| s.source == source);
        let baseline = if request.earlier.is_some() {
            SemanticSource::Earlier
        } else {
            SemanticSource::Before
        };
        if self.schema_version != SEMANTIC_SCHEMA_VERSION
            || !valid_spans(&self.relation_evidence)
            || !valid_spans(&self.intent_evidence)
            || (self.relation != SemanticRelation::Unknown
                && (!cites(&self.relation_evidence, baseline)
                    || !cites(&self.relation_evidence, SemanticSource::After)))
            || (self.intent.is_some() && !cites(&self.intent_evidence, SemanticSource::After))
            || (self.intent.is_none() && !self.intent_evidence.is_empty())
            || self.memories.len() > request.memories.len()
            || self
                .memories
                .iter()
                .map(|m| &m.id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.memories.len()
            || self.memories.iter().any(|m| {
                !request.memories.iter().any(|allowed| allowed.id == m.id)
                    || !valid_spans(&m.evidence)
                    || !cites(&m.evidence, SemanticSource::After)
            })
        {
            return Err("Semantic proposal cites invalid or missing evidence");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> SemanticAnalysisRequest {
        SemanticAnalysisRequest {
            schema_version: 1,
            origin: crate::SemanticOrigin::AuthoredEvaluation,
            session_id: None,
            context: LearningContext {
                result_id: "r".into(),
                source_key: "audio".into(),
                episode_id: "e".into(),
                learning_enabled: true,
                ..Default::default()
            },
            result_revision: 1,
            before: "Café".into(),
            after: "Café note".into(),
            earlier: None,
            memories: vec![],
        }
    }
    const ABSTAIN: &str = r#"{"schemaVersion":1,"relation":"unknown","relationEvidence":[],"intent":null,"intentEvidence":[],"memories":[]}"#;
    #[test]
    fn schema_rejects_invention_truncation_and_untrusted_confidence() {
        let r = request();
        assert!(r.parse_proposal(ABSTAIN).is_ok());
        for bad in [
            ABSTAIN.replace("unknown", "certain"),
            ABSTAIN.replace("null", "\"email\""),
            ABSTAIN.replace(
                "\"schemaVersion\":1",
                "\"schemaVersion\":1,\"confidence\":1",
            ),
            ABSTAIN[..ABSTAIN.len() - 1].to_owned(),
            format!("```json\n{ABSTAIN}\n```"),
            ABSTAIN.replace(
                "\"memories\":[]",
                "\"memories\":[{\"id\":\"invented\",\"evidence\":[]}]",
            ),
            ABSTAIN.replace("unknown", "repair"),
        ] {
            assert!(r.parse_proposal(&bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn evidence_is_utf8_bounded_and_must_cover_both_sides() {
        let mut r = request();
        let mut p = r.parse_proposal(ABSTAIN).unwrap();
        p.relation = SemanticRelation::Repair;
        p.relation_evidence = vec![
            SemanticSpan {
                source: SemanticSource::Before,
                start: 0,
                end: 5,
            },
            SemanticSpan {
                source: SemanticSource::After,
                start: 0,
                end: 5,
            },
        ];
        assert!(p.validate(&r).is_ok());
        p.relation_evidence[0].end = 4; // splits é
        assert!(p.validate(&r).is_err());
        p.relation_evidence[0].end = 5;
        r.earlier = Some("earlier".into());
        assert!(p.validate(&r).is_err());
        p.relation_evidence[0].source = SemanticSource::Earlier;
        assert!(p.validate(&r).is_ok());
        p.intent = Some(OutputIntent::Technical);
        assert!(p.validate(&r).is_err());
    }
}
