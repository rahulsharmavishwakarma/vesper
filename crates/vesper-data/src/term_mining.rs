//! Deterministic term miners over user-asserted and user-authored text. No model,
//! embedding or network. Parsers are pure; the workspace scanner reads only files
//! that git tracks under a root the user opted into and never stores paths.
//! Machine output (ASR, writer, rules) is never an input here.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use vesper_core::{identifier_spoken_form, ordinary_word, TermSource};

use crate::{DatasetError, Result};

pub const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_ASSERTED_BYTES: usize = 64;
const MAX_TERM_CHARS: usize = 48;
const MAX_CONFIG_BYTES: u64 = 2 * 1024 * 1024;
const MAX_ASSERTED_PER_SOURCE: usize = 4096;

/// One mined spelling with its independent evidence count in one scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinedTerm {
    pub surface: String,
    pub source: TermSource,
    /// Workspace scope string; empty for global sources.
    pub scope: String,
    /// Entries (tier A) or distinct files (tier B).
    pub count: u32,
    pub spoken: Vec<String>,
}

impl MinedTerm {
    pub fn new(surface: &str, source: TermSource, scope: &str, count: u32) -> Self {
        Self {
            surface: surface.to_owned(),
            source,
            scope: scope.to_owned(),
            count,
            spoken: identifier_spoken_form(surface).into_iter().collect(),
        }
    }
}

/// A spelling the user typed into a dictionary or replacement list. Rejects
/// sentences, addresses, URLs and numbers; keeps names and short phrases.
pub fn asserted_term(value: &str) -> Option<String> {
    let value = value.trim().trim_start_matches(['!', '+', '~', '*']).trim();
    let words = value.split_whitespace().count();
    let letters = value.chars().filter(|c| c.is_alphabetic()).count();
    let valid = !value.is_empty()
        && value.len() <= MAX_ASSERTED_BYTES
        && (1..=4).contains(&words)
        && letters >= 2
        && !value.chars().any(char::is_control)
        && !value.contains('@')
        && !value.contains("://")
        && !value.starts_with('#')
        && !value.ends_with(['.', '!', '?', ',', ';', ':']);
    valid.then(|| value.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn asserted_terms<'a>(
    values: impl IntoIterator<Item = &'a str>,
    source: TermSource,
    scope: &str,
) -> Vec<MinedTerm> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter_map(asserted_term)
        .filter(|v| seen.insert(v.clone()))
        .take(MAX_ASSERTED_PER_SOURCE)
        .map(|v| MinedTerm::new(&v, source, scope, 1))
        .collect()
}

/// `~/Library/Spelling/LocalDictionary`: one learned word per line.
pub fn local_dictionary_terms(text: &str) -> Vec<MinedTerm> {
    asserted_terms(text.lines(), TermSource::SpellingDictionary, "")
}

/// Text Replacements (shortcut, phrase) pairs. The phrase is the asserted
/// spelling; the typed shortcut is not a spoken form and is dropped.
pub fn text_replacement_terms(pairs: &[(String, String)]) -> Vec<MinedTerm> {
    asserted_terms(
        pairs.iter().map(|(_, phrase)| phrase.as_str()),
        TermSource::TextReplacement,
        "",
    )
}

