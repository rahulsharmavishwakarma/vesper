//! Conservative rejection checks, not a semantic correctness guarantee.
//! In particular these cannot reliably identify names or arbitrary paraphrases.

use crate::{InferenceError, Result};
use transcribe_core::CleanupPreset;
use unicode_segmentation::UnicodeSegmentation;

pub(crate) fn validate(raw: &str, output: &str, preset: CleanupPreset) -> Result<()> {
    let original = words(raw);
    let edited = words(output);
    if numbers(raw) != numbers(output) {
        return Err(InferenceError::Fidelity(
            "numbers or number symbols changed",
        ));
    }
    if negations(&original) != negations(&edited) {
        return Err(InferenceError::Fidelity("negation changed"));
    }
    if preset == CleanupPreset::Minimal {
        return if original == edited {
            Ok(())
        } else {
            Err(InferenceError::Fidelity(
                "formatting-only polishing changed words",
            ))
        };
    }
    let original = without_fillers(original);
    let edited = without_fillers(edited);
    // Allow small grammar edits after ignoring fillers and adjacent repetition.
    // More substantial reconstructions need human review of the raw transcript.
    let budget = (original.len() / 5).max(1);
    if original.len().abs_diff(edited.len()) > budget || edit_distance(&original, &edited) > budget
    {
        return Err(InferenceError::Fidelity(
            "too many words changed or omitted",
        ));
    }
    Ok(())
}

