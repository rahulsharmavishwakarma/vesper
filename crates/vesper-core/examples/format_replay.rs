//! Replays `format_dictation` over transcripts to review what it would change.
//! Input: one JSON string per line on stdin. Output: JSON lines for changed
//! transcripts only, with the rules that fired. Stays local; prints user text.
use std::io::BufRead;

fn main() {
    let (mut total, mut changed) = (0, 0);
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        let Ok(text) = serde_json::from_str::<String>(&line) else {
            continue;
        };
        total += 1;
        let out = vesper_core::format_dictation(&text);
        if out.text != text {
            changed += 1;
            println!(
                "{}",
                serde_json::json!({"before": text, "after": out.text, "rules": out.rules})
            );
        }
    }
    eprintln!("{changed} of {total} transcripts changed");
}
