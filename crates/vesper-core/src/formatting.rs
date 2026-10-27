//! Deterministic dictation formatting (English), the same for every recognizer:
//! hesitations and stutters, spoken self-corrections between like values,
//! emails and web addresses, times, dates, money, percentages and numbers.
//! Rules only rewrite spans they fully recognize; anything else is untouched.
//! See docs/personalization/formatting.md.

/// What one `format_dictation` call changed, by rule name, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Formatted {
    pub text: String,
    pub rules: Vec<&'static str>,
}

#[derive(Debug, Clone)]
struct Tok {
    /// Whitespace before this token in the output.
    space: String,
    lead: String,
    core: String,
    trail: String,
}

impl Tok {
    fn lower(&self) -> String {
        self.core.to_lowercase()
    }
}

fn tokenize(text: &str) -> Vec<Tok> {
    let mut toks = vec![];
    let mut space = String::new();
    let mut word = String::new();
    let flush = |word: &mut String, space: &mut String, toks: &mut Vec<Tok>| {
        if word.is_empty() {
            return;
        }
        let start = word
            .char_indices()
            .find(|(_, c)| c.is_alphanumeric() || matches!(c, '$' | '₹' | '€' | '£'))
            .map_or(word.len(), |(i, _)| i);
        let end = word
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_alphanumeric() || *c == '%')
            .map_or(start, |(i, c)| i + c.len_utf8())
            .max(start);
        toks.push(Tok {
            space: std::mem::take(space),
            lead: word[..start].to_owned(),
            core: word[start..end].to_owned(),
            trail: word[end..].to_owned(),
        });
        word.clear();
    };
    for c in text.chars() {
        if c.is_whitespace() {
            flush(&mut word, &mut space, &mut toks);
            space.push(c);
        } else {
            word.push(c);
        }
    }
    flush(&mut word, &mut space, &mut toks);
    toks
}

fn render(toks: &[Tok]) -> String {
    let mut out = String::new();
    for (i, t) in toks.iter().enumerate() {
        if i > 0 {
            out.push_str(if t.space.is_empty() { " " } else { &t.space });
        } else {
            out.push_str(&t.space);
        }
        out.push_str(&t.lead);
        out.push_str(&t.core);
        out.push_str(&t.trail);
    }
    out
}

/// Replace `toks[start..end]` with one token carrying `core`, keeping the first
/// token's leading space and punctuation and the last token's trailing punctuation.
fn splice(toks: &mut Vec<Tok>, start: usize, end: usize, core: String) {
    let tok = Tok {
        space: toks[start].space.clone(),
        lead: toks[start].lead.clone(),
        core,
        trail: toks[end - 1].trail.clone(),
    };
    toks.splice(start..end, [tok]);
}

/// Tokens joined by punctuation that ends a clause cannot be one spoken unit.
fn breaks(t: &Tok) -> bool {
    t.trail.contains([',', '.', ';', ':', '!', '?'])
}

const FILLERS: &[&str] = &["um", "uh", "erm", "er", "uhm", "umm", "hmm", "uhh"];

fn small(word: &str) -> Option<u64> {
    let v = match word {
        "zero" | "oh" => 0,
        "one" => 1,
        "two" => 2,
        "three" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        "ten" => 10,
        "eleven" => 11,
        "twelve" => 12,
        "thirteen" => 13,
        "fourteen" => 14,
        "fifteen" => 15,
        "sixteen" => 16,
        "seventeen" => 17,
        "eighteen" => 18,
        "nineteen" => 19,
        _ => return None,
    };
    Some(v)
}

fn tens(word: &str) -> Option<u64> {
    let v = match word {
        "twenty" => 20,
        "thirty" => 30,
        "forty" => 40,
        "fifty" => 50,
        "sixty" => 60,
        "seventy" => 70,
        "eighty" => 80,
        "ninety" => 90,
        _ => return None,
    };
    Some(v)
}

fn ordinal_small(word: &str) -> Option<u64> {
    let v = match word {
        "first" => 1,
        "second" => 2,
        "third" => 3,
        "fourth" => 4,
        "fifth" => 5,
        "sixth" => 6,
        "seventh" => 7,
        "eighth" => 8,
        "ninth" => 9,
        "tenth" => 10,
        "eleventh" => 11,
        "twelfth" => 12,
        "thirteenth" => 13,
        "fourteenth" => 14,
        "fifteenth" => 15,
        "sixteenth" => 16,
        "seventeenth" => 17,
        "eighteenth" => 18,
        "nineteenth" => 19,
        "twentieth" => 20,
        "thirtieth" => 30,
        _ => return None,
    };
    Some(v)
}

