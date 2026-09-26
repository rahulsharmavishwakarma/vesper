use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel {
    Unsupported,
    Official,
    LocallyMeasured,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapability {
    pub model_id: String,
    pub display_name: String,
    pub languages: Vec<String>,
    pub modes: Vec<String>,
    pub backends: Vec<String>,
    pub supports_auto_language: bool,
    pub supports_code_switching: bool,
    pub supports_singing: bool,
    pub maximum_seconds: Option<u32>,
    pub executable: bool,
    pub evidence_url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkEvidence {
    pub model_id: String,
    pub language: String,
    pub configured_language: String,
    pub mode: String,
    pub backend: String,
    pub median_word_error_rate: f64,
    pub median_real_time_factor: Option<f64>,
    pub p95_stop_to_final_ms: Option<f64>,
    pub peak_memory_bytes: Option<u64>,
    pub successful_samples: u32,
    pub failed_samples: u32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationPreference {
    Accuracy,
    Latency,
    #[default]
    Balanced,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationRequest {
    pub language: String,
    pub mode: String,
    pub backend: String,
    pub duration_seconds: u32,
    pub require_auto_language: bool,
    pub require_code_switching: bool,
    pub require_singing: bool,
    #[serde(default)]
    pub preference: RecommendationPreference,
    pub maximum_median_word_error_rate: Option<f64>,
    pub maximum_median_real_time_factor: Option<f64>,
    pub maximum_p95_stop_to_final_ms: Option<f64>,
    pub maximum_failure_rate: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub model_id: String,
    pub display_name: String,
    pub support: SupportLevel,
    pub score: f64,
    pub reason: String,
    pub locally_measured: bool,
}

pub fn recommend(
    request: &RecommendationRequest,
    capabilities: &[ModelCapability],
    evidence: &[BenchmarkEvidence],
) -> Vec<Recommendation> {
    let configured_language = if request.require_auto_language {
        "auto".to_owned()
    } else {
        requested_languages(&request.language)
            .first()
            .copied()
            .unwrap_or(request.language.as_str())
            .to_owned()
    };
    let comparable = evidence
        .iter()
        .filter(|item| {
            item.language == request.language
                && item.configured_language == configured_language
                && item.mode == request.mode
                && item.backend == request.backend
        })
        .map(|item| (item.model_id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let mut result = capabilities
        .iter()
        .filter(|model| {
            compatible(request, model)
                && comparable
                    .get(model.model_id.as_str())
                    .is_none_or(|item| passes_qualification(request, item))
        })
        .map(|model| {
            let local = comparable.get(model.model_id.as_str()).copied();
            let support = if local.is_some() {
                SupportLevel::LocallyMeasured
            } else {
                SupportLevel::Official
            };
            let score = local.map_or(10_000.0, |item| {
                let failure_penalty = item.failed_samples as f64
                    / f64::from(item.successful_samples + item.failed_samples).max(1.0);
                let accuracy = item.median_word_error_rate * 100.0;
                let latency = item.median_real_time_factor.unwrap_or(f64::INFINITY) * 100.0;
                let objective = match request.preference {
                    RecommendationPreference::Accuracy => accuracy,
                    RecommendationPreference::Latency => latency,
                    RecommendationPreference::Balanced => accuracy + latency,
                };
                objective + failure_penalty * 100.0
            });
            Recommendation {
                model_id: model.model_id.clone(),
                display_name: model.display_name.clone(),
                support,
                score,
                reason: local.map_or_else(
                    || {
                        "Compatible by official capability; no comparable local benchmark yet."
                            .into()
                    },
                    |item| {
                        format!(
                            "Locally measured on {} successful sample(s): median WER {:.1}%, median RTF {}.",
                            item.successful_samples,
                            item.median_word_error_rate * 100.0,
                            item.median_real_time_factor
                                .map_or_else(|| "unavailable".into(), |value| format!("{value:.3}"))
                        )
                    },
                ),
                locally_measured: local.is_some(),
            }
        })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| {
        right
            .support
            .cmp(&left.support)
            .then_with(|| left.score.total_cmp(&right.score))
            .then_with(|| left.display_name.cmp(&right.display_name))
    });
    result
}

fn passes_qualification(request: &RecommendationRequest, evidence: &BenchmarkEvidence) -> bool {
    let total_samples = evidence.successful_samples + evidence.failed_samples;
    let failure_rate = f64::from(evidence.failed_samples) / f64::from(total_samples).max(1.0);

    evidence.successful_samples > 0
        && evidence.median_word_error_rate.is_finite()
        && (request.preference == RecommendationPreference::Accuracy
            || evidence.median_real_time_factor.is_some_and(f64::is_finite))
        && request
            .maximum_median_word_error_rate
            .is_none_or(|maximum| evidence.median_word_error_rate <= maximum)
        && request
            .maximum_median_real_time_factor
            .is_none_or(|maximum| {
                evidence
                    .median_real_time_factor
                    .is_some_and(|value| value.is_finite() && value <= maximum)
            })
        && request.maximum_p95_stop_to_final_ms.is_none_or(|maximum| {
            evidence
                .p95_stop_to_final_ms
                .is_some_and(|value| value.is_finite() && value <= maximum)
        })
        && request
            .maximum_failure_rate
            .is_none_or(|maximum| failure_rate <= maximum)
}

fn compatible(request: &RecommendationRequest, model: &ModelCapability) -> bool {
    model.executable
        && requested_languages(&request.language)
            .iter()
            .all(|requested| model.languages.iter().any(|value| value == requested))
        && model.modes.iter().any(|value| value == &request.mode)
        && model.backends.iter().any(|value| value == &request.backend)
        && (!request.require_auto_language || model.supports_auto_language)
        && (!request.require_code_switching || model.supports_code_switching)
        && (!request.require_singing || model.supports_singing)
        && model
            .maximum_seconds
            .is_none_or(|maximum| request.duration_seconds <= maximum)
}

fn requested_languages(profile: &str) -> Vec<&str> {
    profile
        .split('+')
        .filter_map(|language| language.split(['-', '_']).next())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capability(id: &str, executable: bool) -> ModelCapability {
        ModelCapability {
            model_id: id.into(),
            display_name: id.into(),
            languages: vec!["hi".into()],
            modes: vec!["streaming".into()],
            backends: vec!["cpu".into()],
            supports_auto_language: true,
            supports_code_switching: false,
            supports_singing: false,
            maximum_seconds: None,
            executable,
            evidence_url: "https://example.test/model".into(),
        }
    }

    fn request() -> RecommendationRequest {
        RecommendationRequest {
            language: "hi".into(),
            mode: "streaming".into(),
            backend: "cpu".into(),
            duration_seconds: 300,
            require_auto_language: false,
            require_code_switching: false,
            require_singing: false,
            preference: RecommendationPreference::Balanced,
            maximum_median_word_error_rate: None,
            maximum_median_real_time_factor: None,
            maximum_p95_stop_to_final_ms: None,
            maximum_failure_rate: None,
        }
    }

    #[test]
    fn comparable_local_evidence_outranks_unmeasured_vendor_capability() {
        let evidence = vec![BenchmarkEvidence {
            model_id: "measured".into(),
            language: "hi".into(),
            configured_language: "hi".into(),
            mode: "streaming".into(),
            backend: "cpu".into(),
            median_word_error_rate: 0.1,
            median_real_time_factor: Some(0.2),
            p95_stop_to_final_ms: Some(250.0),
            peak_memory_bytes: Some(100),
            successful_samples: 10,
            failed_samples: 0,
        }];
        let result = recommend(
            &request(),
            &[
                capability("vendor-only", true),
                capability("measured", true),
            ],
            &evidence,
        );
        assert_eq!(result[0].model_id, "measured");
        assert!(result[0].locally_measured);
        assert!(!result[1].locally_measured);
    }

    #[test]
    fn automatic_language_requires_automatic_language_evidence() {
        let evidence = vec![BenchmarkEvidence {
            model_id: "measured".into(),
            language: "hi".into(),
            configured_language: "hi".into(),
            mode: "streaming".into(),
            backend: "cpu".into(),
            median_word_error_rate: 0.1,
            median_real_time_factor: Some(0.2),
            p95_stop_to_final_ms: Some(250.0),
            peak_memory_bytes: Some(100),
            successful_samples: 10,
            failed_samples: 0,
        }];
        let mut auto_request = request();
        auto_request.require_auto_language = true;
        let result = recommend(&auto_request, &[capability("measured", true)], &evidence);
        assert_eq!(result[0].support, SupportLevel::Official);
        assert!(!result[0].locally_measured);
    }

    #[test]
    fn recommendation_preference_separates_accuracy_from_interactive_balance() {
        let evidence = [
            BenchmarkEvidence {
                model_id: "fast".into(),
                language: "hi".into(),
                configured_language: "hi".into(),
                mode: "streaming".into(),
                backend: "cpu".into(),
                median_word_error_rate: 0.1,
                median_real_time_factor: Some(0.05),
                p95_stop_to_final_ms: Some(20.0),
                peak_memory_bytes: Some(100),
                successful_samples: 10,
                failed_samples: 0,
            },
            BenchmarkEvidence {
                model_id: "accurate".into(),
                language: "hi".into(),
                configured_language: "hi".into(),
                mode: "streaming".into(),
                backend: "cpu".into(),
                median_word_error_rate: 0.05,
                median_real_time_factor: Some(0.5),
                p95_stop_to_final_ms: Some(20.0),
                peak_memory_bytes: Some(100),
                successful_samples: 10,
                failed_samples: 0,
            },
        ];
        let capabilities = [capability("fast", true), capability("accurate", true)];
        let mut preference = request();
        preference.preference = RecommendationPreference::Accuracy;
        assert_eq!(
            recommend(&preference, &capabilities, &evidence)[0].model_id,
            "accurate"
        );
        preference.preference = RecommendationPreference::Balanced;
        assert_eq!(
            recommend(&preference, &capabilities, &evidence)[0].model_id,
            "fast"
        );
    }

    #[test]
    fn unavailable_and_incompatible_models_are_not_recommended() {
        let mut wrong_language = capability("wrong", true);
        wrong_language.languages = vec!["en".into()];
        assert!(recommend(
            &request(),
            &[capability("unavailable", false), wrong_language],
            &[]
        )
        .is_empty());
    }

    #[test]
    fn failed_local_qualification_is_not_downgraded_to_official_support() {
        let evidence = vec![BenchmarkEvidence {
            model_id: "measured".into(),
            language: "hi".into(),
            configured_language: "hi".into(),
            mode: "streaming".into(),
            backend: "cpu".into(),
            median_word_error_rate: 0.51,
            median_real_time_factor: Some(0.2),
            p95_stop_to_final_ms: Some(250.0),
            peak_memory_bytes: Some(100),
            successful_samples: 10,
            failed_samples: 0,
        }];
        let mut qualified = request();
        qualified.maximum_median_word_error_rate = Some(0.3);

        assert!(recommend(&qualified, &[capability("measured", true)], &evidence).is_empty());
    }

    #[test]
    fn code_switching_and_duration_are_hard_requirements() {
        let mut limited = capability("limited", true);
        limited.maximum_seconds = Some(60);
        let mut code_switch = request();
        code_switch.duration_seconds = 120;
        code_switch.require_code_switching = true;
        assert!(recommend(&code_switch, &[limited], &[]).is_empty());
    }

    #[test]
    fn locales_match_base_capabilities_and_code_switching_requires_both_languages() {
        let mut english = capability("english", true);
        english.languages = vec!["en".into()];
        let mut indian_english = request();
        indian_english.language = "en-IN".into();
        assert_eq!(recommend(&indian_english, &[english.clone()], &[]).len(), 1);
        let measured = BenchmarkEvidence {
            model_id: "english".into(),
            language: "en-IN".into(),
            configured_language: "en".into(),
            mode: "streaming".into(),
            backend: "cpu".into(),
            median_word_error_rate: 0.1,
            median_real_time_factor: Some(0.2),
            p95_stop_to_final_ms: Some(20.0),
            peak_memory_bytes: Some(100),
            successful_samples: 10,
            failed_samples: 0,
        };
        assert!(recommend(&indian_english, &[english.clone()], &[measured])[0].locally_measured);

        let mut hinglish = request();
        hinglish.language = "hi+en".into();
        hinglish.require_auto_language = true;
        hinglish.require_code_switching = true;
        english.supports_code_switching = true;
        assert!(recommend(&hinglish, &[english.clone()], &[]).is_empty());
        english.languages.push("hi".into());
        assert_eq!(recommend(&hinglish, &[english], &[]).len(), 1);
    }
}
