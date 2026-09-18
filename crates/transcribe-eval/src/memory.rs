//! Development-only corpus-reference memory. Never writes the user's profile.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs, path::Path};
use transcribe_core::{PersonalizationProfile, VocabularyEntry};
use transcribe_eval::{score, BenchmarkManifest};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvaluationMemory {
    pub schema_version: u32,
    pub profile: PersonalizationProfile,
    pub calibration_manifest_sha256: String,
    pub calibration_report_sha256: String,
    pub source_audio_hashes: Vec<String>,
    pub source_references: Vec<String>,
}

impl EvaluationMemory {
    pub fn validate_holdout(&self, manifest: &BenchmarkManifest) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("unsupported evaluation-memory schema".into());
        }
        self.profile.validate()?;
        let audio = self.source_audio_hashes.iter().collect::<HashSet<_>>();
        let references = self.source_references.iter().collect::<HashSet<_>>();
        for sample in &manifest.samples {
            if audio.contains(&sample.sha256)
                || references.contains(&score(&sample.reference, "").normalized_reference)
            {
                return Err(format!(
                    "calibration/holdout overlap at sample {}",
                    sample.id
                ));
            }
        }
        Ok(())
    }
}

pub fn build(
    manifest_path: &str,
    report_path: &str,
    output: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let manifest_bytes = fs::read(manifest_path)?;
    let manifest = BenchmarkManifest::from_path(Path::new(manifest_path))?;
    let report_bytes = fs::read(report_path)?;
    let report: serde_json::Value = serde_json::from_slice(&report_bytes)?;
    let manifest_hash = format!("{:x}", Sha256::digest(&manifest_bytes));
    if report["manifestSha256"].as_str() != Some(manifest_hash.as_str()) {
        return Err("calibration report does not match the supplied manifest".into());
    }
    if !report["personalization"].is_null() {
        return Err("build memory only from a baseline calibration run".into());
    }
    let runs = report["runs"]
        .as_array()
        .ok_or("missing calibration runs")?;
    let mut profile = PersonalizationProfile {
        revision: 1,
        ..Default::default()
    };
    for sample in &manifest.samples {
        let Some(run) = runs
            .iter()
            .find(|r| r["sampleId"].as_str() == Some(&sample.id))
        else {
            return Err(format!("missing calibration run {}", sample.id).into());
        };
        if !run["error"].is_null() {
            continue;
        }
        let Some(raw) = run["rawHypothesis"].as_str() else {
            continue;
        };
        for pair in transcribe_core::correction_suggestions(raw, &sample.reference) {
            if transcribe_eval::word_edit_distance(&pair.heard, &pair.intended) == 0 {
                continue;
            }
            let language = super::language_for_sample(&sample.language);
            if let Some(entry) = profile
                .vocabulary
                .iter_mut()
                .find(|e| e.phrase == pair.intended && e.language == language)
            {
                if !entry.aliases.contains(&pair.heard) && entry.aliases.len() < 8 {
                    entry.aliases.push(pair.heard);
                }
            } else if profile.vocabulary.len() < 128 {
                profile.vocabulary.push(VocabularyEntry {
                    id: format!("corpus-{}", profile.vocabulary.len()),
                    phrase: pair.intended,
                    aliases: vec![pair.heard],
                    language,
                    enabled: true,
                    // Corpus references simulate confirmation; these are not app review IDs.
                    learned_at: chrono::Utc::now().to_rfc3339(),
                    ..Default::default()
                });
            }
        }
    }
    profile.validate()?;
    let memory = EvaluationMemory {
        schema_version: 1,
        profile,
        calibration_manifest_sha256: manifest_hash,
        calibration_report_sha256: format!("{:x}", Sha256::digest(&report_bytes)),
        source_audio_hashes: manifest.samples.iter().map(|s| s.sha256.clone()).collect(),
        source_references: manifest
            .samples
            .iter()
            .map(|s| score(&s.reference, "").normalized_reference)
            .collect(),
    };
    fs::write(output, serde_json::to_vec_pretty(&memory)?)?;
    println!(
        "froze {} corpus-reference memories",
        memory.profile.vocabulary.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn holdout_rejects_different_audio_with_the_same_prompt() {
        let manifest: BenchmarkManifest = serde_json::from_value(serde_json::json!({
            "schemaVersion":1,"sources":[],"samples":[{"id":"test","sourceId":"public","split":"test",
            "language":"en-US","audioPath":"test.wav","sha256":"a".repeat(64),"reference":"Run kubectl!",
            "durationSeconds":1,"scenario":"read","replayKind":"natural"}]
        })).unwrap();
        let mut memory = EvaluationMemory {
            schema_version: 1,
            profile: Default::default(),
            calibration_manifest_sha256: String::new(),
            calibration_report_sha256: String::new(),
            source_audio_hashes: vec![],
            source_references: vec!["run kubectl".into()],
        };
        assert!(memory.validate_holdout(&manifest).is_err());
        memory.source_references.clear();
        memory.source_audio_hashes.push("a".repeat(64));
        assert!(memory.validate_holdout(&manifest).is_err());
        memory.source_audio_hashes.clear();
        assert!(memory.validate_holdout(&manifest).is_ok());
    }
}
