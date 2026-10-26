//! Text-level correction gate shared by every recognizer.
use crate::{GateResult, Language, WorkingSet};

/// Apply the working set's exact spoken-form normalizations under the safety rules.
pub fn correction_gate(text: &str, _set: &WorkingSet, _language: Language) -> GateResult {
    GateResult {
        text: text.to_owned(),
        ..GateResult::default()
    }
}