/// A spoken whole number below 100 at `toks[i..]` (`twenty one`, `twenty-one`,
/// `seven`): (value, tokens used).
fn under_hundred(toks: &[Tok], i: usize) -> Option<(u64, usize)> {
    let word = toks.get(i)?.lower();
    if let Some((a, b)) = word.split_once('-') {
        return Some((tens(a)? + small(b).filter(|v| (1..10).contains(v))?, 1));
    }
    if let Some(v) = small(&word).filter(|_| word != "oh") {
        return Some((v, 1));
    }
    let t = tens(&word)?;
    if !breaks(&toks[i]) {
        if let Some(u) = toks
            .get(i + 1)
            .and_then(|n| small(&n.lower()))
            .filter(|v| (1..10).contains(v))
        {
            return Some((t + u, 2));
        }
    }
    Some((t, 1))
}

/// A spoken cardinal at `toks[i..]`: (value, tokens used). Accepts hundreds and
/// thousand/million/billion scales; stops at the first word that is not part of it.
fn cardinal(toks: &[Tok], i: usize) -> Option<(u64, usize)> {
    let (mut total, mut current, mut j, mut any) = (0u64, 0u64, i, false);
    loop {
        if j > i && breaks(&toks[j - 1]) {
            break;
        }
        let Some(tok) = toks.get(j) else { break };
        let word = tok.lower();
        let scale = match word.as_str() {
            "hundred" => Some(100),
            "thousand" => Some(1_000),
            "million" => Some(1_000_000),
            "billion" => Some(1_000_000_000),
            _ => None,
        };
        match scale {
            Some(100) if current > 0 && current < 100 => {
                current *= 100;
                j += 1;
            }
            Some(s) if s > 100 && (current > 0 || total > 0) && current < 1000 => {
                if current == 0 {
                    break;
                }
                total += current * s;
                current = 0;
                j += 1;
            }
            Some(_) => break,
            None => {
                // A new group may follow only a scale word or a hundred (`two
                // hundred five`), never another bare number (`four fifteen`).
                let after_scale = j > i
                    && matches!(
                        toks[j - 1].lower().as_str(),
                        "hundred" | "thousand" | "million" | "billion"
                    );
                if any && !after_scale {
                    break;
                }
                if after_scale && word == "and" {
                    j += 1;
                    continue;
                }
                let Some((v, used)) = under_hundred(toks, j) else {
                    break;
                };
                if current % 100 != 0
                    || (after_scale && current != 0 && current % 100 == 0 && v >= 100)
                {
                    break;
                }
                current += v;
                j += used;
            }
        }
        any = true;
    }
    // Trailing `and` is never part of the number.
    while j > i && toks[j - 1].lower() == "and" {
        j -= 1;
    }
    (any && j > i).then_some((total + current, j - i))
}

/// Spoken digits after `point`: `point five`, `point two five`.
fn decimals(toks: &[Tok], i: usize) -> Option<(String, usize)> {
    if toks.get(i)?.lower() != "point" || breaks(&toks[i]) {
        return None;
    }
    let mut digits = String::new();
    let mut j = i + 1;
    while let Some(v) = toks
        .get(j)
        .and_then(|t| small(&t.lower()))
        .filter(|v| *v < 10)
    {
        digits.push_str(&v.to_string());
        j += 1;
        if breaks(&toks[j - 1]) {
            break;
        }
    }
    (!digits.is_empty()).then_some((digits, j - i))
}

fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (k, c) in digits.chars().enumerate() {
        if k > 0 && (digits.len() - k) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A full spoken number (cardinal with optional decimals) at `toks[i..]`:
/// (written form, tokens used, value).
fn spoken_number(toks: &[Tok], i: usize) -> Option<(String, usize, f64)> {
    let (value, used) = cardinal(toks, i)?;
    if let Some((digits, more)) = (!breaks(&toks[i + used - 1]))
        .then(|| decimals(toks, i + used))
        .flatten()
    {
        let written = format!("{value}.{digits}");
        let parsed = written.parse().unwrap_or(0.0);
        return Some((written, used + more, parsed));
    }
    Some((grouped(value), used, value as f64))
}

const WEEKDAYS: &[&str] = &[
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];
const MONTHS: &[&str] = &[
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

fn month(word: &str) -> bool {
    MONTHS.contains(&word)
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn suffix(n: u64) -> &'static str {
    match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    }
}

/// A spoken ordinal (`fifteenth`, `twenty first`): (value, tokens used).
fn ordinal(toks: &[Tok], i: usize) -> Option<(u64, usize)> {
    let word = toks.get(i)?.lower();
    if let Some(v) = ordinal_small(&word) {
        return Some((v, 1));
    }
    if let Some((a, b)) = word.split_once('-') {
        return Some((tens(a)? + ordinal_small(b).filter(|v| *v < 10)?, 1));
    }
    let t = tens(&word)?;
    let next = toks.get(i + 1)?.lower();
    let u = ordinal_small(&next).filter(|v| *v < 10)?;
    (!breaks(&toks[i])).then_some((t + u, 2))
}

/// Class of a value a spoken self-correction may replace.
#[derive(PartialEq)]
enum Kind {
    Weekday,
    Month,
    Number,
}

/// The value starting at `i`: its kind and token length.
fn value_at(toks: &[Tok], i: usize) -> Option<(Kind, usize)> {
    let word = toks.get(i)?.lower();
    if WEEKDAYS.contains(&word.as_str()) {
        return Some((Kind::Weekday, 1));
    }
    if month(&word) && word != "may" {
        return Some((Kind::Month, 1));
    }
    let digits = |t: &Tok| {
        let c = t.core.trim_start_matches(['$', '₹', '€', '£']);
        !c.is_empty()
            && c.chars()
                .all(|c| c.is_ascii_digit() || c == ',' || c == '.')
    };
    if digits(&toks[i]) {
        return Some((Kind::Number, 1));
    }
    let (_, used, _) = spoken_number(toks, i)?;
    // `one` alone is usually a pronoun, not a value.
    if used == 1 && word == "one" {
        return None;
    }
    let unit = toks.get(i + used).is_some_and(|t| {
        matches!(
            t.lower().as_str(),
            "dollars" | "dollar" | "rupees" | "percent" | "euros"
        )
    });
    Some((
        Kind::Number,
        used + usize::from(unit && !breaks(&toks[i + used - 1])),
    ))
}

/// The value ending right before `end`: its kind and start.
fn value_before(toks: &[Tok], end: usize) -> Option<(Kind, usize)> {
    (end.saturating_sub(6)..end)
        .filter_map(|s| {
            value_at(toks, s)
                .filter(|(_, len)| s + len == end)
                .map(|(k, _)| (k, s))
        })
        .next()
}

fn cue_len(toks: &[Tok], i: usize) -> Option<usize> {
    let w = |k: usize| toks.get(i + k).map(Tok::lower).unwrap_or_default();
    let phrases: &[&[&str]] = &[
        &["actually", "make", "it"],
        &["actually", "make", "that"],
        &["no", "make", "it"],
        &["sorry", "make", "it"],
        &["i", "mean"],
        &["or", "rather"],
        &["make", "that"],
        &["sorry"],
        &["actually"],
        &["no"],
    ];
    phrases
        .iter()
        .find(|p| p.iter().enumerate().all(|(k, word)| w(k) == *word))
        .map(|p| p.len())
}

fn self_corrections(toks: &mut Vec<Tok>, rules: &mut Vec<&'static str>) {
    let mut i = 1;
    while i < toks.len() {
        let Some(cue) = cue_len(toks, i) else {
            i += 1;
            continue;
        };
        let replaced = value_before(toks, i).and_then(|(kind, start)| {
            value_at(toks, i + cue)
                .filter(|(k, _)| *k == kind)
                .map(|(_, len)| (start, len))
        });
        // Never across a sentence or clause boundary before the cue.
        if let Some((start, len)) =
            replaced.filter(|_| !breaks(&toks[i - 1]) || toks[i - 1].trail == ",")
        {
            let keep: Vec<Tok> = toks[i + cue..i + cue + len].to_vec();
            let mut first = keep[0].clone();
            first.space = toks[start].space.clone();
            first.lead = toks[start].lead.clone();
            let mut keep = keep;
            keep[0] = first;
            toks.splice(start..i + cue + len, keep);
            rules.push("self-correction");
            i = start + 1;
        } else {
            i += 1;
        }
    }
}

fn disfluencies(toks: &mut Vec<Tok>, rules: &mut Vec<&'static str>) {
    let mut i = 0;
    while i < toks.len() {
        let word = toks[i].lower();
        if FILLERS.contains(&word.as_str()) && toks[i].lead.is_empty() {
            let trail = toks[i].trail.trim_start_matches(',').to_owned();
            let space = toks[i].space.clone();
            toks.remove(i);
            let sentence_start = i == 0 || breaks_sentence(&toks_prev(&toks[..i]));
            if let Some(next) = toks.get_mut(i) {
                next.space = space;
                if sentence_start {
                    next.core = capitalize(&next.core);
                }
            } else if let Some(prev) = toks.last_mut() {
                prev.trail.push_str(&trail);
            }
            rules.push("filler");
            continue;
        }
        if i + 1 < toks.len() && !breaks(&toks[i]) {
            let next = toks[i + 1].lower();
            let latin = word.chars().all(|c| c.is_ascii_alphabetic() || c == '\'');
            // A repeated capitalized word mid-sentence may be a name (`Rabu Rabu`).
            let name = toks[i].core.chars().next().is_some_and(char::is_uppercase)
                && toks[i + 1]
                    .core
                    .chars()
                    .next()
                    .is_some_and(char::is_uppercase)
                && i > 0
                && !toks[i - 1].trail.ends_with(['.', '!', '?'])
                && word != "i";
            let repeat = word == next
                && latin
                && !word.is_empty()
                && !name
                && !matches!(word.as_str(), "that" | "had" | "very" | "bye" | "no" | "so");
            // A cut-off start of the next word (`inf inspiration`).
            let stub = word.len() >= 2
                && next.len() > word.len() + 2
                && next.starts_with(&word)
                && !crate::ordinary_word(&word)
                && word.chars().all(|c| c.is_ascii_lowercase());
            if repeat || stub {
                let space = toks[i].space.clone();
                let lead = toks[i].lead.clone();
                let upper = toks[i].core.chars().next().is_some_and(char::is_uppercase);
                toks.remove(i);
                toks[i].space = space;
                toks[i].lead = lead;
                // The kept word takes the removed word's capitalization (`is Is` → `is`).
                let kept = &toks[i].core;
                let acronym = kept.len() > 1 && kept.chars().all(|c| !c.is_lowercase());
                if upper {
                    toks[i].core = capitalize(kept);
                } else if !acronym && kept != "I" {
                    let mut chars = kept.chars();
                    toks[i].core = chars
                        .next()
                        .map(|c| c.to_lowercase().chain(chars).collect())
                        .unwrap_or_default();
                }
                rules.push(if repeat { "repeat" } else { "stutter" });
                continue;
            }
        }
        i += 1;
    }
}

fn toks_prev(toks: &[Tok]) -> Option<&Tok> {
    toks.last()
}

fn breaks_sentence(prev: &Option<&Tok>) -> bool {
    prev.is_none_or(|t| t.trail.ends_with(['.', '!', '?']))
}

const TLDS: &[&str] = &[
    "com", "org", "net", "io", "ai", "dev", "app", "co", "in", "edu", "gov", "uk", "us", "me",
    "info", "xyz",
];
const STOP: &[&str] = &[
    "the", "a", "an", "is", "are", "was", "to", "at", "on", "in", "of", "for", "and", "or", "go",
    "open", "visit", "see", "check", "page", "site", "from", "my", "your", "our", "its", "it",
    "me", "us", "we", "i", "you", "them", "him", "her", "be", "this", "that", "with", "send", "cc",
    "email", "mail", "address", "url", "link", "website", "contact",
];

fn spoken_join(word: &str) -> Option<&'static str> {
    match word {
        "dot" => Some("."),
        "underscore" => Some("_"),
        "dash" | "hyphen" => Some("-"),
        _ => None,
    }
}