/// A technical-tail pass may change one contiguous lexical location only. The
/// normal full-transcript guard still validates the eventual cleanup against
/// the original raw ASR text.
pub(crate) fn validate_technical_tail(raw: &str, output: &str) -> Result<()> {
    let original = words(raw);
    let edited = words(output);
    if numbers(raw) != numbers(output) {
        return Err(InferenceError::Fidelity(
            "technical correction changed numbers or number symbols",
        ));
    }
    if negations(&original) != negations(&edited) {
        return Err(InferenceError::Fidelity(
            "technical correction changed negation",
        ));
    }
    if original == edited {
        return Ok(());
    }

    let prefix = original
        .iter()
        .zip(&edited)
        .take_while(|(left, right)| left == right)
        .count();
    let suffix = original[prefix..]
        .iter()
        .rev()
        .zip(edited[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    let original_change = &original[prefix..original.len() - suffix];
    let edited_change = &edited[prefix..edited.len() - suffix];
    if original_change.is_empty()
        || edited_change.is_empty()
        || original_change.len() > 2
        || edited_change.len() > 2
        || original.len().abs_diff(edited.len()) > 1
        || edit_distance(original_change, edited_change) > 2
        || !contains_changed_technical_token(output, edited_change)
    {
        return Err(InferenceError::Fidelity(
            "technical correction changed more than one bounded term",
        ));
    }
    Ok(())
}

/// Extract one nearby technical spelling from a potentially over-edited model
/// response, then apply only that token to the untouched raw tail.
pub(crate) fn apply_technical_tail(raw: &str, model_output: &str) -> Result<Option<String>> {
    let raw_tokens = whitespace_tokens(raw);
    let mut correction: Option<(usize, usize, &str)> = None;
    for candidate in model_output.split_whitespace() {
        let candidate = candidate.trim_matches(|character: char| {
            matches!(
                character,
                ',' | ';' | ':' | '?' | '!' | '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}'
            )
        });
        if candidate.is_empty() || !looks_technical(candidate) {
            continue;
        }
        let normalized_candidate = alphanumeric(candidate);
        if normalized_candidate.is_empty()
            || raw_tokens
                .iter()
                .any(|(_, _, token)| alphanumeric(token) == normalized_candidate)
        {
            continue;
        }
        let mut best: Option<(usize, usize, usize)> = None;
        let mut tied = false;
        for (start, end, token) in &raw_tokens {
            let distance = character_edit_distance(&alphanumeric(token), &normalized_candidate);
            match best {
                None => {
                    best = Some((*start, *end, distance));
                    tied = false;
                }
                Some((_, _, best_distance)) if distance < best_distance => {
                    best = Some((*start, *end, distance));
                    tied = false;
                }
                Some((_, _, best_distance)) if distance == best_distance => tied = true,
                Some(_) => {}
            }
        }
        let Some((start, end, distance)) = best else {
            continue;
        };
        if tied || !(1..=2).contains(&distance) {
            continue;
        }
        if correction.is_some() {
            return Err(InferenceError::Fidelity(
                "technical correction proposed more than one candidate",
            ));
        }
        correction = Some((start, end, candidate));
    }
    let Some((start, end, candidate)) = correction else {
        return Ok(None);
    };
    let corrected = format!("{}{}{}", &raw[..start], candidate, &raw[end..]);
    validate_technical_tail(raw, &corrected)?;
    Ok(Some(corrected))
}

/// A broad cleanup pass may not undo the one accepted technical correction.
/// Reapply only that candidate to the generated text; fail if it cannot be
/// located safely.
pub(crate) fn preserve_technical_correction(
    original: &str,
    prepared: &str,
    output: &str,
) -> Result<String> {
    let original_tokens = whitespace_tokens(original);
    let mut candidates = prepared
        .split_whitespace()
        .filter(|token| looks_technical(token))
        .filter(|candidate| {
            let normalized = alphanumeric(candidate);
            !original_tokens
                .iter()
                .any(|(_, _, token)| alphanumeric(token) == normalized)
        })
        .map(alphanumeric)
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.dedup();
    if candidates.len() != 1 {
        return Err(InferenceError::Fidelity(
            "accepted technical correction could not be identified",
        ));
    }
    if output
        .split_whitespace()
        .any(|token| alphanumeric(token) == candidates[0])
    {
        return Ok(output.to_owned());
    }
    apply_technical_tail(output, prepared)?.ok_or(InferenceError::Fidelity(
        "final polishing lost the accepted technical correction",
    ))
}

fn words(text: &str) -> Vec<String> {
    text.replace('’', "'")
        .unicode_words()
        .map(str::to_lowercase)
        .collect()
}

fn numbers(text: &str) -> Vec<String> {
    let text = text.to_lowercase();
    text.unicode_word_indices()
        .filter(|(_, word)| word.chars().any(char::is_numeric) || is_number_word(word))
        .map(|(offset, word)| {
            let prefix = text[..offset].chars().next_back().filter(|character| {
                matches!(character, '-' | '+' | '−' | '$' | '₹' | '£' | '€' | '¥')
            });
            let suffix = text[offset + word.len()..].trim_start().starts_with('%');
            format!(
                "{}{word}{}",
                prefix.map_or(String::new(), |c| c.to_string()),
                if suffix { "%" } else { "" }
            )
        })
        .collect()
}

fn is_number_word(word: &str) -> bool {
    matches!(
        word,
        "zero"
            | "one"
            | "two"
            | "three"
            | "four"
            | "five"
            | "six"
            | "seven"
            | "eight"
            | "nine"
            | "ten"
            | "eleven"
            | "twelve"
            | "thirteen"
            | "fourteen"
            | "fifteen"
            | "sixteen"
            | "seventeen"
            | "eighteen"
            | "nineteen"
            | "twenty"
            | "thirty"
            | "forty"
            | "fifty"
            | "sixty"
            | "seventy"
            | "eighty"
            | "ninety"
            | "hundred"
            | "thousand"
            | "million"
            | "billion"
            | "lakh"
            | "crore"
            | "minus"
            | "plus"
            | "percent"
    )
}

fn negations(words: &[String]) -> Vec<&str> {
    words
        .iter()
        .filter_map(|word| {
            if word == "cannot" || word.ends_with("n't") {
                Some("not")
            } else if matches!(
                word.as_str(),
                "not" | "no" | "never" | "neither" | "nor" | "without" | "नहीं" | "मत" | "ना"
            ) {
                Some(word.as_str())
            } else {
                None
            }
        })
        .collect()
}

fn without_fillers(words: Vec<String>) -> Vec<String> {
    let mut words: Vec<String> = words
        .into_iter()
        .flat_map(|word| {
            let expansion = match word.as_str() {
                "cannot" | "can't" => Some("can"),
                "won't" => Some("will"),
                "shan't" => Some("shall"),
                "isn't" | "aren't" | "wasn't" | "weren't" | "don't" | "doesn't" | "didn't"
                | "hasn't" | "haven't" | "hadn't" | "couldn't" | "shouldn't" | "wouldn't"
                | "mustn't" | "needn't" => word.strip_suffix("n't"),
                _ => None,
            };
            match expansion {
                Some(stem) => vec![stem.to_owned(), "not".to_owned()],
                None => vec![word],
            }
        })
        .collect();
    words.retain(|word| !matches!(word.as_str(), "um" | "uh" | "erm" | "er" | "hmm"));
    words.dedup();
    words
}

fn edit_distance(original: &[String], edited: &[String]) -> usize {
    let mut row: Vec<usize> = (0..=edited.len()).collect();
    for (i, original_word) in original.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, edited_word) in edited.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (row[j] + 1)
                .min(above + 1)
                .min(diagonal + usize::from(original_word != edited_word));
            diagonal = above;
        }
    }
    row[edited.len()]
}

fn contains_changed_technical_token(text: &str, changed: &[String]) -> bool {
    text.split_whitespace().any(|token| {
        let token_words = words(token);
        token_words == changed && looks_technical(token)
    })
}