/// Read live (not deleted) Text Replacements read-only. Missing store → empty.
pub fn read_text_replacements(database: &Path) -> Result<Vec<(String, String)>> {
    if !database.is_file() {
        return Ok(vec![]);
    }
    let connection = rusqlite::Connection::open_with_flags(
        database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(std::time::Duration::from_millis(100))?;
    let mut statement = connection.prepare(
        "SELECT COALESCE(ZSHORTCUT, ''), COALESCE(ZPHRASE, '') FROM ZTEXTREPLACEMENTENTRY
         WHERE COALESCE(ZWASDELETED, 0) = 0 LIMIT 4096",
    )?;
    let rows = statement.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Remove `//` and `/* */` comments and trailing commas (JSONC, as VS Code and
/// cspell accept), leaving string contents intact.
pub fn strip_jsonc(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut in_string = false;
    while i < bytes.len() {
        let c = bytes[i];
        if in_string {
            out.push(c as char);
            if c == b'\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            in_string = c != b'"';
            i += 1;
        } else if c == b'"' {
            in_string = true;
            out.push('"');
            i += 1;
        } else if c == b'/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
        } else if c == b',' {
            let rest = text[i + 1..].trim_start();
            if !(rest.starts_with(']') || rest.starts_with('}')) {
                out.push(',');
            }
            i += 1;
        } else {
            // Copy the whole UTF-8 sequence starting here.
            let width = utf8_width(c);
            out.push_str(&text[i..(i + width).min(text.len())]);
            i += width;
        }
    }
    out
}

fn utf8_width(first: u8) -> usize {
    match first {
        0xF0..=0xFF => 4,
        0xE0..=0xEF => 3,
        0xC0..=0xDF => 2,
        _ => 1,
    }
}

fn string_array(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(serde_json::Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// cspell JSON/JSONC configuration: `words` plus custom dictionary file paths
/// (relative to the configuration file). `flagWords` are errors, not spellings.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CspellConfig {
    pub words: Vec<String>,
    pub dictionary_paths: Vec<String>,
}

pub fn parse_cspell_json(text: &str) -> CspellConfig {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&strip_jsonc(text)) else {
        return CspellConfig::default();
    };
    cspell_from_value(&value)
}

fn cspell_from_value(value: &serde_json::Value) -> CspellConfig {
    CspellConfig {
        words: string_array(value.get("words")),
        dictionary_paths: value
            .get("dictionaryDefinitions")
            .and_then(serde_json::Value::as_array)
            .map(|defs| {
                defs.iter()
                    .filter_map(|d| d.get("path").and_then(|p| p.as_str()).map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// Minimal YAML reader for cspell's top-level `words:` and `dictionaryDefinitions`
/// `path:` entries; anything more complex is ignored rather than guessed.
pub fn parse_cspell_yaml(text: &str) -> CspellConfig {
    let mut config = CspellConfig::default();
    let mut section = "";
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() || trimmed.trim_start().starts_with('#') {
            continue;
        }
        if !line.starts_with([' ', '\t', '-']) {
            section = trimmed.split(':').next().unwrap_or("").trim();
            if section == "words" {
                if let Some(rest) = trimmed.split_once(':').map(|(_, r)| r.trim()) {
                    if rest.starts_with('[') {
                        config.words.extend(inline_list(rest));
                    }
                }
            }
            continue;
        }
        let item = trimmed.trim_start();
        match section {
            "words" => {
                if let Some(word) = item.strip_prefix("- ") {
                    config.words.push(unquote(word));
                }
            }
            "dictionaryDefinitions" => {
                let item = item.trim_start_matches("- ");
                if let Some(path) = item.strip_prefix("path:") {
                    config.dictionary_paths.push(unquote(path));
                }
            }
            _ => {}
        }
    }
    config
}

fn unquote(value: &str) -> String {
    let value = value.split(" #").next().unwrap_or(value).trim();
    value.trim_matches(['"', '\'']).to_owned()
}

fn inline_list(value: &str) -> Vec<String> {
    value
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(unquote)
        .filter(|v| !v.is_empty())
        .collect()
}

/// A cspell custom dictionary: one word per line, `#` comments.
pub fn parse_word_list(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim().to_owned())
        .filter(|l| !l.is_empty())
        .collect()
}

/// VS Code settings (JSONC): `cSpell.words` and `cSpell.userWords`.
pub fn parse_vscode_cspell_words(text: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&strip_jsonc(text)) else {
        return vec![];
    };
    let mut words = string_array(value.get("cSpell.words"));
    words.extend(string_array(value.get("cSpell.userWords")));
    if let Some(nested) = value.get("cSpell") {
        words.extend(string_array(nested.get("words")));
    }
    words
}

/// JetBrains project dictionary (`.idea/dictionaries/*.xml`): `<w>word</w>`.
pub fn parse_jetbrains_dictionary(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("<w>") {
        rest = &rest[start + 3..];
        let Some(end) = rest.find("</w>") else { break };
        words.push(xml_unescape(rest[..end].trim()));
        rest = &rest[end + 4..];
    }
    words
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn read_bounded(path: &Path, limit: u64) -> Option<String> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > limit {
        return None;
    }
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(limit)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

/// Inside `root` after resolving symlinks; never follows a path out of it.
fn contained(root: &Path, path: &Path) -> Option<PathBuf> {
    let resolved = path.canonicalize().ok()?;
    resolved.starts_with(root).then_some(resolved)
}

/// Tier-A spelling lists inside an opted-in workspace: cspell configurations and
/// their word lists, VS Code workspace `cSpell.words`, JetBrains dictionaries.
pub fn mine_workspace_dictionaries(root: &Path, scope: &str) -> Vec<MinedTerm> {
    let Ok(root) = root.canonicalize() else {
        return vec![];
    };
    let mut words = Vec::new();
    let mut configs = Vec::new();
    for name in [
        "cspell.json",
        ".cspell.json",
        "cspell.config.json",
        "cspell.jsonc",
        ".cspell.jsonc",
        ".vscode/cspell.json",
        "cspell.config.yaml",
        "cspell.config.yml",
        "cspell.yaml",
        "cspell.yml",
        ".cspell.yaml",
        ".cspell.yml",
    ] {
        let Some(path) = contained(&root, &root.join(name)) else {
            continue;
        };
        let Some(text) = read_bounded(&path, MAX_CONFIG_BYTES) else {
            continue;
        };
        let config = if name.ends_with(".yaml") || name.ends_with(".yml") {
            parse_cspell_yaml(&text)
        } else {
            parse_cspell_json(&text)
        };
        configs.push((path, config));
    }
    if let Some(text) = contained(&root, &root.join("package.json"))
        .and_then(|p| read_bounded(&p, MAX_CONFIG_BYTES))
    {
        if let Some(cspell) = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("cspell").cloned())
        {
            configs.push((root.join("package.json"), cspell_from_value(&cspell)));
        }
    }
    for (path, config) in configs {
        words.extend(config.words);
        let base = path.parent().unwrap_or(&root).to_path_buf();
        for relative in config.dictionary_paths.iter().take(16) {
            if let Some(text) = contained(&root, &base.join(relative))
                .and_then(|p| read_bounded(&p, MAX_CONFIG_BYTES))
            {
                words.extend(parse_word_list(&text));
            }
        }
    }
    if let Some(text) = contained(&root, &root.join(".vscode/settings.json"))
        .and_then(|p| read_bounded(&p, MAX_CONFIG_BYTES))
    {
        words.extend(parse_vscode_cspell_words(&text));
    }
    if let Some(dir) = contained(&root, &root.join(".idea/dictionaries")) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten().take(32) {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "xml") {
                    if let Some(text) =
                        contained(&root, &path).and_then(|p| read_bounded(&p, MAX_CONFIG_BYTES))
                    {
                        words.extend(parse_jetbrains_dictionary(&text));
                    }
                }
            }
        }
    }
    asserted_terms(
        words.iter().map(String::as_str),
        TermSource::ProjectDictionary,
        scope,
    )
}

// ---------------------------------------------------------------------------
// Tier B: identifiers, manifest names and documentation headings.

/// Directories that hold third-party or generated material.
const SKIPPED_DIRECTORIES: &[&str] = &[
    "node_modules",
    "vendor",
    "target",
    "dist",
    "build",
    "out",
    ".venv",
    "venv",
    "site-packages",
    "third_party",
    "third-party",
    "external",
    "Pods",
    "DerivedData",
    "__pycache__",
    ".next",
    "coverage",
    ".git",
    "bower_components",
    "generated",
];
const CODE_EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "svelte", "vue", "py", "pyi", "go", "java", "kt",
    "kts", "swift", "m", "mm", "h", "hh", "hpp", "c", "cc", "cpp", "cxx", "cs", "rb", "php",
    "scala", "sh", "zsh", "bash", "fish", "lua", "dart", "ex", "exs", "hs", "ml", "jl", "sql",
    "html", "css", "scss", "sass", "less",
];
const DOC_EXTENSIONS: &[&str] = &["md", "mdx", "rst", "txt", "adoc"];
const CONFIG_EXTENSIONS: &[&str] = &["toml", "yaml", "yml"];
/// Words that introduce a definition in common languages.
const DEFINITION_KEYWORDS: &[&str] = &[
    "fn",
    "struct",
    "enum",
    "trait",
    "type",
    "mod",
    "const",
    "static",
    "class",
    "def",
    "function",
    "interface",
    "func",
    "protocol",
    "define",
    "implementation",
    "union",
    "macro_rules",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileClass {
    Code,
    Doc,
    Config,
    Manifest(ManifestKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManifestKind {
    Cargo,
    Npm,
    Python,
}

fn classify(relative: &str) -> Option<FileClass> {
    let path = Path::new(relative);
    if path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .is_some_and(|c| SKIPPED_DIRECTORIES.contains(&c))
    }) {
        return None;
    }
    let name = path.file_name()?.to_str()?;
    match name {
        "Cargo.toml" => return Some(FileClass::Manifest(ManifestKind::Cargo)),
        "package.json" => return Some(FileClass::Manifest(ManifestKind::Npm)),
        "pyproject.toml" => return Some(FileClass::Manifest(ManifestKind::Python)),
        _ => {}
    }
    let upper = name.to_ascii_uppercase();
    if name.contains(".min.")
        || name.ends_with(".lock")
        || name.ends_with("-lock.yaml")
        || ["LICENSE", "LICENCE", "COPYING", "NOTICE"]
            .iter()
            .any(|prefix| upper.starts_with(prefix))
    {
        return None;
    }
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    if CODE_EXTENSIONS.contains(&extension.as_str()) {
        Some(FileClass::Code)
    } else if DOC_EXTENSIONS.contains(&extension.as_str()) {
        Some(FileClass::Doc)
    } else if CONFIG_EXTENSIONS.contains(&extension.as_str()) {
        Some(FileClass::Config)
    } else {
        None
    }
}

/// Maximal `[A-Za-z0-9_]` runs (plus `-` when `hyphen`) starting with a letter.
fn words(text: &str, hyphen: bool) -> impl Iterator<Item = &str> {
    let bytes = text.as_bytes();
    let mut i = 0;
    std::iter::from_fn(move || {
        while i < bytes.len() {
            let start = i;
            let part = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || (hyphen && b == b'-');
            while i < bytes.len() && part(bytes[i]) {
                i += 1;
            }
            if i == start {
                i += 1;
                continue;
            }
            let token = text[start..i].trim_end_matches('-');
            if token
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
            {
                return Some(token);
            }
        }
        None
    })
}

/// Drop `scheme://…` spans so hosts, user names and paths in links are not mined.
fn without_urls(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains("://") {
        return std::borrow::Cow::Borrowed(text);
    }
    let parts = text.split("://").collect::<Vec<_>>();
    let mut out = String::with_capacity(text.len());
    for (i, part) in parts.iter().enumerate() {
        let mut part = *part;
        if i > 0 {
            // Skip the link body up to the first delimiter.
            part = part
                .find(|c: char| c.is_whitespace() || ")>]\"'`".contains(c))
                .map_or("", |end| &part[end..]);
        }
        if i + 1 < parts.len() {
            // Drop the scheme word (`https`) that precedes the next `://`.
            part = part.trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '+');
        }
        out.push_str(part);
        out.push(' ');
    }
    std::borrow::Cow::Owned(out)
}