/// A written domain-ish core (`gmail.com`, `Fermianresearch.com`).
fn written_host(core: &str) -> bool {
    core.rsplit_once('.')
        .is_some_and(|(name, tld)| !name.is_empty() && TLDS.contains(&tld.to_lowercase().as_str()))
}

/// Parse a host at `i`: words joined by spoken or written dots, ending in a known
/// top-level domain. Returns (host, tokens used).
fn host_at(toks: &[Tok], i: usize) -> Option<(String, usize)> {
    let mut parts: Vec<String> = vec![];
    let mut j = i;
    let mut has_dot = false;
    while let Some(t) = toks.get(j) {
        let word = t.lower();
        if spoken_join(&word) == Some(".") && !parts.is_empty() {
            parts.push(".".into());
            has_dot = true;
        } else if word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
            && !word.is_empty()
            && (parts.is_empty() || parts.last().is_some_and(|p| p == "."))
        {
            has_dot |= word.contains('.');
            parts.push(word);
        } else {
            break;
        }
        j += 1;
        if breaks(&toks[j - 1]) {
            break;
        }
    }
    while parts.last().is_some_and(|p| p == ".") {
        parts.pop();
        j -= 1;
    }
    let host: String = parts.concat();
    (has_dot && written_host(&host)).then_some((host, j - i))
}

/// Spoken path after a host: `slash vesper slash docs`.
fn path_at(toks: &[Tok], mut j: usize, host_end_break: bool) -> (String, usize) {
    let start = j;
    let mut path = String::new();
    if host_end_break {
        return (path, 0);
    }
    while toks.get(j).is_some_and(|t| t.lower() == "slash") && !breaks(&toks[j]) {
        let Some(seg) = toks.get(j + 1).filter(|t| {
            t.core
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        }) else {
            break;
        };
        path.push('/');
        path.push_str(&seg.lower());
        j += 2;
        if breaks(&toks[j - 1]) {
            break;
        }
    }
    (path, j - start)
}