fn looks_technical(token: &str) -> bool {
    let characters = token.chars().collect::<Vec<_>>();
    let internal_separator = characters.windows(3).any(|window| {
        window[0].is_alphanumeric()
            && matches!(window[1], '.' | '_' | '-' | '/' | '\\' | '#')
            && window[2].is_alphanumeric()
    }) || token.contains("::");
    let letters = characters
        .iter()
        .copied()
        .filter(|character| character.is_alphabetic())
        .collect::<Vec<_>>();
    let conventional_case = letters.len() >= 2
        && (letters.iter().all(|character| character.is_uppercase())
            || letters
                .iter()
                .skip(1)
                .any(|character| character.is_uppercase()));
    internal_separator || conventional_case
}

fn whitespace_tokens(text: &str) -> Vec<(usize, usize, &str)> {
    text.split_whitespace()
        .map(|token| {
            let start = token.as_ptr() as usize - text.as_ptr() as usize;
            (start, start + token.len(), token)
        })
        .collect()
}

fn alphanumeric(text: &str) -> String {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn character_edit_distance(left: &str, right: &str) -> usize {
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();
    let mut row = (0..=right.len()).collect::<Vec<_>>();
    for (i, left_character) in left.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, right_character) in right.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (row[j] + 1)
                .min(above + 1)
                .min(diagonal + usize::from(left_character != right_character));
            diagonal = above;
        }
    }
    row[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observed_omission_and_changed_numbers_are_rejected() {
        assert!(validate(
            "Ask not what your country can do for you, ask what you can do for your country.",
            "Ask what you can do for your country.",
            CleanupPreset::Balanced
        )
        .is_err());
        for (raw, edited) in [
            ("send fifteen rupees", "Send fifty rupees."),
            ("send 1.5 rupees", "Send 15 rupees."),
            ("set -5 degrees", "Set 5 degrees."),
            ("set minus five degrees", "Set five degrees."),
            ("pay ₹500", "Pay $500."),
        ] {
            assert!(validate(raw, edited, CleanupPreset::Balanced).is_err());
        }
    }
    #[test]
    fn punctuation_fillers_and_normalized_negation_are_accepted() {
        assert!(validate(
            "um Rahul do not send 15 rupees",
            "Rahul, do not send 15 rupees.",
            CleanupPreset::Balanced
        )
        .is_ok());
        assert!(validate(
            "I can't send it",
            "I cannot send it.",
            CleanupPreset::Balanced
        )
        .is_ok());
        assert!(validate("Don't send it", "Do not send it.", CleanupPreset::Balanced).is_ok());
        assert!(validate(
            "can Rahul come tomorrow",
            "Can Rahul come tomorrow?",
            CleanupPreset::Minimal
        )
        .is_ok());
        assert!(validate(
            "send नहीं fifteen rupees",
            "Send fifteen rupees.",
            CleanupPreset::Balanced
        )
        .is_err());
        assert!(validate(
            "can you edit this readmixt file in the repository please",
            "Can you edit this README.txt file in the repository, please?",
            CleanupPreset::Balanced
        )
        .is_ok());
    }
    #[test]
    fn minimal_and_large_edits_are_rejected() {
        assert!(validate("hello Rahul", "Hello Rohan.", CleanupPreset::Minimal).is_err());
        assert!(validate(
            "please send the report and also call Rahul tomorrow",
            "Send report.",
            CleanupPreset::Balanced
        )
        .is_err());
        assert!(validate(
            "send the report",
            "Send the report and then book a flight to Paris.",
            CleanupPreset::Balanced
        )
        .is_err());
    }

    #[test]
    fn technical_tail_allows_one_notated_term_and_rejects_broader_changes() {
        assert!(validate_technical_tail(
            "please open the repository and edit this readmixt file",
            "please open the repository and edit this README.txt file"
        )
        .is_ok());
        for (raw, edited) in [
            (
                "do not edit readmixt with version 15",
                "edit README.txt with version 16",
            ),
            (
                "edit readmixt and open packagjson",
                "edit README.txt and open package.json",
            ),
            ("send the report to Rahul", "send package.json to Rohan"),
            (
                "edit the report",
                "edit package.json and then deploy the application",
            ),
        ] {
            assert!(validate_technical_tail(raw, edited).is_err());
        }
        let raw = "local application and reviewed details before deciding to open the repository and edit this readmixt file please";
        let over_edited = "local application reviewed details before deciding to open the repository and edit this README.txt file";
        let extracted = apply_technical_tail(raw, over_edited).unwrap().unwrap();
        assert_eq!(
            extracted,
            "local application and reviewed details before deciding to open the repository and edit this README.txt file please"
        );
        assert!(apply_technical_tail(
            "edit readmixt and packagjson",
            "edit README.txt and package.json"
        )
        .is_err());
        assert!(apply_technical_tail("send the report", "send package.json")
            .unwrap()
            .is_none());
        let original = "please edit this readmixt file";
        let prepared = "please edit this README.txt file";
        assert_eq!(
            preserve_technical_correction(original, prepared, "Please edit this readmixt file.")
                .unwrap(),
            "Please edit this README.txt file."
        );
        assert!(preserve_technical_correction(
            original,
            prepared,
            "Please deploy the application."
        )
        .is_err());
    }
}