/// Weekday and month names are capitalized by English convention, not by the
/// project, and are protected classes for correction.
const CALENDAR_WORDS: &[&str] = &[
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
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

/// Prose words of one documentation line (backtick spans removed). Records a
/// capitalized word whose lowercase form is ordinary when it is mid-sentence
/// (not after sentence punctuation, markup or a line start), and every lowercase
/// ordinary word, so admission can compare the two spellings.
fn prose_words(line: &str, names: &mut HashSet<String>, lower: &mut HashSet<String>) {
    let mut prose = String::with_capacity(line.len());
    for (i, part) in line.split('`').enumerate() {
        prose.push_str(if i % 2 == 0 { part } else { " ` " });
    }
    let bytes = prose.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        let joined =
            |b: Option<&u8>| b.is_some_and(|b| b.is_ascii_digit() || *b == b'_' || *b == b'-');
        if joined(start.checked_sub(1).map(|p| &bytes[p])) || joined(bytes.get(i)) {
            continue;
        }
        let word = &prose[start..i];
        if word.len() < 3 || STOP_TOKENS.contains(&word.to_ascii_lowercase().as_str()) {
            continue;
        }
        let lowercase = word.to_ascii_lowercase();
        if !ordinary_word(&lowercase) || CALENDAR_WORDS.contains(&lowercase.as_str()) {
            continue;
        }
        if word == lowercase {
            lower.insert(lowercase);
            continue;
        }
        let first_upper = word.as_bytes()[0].is_ascii_uppercase();
        let rest = &word[1..];
        // All capitals only up to six letters (`NVIDIA`); longer runs are
        // emphasis or legal text (`LIABILITY`).
        let name_case = first_upper
            && (rest.bytes().all(|b| b.is_ascii_lowercase())
                || (word.len() <= 6 && rest.bytes().all(|b| b.is_ascii_uppercase())));
        let previous = prose[..start].trim_end().chars().next_back();
        let initial = previous
            .is_none_or(|c| c.is_ascii_digit() || ".!?:;\"'“‘([{*_|>#-–—/\\=<+~`".contains(c));
        if name_case && !initial {
            names.insert(word.to_owned());
        }
    }
}

/// Shape class of a candidate token, or `None` when it is ordinary or junk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// `_`/`-` separated, camelCase, or letters mixed with digits.
    Identifier,
    /// `Capitalized` word that is not an ordinary English word.
    Name,
    /// 2–8 capitals.
    Acronym,
    /// Lowercase non-dictionary word (`rusqlite`, `kubectl`).
    Lower,
}

fn shape(token: &str) -> Option<Shape> {
    let letters = token.chars().filter(char::is_ascii_alphabetic).count();
    if token.len() < 2
        || STOP_TOKENS.contains(&token.to_ascii_lowercase().as_str())
        || token.chars().count() > MAX_TERM_CHARS
        || letters < 2
        || token.starts_with(['_', '-'])
        || token.ends_with(['_', '-'])
        || token.contains("__")
        || looks_like_hash(token)
    {
        return None;
    }
    let has_digit = token.chars().any(|c| c.is_ascii_digit());
    let separated = token.contains(['_', '-']);
    let camel = token
        .as_bytes()
        .windows(2)
        .any(|w| w[0].is_ascii_lowercase() && w[1].is_ascii_uppercase());
    if separated || camel || has_digit {
        let parts = token
            .split(['_', '-'])
            .filter(|p| !p.is_empty())
            .count()
            .max(identifier_spoken_form(token).map_or(1, |s| s.split(' ').count()));
        if parts > 5 || token.split(['_', '-']).any(|p| p.len() == 1 && parts > 2) {
            return None;
        }
        return Some(Shape::Identifier);
    }
    let lower = token.to_ascii_lowercase();
    if ordinary_word(&lower) || inflected_word(&lower) {
        return None;
    }
    let upper = token.chars().filter(char::is_ascii_uppercase).count();
    if upper == token.len() {
        (3..=8).contains(&token.len()).then_some(Shape::Acronym)
    } else if upper == 1 && token.as_bytes()[0].is_ascii_uppercase() {
        Some(Shape::Name)
    } else if upper == 0 {
        (token.len() >= 4 && !compound_word(&lower)).then_some(Shape::Lower)
    } else {
        Some(Shape::Identifier)
    }
}

