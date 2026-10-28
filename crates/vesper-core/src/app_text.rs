//! Text read from the app a dictation goes into (both opt-in, see
//! docs/personalization/app-learning.md). `field_terms` picks unusual spellings
//! already in the destination field for this one recording; `aligned_edit` finds
//! what became of pasted text so the user's own fixes there count as edits.
//! Neither function stores anything.
use std::collections::HashMap;

use crate::ordinary_word;

/// Spellings taken from one field, at most; they join that recording's references.
pub const MAX_FIELD_TERMS: usize = 24;

fn trim_token(token: &str) -> &str {
    token.trim_matches(|c: char| !(c.is_alphanumeric() || c == '_'))
}

fn camel(token: &str) -> bool {
    let chars: Vec<char> = token.chars().collect();
    chars
        .windows(2)
        .any(|w| w[0].is_lowercase() && w[1].is_uppercase())
}

/// Names, acronyms and identifiers in the text before the caret. Lowercase
/// non-words count only when repeated, so a single typo never becomes a spelling.
pub fn field_terms(text: &str) -> Vec<String> {
    // (count, first position, seen capitalized mid-sentence)
    let mut counts: HashMap<&str, (usize, usize, bool)> = HashMap::new();
    let mut sentence_start = true;
    for (order, raw) in text.split_whitespace().enumerate() {
        let starts = sentence_start;
        sentence_start = raw.ends_with(['.', '!', '?', ':']);
        if raw.contains('@') || raw.contains("://") {
            continue;
        }
        let token = trim_token(raw);
        let len = token.chars().count();
        if !(3..=40).contains(&len) || token.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let entry = counts.entry(token).or_insert((0, order, false));
        entry.0 += 1;
        entry.2 |= !starts;
    }
    let mut terms: Vec<(&str, usize, usize)> = counts
        .into_iter()
        .filter(|(token, (count, _, mid_sentence))| {
            let letters = token.chars().filter(|c| c.is_alphabetic()).count();
            let upper = token.chars().filter(|c| c.is_uppercase()).count();
            let identifier = token.contains('_')
                || camel(token)
                || (letters > 0 && token.chars().any(|c| c.is_ascii_digit()));
            // Writing a word in capitals already asserts its spelling (NVIDIA).
            let acronym = (2..=8).contains(&letters) && upper == letters;
            // A capitalized dictionary word mid-sentence is a name (the Vesper app).
            let name = token.chars().next().is_some_and(char::is_uppercase)
                && upper == 1
                && (!ordinary_word(token) || *mid_sentence);
            let lower_nonword =
                upper == 0 && letters == token.chars().count() && !ordinary_word(token);
            identifier || acronym || name || (lower_nonword && *count >= 2)
        })
        .map(|(token, (count, order, _))| (token, count, order))
        .collect();
    terms.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));
    terms
        .into_iter()
        .take(MAX_FIELD_TERMS)
        .map(|(token, _, _)| token.to_owned())
        .collect()
}

fn word_key(word: &str) -> String {
    trim_token(&word.replace('’', "'")).to_lowercase()
}

/// Longest pasted text compared word by word; longer pastes are not read back.
pub const MAX_ALIGNED_WORDS: usize = 1500;

/// `region` is the destination text starting where the paste began, read some
/// time later and possibly continuing past it. Returns the part of `region` that
/// the pasted text became, or `None` when it no longer lines up (rewritten,
/// moved or deleted). Equal text comes back unchanged.
pub fn aligned_edit(inserted: &str, region: &str) -> Option<String> {
    let pasted: Vec<String> = inserted.split_whitespace().map(word_key).collect();
    let spans: Vec<(usize, usize)> = {
        let mut spans = vec![];
        let mut start = None;
        for (i, c) in region.char_indices() {
            match (c.is_whitespace(), start) {
                (false, None) => start = Some(i),
                (true, Some(s)) => {
                    spans.push((s, i));
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(s) = start {
            spans.push((s, region.len()));
        }
        spans
    };
    let (n, m) = (pasted.len(), spans.len());
    if n == 0 || m == 0 || n > MAX_ALIGNED_WORDS {
        return None;
    }
    let found: Vec<String> = spans
        .iter()
        .map(|&(s, e)| word_key(&region[s..e]))
        .collect();
    // Anchored at the paste start: dp[j] = distance(pasted, found[..j]).
    let mut previous: Vec<usize> = (0..=m).collect();
    for word in &pasted {
        let mut current = vec![previous[0] + 1; m + 1];
        for j in 1..=m {
            let substitute = previous[j - 1] + usize::from(*word != found[j - 1]);
            current[j] = substitute.min(previous[j] + 1).min(current[j - 1] + 1);
        }
        previous = current;
    }
    let (end, distance) = (1..=m)
        .map(|j| (j, previous[j]))
        .min_by_key(|&(j, d)| (d, j.abs_diff(n)))?;
    if distance > (n / 3).max(1) || word_key(&region[spans[0].0..spans[0].1]).is_empty() && n > 1 {
        return None;
    }
    Some(region[spans[0].0..spans[end - 1].1].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_terms_keep_names_identifiers_and_acronyms_not_prose() {
        let text = "We deploy Kubernetes with sherpa_onnx and NVIDIA GPUs. The AsrModel \
                    loads gemma3 weights on the Vesper runner. Ordinary words like table stay \
                    out. kubectl applies it; then kubectl waits. A single teh typo stays out.";
        let terms = field_terms(text);
        for want in [
            "Vesper",
            "Kubernetes",
            "sherpa_onnx",
            "NVIDIA",
            "AsrModel",
            "gemma3",
            "kubectl",
        ] {
            assert!(
                terms.iter().any(|t| t == want),
                "{want} missing from {terms:?}"
            );
        }
        for unwanted in ["We", "deploy", "The", "Ordinary", "teh", "table", "single"] {
            assert!(
                !terms.iter().any(|t| t == unwanted),
                "{unwanted} in {terms:?}"
            );
        }
    }

    #[test]
    fn field_terms_skip_addresses_and_numbers_and_stay_bounded() {
        assert!(field_terms("mail me@example.com or https://example.com 2026 1500").is_empty());
        let many = (0..100)
            .map(|i| format!("Zorb{i}x"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(field_terms(&many).len(), MAX_FIELD_TERMS);
    }

    #[test]
    fn aligned_edit_finds_a_fixed_word_and_ignores_text_typed_after() {
        let pasted = "We moved the vasper build to the new runner.";
        let region = "We moved the Vesper build to the new runner. Next I will check the logs";
        assert_eq!(
            aligned_edit(pasted, region).as_deref(),
            Some("We moved the Vesper build to the new runner.")
        );
        assert_eq!(aligned_edit(pasted, pasted).as_deref(), Some(pasted));
    }

    #[test]
    fn aligned_edit_rejects_rewritten_or_deleted_text() {
        let pasted = "Please send the quarterly report to the finance team today.";
        assert_eq!(
            aligned_edit(
                pasted,
                "Totally different words were typed in here instead now."
            ),
            None
        );
        assert_eq!(aligned_edit(pasted, ""), None);
        assert_eq!(aligned_edit("", "anything"), None);
    }

    #[test]
    fn aligned_edit_keeps_inserted_and_removed_words_within_the_budget() {
        let pasted = "the build uses sherpa onnx for decoding on device";
        let region = "the build uses sherpa_onnx for decoding on device\nmore text";
        assert_eq!(
            aligned_edit(pasted, region).as_deref(),
            Some("the build uses sherpa_onnx for decoding on device")
        );
    }
}
