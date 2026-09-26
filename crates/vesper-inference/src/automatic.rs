//! Bounded same-model initialization retry, shared by resident and one-shot calls.
use crate::{
    adapters, check_cancelled, resident_sherpa::verification_error, InferenceError, Result,
};
use std::{path::Path, sync::atomic::AtomicBool};
use vesper_core::{AppConfig, ComputeBackend};
use vesper_models::SpeechSelection;

#[derive(Default)]
pub(crate) struct InitializationFallback {
    failed: Option<(adapters::SpeechKey, String)>,
}

impl InitializationFallback {
    pub(crate) fn load<T>(
        &mut self,
        selection: &mut SpeechSelection,
        root: &Path,
        cancelled: &AtomicBool,
        mut load: impl FnMut(&AppConfig) -> Result<T>,
    ) -> Result<T> {
        check_cancelled(cancelled)?;
        let automatic_gpu = selection.requested_backend == ComputeBackend::Auto
            && selection.config.uses_mlx_speech();
        let key = if automatic_gpu {
            Some(adapters::speech_key(&selection.config, root)?)
        } else {
            None
        };
        if let Some((failed, reason)) = &self.failed {
            if Some(failed) == key.as_ref() {
                selection
                    .cpu_fallback(root, reason.clone())
                    .map_err(verification_error)?;
                return load(&selection.config);
            }
        }
        self.failed = None;
        match load(&selection.config) {
            Err(InferenceError::AcceleratorInitialization(reason)) if automatic_gpu => {
                check_cancelled(cancelled)?;
                // The failed native load has returned and dropped its resources.
                // Do not retry verification, cancellation or decoding failures.
                selection
                    .cpu_fallback(root, reason.clone())
                    .map_err(verification_error)?;
                self.failed = key.map(|key| (key, reason));
                load(&selection.config)
            }
            result => result,
        }
    }
}

#[cfg(all(test, feature = "mlx"))]
mod tests {
    use super::*;
    use std::{fs::File, sync::atomic::Ordering};
    use vesper_core::AsrModel;

    fn selection() -> SpeechSelection {
        let mut config = AppConfig::default();
        config.models.asr = AsrModel::Qwen3Asr06b;
        config.compute_backend = ComputeBackend::Metal;
        SpeechSelection {
            config,
            requested_backend: ComputeBackend::Auto,
            preferred_backend: ComputeBackend::Metal,
            fallback_reason: None,
        }
    }

    fn cpu_files(root: &Path) {
        let mut config = selection().config;
        config.compute_backend = ComputeBackend::Cpu;
        for a in vesper_models::selected(&config)
            .unwrap()
            .into_iter()
            .filter(|a| a.role == "asr")
        {
            let path = root.join(a.relative_path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            File::create(path).unwrap().set_len(a.size_bytes).unwrap();
        }
    }

    #[test]
    fn initialization_retry_is_same_model_bounded_and_invalidated_by_selection() {
        let root = tempfile::tempdir().unwrap();
        cpu_files(root.path());
        let flag = AtomicBool::new(false);
        let mut cache = InitializationFallback::default();
        let mut backends = vec![];
        for pass in 0..2 {
            let mut chosen = selection();
            cache
                .load(&mut chosen, root.path(), &flag, |config| {
                    backends.push(config.compute_backend);
                    assert_eq!(config.models.asr, AsrModel::Qwen3Asr06b);
                    if config.uses_mlx_speech() {
                        Err(InferenceError::AcceleratorInitialization("fixture".into()))
                    } else {
                        Ok(())
                    }
                })
                .unwrap();
            assert_eq!(chosen.config.compute_backend, ComputeBackend::Cpu);
            assert!(chosen.fallback_reason.is_some());
            assert_eq!(backends.len(), pass + 2);
        }
        assert_eq!(
            backends,
            [
                ComputeBackend::Metal,
                ComputeBackend::Cpu,
                ComputeBackend::Cpu
            ]
        );
        let mut manual = selection();
        manual.requested_backend = ComputeBackend::Metal;
        cache
            .load(&mut manual, root.path(), &flag, |_| Ok(()))
            .unwrap();
        assert!(cache.failed.is_none());
    }

    #[test]
    fn cancellation_verification_decode_and_explicit_errors_never_retry() {
        let root = tempfile::tempdir().unwrap();
        cpu_files(root.path());
        for scenario in 0..5 {
            let flag = AtomicBool::new(false);
            let mut selected = selection();
            if scenario == 3 {
                selected.requested_backend = ComputeBackend::Metal;
            }
            let mut calls = 0;
            let result: Result<()> =
                InitializationFallback::default().load(&mut selected, root.path(), &flag, |_| {
                    calls += 1;
                    match scenario {
                        0 => Err(InferenceError::Cancelled),
                        1 => Err(crate::native("verification", "hash mismatch")),
                        2 => Err(crate::native("MLX speech", "incomplete decoding")),
                        _ => {
                            if scenario == 4 {
                                flag.store(true, Ordering::Release);
                            }
                            Err(InferenceError::AcceleratorInitialization("fixture".into()))
                        }
                    }
                });
            assert!(result.is_err());
            assert_eq!(calls, 1);
            assert_eq!(selected.config.compute_backend, ComputeBackend::Metal);
        }
    }

    #[test]
    fn missing_cpu_and_corrupt_cpu_are_not_successful_fallbacks() {
        let root = tempfile::tempdir().unwrap();
        let flag = AtomicBool::new(false);
        let mut calls = 0;
        let result: Result<()> =
            InitializationFallback::default().load(&mut selection(), root.path(), &flag, |_| {
                calls += 1;
                Err(InferenceError::AcceleratorInitialization("fixture".into()))
            });
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("CPU model files are not installed"));
        assert_eq!(calls, 1);
        cpu_files(root.path());
        let result: Result<()> = InitializationFallback::default().load(
            &mut selection(),
            root.path(),
            &flag,
            |config| {
                if config.uses_mlx_speech() {
                    Err(InferenceError::AcceleratorInitialization("fixture".into()))
                } else {
                    vesper_models::verify_asr(config, root.path(), &flag)
                        .map_err(verification_error)
                }
            },
        );
        assert!(result.unwrap_err().to_string().contains("integrity"));
    }
}