/// Programming keywords, ubiquitous library names and generic tech words that
/// pass the shape test but are not personal vocabulary.
const STOP_TOKENS: &[&str] = &[
    "fn",
    "impl",
    "struct",
    "enum",
    "trait",
    "const",
    "static",
    "async",
    "await",
    "def",
    "func",
    "elif",
    "init",
    "args",
    "kwargs",
    "argv",
    "argc",
    "bool",
    "int",
    "str",
    "usize",
    "isize",
    "uint",
    "vec",
    "eq",
    "ne",
    "ord",
    "hashmap",
    "hashset",
    "btreemap",
    "btreeset",
    "println",
    "eprintln",
    "unwrap",
    "deserialize",
    "serialize",
    "starts_with",
    "ends_with",
    "and_then",
    "or_else",
    "unwrap_or",
    "unwrap_or_default",
    "unwrap_or_else",
    "map_err",
    "to_string",
    "to_owned",
    "as_str",
    "as_ref",
    "is_empty",
    "is_some",
    "is_none",
    "is_ok",
    "is_err",
    "push_str",
    "to_lowercase",
    "to_uppercase",
    "into_iter",
    "iter_mut",
    "len",
    "config",
    "configs",
    "json",
    "yaml",
    "toml",
    "http",
    "https",
    "www",
    "html",
    "css",
    "url",
    "urls",
    "uri",
    "api",
    "apis",
    "readme",
    "todo",
    "fixme",
    "src",
    "lib",
    "libs",
    "utils",
    "util",
    "env",
    "tmp",
    "temp",
    "dir",
    "dirs",
    "msg",
    "err",
    "ctx",
    "cfg",
    "param",
    "params",
    "typeof",
    "instanceof",
    "undefined",
    "nullptr",
    "onclick",
    "classname",
    "stdout",
    "stderr",
    "stdin",
    "async_trait",
    "pycache",
    "dataclass",
    "kwarg",
    "subprocess",
    "mkdir",
    "chmod",
    "sudo",
    "dev",
    "localhost",
    "metadata",
    "runtime",
    "frontend",
    "backend",
    "dataset",
    "datasets",
    "timestamp",
    "timestamps",
    "workflow",
    "workflows",
    "namespace",
    "boolean",
];

/// Regular English inflections of an ordinary word (`timings`, `retries`, `scoped`).
fn inflected_word(lower: &str) -> bool {
    const RULES: &[(&str, &str)] = &[
        ("ies", "y"),
        ("es", ""),
        ("s", ""),
        ("ed", ""),
        ("ed", "e"),
        ("ing", ""),
        ("ing", "e"),
        ("er", ""),
        ("er", "e"),
        ("ers", ""),
        ("ers", "e"),
        ("ly", ""),
        ("ation", "ate"),
        ("ation", "e"),
        ("ization", "ize"),
        ("ness", ""),
        ("ment", ""),
        ("able", ""),
        ("able", "e"),
        ("ity", ""),
        ("al", ""),
        ("ful", ""),
        ("less", ""),
    ];
    const PREFIXES: &[&str] = &[
        "re", "de", "un", "pre", "non", "sub", "over", "under", "mis", "dis",
    ];
    let suffixed = |word: &str| {
        RULES.iter().any(|(suffix, replacement)| {
            word.strip_suffix(suffix).is_some_and(|stem| {
                stem.len() >= 3 && ordinary_word(&format!("{stem}{replacement}"))
            })
        })
    };
    suffixed(lower)
        || PREFIXES.iter().any(|prefix| {
            lower
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.len() >= 4 && (ordinary_word(rest) || suffixed(rest)))
        })
}

/// Two ordinary words written together (`runtime`, `frontend`, `metadata`).
fn compound_word(lower: &str) -> bool {
    (3..lower.len().saturating_sub(2)).any(|i| {
        lower.is_char_boundary(i)
            && ordinary_word(&lower[..i])
            && (ordinary_word(&lower[i..]) || inflected_word(&lower[i..]))
            && lower.len() - i >= 3
    })
}

fn looks_like_hash(token: &str) -> bool {
    token.len() >= 8
        && token.chars().all(|c| c.is_ascii_hexdigit())
        && token.chars().any(|c| c.is_ascii_digit())
}

fn fold(token: &str) -> String {
    token
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenStat {
    /// Distinct files containing this exact spelling.
    pub files: u32,
    /// Distinct documentation files containing it.
    pub doc_files: u32,
    pub defined: bool,
    pub manifest: bool,
    pub heading: bool,
}

/// Partial pass state; holds candidate spellings and counts, never paths.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Accumulator {
    pub tokens: BTreeMap<String, TokenStat>,
    /// Distinct files containing any spelling that folds to this key.
    pub folded_files: BTreeMap<String, u32>,
    /// Capitalized words whose lowercase form is ordinary (`Vesper`, `NVIDIA`):
    /// documentation files where the capitalized form appears mid-sentence.
    #[serde(default)]
    pub names: BTreeMap<String, u32>,
    /// Documentation files using an ordinary word in lowercase prose.
    #[serde(default)]
    pub prose_lower: BTreeMap<String, u32>,
}

#[derive(Default)]
struct FileTokens {
    names: HashSet<String>,
    prose_lower: HashSet<String>,
    seen: HashSet<String>,
    defined: HashSet<String>,
    manifest: HashSet<String>,
    heading: HashSet<String>,
}

impl FileTokens {
    fn add(&mut self, token: &str) -> bool {
        if shape(token).is_some() {
            self.seen.insert(token.to_owned());
            true
        } else {
            false
        }
    }
}

