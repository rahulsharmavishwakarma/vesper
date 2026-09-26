//! Portable recording-binding contract; no desktop runtime dependency.
//! Keep accepted keys/modifiers aligned with the frontend recording-shortcuts.ts.

pub fn recording_shortcut_identity(value: &str, mac: bool) -> Option<String> {
    if value.len() > 100 {
        return None;
    }
    let mut tokens = value.split('+').collect::<Vec<_>>();
    let key = tokens.pop()?;
    let valid_key = (key.len() == 1
        && key
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
        || (1..=24).any(|i| key == format!("F{i}"))
        || [
            "Space",
            "ArrowUp",
            "ArrowDown",
            "ArrowLeft",
            "ArrowRight",
            "Home",
            "End",
            "PageUp",
            "PageDown",
            "Backquote",
            "Minus",
            "Equal",
            "BracketLeft",
            "BracketRight",
            "Backslash",
            "Semicolon",
            "Quote",
            "Comma",
            "Period",
            "Slash",
        ]
        .contains(&key);
    if !valid_key || tokens.is_empty() {
        return None;
    }
    let mut modifiers = [false; 4];
    for token in tokens {
        let slot = match token {
            "CommandOrControl" => usize::from(mac),
            "Control" => 0,
            "Super" => 1,
            "Alt" => 2,
            "Shift" => 3,
            _ => return None,
        };
        if modifiers[slot] {
            return None;
        }
        modifiers[slot] = true;
    }
    if !modifiers[..3].contains(&true) {
        return None;
    }
    let mut normalized = ["Control", "Super", "Alt", "Shift"]
        .into_iter()
        .enumerate()
        .filter_map(|(i, name)| modifiers[i].then_some(name))
        .collect::<Vec<_>>();
    normalized.push(key);
    Some(normalized.join("+"))
}

pub fn same_recording_shortcut(left: &str, right: &str) -> bool {
    let left = recording_shortcut_identity(left, cfg!(target_os = "macos"));
    left.is_some() && left == recording_shortcut_identity(right, cfg!(target_os = "macos"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_keys_normalize_platform_modifiers_and_reject_unsupported_chords() {
        for mac in [false, true] {
            let primary = if mac { "Super" } else { "Control" };
            assert_eq!(
                recording_shortcut_identity("Shift+CommandOrControl+K", mac),
                Some(format!("{primary}+Shift+K"))
            );
            for valid in ["Alt+7", "Control+Alt+F24", "Super+Space", "Control+Slash"] {
                assert!(recording_shortcut_identity(valid, mac).is_some(), "{valid}");
            }
            for invalid in [
                "K",
                "Shift+K",
                "Control",
                "Fn",
                "Alt+Escape",
                "Control+Enter",
                "Alt+F25",
                "Alt++K",
                "Ctrl+K",
                "Alt+Alt+K",
                "Alt+K+Shift",
                "Alt+😀",
            ] {
                assert!(
                    recording_shortcut_identity(invalid, mac).is_none(),
                    "{invalid}"
                );
            }
            assert!(
                recording_shortcut_identity(&format!("CommandOrControl+{primary}+K"), mac)
                    .is_none()
            );
        }
    }
}