fn addresses(toks: &mut Vec<Tok>, rules: &mut Vec<&'static str>) {
    // Emails: local part, `at`, host.
    let mut i = 0;
    while i < toks.len() {
        if toks[i].lower() != "at" || i == 0 || breaks(&toks[i - 1]) {
            i += 1;
            continue;
        }
        let Some((host, used)) = host_at(toks, i + 1) else {
            i += 1;
            continue;
        };
        // Local part: one word, extended back over spoken joiners.
        let mut start = i - 1;
        let mut connected = false;
        while start >= 2
            && spoken_join(&toks[start - 1].lower()).is_some()
            && !breaks(&toks[start - 2])
            && !STOP.contains(&toks[start - 2].lower().as_str())
        {
            start -= 2;
            connected = true;
        }
        let local_word = toks[start].lower();
        let cue = start > 0
            && matches!(
                toks[start - 1].lower().as_str(),
                "to" | "cc" | "bcc" | "email" | "mail" | "is" | "contact" | "address"
            );
        let plain = local_word
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '_');
        if !plain || STOP.contains(&local_word.as_str()) || !(connected || cue) {
            i += 1;
            continue;
        }
        let local: String = toks[start..i]
            .iter()
            .map(|t| {
                let w = t.lower();
                spoken_join(&w).map_or(w, str::to_owned)
            })
            .collect();
        splice(toks, start, i + 1 + used, format!("{local}@{host}"));
        rules.push("email");
        i = start + 1;
    }
    // Web addresses: host (spoken dots, or a written host followed by a spoken path).
    let mut i = 0;
    while i < toks.len() {
        if toks[i].core.contains('@') || STOP.contains(&toks[i].lower().as_str()) {
            i += 1;
            continue;
        }
        // Join leading words of a multi-word name (`fermion research dot com`).
        let mut name_start = i;
        let Some((host, used)) = host_at(toks, i) else {
            i += 1;
            continue;
        };
        let spoken = toks[i..i + used].iter().any(|t| t.lower() == "dot");
        let (path, path_used) = path_at(toks, i + used, breaks(&toks[i + used - 1]));
        if !spoken && path_used == 0 {
            i += 1;
            continue;
        }
        let mut prefix = String::new();
        if spoken {
            while name_start > 0
                && !breaks(&toks[name_start - 1])
                && !STOP.contains(&toks[name_start - 1].lower().as_str())
                && toks[name_start - 1].core.chars().all(|c| c.is_alphabetic())
                && i - name_start < 2
            {
                name_start -= 1;
            }
            prefix = toks[name_start..i].iter().map(Tok::lower).collect();
        }
        splice(
            toks,
            name_start,
            i + used + path_used,
            format!("{prefix}{host}{path}"),
        );
        rules.push("web address");
        i = name_start + 1;
    }
}

fn meridiem(toks: &[Tok], i: usize) -> Option<(&'static str, usize)> {
    let w = |k: usize| toks.get(i + k).map(Tok::lower).unwrap_or_default();
    let one = w(0).replace('.', "");
    match one.as_str() {
        "am" => Some(("AM", 1)),
        "pm" => Some(("PM", 1)),
        "a" if w(1).replace('.', "") == "m" => Some(("AM", 2)),
        "p" if w(1).replace('.', "") == "m" => Some(("PM", 2)),
        _ => None,
    }
}

/// An hour (1–12) at `i`, spoken or digits: (hour, tokens used).
fn hour_at(toks: &[Tok], i: usize) -> Option<(u64, usize)> {
    let t = toks.get(i)?;
    if let Ok(h) = t.core.parse::<u64>() {
        return (1..=12).contains(&h).then_some((h, 1));
    }
    let (h, used) = under_hundred(toks, i).filter(|(_, used)| *used == 1)?;
    (1..=12).contains(&h).then_some((h, used))
}

fn minutes_at(toks: &[Tok], i: usize) -> Option<(u64, usize)> {
    let t = toks.get(i)?;
    if t.core.len() == 2 {
        if let Ok(m) = t.core.parse::<u64>() {
            return (m < 60).then_some((m, 1));
        }
    }
    if t.lower() == "oh" {
        let (m, used) = under_hundred(toks, i + 1)?;
        return (m < 10).then_some((m, used + 1));
    }
    let (m, used) = under_hundred(toks, i)?;
    (10..60).contains(&m).then_some((m, used))
}

/// `30am` / `10pm`: digits with the meridiem written on.
fn glued(core: &str) -> Option<(u64, &'static str)> {
    let lower = core.to_lowercase().replace('.', "");
    let (digits, mer) = if let Some(d) = lower.strip_suffix("am") {
        (d, "AM")
    } else {
        (lower.strip_suffix("pm")?, "PM")
    };
    Some((digits.parse().ok()?, mer))
}