impl Accumulator {
    /// Add one file's text. `stem` is the file name without extension.
    pub fn add_file(&mut self, relative: &str, text: &str) {
        let Some(class) = classify(relative) else {
            return;
        };
        let mut file = FileTokens::default();
        match class {
            FileClass::Code => {
                for line in text.lines() {
                    let line = without_urls(line);
                    let mut previous = "";
                    for token in words(&line, false) {
                        if file.add(token)
                            && DEFINITION_KEYWORDS.contains(&previous)
                            && !DEFINITION_KEYWORDS.contains(&token)
                        {
                            file.defined.insert(token.to_owned());
                        }
                        previous = token;
                    }
                }
                if let Some(stem) = Path::new(relative).file_stem().and_then(|s| s.to_str()) {
                    if stem.split('.').count() == 1 && file.add(stem) {
                        file.defined.insert(stem.to_owned());
                    }
                }
            }
            FileClass::Doc => {
                let mut fenced = false;
                for line in text.lines() {
                    if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~")
                    {
                        fenced = !fenced;
                        continue;
                    }
                    let line = without_urls(line);
                    let marks = line.bytes().take_while(|b| *b == b'#').count();
                    let heading = !fenced
                        && (1..=6).contains(&marks)
                        && line.as_bytes().get(marks) == Some(&b' ');
                    if !fenced && !heading {
                        prose_words(&line, &mut file.names, &mut file.prose_lower);
                    }
                    for token in words(&line, false) {
                        if file.add(token) && heading {
                            file.heading.insert(token.to_owned());
                        }
                    }
                    for span in line.split('`').skip(1).step_by(2) {
                        for token in words(span, true) {
                            if file.add(token) && heading {
                                file.heading.insert(token.to_owned());
                            }
                        }
                    }
                }
            }
            FileClass::Config => {
                for token in words(&without_urls(text), false) {
                    file.add(token);
                }
            }
            FileClass::Manifest(kind) => {
                for token in words(text, false) {
                    file.add(token);
                }
                for name in manifest_names(kind, text) {
                    for part in name.split('/') {
                        let part = part.trim_start_matches('@');
                        if file.add(part) {
                            file.manifest.insert(part.to_owned());
                        }
                    }
                }
            }
        }
        let doc = class == FileClass::Doc;
        let mut folds = HashSet::new();
        for token in &file.seen {
            let stat = self.tokens.entry(token.clone()).or_default();
            stat.files += 1;
            stat.doc_files += u32::from(doc);
            stat.defined |= file.defined.contains(token);
            stat.manifest |= file.manifest.contains(token);
            stat.heading |= file.heading.contains(token);
            folds.insert(fold(token));
        }
        for key in folds {
            *self.folded_files.entry(key).or_default() += 1;
        }
        for name in file.names {
            *self.names.entry(name).or_default() += 1;
        }
        for word in file.prose_lower {
            *self.prose_lower.entry(word).or_default() += 1;
        }
    }

    /// Tier-B admission: independent repetition (≥ 2 files across all spellings of
    /// the folded key), the majority spelling, and evidence that matches the
    /// shape. Deterministic; no lexicon beyond the shared ordinary-word list.
    pub fn admit(&self, scope: &str) -> Vec<MinedTerm> {
        let mut groups: HashMap<String, Vec<(&String, &TokenStat)>> = HashMap::new();
        for (token, stat) in &self.tokens {
            groups.entry(fold(token)).or_default().push((token, stat));
        }
        let mut admitted = Vec::new();
        for (key, mut variants) in groups {
            let files = self.folded_files.get(&key).copied().unwrap_or(0);
            if files < 2 {
                continue;
            }
            variants.sort_by(|a, b| {
                b.1.files
                    .cmp(&a.1.files)
                    .then(b.1.defined.cmp(&a.1.defined))
                    .then(a.0.cmp(b.0))
            });
            let (surface, _) = variants[0];
            let defined = variants.iter().any(|v| v.1.defined);
            let manifest = variants.iter().any(|v| v.1.manifest);
            let heading = variants.iter().any(|v| v.1.heading);
            let doc_files: u32 = variants.iter().map(|v| v.1.doc_files).sum();
            let evidence = match shape(surface) {
                Some(Shape::Identifier | Shape::Lower) => defined || manifest || heading,
                Some(Shape::Name) => true,
                Some(Shape::Acronym) => heading || doc_files > 0,
                None => false,
            };
            if !evidence {
                continue;
            }
            let source = if manifest {
                TermSource::Manifest
            } else if heading {
                TermSource::Documentation
            } else if defined {
                TermSource::Identifier
            } else if doc_files > 0 {
                TermSource::Documentation
            } else {
                TermSource::Identifier
            };
            admitted.push(MinedTerm::new(surface, source, scope, files));
        }
        // Consistently capitalized names whose lowercase form is an ordinary
        // word: the capitalized spelling must appear mid-sentence in at least two
        // documentation files and in more files than the lowercase prose word.
        let mut best: HashMap<String, (&String, u32)> = HashMap::new();
        for (surface, files) in &self.names {
            let entry = best.entry(surface.to_lowercase()).or_insert((surface, 0));
            if *files > entry.1 || (*files == entry.1 && surface < entry.0) {
                *entry = (surface, *files);
            }
        }
        for (lower, (surface, files)) in best {
            let lowercase_files = self.prose_lower.get(&lower).copied().unwrap_or(0);
            if files >= 2
                && files > lowercase_files
                && !admitted
                    .iter()
                    .any(|t| t.surface.eq_ignore_ascii_case(surface))
            {
                admitted.push(MinedTerm::new(
                    surface,
                    TermSource::Documentation,
                    scope,
                    files,
                ));
            }
        }
        admitted.sort_by(|a, b| b.count.cmp(&a.count).then(a.surface.cmp(&b.surface)));
        admitted
    }
}

/// Package and dependency names from a manifest. Line-oriented for TOML (no
/// parser dependency); JSON through serde.
fn manifest_names(kind: ManifestKind, text: &str) -> Vec<String> {
    let mut names = Vec::new();
    match kind {
        ManifestKind::Npm => {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
                if let Some(name) = value.get("name").and_then(|n| n.as_str()) {
                    names.push(name.to_owned());
                }
                for key in [
                    "dependencies",
                    "devDependencies",
                    "peerDependencies",
                    "optionalDependencies",
                ] {
                    if let Some(map) = value.get(key).and_then(|m| m.as_object()) {
                        names.extend(map.keys().cloned());
                    }
                }
            }
        }
        ManifestKind::Cargo | ManifestKind::Python => {
            let mut section = String::new();
            let mut in_list = false;
            for line in text.lines() {
                let line = line.split(" #").next().unwrap_or("").trim();
                if line.starts_with('[') && !in_list {
                    section = line.trim_matches(['[', ']']).to_owned();
                    if let Some(name) = section.strip_prefix("dependencies.") {
                        names.push(name.to_owned());
                    }
                    continue;
                }
                let dependencies = section.contains("dependencies");
                if kind == ManifestKind::Python
                    && (in_list || dependencies || line.starts_with("dependencies"))
                {
                    // `dependencies = ["numpy>=2", ...]` and group arrays.
                    let open = line.contains('[');
                    if open && line.contains('=') {
                        in_list = !line.contains(']');
                    } else if line.starts_with(']') {
                        in_list = false;
                    }
                    for quoted in line.split('"').skip(1).step_by(2) {
                        let name = quoted
                            .split(|c: char| "<>=!~;[ (".contains(c))
                            .next()
                            .unwrap_or("");
                        if !name.is_empty() {
                            names.push(name.to_owned());
                        }
                    }
                    continue;
                }
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                let key = key.trim();
                if key == "name"
                    && (section == "package" || section == "project" || section.is_empty())
                {
                    names.push(value.trim().trim_matches(['"', '\'']).to_owned());
                } else if kind == ManifestKind::Cargo && section.ends_with("dependencies") {
                    names.push(
                        key.split('.')
                            .next()
                            .unwrap_or(key)
                            .trim_matches('"')
                            .to_owned(),
                    );
                }
            }
        }
    }
    names.retain(|n| !n.is_empty() && n.len() <= 64);
    names
}

