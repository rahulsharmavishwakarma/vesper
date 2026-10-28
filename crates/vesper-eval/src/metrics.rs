use serde::{Deserialize, Serialize};
use unicode_categories::UnicodeCategories;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ErrorRate {
    pub edits: usize,
    pub reference_units: usize,
    pub rate: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptScore {
    pub normalized_reference: String,
    pub normalized_hypothesis: String,
    pub word_error_rate: ErrorRate,
    pub grapheme_error_rate: ErrorRate,
}

/// Empty-reference audio has no conventional WER denominator. Report false
/// transcripts and insertions per audio minute instead, including punctuation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NonSpeechScore {
    pub false_transcript: bool,
    pub inserted_words: usize,
    pub inserted_words_per_minute: f64,
}

pub fn score_non_speech(hypothesis: &str, audio_seconds: f64) -> Option<NonSpeechScore> {
    if !audio_seconds.is_finite() || audio_seconds <= 0.0 {
        return None;
    }
    let inserted_words = normalize(hypothesis).split_whitespace().count();
    Some(NonSpeechScore {
        false_transcript: !hypothesis.trim().is_empty(),
        inserted_words,
        inserted_words_per_minute: inserted_words as f64 * 60.0 / audio_seconds,
    })
}

pub fn score(reference: &str, hypothesis: &str) -> TranscriptScore {
    let normalized_reference = normalize(reference);
    let normalized_hypothesis = normalize(hypothesis);
    let reference_words = normalized_reference.split_whitespace().collect::<Vec<_>>();
    let hypothesis_words = normalized_hypothesis.split_whitespace().collect::<Vec<_>>();
    let reference_graphemes = UnicodeSegmentation::graphemes(normalized_reference.as_str(), true)
        .filter(|unit| !unit.chars().all(char::is_whitespace))
        .collect::<Vec<_>>();
    let hypothesis_graphemes = UnicodeSegmentation::graphemes(normalized_hypothesis.as_str(), true)
        .filter(|unit| !unit.chars().all(char::is_whitespace))
        .collect::<Vec<_>>();
    TranscriptScore {
        word_error_rate: error_rate(&reference_words, &hypothesis_words),
        grapheme_error_rate: error_rate(&reference_graphemes, &hypothesis_graphemes),
        normalized_reference,
        normalized_hypothesis,
    }
}

/// Partial-stability scoring needs words only, not a second grapheme DP pass.
pub fn word_edit_distance(reference: &str, hypothesis: &str) -> usize {
    let reference = normalize(reference);
    let hypothesis = normalize(hypothesis);
    edit_distance(
        &reference.split_whitespace().collect::<Vec<_>>(),
        &hypothesis.split_whitespace().collect::<Vec<_>>(),
    )
}

pub(crate) fn normalize(text: &str) -> String {
    let mut normalized = String::new();
    let mut separated = true;
    for character in text.nfkc().flat_map(char::to_lowercase) {
        let separator = character.is_whitespace()
            || character.is_punctuation()
            || character.is_symbol()
            || matches!(character, '\u{0964}' | '\u{0965}');
        if separator {
            if !separated && !normalized.is_empty() {
                normalized.push(' ');
            }
            separated = true;
        } else if !character.is_control() {
            normalized.push(character);
            separated = false;
        }
    }
    normalized.trim().to_owned()
}

fn error_rate<T: Eq>(reference: &[T], hypothesis: &[T]) -> ErrorRate {
    let edits = edit_distance(reference, hypothesis);
    ErrorRate {
        edits,
        reference_units: reference.len(),
        rate: if reference.is_empty() {
            if hypothesis.is_empty() {
                0.0
            } else {
                1.0
            }
        } else {
            edits as f64 / reference.len() as f64
        },
    }
}

pub(crate) fn edit_distance<T: Eq>(left: &[T], right: &[T]) -> usize {
    // Removing identical ends preserves Levenshtein distance. Long stable
    // streaming prefixes must not make observer scoring quadratic per update.
    let prefix = left.iter().zip(right).take_while(|(a, b)| a == b).count();
    let (left, right) = (&left[prefix..], &right[prefix..]);
    let suffix = left
        .iter()
        .rev()
        .zip(right.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (left, right) = (&left[..left.len() - suffix], &right[..right.len() - suffix]);
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }
    if left.len() > right.len() {
        return edit_distance(right, left);
    }
    let mut previous = (0..=left.len()).collect::<Vec<_>>();
    let mut current = vec![0; left.len() + 1];
    for (right_index, right_item) in right.iter().enumerate() {
        current[0] = right_index + 1;
        for (left_index, left_item) in left.iter().enumerate() {
            current[left_index + 1] = if left_item == right_item {
                previous[left_index]
            } else {
                1 + previous[left_index]
                    .min(previous[left_index + 1])
                    .min(current[left_index])
            };
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[left.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_streaming_prefixes_preserve_word_edit_counts() {
        let prefix = "a b c ".repeat(1000);
        assert_eq!(word_edit_distance(&prefix, &format!("{prefix}new word")), 2);
        assert_eq!(
            word_edit_distance(
                &format!("{prefix}old ending"),
                &format!("{prefix}new ending")
            ),
            1
        );
        assert_eq!(word_edit_distance("a b c", "b c a"), 2);
        assert_eq!(word_edit_distance("", "one"), 1);
    }

    #[test]
    fn word_error_rate_handles_insert_delete_and_substitute() {
        let result = score("one two three", "one too three four");
        assert_eq!(result.word_error_rate.edits, 2);
        assert_eq!(result.word_error_rate.reference_units, 3);
        assert!((result.word_error_rate.rate - 2.0 / 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn multilingual_normalization_preserves_devanagari_graphemes() {
        let result = score("क्या आप सुन सकते हैं?", "क्या आप सुन सकते हैं");
        assert_eq!(result.word_error_rate.edits, 0);
        assert_eq!(result.grapheme_error_rate.edits, 0);
        assert!(result.normalized_reference.contains("क्या"));
    }

    #[test]
    fn normalization_is_case_punctuation_and_compatibility_insensitive() {
        let result = score("README.txt — Café!", "readme txt cafe\u{301}");
        assert_eq!(result.word_error_rate.edits, 0);
        assert_eq!(result.grapheme_error_rate.edits, 0);
    }

    #[test]
    fn empty_reference_never_divides_by_zero() {
        assert_eq!(score("", "").word_error_rate.rate, 0.0);
        assert_eq!(score("", "speech").word_error_rate.rate, 1.0);
    }

    #[test]
    fn non_speech_counts_hallucinations_without_a_wer_denominator() {
        let value = score_non_speech("Thank you!", 30.0).unwrap();
        assert!(value.false_transcript);
        assert_eq!(value.inserted_words, 2);
        assert_eq!(value.inserted_words_per_minute, 4.0);
        assert!(score_non_speech("...", 10.0).unwrap().false_transcript);
        assert!(!score_non_speech("  ", 10.0).unwrap().false_transcript);
        assert!(score_non_speech("words", 0.0).is_none());
        assert!(score_non_speech("words", f64::NAN).is_none());
    }
}
