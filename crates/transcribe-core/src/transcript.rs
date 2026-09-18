use serde::{Deserialize, Serialize};

/// Shared stage names; the application layer owns transitions and session IDs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStage {
    #[default]
    Idle,
    Recording,
    Transcribing,
    Rewriting,
    Ready,
    Cancelled,
    Error,
}

/// The recognizer's output remains available even if cleanup fails or is replaced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    raw: String,
    cleaned: Option<String>,
}

impl Transcript {
    pub fn new(raw: impl Into<String>) -> Self {
        Self {
            raw: raw.into(),
            cleaned: None,
        }
    }

    pub fn raw(&self) -> &str {
        &self.raw
    }

    pub fn cleaned(&self) -> Option<&str> {
        self.cleaned.as_deref()
    }

    pub fn set_cleaned(&mut self, cleaned: impl Into<String>) {
        self.cleaned = Some(cleaned.into());
    }

    pub fn clear_cleaned(&mut self) {
        self.cleaned = None;
    }
}

#[cfg(test)]
mod tests {
    use super::Transcript;

    #[test]
    fn replacing_or_discarding_cleanup_preserves_verbatim_recognition() {
        let raw = "um do not send fifteen sorry fifty rupees";
        let mut transcript = Transcript::new(raw);
        transcript.set_cleaned("Do not send fifty rupees.");
        transcript.set_cleaned("Do not send 50 rupees.");
        assert_eq!(transcript.raw(), raw);
        transcript.clear_cleaned();
        assert_eq!(transcript.raw(), raw);
        assert_eq!(transcript.cleaned(), None);
    }
}