/// Control over a bounded scan: the caller enforces CPU, cancellation and
/// system gates; the scanner enforces file and byte limits.
pub trait ScanControl {
    fn should_stop(&self) -> bool;
}

impl<F: Fn() -> bool> ScanControl for F {
    fn should_stop(&self) -> bool {
        self()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ScanBudget {
    pub max_files: usize,
    pub max_bytes: u64,
}

impl Default for ScanBudget {
    fn default() -> Self {
        Self {
            max_files: 2000,
            max_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Resumable per-workspace pass state. The fingerprint is a hash over tracked
/// paths, sizes and modification times; neither paths nor text are stored.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCursor {
    pub completed_fingerprint: String,
    pub pass_fingerprint: String,
    pub next_index: usize,
    pub partial: Accumulator,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub files_listed: usize,
    pub files_read: usize,
    pub bytes_read: u64,
    pub files_skipped: usize,
    /// Nothing changed since the last completed pass.
    pub unchanged: bool,
    pub completed: bool,
    pub stopped: bool,
    /// Present only when a pass completed.
    pub admitted: Option<Vec<MinedTerm>>,
}

/// A real git binary; never the `/usr/bin/git` shim, which can open an install
/// dialog when developer tools are missing.
pub fn git_binary() -> Option<PathBuf> {
    [
        "/Library/Developer/CommandLineTools/usr/bin/git",
        "/Applications/Xcode.app/Contents/Developer/usr/bin/git",
        "/opt/homebrew/bin/git",
        "/usr/local/bin/git",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|p| p.is_file())
}

/// Tracked files under `root`, relative and sorted. Not a repository → `None`.
pub fn tracked_files(git: &Path, root: &Path) -> Option<Vec<String>> {
    let output = std::process::Command::new(git)
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z", "--cached"])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let mut files = output
        .stdout
        .split(|b| *b == 0)
        .filter_map(|p| std::str::from_utf8(p).ok())
        .filter(|p| !p.is_empty() && classify(p).is_some())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    files.sort();
    Some(files)
}

fn fingerprint(root: &Path, files: &[String]) -> String {
    let mut hash = Sha256::new();
    for file in files {
        let (size, modified) = std::fs::symlink_metadata(root.join(file))
            .map(|m| {
                let modified = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos());
                (m.len(), modified)
            })
            .unwrap_or_default();
        hash.update(file.as_bytes());
        hash.update([0]);
        hash.update(size.to_le_bytes());
        hash.update(modified.to_le_bytes());
    }
    format!("{:x}", hash.finalize())
}

fn readable_text(bytes: &[u8]) -> Option<&str> {
    if bytes[..bytes.len().min(8192)].contains(&0) {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    // Minified or generated: a very long line early in the file.
    if text[..text.len().min(65_536)]
        .lines()
        .any(|l| l.len() > 2000)
    {
        return None;
    }
    Some(text)
}

/// Continue (or start) a bounded pass over the tracked files of an opted-in root.
pub fn scan_workspace(
    git: &Path,
    root: &Path,
    scope: &str,
    cursor: &mut WorkspaceCursor,
    budget: ScanBudget,
    control: &dyn ScanControl,
) -> Result<ScanReport> {
    let root = root
        .canonicalize()
        .map_err(|_| DatasetError::InvalidConfig("Mining root is unavailable.".into()))?;
    let Some(files) = tracked_files(git, &root) else {
        return Err(DatasetError::InvalidConfig(
            "Mining root is not a git repository.".into(),
        ));
    };
    let mut report = ScanReport {
        files_listed: files.len(),
        ..ScanReport::default()
    };
    let current = fingerprint(&root, &files);
    if cursor.pass_fingerprint.is_empty() || cursor.pass_fingerprint != current {
        if current == cursor.completed_fingerprint {
            report.unchanged = true;
            report.completed = true;
            cursor.pass_fingerprint.clear();
            cursor.next_index = 0;
            cursor.partial = Accumulator::default();
            return Ok(report);
        }
        // Start a fresh pass; a pass interrupted by file changes restarts.
        cursor.pass_fingerprint = current.clone();
        cursor.next_index = 0;
        cursor.partial = Accumulator::default();
    }
    let mut buffer = Vec::new();
    while cursor.next_index < files.len() {
        if report.files_read >= budget.max_files
            || report.bytes_read >= budget.max_bytes
            || control.should_stop()
        {
            report.stopped = true;
            return Ok(report);
        }
        let relative = &files[cursor.next_index];
        cursor.next_index += 1;
        let Some(path) = contained(&root, &root.join(relative)) else {
            report.files_skipped += 1;
            continue;
        };
        let Ok(metadata) = std::fs::metadata(&path) else {
            report.files_skipped += 1;
            continue;
        };
        if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
            report.files_skipped += 1;
            continue;
        }
        buffer.clear();
        let read = std::fs::File::open(&path)
            .and_then(|f| f.take(MAX_FILE_BYTES).read_to_end(&mut buffer));
        if read.is_err() {
            report.files_skipped += 1;
            continue;
        }
        report.files_read += 1;
        report.bytes_read += buffer.len() as u64;
        match readable_text(&buffer) {
            Some(text) => cursor.partial.add_file(relative, text),
            None => report.files_skipped += 1,
        }
    }
    report.completed = true;
    report.admitted = Some(cursor.partial.admit(scope));
    cursor.completed_fingerprint = std::mem::take(&mut cursor.pass_fingerprint);
    cursor.next_index = 0;
    cursor.partial = Accumulator::default();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asserted_terms_keep_names_and_drop_sentences_addresses_and_numbers() {
        let pairs = [
            ("omw", "On my way!"),
            ("addr", "someone@example.test"),
            ("ph", "555 0100"),
            ("vsp", "Vesper"),
            ("k8", "kubectl"),
            ("url", "https://example.test"),
            ("dup", "Vesper"),
        ]
        .map(|(a, b)| (a.to_owned(), b.to_owned()));
        let terms = text_replacement_terms(&pairs);
        let surfaces = terms.iter().map(|t| t.surface.as_str()).collect::<Vec<_>>();
        assert_eq!(surfaces, ["Vesper", "kubectl"]);
        assert!(terms
            .iter()
            .all(|t| t.source == TermSource::TextReplacement && t.scope.is_empty()));
        let local = local_dictionary_terms("Zorblax\n\nfooBar\n");
        assert_eq!(local[1].spoken, ["foo bar"]);
    }

    #[test]
    fn text_replacements_read_live_rows_from_the_core_data_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("TextReplacements.db");
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE ZTEXTREPLACEMENTENTRY (Z_PK INTEGER PRIMARY KEY, ZWASDELETED INTEGER, ZPHRASE VARCHAR, ZSHORTCUT VARCHAR);
             INSERT INTO ZTEXTREPLACEMENTENTRY VALUES (1, 0, 'Zorblax Labs', 'zl'), (2, 1, 'Gone', 'gn'), (3, NULL, 'Quuxly', 'qx');",
        )
        .unwrap();
        drop(db);
        let mut rows = read_text_replacements(&path).unwrap();
        rows.sort();
        assert_eq!(
            rows,
            [
                ("qx".into(), "Quuxly".into()),
                ("zl".into(), "Zorblax Labs".into())
            ]
        );
        assert!(read_text_replacements(&dir.path().join("missing.db"))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn cspell_vscode_and_jetbrains_formats_parse() {
        let json = r#"{
            // comment
            "version": "0.2",
            "words": ["Zorblax", "fooBar",], /* block */
            "flagWords": ["nope"],
            "dictionaryDefinitions": [{"name": "project", "path": "./project-words.txt"}],
        }"#;
        let config = parse_cspell_json(json);
        assert_eq!(config.words, ["Zorblax", "fooBar"]);
        assert_eq!(config.dictionary_paths, ["./project-words.txt"]);
        let yaml = "version: \"0.2\"\nwords:\n  - Zorblax\n  - 'quuxCore' # note\ndictionaryDefinitions:\n  - name: p\n    path: ./words.txt\nignorePaths:\n  - x\n";
        let config = parse_cspell_yaml(yaml);
        assert_eq!(config.words, ["Zorblax", "quuxCore"]);
        assert_eq!(config.dictionary_paths, ["./words.txt"]);
        assert_eq!(
            parse_cspell_yaml("words: [alpha, \"Beta\"]\n").words,
            ["alpha", "Beta"]
        );
        assert_eq!(parse_word_list("# c\nzorb\n  quux # x\n"), ["zorb", "quux"]);
        let settings = "{\n  \"editor.fontSize\": 13, // keep\n  \"cSpell.words\": [\"Zorblax\", \"https://not//comment\"],\n}";
        assert_eq!(
            parse_vscode_cspell_words(settings),
            ["Zorblax", "https://not//comment"]
        );
        let xml = "<component name=\"ProjectDictionaryState\"><dictionary name=\"me\"><words><w>zorblax</w><w>quux&amp;co</w></words></dictionary></component>";
        assert_eq!(parse_jetbrains_dictionary(xml), ["zorblax", "quux&co"]);
    }

    #[test]
    fn workspace_dictionaries_stay_inside_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        std::fs::create_dir_all(root.join(".idea/dictionaries")).unwrap();
        std::fs::create_dir_all(root.join(".vscode")).unwrap();
        std::fs::write(dir.path().join("outside.txt"), "Leaked\n").unwrap();
        std::fs::write(
            root.join("cspell.json"),
            r#"{"words":["Zorblax"],"dictionaryDefinitions":[{"path":"words.txt"},{"path":"../outside.txt"}]}"#,
        )
        .unwrap();
        std::fs::write(root.join("words.txt"), "quuxCore\n").unwrap();
        std::fs::write(
            root.join(".vscode/settings.json"),
            r#"{"cSpell.words":["Blorptastic"]}"#,
        )
        .unwrap();
        std::fs::write(root.join(".idea/dictionaries/me.xml"), "<w>frobnicate</w>").unwrap();
        let mut terms = mine_workspace_dictionaries(&root, "ws")
            .into_iter()
            .map(|t| t.surface)
            .collect::<Vec<_>>();
        terms.sort();
        assert_eq!(terms, ["Blorptastic", "Zorblax", "frobnicate", "quuxCore"]);
    }

    #[test]
    fn shapes_reject_ordinary_words_hashes_and_long_chains() {
        assert_eq!(shape("WorkingSet"), Some(Shape::Identifier));
        assert_eq!(shape("sherpa_onnx"), Some(Shape::Identifier));
        assert_eq!(shape("k8s"), Some(Shape::Identifier));
        assert_eq!(shape("Zorblax"), Some(Shape::Name));
        assert_eq!(shape("GGUF"), Some(Shape::Acronym));
        assert_eq!(shape("rusqlite"), Some(Shape::Lower));
        assert_eq!(shape("point"), None);
        assert_eq!(shape("Monday"), None);
        assert_eq!(shape("deadbeef1234"), None);
        assert_eq!(shape("__init__"), None);
        assert_eq!(shape("a_very_long_chain_of_many_words"), None);
        assert_eq!(shape("timings"), None);
        assert_eq!(shape("retries"), None);
        assert_eq!(shape("frontend"), None);
        assert_eq!(shape("Nemotron"), Some(Shape::Name));
        assert_eq!(shape("diarization"), Some(Shape::Lower));
        assert_eq!(shape("GB"), None);
        assert_eq!(shape("and_then"), None);
        assert_eq!(shape("Revalidate"), None);
        assert_eq!(shape("denoising"), None);
    }

    #[test]
    fn links_are_not_mined_and_fenced_comments_are_not_headings() {
        assert_eq!(
            without_urls("see https://example.test/someUser/repo, then (http://x.test) done")
                .split_whitespace()
                .collect::<Vec<_>>(),
            ["see", "then", "(", ")", "done"]
        );
        let mut acc = Accumulator::default();
        acc.add_file("a.md", "```rust\n# not_a_heading\n#[derive(Zorblax)]\n```\n## Real `quux_core`\n[x](https://host.test/linkOnlyName)\n");
        acc.add_file("b.md", "not_a_heading quux_core Zorblax linkOnlyName\n");
        let admitted = acc.admit("ws");
        let names = admitted
            .iter()
            .map(|t| t.surface.as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"quux_core"));
        assert!(!names.contains(&"not_a_heading"));
        assert!(!names.contains(&"linkOnlyName"));
    }

