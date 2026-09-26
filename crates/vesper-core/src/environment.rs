//! Vesper overrides prefer the current name and retain prior Transcribe aliases.
use std::{env::VarError, ffi::OsString};

fn lookup(
    name: &str,
    nonempty: bool,
    mut read: impl FnMut(&str) -> Option<OsString>,
) -> Option<OsString> {
    let suffix = name.strip_prefix("VESPER_")?;
    read(name)
        .filter(|value| !nonempty || !value.is_empty())
        .or_else(|| read(&format!("TRANSCRIBE_{suffix}")).filter(|v| !nonempty || !v.is_empty()))
}

/// An explicitly set current value, including an empty/non-Unicode one, wins.
pub fn var_os(name: &str) -> Option<OsString> {
    lookup(name, false, |key| std::env::var_os(key))
}

/// Directory overrides historically treat an empty value as absent.
pub fn nonempty_var_os(name: &str) -> Option<OsString> {
    lookup(name, true, |key| std::env::var_os(key))
}

pub fn var(name: &str) -> Result<String, VarError> {
    var_os(name)
        .ok_or(VarError::NotPresent)?
        .into_string()
        .map_err(VarError::NotUnicode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_override_wins_and_legacy_is_only_a_fallback() {
        for (current, legacy, expected) in [
            (Some("new"), Some("old"), Some("new")),
            (None, Some("old"), Some("old")),
            (Some(""), Some("old"), Some("")),
            (None, None, None),
        ] {
            let read = |key: &str| match key {
                "VESPER_TEST" => current.map(OsString::from),
                "TRANSCRIBE_TEST" => legacy.map(OsString::from),
                _ => panic!("unexpected key"),
            };
            assert_eq!(
                lookup("VESPER_TEST", false, read),
                expected.map(OsString::from)
            );
        }
    }

    #[test]
    fn directory_lookup_skips_empty_current_values() {
        assert_eq!(
            lookup("VESPER_MODEL_DIR", true, |key| Some(
                if key == "VESPER_MODEL_DIR" { "" } else { "old" }.into()
            )),
            Some("old".into())
        );
        assert_eq!(
            lookup("PATH", false, |_| panic!("not a Vesper override")),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_current_value_is_not_hidden_by_legacy() {
        use std::os::unix::ffi::OsStringExt;
        let current = OsString::from_vec(vec![0xff]);
        assert_eq!(
            lookup("VESPER_TEST", false, |key| Some(if key == "VESPER_TEST" {
                current.clone()
            } else {
                "old".into()
            })),
            Some(current)
        );
    }
}