fn times(toks: &mut Vec<Tok>, rules: &mut Vec<&'static str>) {
    let mut i = 0;
    while i < toks.len() {
        // `10am`, and `10 30am` with the meridiem glued to the minutes.
        if let Some((h, m)) = glued(&toks[i].core).filter(|(h, _)| (1..=12).contains(h)) {
            splice(toks, i, i + 1, format!("{h}:00 {m}"));
            rules.push("time");
            i += 1;
            continue;
        }
        if let (Some((h, 1)), Some((min, m))) = (
            hour_at(toks, i),
            toks.get(i + 1)
                .and_then(|t| glued(&t.core))
                .filter(|(min, _)| *min < 60),
        ) {
            if !breaks(&toks[i]) && toks[i + 1].core.len() >= 4 {
                splice(toks, i, i + 2, format!("{h}:{min:02} {m}"));
                rules.push("time");
                i += 1;
                continue;
            }
        }
        // Already written `10:30am` / `10:30 a.m.`: normalize the meridiem only.
        let Some((hour, used)) = hour_at(toks, i) else {
            i += 1;
            continue;
        };
        let mut j = i + used;
        let mut minutes = None;
        if !breaks(&toks[j - 1]) {
            if let Some((m, more)) = minutes_at(toks, j) {
                minutes = Some(m);
                j += more;
            }
        }
        let clock = toks
            .get(j)
            .is_some_and(|t| t.lower() == "o'clock" || t.lower() == "o’clock");
        let mer = (!breaks(&toks[j - 1])).then(|| meridiem(toks, j)).flatten();
        if !(clock || mer.is_some()) || (clock && minutes.is_some()) {
            i += 1;
            continue;
        }
        let end = j + if clock { 1 } else { mer.map_or(0, |(_, n)| n) };
        let written = match mer {
            Some((m, _)) => format!("{hour}:{:02} {m}", minutes.unwrap_or(0)),
            None => format!("{hour}:00"),
        };
        // `p.m.` ends with the abbreviation's own period, not a sentence's,
        // unless a capital or the end of the text follows.
        let dotted = toks[end - 1].core.contains('.') && toks[end - 1].trail.starts_with('.');
        let continues = toks
            .get(end)
            .is_some_and(|t| t.core.chars().next().is_some_and(char::is_lowercase));
        splice(toks, i, end, written);
        if dotted && continues {
            toks[i].trail.remove(0);
        }
        rules.push("time");
        i += 1;
    }
}

fn dates(toks: &mut Vec<Tok>, rules: &mut Vec<&'static str>) {
    let mut i = 0;
    while i < toks.len() {
        let word = toks[i].lower();
        // `the fifteenth of october` → `the 15th of October`
        if let Some((n, used)) = ordinal(toks, i) {
            let of_month = toks.get(i + used).is_some_and(|t| t.lower() == "of")
                && toks.get(i + used + 1).is_some_and(|t| month(&t.lower()));
            let after_month = i > 0 && month(&toks[i - 1].lower()) && toks[i - 1].lower() != "may";
            let on_the =
                i >= 2 && toks[i - 1].lower() == "the" && toks[i - 2].lower() == "on" && n > 10;
            if of_month || after_month || on_the {
                splice(toks, i, i + used, format!("{n}{}", suffix(n)));
                rules.push("date");
            }
        }
        // `March 3, 2027` / `march third twenty twenty seven`
        if month(&word) && word != "may"
            || (word == "may"
                && toks.get(i + 1).is_some_and(|t| {
                    ordinal(toks, i + 1).is_some() || t.core.parse::<u64>().is_ok()
                }))
        {
            toks[i].core = capitalize(&toks[i].core);
            // `March 3,2027` (no space after the comma): split day and year.
            if let Some((d, y)) =
                toks.get(i + 1)
                    .and_then(|t| t.core.split_once(','))
                    .filter(|(d, y)| {
                        d.parse::<u64>().is_ok_and(|d| (1..=31).contains(&d))
                            && y.len() == 4
                            && y.parse::<u64>().is_ok()
                    })
            {
                let (d, y) = (d.parse::<u64>().unwrap_or(1), y.to_owned());
                let trail = toks[i + 1].trail.clone();
                toks[i + 1].core = format!("{d}{}", suffix(d));
                toks[i + 1].trail = ",".into();
                toks.insert(
                    i + 2,
                    Tok {
                        space: " ".into(),
                        lead: String::new(),
                        core: y,
                        trail,
                    },
                );
                rules.push("date");
                i += 3;
                continue;
            }
            let day = toks.get(i + 1).and_then(|t| {
                t.core
                    .trim_end_matches(char::is_alphabetic)
                    .parse::<u64>()
                    .ok()
            });
            if day.is_some_and(|d| (1..=31).contains(&d))
                && !toks[i + 1].core.ends_with(char::is_alphabetic)
            {
                let d = day.unwrap_or(0);
                toks[i + 1].core = format!("{d}{}", suffix(d));
                rules.push("date");
            }
            // A spoken year after the day: `twenty twenty seven`.
            if let Some(k) = (i + 2 < toks.len()).then_some(i + 2) {
                if let (Some((a, ua)), true) = (
                    under_hundred(toks, k),
                    !breaks(&toks[k - 1]) || toks[k - 1].trail == ",",
                ) {
                    if (19..=20).contains(&a) || a == 20 {
                        if let Some((b, ub)) = under_hundred(toks, k + ua).filter(|(b, _)| *b < 100)
                        {
                            let year = a * 100 + b;
                            if !toks[i + 1].trail.contains(',') {
                                toks[i + 1].trail.push(',');
                            }
                            splice(toks, k, k + ua + ub, year.to_string());
                            rules.push("date");
                        }
                    }
                }
            }
        }
        i += 1;
    }
}