    #[test]
    fn admission_needs_two_files_majority_spelling_and_definition_evidence() {
        let mut acc = Accumulator::default();
        acc.add_file(
            "src/a.rs",
            "pub struct WorkingSet;\nfn helperThing() {}\nlet x = unwrapOrElse;\n",
        );
        acc.add_file(
            "src/b.rs",
            "use crate::WorkingSet;\nlet y = unwrapOrElse(helperThing);\n// Zorblax and Zorblax\n",
        );
        acc.add_file(
            "src/c.rs",
            "fn working_set() {}\nlet once = soloIdent; // Zorblax\n",
        );
        acc.add_file("README.md", "# The `vesper-core` crate\nUses GGUF files.\n");
        acc.add_file(
            "crates/x/Cargo.toml",
            "[package]\nname = \"vesper-core\"\n[dependencies]\nserde_json = \"1\"\n",
        );
        acc.add_file("src/d.rs", "use serde_json;\nuse vesper_core::GGUF;\n");
        acc.add_file(
            "node_modules/z/index.js",
            "function vendoredName() {}\nvendoredName()\n",
        );
        let admitted = acc.admit("ws");
        let get = |s: &str| admitted.iter().find(|t| t.surface == s);
        // Majority spelling of the folded key across 3 files.
        assert_eq!(get("WorkingSet").unwrap().count, 3);
        assert!(get("working_set").is_none());
        assert_eq!(get("helperThing").unwrap().source, TermSource::Identifier);
        // Used in two files but never defined: a library name, not ours.
        assert!(get("unwrapOrElse").is_none());
        assert!(get("soloIdent").is_none());
        assert_eq!(get("Zorblax").unwrap().count, 2);
        assert_eq!(get("serde_json").unwrap().source, TermSource::Manifest);
        assert!(get("GGUF").is_some());
        assert!(get("vendoredName").is_none());
        let vesper = admitted
            .iter()
            .find(|t| fold(&t.surface) == "vespercore")
            .unwrap();
        assert_eq!(vesper.source, TermSource::Manifest);
        assert_eq!(vesper.spoken, ["vesper core"]);
    }

