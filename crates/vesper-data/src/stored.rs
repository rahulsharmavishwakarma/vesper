//! Reading JSON that another version of Vesper stored.
//!
//! Older data loads through `#[serde(default)]` on new fields. Newer data can
//! carry fields and enum values this build does not know, and the strict
//! (`deny_unknown_fields`) types reject the whole document, which once stopped
//! recording after a rollback (issue #362). `decode` parses strictly first; on
//! an unknown field or variant it drops the smallest part that names it (the
//! field, or the list entry that holds it), keeps the edit only when that error
//! goes away, and retries. Dropping happens in memory: the next save writes
//! this build's shape. Saves from the interface stay strict.

use serde::de::DeserializeOwned;
use serde_json::Value;

/// Upper bound on dropped parts per document, so a hostile row cannot loop.
const MAX_REPAIRS: usize = 256;

pub fn decode<T: DeserializeOwned>(json: &str) -> serde_json::Result<T> {
    let strict = match serde_json::from_str::<T>(json) {
        Ok(decoded) => return Ok(decoded),
        Err(error) => error,
    };
    // A syntax error has nothing to repair; report the strict error.
    let Ok(mut value) = serde_json::from_str::<Value>(json) else {
        return Err(strict);
    };
    decode_value(&mut value).map_err(|_| strict)
}

/// The repaired value is left in `value`, for callers that keep it.
pub(crate) fn decode_value<T: DeserializeOwned>(value: &mut Value) -> serde_json::Result<T> {
    let mut last = None;
    for _ in 0..=MAX_REPAIRS {
        let (text, occurrences) = match T::deserialize(&*value) {
            Ok(decoded) => return Ok(decoded),
            Err(error) => match unknown_name(&error.to_string()) {
                Some((kind, name)) => {
                    let mut located = Located::default();
                    write(value, kind, name, &mut Vec::new(), &mut located);
                    last = Some(error);
                    (located.text, located.occurrences)
                }
                None => return Err(error),
            },
        };
        // Parsing the written text gives the error a position, which picks
        // the one occurrence that failed when a name appears more than once.
        let column = match serde_json::from_str::<T>(&text) {
            Ok(decoded) => return Ok(decoded),
            Err(error) => error.column(),
        };
        let Some((path, _)) = occurrences
            .into_iter()
            .min_by_key(|(_, end)| end.abs_diff(column))
        else {
            break;
        };
        let kind = unknown_name(&last.as_ref().map(ToString::to_string).unwrap_or_default())
            .map(|(kind, _)| kind)
            .unwrap_or(Unknown::Field);
        if !drop_smallest_part::<T>(value, &path, kind) {
            break;
        }
    }
    Err(last.unwrap_or_else(|| serde::de::Error::custom("stored data could not be repaired")))
}

#[derive(Clone, Copy, PartialEq)]
enum Unknown {
    Field,
    Variant,
}

#[derive(Clone)]
enum Step {
    Key(String),
    Index(usize),
}

fn unknown_name(message: &str) -> Option<(Unknown, &str)> {
    for (prefix, kind) in [
        ("unknown field `", Unknown::Field),
        ("unknown variant `", Unknown::Variant),
    ] {
        if let Some(rest) = message.split_once(prefix).map(|(_, rest)| rest) {
            return rest.split_once('`').map(|(name, _)| (kind, name));
        }
    }
    None
}

/// Removes the field (or a single-key variant object), or failing that the
/// enclosing list entry from the innermost out. A cut that leaves a missing
/// required field is undone and the next larger part is tried.
fn drop_smallest_part<T: DeserializeOwned>(
    value: &mut Value,
    path: &[Step],
    kind: Unknown,
) -> bool {
    for cut in candidates(path, kind) {
        let mut trial = value.clone();
        if !remove(&mut trial, &path[..cut]) {
            continue;
        }
        let missing = T::deserialize(&trial)
            .err()
            .is_some_and(|error| error.to_string().starts_with("missing field"));
        if !missing {
            *value = trial;
            return true;
        }
    }
    false
}

#[derive(Default)]
struct Located {
    text: String,
    /// Each occurrence's path and the byte offset where its token ends.
    occurrences: Vec<(Vec<Step>, usize)>,
}

/// Writes compact JSON and records where `name` appears: an object key for a
/// field; a string value or a single-key object for a variant.
fn write(value: &Value, kind: Unknown, name: &str, path: &mut Vec<Step>, out: &mut Located) {
    match value {
        Value::Object(map) => {
            out.text.push('{');
            for (index, (key, child)) in map.iter().enumerate() {
                if index > 0 {
                    out.text.push(',');
                }
                path.push(Step::Key(key.clone()));
                out.text.push_str(&Value::String(key.clone()).to_string());
                let tagged = kind == Unknown::Variant && map.len() == 1 && key == name;
                if (kind == Unknown::Field && key == name) || tagged {
                    out.occurrences.push((path.clone(), out.text.len()));
                }
                out.text.push(':');
                write(child, kind, name, path, out);
                path.pop();
            }
            out.text.push('}');
        }
        Value::Array(items) => {
            out.text.push('[');
            for (index, child) in items.iter().enumerate() {
                if index > 0 {
                    out.text.push(',');
                }
                path.push(Step::Index(index));
                write(child, kind, name, path, out);
                path.pop();
            }
            out.text.push(']');
        }
        Value::String(text) => {
            out.text.push_str(&value.to_string());
            if kind == Unknown::Variant && text == name {
                out.occurrences.push((path.clone(), out.text.len()));
            }
        }
        _ => out.text.push_str(&value.to_string()),
    }
}