fn numbers(toks: &mut Vec<Tok>, rules: &mut Vec<&'static str>) {
    let mut i = 0;
    while i < toks.len() {
        let Some((written, used, value)) = spoken_number(toks, i) else {
            i += 1;
            continue;
        };
        let mut end = i + used;
        // `one point two million`: a decimal before a scale word keeps the word.
        let mut written = written;
        let mut scaled = false;
        if written.contains('.')
            && !breaks(&toks[end - 1])
            && toks
                .get(end)
                .is_some_and(|t| matches!(t.lower().as_str(), "million" | "billion"))
        {
            written = format!("{written} {}", toks[end].lower());
            end += 1;
            scaled = true;
        }
        let used = end - i;
        let next = toks.get(end).map(Tok::lower).unwrap_or_default();
        let joined = !breaks(&toks[end - 1]);
        // Two numbers in a row (`forty nine ninety nine`) are ambiguous: keep words.
        if let Some((_, more, _)) = joined.then(|| spoken_number(toks, end)).flatten() {
            i = end + more;
            continue;
        }
        let scale_word = toks[i..end]
            .iter()
            .any(|t| matches!(t.lower().as_str(), "million" | "billion"));
        let money = joined && matches!(next.as_str(), "dollars" | "dollar" | "bucks");
        let rupees = joined && matches!(next.as_str(), "rupees" | "rupee");
        let euros = joined && matches!(next.as_str(), "euros" | "euro");
        let percent = joined && next == "percent";
        let decimal = written.contains('.');
        if money || rupees || euros {
            let symbol = if money {
                "$"
            } else if rupees {
                "₹"
            } else {
                "€"
            };
            // `one point two million dollars` → `$1.2 million`
            let core = if scaled {
                format!("{symbol}{written}")
            } else if scale_word {
                let scale = toks[end - 1].lower();
                let base = spoken_number(&toks[..end - 1], i)
                    .map(|(w, _, _)| w)
                    .unwrap_or(written.clone());
                format!("{symbol}{base} {scale}")
            } else {
                format!("{symbol}{written}")
            };
            splice(toks, i, end + 1, core);
            rules.push("money");
        } else if percent {
            splice(toks, i, end + 1, format!("{written}%"));
            rules.push("percent");
        } else if scaled {
            splice(toks, i, end, written);
            rules.push("number");
        } else if value >= 10.0 || decimal || used > 1 {
            // Words for one to nine stay words, as in ordinary prose.
            if scale_word && value >= 1_000_000.0 {
                i = end;
                continue;
            }
            splice(toks, i, end, written);
            rules.push("number");
        } else {
            i = end;
            continue;
        }
        i += 1;
    }
}

