//! Development-only golden replay of public-corpus-derived and constructed cases.
use serde::Deserialize;
use serde_json::{json, Value};
use std::{env, fs};
use transcribe_core::{correction_suggestions, Language, OutputIntent, PersonalizationProfile};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    id: String,
    category: String,
    profile: PersonalizationProfile,
    input: String,
    language: Language,
    expected: String,
    expected_intent: Option<OutputIntent>,
    reviewed: Option<String>,
    expected_suggestions: Option<usize>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err("usage: memory_replay CASES_JSON RESULTS_JSON".into());
    }
    let source: Value = serde_json::from_slice(&fs::read(&args[0])?)?;
    let cases: Vec<Case> = serde_json::from_value(source["cases"].clone())?;
    let mut results = Vec::new();
    for case in cases {
        case.profile.validate()?;
        let result = case.profile.personalize(&case.input, case.language);
        let intent = case.profile.effective_intent(&case.input, case.language);
        let suggestions = case
            .reviewed
            .as_ref()
            .map(|s| correction_suggestions(&case.input, s));
        let passed = result.text == case.expected
            && case
                .expected_intent
                .is_none_or(|expected| expected == intent)
            && case
                .expected_suggestions
                .is_none_or(|n| suggestions.as_ref().is_some_and(|s| s.len() == n));
        results.push(
            json!({"id":case.id,"category":case.category,"passed":passed,
            "expected":case.expected,"actual":result.text,"evidence":result.evidence,
            "intent":intent,"suggestions":suggestions}),
        );
    }
    let failed = results.iter().filter(|r| r["passed"] == false).count();
    fs::write(
        &args[1],
        serde_json::to_vec_pretty(&json!({"schemaVersion":1,
        "source": source["source"], "cases":results.len(), "failed":failed,"results":results}))?,
    )?;
    println!("{} cases; {failed} failures", results.len());
    if failed != 0 {
        return Err("golden memory replay failed; see result artifact".into());
    }
    Ok(())
}