/// Prefix lengths to cut, smallest part first: the field itself (for a
/// single-key variant object, also its slot in the parent), then each
/// enclosing list entry from the innermost out.
fn candidates(path: &[Step], kind: Unknown) -> Vec<usize> {
    let mut cuts = Vec::new();
    if matches!(path.last(), Some(Step::Key(_))) {
        cuts.push(path.len());
        if kind == Unknown::Variant
            && path.len() >= 2
            && matches!(path[path.len() - 2], Step::Key(_))
        {
            cuts.push(path.len() - 1);
        }
    }
    for (index, step) in path.iter().enumerate().rev() {
        if matches!(step, Step::Index(_)) {
            cuts.push(index + 1);
        }
    }
    cuts.dedup();
    cuts
}

fn remove(value: &mut Value, path: &[Step]) -> bool {
    let Some((last, parents)) = path.split_last() else {
        return false;
    };
    let mut node = value;
    for step in parents {
        node = match (step, node) {
            (Step::Key(key), Value::Object(map)) => match map.get_mut(key) {
                Some(child) => child,
                None => return false,
            },
            (Step::Index(index), Value::Array(items)) => match items.get_mut(*index) {
                Some(child) => child,
                None => return false,
            },
            _ => return false,
        };
    }
    match (last, node) {
        (Step::Key(key), Value::Object(map)) => map.remove(key).is_some(),
        (Step::Index(index), Value::Array(items)) if *index < items.len() => {
            items.remove(*index);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    enum Reason {
        #[default]
        Manual,
        Folder,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Word {
        phrase: String,
        #[serde(default)]
        reason: Reason,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
    enum Source {
        Typed { text: String },
        Spoken { text: String },
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Profile {
        name: String,
        #[serde(default)]
        note: String,
        #[serde(default)]
        reason: Reason,
        words: Vec<Word>,
        #[serde(default)]
        sources: Vec<Source>,
    }

    fn decode_json(value: serde_json::Value) -> serde_json::Result<Profile> {
        decode(&value.to_string())
    }

    #[test]
    fn current_data_decodes_unchanged() {
        let profile = json!({"name": "a", "reason": "folder", "words": [{"phrase": "Vesper"}]});
        assert_eq!(decode_json(profile.clone()).unwrap().reason, Reason::Folder);
    }

    #[test]
    fn unknown_fields_are_dropped_at_any_depth() {
        let profile = decode_json(json!({
            "name": "a",
            "settingFromANewerVersion": true,
            "words": [{"phrase": "Vesper", "weight": 3}, {"phrase": "Ember", "weight": 1}],
        }))
        .unwrap();
        assert_eq!(profile.name, "a");
        assert_eq!(profile.words.len(), 2);
    }

    #[test]
    fn a_known_field_with_the_same_name_elsewhere_is_kept() {
        // `phrase` is known in a word; only the profile's unknown copy goes.
        let profile = decode_json(json!({
            "name": "a",
            "phrase": "newer",
            "words": [{"phrase": "Vesper"}],
        }))
        .unwrap();
        assert_eq!(profile.words[0].phrase, "Vesper");
    }

    #[test]
    fn unknown_variants_fall_back_to_the_default_or_drop_their_entry() {
        let profile = decode_json(json!({
            "name": "category",
            "note": "category",
            "reason": "category",
            "words": [{"phrase": "Vesper", "reason": "category"}],
            "sources": [{"kind": "pasted", "text": "x"}, {"kind": "typed", "text": "y"}],
        }))
        .unwrap();
        // A free-text value that happens to match is never touched.
        assert_eq!(profile.name, "category");
        assert_eq!(profile.note, "category");
        assert_eq!(profile.reason, Reason::Manual);
        assert_eq!(profile.words[0].reason, Reason::Manual);
        assert_eq!(profile.sources, vec![Source::Typed { text: "y".into() }]);
    }

    #[test]
    fn errors_it_cannot_repair_are_reported_as_the_strict_error() {
        let missing = decode_json(json!({"words": []})).unwrap_err();
        assert!(missing.to_string().contains("missing field `name`"));
        assert!(decode::<Profile>("{not json").is_err());
        // A required field with an unknown value has no smaller part to drop.
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Required {
            #[allow(dead_code)]
            reason: Reason,
        }
        assert!(decode::<Required>(r#"{"reason":"category"}"#).is_err());
    }
}
