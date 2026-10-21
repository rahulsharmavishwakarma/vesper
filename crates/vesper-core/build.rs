//! Generate adapter language tables from models/languages.json.
//!
//! The manifest is the single human-edited source of record (Refs #100).
//! This script emits per-model `&[Language]` consts, the (runtime, model,
//! ISO code) lookup for inference backends, and the built-in writer table.
//! Anything malformed fails the build with the offending key named, so a
//! bad manifest can never ship a silently wrong capability set.
use std::env;
use std::fs;
use std::path::PathBuf;

const MANIFEST: &str = "../../models/languages.json";

/// ISO code -> Language variant. Must mirror `Language::code`; the build
/// fails closed on any manifest code missing here.
const VARIANTS: &[(&str, &str)] = &[
    ("ar", "Arabic"),
    ("bg", "Bulgarian"),
    ("hr", "Croatian"),
    ("cs", "Czech"),
    ("da", "Danish"),
    ("nl", "Dutch"),
    ("en", "English"),
    ("et", "Estonian"),
    ("fi", "Finnish"),
    ("fr", "French"),
    ("de", "German"),
    ("el", "Greek"),
    ("hu", "Hungarian"),
    ("hi", "Hindi"),
    ("it", "Italian"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("lv", "Latvian"),
    ("lt", "Lithuanian"),
    ("ms", "Malay"),
    ("mt", "Maltese"),
    ("nb", "NorwegianBokmal"),
    ("fa", "Persian"),
    ("pl", "Polish"),
    ("pt", "Portuguese"),
    ("ro", "Romanian"),
    ("sk", "Slovak"),
    ("sl", "Slovenian"),
    ("es", "Spanish"),
    ("sv", "Swedish"),
    ("zh", "Chinese"),
    ("yue", "Cantonese"),
    ("fil", "Filipino"),
    ("id", "Indonesian"),
    ("mk", "Macedonian"),
    ("ru", "Russian"),
    ("th", "Thai"),
    ("tr", "Turkish"),
    ("uk", "Ukrainian"),
    ("vi", "Vietnamese"),
];

/// Manifest model id -> generated const. Adding a model wires one line here
/// plus its ADAPTERS registration; the language data itself stays in JSON.
const TABLES: &[(&str, &str)] = &[
    ("nvidia/parakeet-tdt-0.6b-v3", "PARAKEET_LANGUAGES"),
    (
        "nvidia/nemotron-3.5-asr-streaming-0.6b",
        "NEMOTRON_LANGUAGES",
    ),
    ("Qwen/Qwen3-ASR-0.6B", "QWEN_ASR_LANGUAGES"),
    ("CohereLabs/cohere-transcribe-03-2026", "COHERE_LANGUAGES"),
    ("openai/whisper-small", "WHISPER_LANGUAGES"),
    ("openai/whisper-large-v3-turbo", "WHISPER_LANGUAGES"),
    ("LiquidAI/LFM2.5-Audio-1.5B", "LFM_LANGUAGES"),
    ("moonshine-ai/moonshine-base", "MOONSHINE_LANGUAGES"),
    (
        "ibm-granite/granite-speech-5.0-470m-turboctc",
        "GRANITE_LANGUAGES",
    ),
    ("FermionResearch/Phonon-2", "PHONON2_LANGUAGES"),
    ("moondream/parakeet-redux", "REDUX_LANGUAGES"),
];

fn variant(code: &str) -> &str {
    VARIANTS
        .iter()
        .find(|(c, _)| *c == code)
        .unwrap_or_else(|| panic!("{MANIFEST}: unknown language code '{code}'"))
        .1
}

fn main() {
    let path: PathBuf = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join(MANIFEST);
    println!("cargo:rerun-if-changed={}", path.display());
    let text = fs::read_to_string(&path).expect("models/languages.json must be readable");
    let manifest: serde_json::Value =
        serde_json::from_str(&text).expect("models/languages.json must be valid JSON");
    let table = manifest
        .as_object()
        .expect("models/languages.json must be an object");

    let mut out = String::from(
        "/// Generated from models/languages.json by crates/vesper-core/build.rs. Do not edit.\nuse crate::Language;\n",
    );

    let mut emitted: Vec<&str> = Vec::new();
    for (id, name) in TABLES {
        let entry = table
            .get(*id)
            .unwrap_or_else(|| panic!("{MANIFEST}: missing entry for '{id}'"));
        // Shared tables (both Whisper checkpoints) are emitted once.
        let fresh = !emitted.contains(name);
        emitted.push(name);
        let codes: Vec<&str> = entry["languages"]
            .as_array()
            .unwrap_or_else(|| panic!("{MANIFEST}: '{id}' needs a languages array"))
            .iter()
            .map(|v| {
                v.as_str()
                    .unwrap_or_else(|| panic!("{MANIFEST}: '{id}' language codes must be strings"))
            })
            .collect();
        assert!(
            !codes.is_empty(),
            "{MANIFEST}: '{id}' languages must not be empty"
        );
        let auto = entry["autoDetect"]
            .as_bool()
            .unwrap_or_else(|| panic!("{MANIFEST}: '{id}' needs autoDetect"));
        let mut items = Vec::new();
        if auto {
            items.push("Language::Auto".to_string());
        }
        for code in &codes {
            items.push(format!("Language::{}", variant(code)));
        }
        if fresh {
            out.push_str(&format!(
                "\npub(crate) const {name}: &[Language] = &[{items}];\n",
                items = items.join(", ")
            ));
        }

        if let Some(runtimes) = entry.get("runtimeCodes").and_then(|v| v.as_object()) {
            for (runtime, map) in runtimes {
                let map = map.as_object().unwrap_or_else(|| {
                    panic!("{MANIFEST}: '{id}' runtimeCodes['{runtime}'] must be an object")
                });
                for (code, value) in map {
                    if !codes.contains(&code.as_str()) {
                        panic!("{MANIFEST}: '{id}' runtimeCodes['{runtime}'] key '{code}' is outside languages");
                    }
                    if !value.is_string() {
                        panic!("{MANIFEST}: '{id}' runtimeCodes['{runtime}']['{code}'] must be a string");
                    }
                }
            }
        }
    }

    out.push_str(
        "\n/// Inference runtime code for an explicit ISO language, if the runtime\n/// takes one for this model. Missing entries mean auto-detect or reject.\npub fn runtime_language_code(runtime: &str, model_id: &str, iso: &str) -> Option<&'static str> {\n    match (runtime, model_id, iso) {\n",
    );
    for (id, _) in TABLES {
        let entry = &table[*id];
        if let Some(runtimes) = entry.get("runtimeCodes").and_then(|v| v.as_object()) {
            for (runtime, map) in runtimes {
                for (code, value) in map.as_object().unwrap() {
                    out.push_str(&format!(
                        "        (\"{runtime}\", \"{id}\", \"{code}\") => Some(\"{value}\"),\n",
                        value = value.as_str().unwrap()
                    ));
                }
            }
        }
    }
    out.push_str("        _ => None,\n    }\n}\n");

    out.push_str(
        "\n/// Built-in writer language support from the manifest 'writers' section.\n/// `unrestricted` marks writers whose vendor publishes no per-language list:\n/// all application modes stay allowed and nothing is invented.\npub(crate) struct GeneratedWriter {\n    pub(crate) id: &'static str,\n    pub(crate) unrestricted: bool,\n    pub(crate) languages: &'static [Language],\n}\n",
    );
    let writers = table
        .get("writers")
        .and_then(|v| v.as_object())
        .unwrap_or_else(|| panic!("{MANIFEST}: missing 'writers' section"));
    let mut entries = Vec::new();
    for (id, entry) in writers {
        let unrestricted = entry["unrestricted"].as_bool().unwrap_or(false);
        let codes: Vec<&str> = entry["languages"]
            .as_array()
            .unwrap_or_else(|| panic!("{MANIFEST}: writer '{id}' needs a languages array"))
            .iter()
            .map(|v| {
                v.as_str().unwrap_or_else(|| {
                    panic!("{MANIFEST}: writer '{id}' language codes must be strings")
                })
            })
            .collect();
        if !unrestricted && codes.is_empty() {
            panic!("{MANIFEST}: writer '{id}' must declare languages or set unrestricted");
        }
        let items: Vec<String> = codes
            .iter()
            .map(|c| format!("Language::{}", variant(c)))
            .collect();
        entries.push(format!(
            "    GeneratedWriter {{ id: \"{id}\", unrestricted: {unrestricted}, languages: &[{items}] }},",
            items = items.join(", ")
        ));
    }
    out.push_str(&format!(
        "\npub(crate) const GENERATED_WRITERS: &[GeneratedWriter] = &[\n{entries}\n];\n",
        entries = entries.join("\n")
    ));
    out.push_str(
        "\npub(crate) fn generated_writer(id: &str) -> Option<&'static GeneratedWriter> {\n    GENERATED_WRITERS.iter().find(|w| w.id == id)\n}\n",
    );

    let dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(dir.join("language_tables.generated.rs"), out).unwrap();
}