    #[test]
    fn consistently_capitalized_names_are_admitted_even_when_ordinary_words() {
        let mut acc = Accumulator::default();
        acc.add_file(
            "a.md",
            "Install Vesper first. Then Vesper listens.\nWrite the note. I see the point.\n",
        );
        acc.add_file("b.md", "Open Vesper and an NVIDIA card. Write it down.\n");
        acc.add_file("c.md", "We trained on NVIDIA hardware with Gemma.\n");
        acc.add_file(
            "d.md",
            "A vesper is an evening prayer; the point stands. Gemma ran.\n",
        );
        acc.add_file(
            "e.md",
            "```\nuse Gemma here and Gemma there\n```\n## About Gemma\n**Gemma** | Gemma\n",
        );
        let admitted = acc.admit("ws");
        let get = |s: &str| admitted.iter().find(|t| t.surface == s);
        // Mid-sentence capitalized in two files, lowercase prose in one.
        assert_eq!(get("Vesper").unwrap().count, 2);
        assert_eq!(get("NVIDIA").unwrap().source, TermSource::Documentation);
        // Sentence-initial only ("Gemma ran." after a semicolon-free sentence
        // end), headings, fences, bold labels and table cells do not count.
        assert!(get("Gemma").is_none());
        assert!(get("Write").is_none());
        assert!(get("Monday").is_none());
        let mut legal = Accumulator::default();
        legal.add_file(
            "a.md",
            "Provided without WARRANTY or LIABILITY of any kind.\n",
        );
        legal.add_file("b.md", "No WARRANTY and no LIABILITY here either.\n");
        legal.add_file(
            "LICENSE.md",
            "Copyright Zorblax Holdings. Zorblax Holdings.\n",
        );
        legal.add_file("LICENSE-APACHE.txt", "Zorblax Holdings again.\n");
        assert!(legal.admit("ws").is_empty());
        assert!(get("point").is_none() && get("Point").is_none());
    }

    #[test]
    fn manifests_yield_package_and_dependency_names() {
        let cargo = "[package]\nname = \"alpha-crate\"\n[dependencies]\nserde = { version = \"1\" }\nrusqlite.workspace = true\n[dependencies.tokio_util]\nversion = \"1\"\n";
        assert_eq!(
            manifest_names(ManifestKind::Cargo, cargo),
            ["alpha-crate", "serde", "rusqlite", "tokio_util"]
        );
        let npm =
            r#"{"name":"@scope/app","dependencies":{"svelte":"5"},"devDependencies":{"vite":"6"}}"#;
        assert_eq!(
            manifest_names(ManifestKind::Npm, npm),
            ["@scope/app", "svelte", "vite"]
        );
        let py = "[project]\nname = \"beta-tool\"\ndependencies = [\n  \"numpy>=2\",\n  \"torch[cuda] ; sys_platform == 'linux'\",\n]\n[tool.ruff]\nline-length = 100\n";
        assert_eq!(
            manifest_names(ManifestKind::Python, py),
            ["beta-tool", "numpy", "torch"]
        );
    }

    #[test]
    fn scanning_a_git_repository_is_bounded_resumable_and_incremental() {
        let Some(git) = git_binary() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let run = |args: &[&str]| {
            assert!(std::process::Command::new(&git)
                .arg("-C")
                .arg(root)
                .args(args)
                .stdout(std::process::Stdio::null())
                .status()
                .unwrap()
                .success());
        };
        run(&["init", "-q"]);
        for i in 0..5 {
            std::fs::write(
                root.join(format!("f{i}.rs")),
                format!("fn zorbQuux{i}() {{}}\nfn sharedHelper() {{}}\n"),
            )
            .unwrap();
        }
        std::fs::write(root.join("big.rs"), "x".repeat(MAX_FILE_BYTES as usize + 1)).unwrap();
        std::fs::write(root.join("untracked.rs"), "fn sharedHelper() {}").unwrap();
        run(&["add", "f0.rs", "f1.rs", "f2.rs", "f3.rs", "f4.rs", "big.rs"]);
        let mut cursor = WorkspaceCursor::default();
        let budget = ScanBudget {
            max_files: 2,
            max_bytes: u64::MAX,
        };
        let never = || false;
        let first = scan_workspace(&git, root, "ws", &mut cursor, budget, &never).unwrap();
        assert!(first.stopped && !first.completed);
        assert_eq!(first.files_read, 2);
        assert!(!serde_json::to_string(&cursor).unwrap().contains(".rs"));
        let mut report = first;
        while !report.completed {
            report = scan_workspace(&git, root, "ws", &mut cursor, budget, &never).unwrap();
        }
        let admitted = report.admitted.unwrap();
        assert_eq!(
            admitted
                .iter()
                .map(|t| t.surface.as_str())
                .collect::<Vec<_>>(),
            ["sharedHelper"]
        );
        assert_eq!(admitted[0].count, 5);
        let again = scan_workspace(&git, root, "ws", &mut cursor, budget, &never).unwrap();
        assert!(again.unchanged && again.files_read == 0);
        let stop = || true;
        std::fs::write(root.join("f0.rs"), "fn changed() {}").unwrap();
        let stopped = scan_workspace(&git, root, "ws", &mut cursor, budget, &stop).unwrap();
        assert!(stopped.stopped && stopped.files_read == 0);
    }
}