/// Format one English dictation. Returns the text unchanged when no rule applies.
pub fn format_dictation(text: &str) -> Formatted {
    let mut toks = tokenize(text);
    let mut rules = vec![];
    disfluencies(&mut toks, &mut rules);
    self_corrections(&mut toks, &mut rules);
    addresses(&mut toks, &mut rules);
    times(&mut toks, &mut rules);
    dates(&mut toks, &mut rules);
    numbers(&mut toks, &mut rules);
    if rules.is_empty() {
        return Formatted {
            text: text.to_owned(),
            rules,
        };
    }
    if let Some(first) = toks.first_mut() {
        if text
            .trim_start()
            .chars()
            .next()
            .is_some_and(char::is_uppercase)
        {
            first.core = capitalize(&first.core);
        }
    }
    Formatted {
        text: render(&toks),
        rules,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(text: &str) -> String {
        format_dictation(text).text
    }

    /// Detta's published examples and the same kinds, as two recognizers wrote them.
    #[test]
    fn formats_spoken_and_partly_written_forms() {
        let cases = [
            (
                "Send it to john dot smith at gmail dot com.",
                "Send it to john.smith@gmail.com.",
            ),
            (
                "Send it to John.smith at gmail.com.",
                "Send it to john.smith@gmail.com.",
            ),
            (
                "Let's meet on Tuesday no Wednesday at noon.",
                "Let's meet on Wednesday at noon.",
            ),
            (
                "The invoice is two thousand five hundred dollars.",
                "The invoice is $2,500.",
            ),
            (
                "The launch is on the fifteenth of October.",
                "The launch is on the 15th of October.",
            ),
            (
                "Um so basically we we should ship it this week.",
                "So basically we should ship it this week.",
            ),
            (
                "The stand up moves to ten thirty AM.",
                "The stand up moves to 10:30 AM.",
            ),
            (
                "The stand-up moves to 10 30am.",
                "The stand-up moves to 10:30 AM.",
            ),
            (
                "Revenue grew three point five percent last quarter.",
                "Revenue grew 3.5% last quarter.",
            ),
            (
                "The page is fermion research dot com slash detta.",
                "The page is fermionresearch.com/detta.",
            ),
            (
                "My address is priya underscore k at outlook dot com.",
                "My address is priya_k@outlook.com.",
            ),
            (
                "CC sales at acme.io on the thread.",
                "CC sales@acme.io on the thread.",
            ),
            (
                "Book the flight for Friday actually make it Saturday.",
                "Book the flight for Saturday.",
            ),
            (
                "The budget is ten thousand sorry twelve thousand dollars.",
                "The budget is $12,000.",
            ),
            (
                "The budget is $10,000 sorry $12,000.",
                "The budget is $12,000.",
            ),
            (
                "We raised one point two million dollars.",
                "We raised $1.2 million.",
            ),
            (
                "The deadline is March 3, 2027.",
                "The deadline is March 3rd, 2027.",
            ),
            (
                "The deadline is March 3,2027.",
                "The deadline is March 3rd, 2027.",
            ),
            ("See you on the twenty first.", "See you on the 21st."),
            (
                "Uh I think the the build is broken.",
                "I think the build is broken.",
            ),
            (
                "Call me at four fifteen PM tomorrow.",
                "Call me at 4:15 PM tomorrow.",
            ),
            (
                "Call me at 4 15 p.m. tomorrow.",
                "Call me at 4:15 PM tomorrow.",
            ),
            ("The meeting is at nine o'clock.", "The meeting is at 9:00."),
            (
                "Conversion dropped twelve percent.",
                "Conversion dropped 12%.",
            ),
            (
                "Go to github dot com slash vesper.",
                "Go to github.com/vesper.",
            ),
            ("Go to github.com slash vesper.", "Go to github.com/vesper."),
            (
                "So when I I said it was ninety six percent.",
                "So when I said it was 96%.",
            ),
            (
                "We need the transcri transcription now.",
                "We need the transcription now.",
            ),
            ("Uh what is Is that right?", "What is that right?"),
            (
                "Now it looks good, twelve thousand words.",
                "Now it looks good, 12,000 words.",
            ),
        ];
        for (input, want) in cases {
            assert_eq!(f(input), want, "input: {input}");
        }
    }

    /// Ordinary prose the rules must leave exactly as it is.
    #[test]
    fn leaves_ordinary_prose_alone() {
        for text in [
            "I said no to that.",
            "No, I don't think so.",
            "We met at the office on Monday.",
            "That that is fine with me.",
            "One of them was there.",
            "It was the first time we tried.",
            "He works at Google now.",
            "Four of us went home.",
            "I have two dogs and three cats.",
            "Meet me at noon.",
            "It costs forty nine ninety nine a month.",
            "We had had enough by then.",
            "Is it on the second floor?",
            "Look at this dot.",
            "There are twelve of them.",
            "May I come in?",
            "He said sorry to me.",
            "Actually, I agree with you.",
            "We studied the theory and in inside jokes.",
            "Just wanted to give some inf inspiration.",
            "It is Indic AR Rabu Rabu.",
            "تيري ماك به به به ؟",
            "The score was Monday's best.",
        ] {
            let out = format_dictation(text);
            if text == "There are twelve of them." {
                assert_eq!(out.text, "There are 12 of them.");
                continue;
            }
            assert_eq!(out.text, text, "rules: {:?}", out.rules);
        }
    }

    #[test]
    fn records_rule_names_and_is_idempotent() {
        let once = format_dictation("Um call me at ten thirty a m about the two thousand dollars.");
        assert_eq!(once.text, "Call me at 10:30 AM about the $2,000.");
        assert!(once.rules.contains(&"filler") && once.rules.contains(&"time"));
        assert_eq!(f(&once.text), once.text);
    }
}
