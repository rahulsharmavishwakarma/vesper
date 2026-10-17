//! Conservative rejection checks, not a semantic correctness guarantee.
//! In particular these cannot reliably identify names or arbitrary paraphrases.

use crate::{InferenceError, Result};
use unicode_segmentation::UnicodeSegmentation;
use vesper_core::CleanupPreset;

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

fn words(text: &str) -> Vec<String> {
    text.replace('’', "'")
        .unicode_words()
        .map(str::to_lowercase)
        .collect()
}

/// Restore a dictated contraction that a formatting-only model expanded.
/// All other words must align exactly. The ordinary guard still checks the
/// repaired whole output, including number symbols and negation.
pub(crate) fn restore_contractions(raw: &str, output: &str) -> Result<Option<String>> {
    let edited = output.unicode_word_indices().collect::<Vec<_>>();
    let normalize = |word: &str| word.replace('’', "'").to_lowercase();
    let mut cursor = 0;
    let mut replacements = vec![];
    for original in raw.unicode_words() {
        let Some(&(offset, word)) = edited.get(cursor) else {
            return Ok(None);
        };
        let original_key = normalize(original);
        if original_key == normalize(word) {
            cursor += 1;
            continue;
        }
        let Some(&(next_offset, next_word)) = edited.get(cursor + 1) else {
            return Ok(None);
        };
        let between = &output[offset + word.len()..next_offset];
        if between.is_empty()
            || !between.chars().all(char::is_whitespace)
            || !contraction_expansion(&original_key, &normalize(word), &normalize(next_word))
        {
            return Ok(None);
        }
        let replacement = if word.starts_with(|c: char| c.is_uppercase()) {
            let mut chars = original.chars();
            format!("{}{}", chars.next().unwrap().to_uppercase(), chars.as_str())
        } else {
            original.to_owned()
        };
        replacements.push((offset, next_offset + next_word.len(), replacement));
        cursor += 2;
    }
    if cursor != edited.len() || replacements.is_empty() {
        return Ok(None);
    }
    let mut repaired = output.to_owned();
    for (start, end, original) in replacements.into_iter().rev() {
        repaired.replace_range(start..end, &original);
    }
    validate(raw, &repaired, CleanupPreset::Minimal)?;
    Ok(Some(repaired))
}

fn contraction_expansion(original: &str, first: &str, second: &str) -> bool {
    if let Some(stem) = original.strip_suffix("n't") {
        let verb = match stem {
            "ca" => "can",
            "wo" => "will",
            "sha" => "shall",
            value => value,
        };
        return matches!(
            verb,
            "can"
                | "will"
                | "shall"
                | "is"
                | "are"
                | "was"
                | "were"
                | "do"
                | "does"
                | "did"
                | "have"
                | "has"
                | "had"
                | "could"
                | "would"
                | "should"
                | "must"
                | "need"
        ) && first == verb
            && second == "not";
    }
    let Some((subject, suffix)) = original.split_once('\'') else {
        return false;
    };
    if subject != first {
        return false;
    }
    match suffix {
        "m" => subject == "i" && second == "am",
        "re" => matches!(subject, "you" | "we" | "they") && second == "are",
        "ve" => matches!(subject, "i" | "you" | "we" | "they") && second == "have",
        "s" => matches!(subject, "he" | "she" | "it") && matches!(second, "is" | "has"),
        "ll" => {
            matches!(subject, "i" | "you" | "he" | "she" | "it" | "we" | "they") && second == "will"
        }
        "d" => {
            matches!(subject, "i" | "you" | "he" | "she" | "it" | "we" | "they")
                && matches!(second, "had" | "would")
        }
        _ => false,
    }
}

#[cfg(test)]
mod contraction_tests {
    use super::*;
    #[test]
    fn restores_only_source_contractions_in_order() {
        for (raw, generated) in [
            ("it's ready", "It is ready."),
            (
                "we're ready but it isn't done",
                "We are ready, but it is not done.",
            ),
            ("I’m here and I’ll wait", "I am here and I will wait."),
            ("it's ready it's ready", "It is ready. It is ready."),
            ("she said \"don't go\"", "She said \"do not go\"."),
            ("I can't send -15.5%", "I can not send -15.5%."),
        ] {
            let repaired = restore_contractions(raw, generated).unwrap().unwrap();
            validate(raw, &repaired, CleanupPreset::Minimal).unwrap();
            assert!(!repaired.contains(" is not "));
        }
    }
    #[test]
    fn rejects_other_edits_missing_repeats_possessives_and_punctuation_crossing() {
        for (raw, generated) in [
            ("it's ready", "It is not ready."),
            ("it's ready it's ready", "It is ready."),
            ("it's ready", "It is already ready."),
            ("it's ready", "It, is ready."),
            ("it's ready", "It. Is ready."),
            ("John's ready", "John is ready."),
            ("it is ready", "It's ready."),
            ("keep the words", "Keep the words."),
        ] {
            assert!(
                restore_contractions(raw, generated).unwrap().is_none(),
                "{raw} / {generated}"
            );
        }
    }
    #[test]
    fn restoration_cannot_bypass_number_symbol_guard() {
        for changed in ["It is +15.5%.", "It is 15.5.", "It is 16.5%."] {
            assert!(!matches!(
                restore_contractions("it's -15.5%", changed),
                Ok(Some(_))
            ));
        }
    }
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
}
