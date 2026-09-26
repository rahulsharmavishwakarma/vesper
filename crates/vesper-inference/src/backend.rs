//! Shared native device selection. Resolve once per load, never per token/frame.
use crate::{check_cancelled, native, InferenceError, Result};
use llama_cpp_2::{model::params::LlamaModelParams, LlamaBackendDevice, LlamaBackendDeviceType};
use std::sync::atomic::AtomicBool;
use vesper_core::ComputeBackend;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Plan {
    pub(crate) provider: &'static str,
    pub(crate) device: Option<usize>,
}

impl Plan {
    pub(crate) const CPU: Self = Self {
        provider: "cpu",
        device: None,
    };

    pub(crate) fn gpu_layers(self) -> u32 {
        if self.device.is_some() {
            u32::MAX
        } else {
            0
        }
    }

    pub(crate) fn model_params(self) -> Result<LlamaModelParams> {
        let devices: Vec<_> = self.device.into_iter().collect();
        LlamaModelParams::default()
            .with_n_gpu_layers(self.gpu_layers())
            .with_devices(&devices)
            .map_err(|error| native("native device selection", error))
    }
}

pub(crate) fn resolve(requested: ComputeBackend) -> Result<Plan> {
    if requested == ComputeBackend::Cpu {
        return Ok(Plan::CPU);
    }
    select(requested, crate::capabilities::native_devices())
}

fn select(requested: ComputeBackend, devices: &[LlamaBackendDevice]) -> Result<Plan> {
    if requested == ComputeBackend::Cpu {
        return Ok(Plan::CPU);
    }
    for (backend, registry, provider) in [
        (ComputeBackend::Metal, "MTL", "metal"),
        (ComputeBackend::Cuda, "CUDA", "cuda"),
        (ComputeBackend::Vulkan, "Vulkan", "vulkan"),
        (ComputeBackend::Rocm, "ROCm", "rocm"),
    ] {
        if requested != ComputeBackend::Auto && requested != backend {
            continue;
        }
        if let Some(device) = devices.iter().find(|device| {
            device.backend == registry
                && device.memory_total > 0
                && !device.description.trim().is_empty()
                && matches!(
                    device.device_type,
                    LlamaBackendDeviceType::Gpu | LlamaBackendDeviceType::IntegratedGpu
                )
        }) {
            return Ok(Plan {
                provider,
                device: Some(device.index),
            });
        }
    }
    if requested == ComputeBackend::Auto {
        Ok(Plan::CPU)
    } else {
        Err(native(
            "native device selection",
            format!(
            "requested {requested:?} GPU is unavailable in this runtime; select Automatic or CPU"
        ),
        ))
    }
}

/// Retry the complete initialization (load AND context probe) only in Auto.
/// Callers must let a failed attempt drop all native handles before returning.
pub(crate) fn load<T>(
    requested: ComputeBackend,
    plan: Plan,
    cancelled: &AtomicBool,
    mut initialize: impl FnMut(Plan) -> Result<T>,
) -> Result<(T, Plan, Option<String>)> {
    check_cancelled(cancelled)?;
    match initialize(plan) {
        Ok(model) => {
            check_cancelled(cancelled)?;
            Ok((model, plan, None))
        }
        Err(InferenceError::Cancelled) => Err(InferenceError::Cancelled),
        Err(error) if requested == ComputeBackend::Auto && plan.device.is_some() => {
            check_cancelled(cancelled)?;
            let reason = error.to_string();
            eprintln!(
                "automatic {} initialization failed; retrying CPU: {reason}",
                plan.provider
            );
            let model = initialize(Plan::CPU).map_err(|cpu_error| match cpu_error {
                InferenceError::Cancelled => InferenceError::Cancelled,
                _ => native(
                    "native CPU fallback",
                    format!("GPU attempt failed ({reason}); CPU fallback failed ({cpu_error})"),
                ),
            })?;
            check_cancelled(cancelled)?;
            Ok((model, Plan::CPU, Some(reason)))
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(index: usize, backend: &str) -> LlamaBackendDevice {
        LlamaBackendDevice {
            index,
            name: format!("GPU{index}"),
            description: "test GPU".into(),
            backend: backend.into(),
            memory_total: 1024,
            memory_free: 512,
            device_type: LlamaBackendDeviceType::Gpu,
        }
    }

    #[test]
    fn explicit_backend_selects_matching_device_and_never_silently_uses_cpu() {
        let devices = [gpu(2, "Vulkan"), gpu(5, "CUDA"), gpu(7, "MTL")];
        assert_eq!(
            select(ComputeBackend::Cuda, &devices).unwrap().device,
            Some(5)
        );
        assert_eq!(
            select(ComputeBackend::Auto, &devices).unwrap().device,
            Some(7)
        );
        assert_eq!(select(ComputeBackend::Cpu, &devices).unwrap(), Plan::CPU);
        assert_eq!(select(ComputeBackend::Auto, &[]).unwrap(), Plan::CPU);
        assert!(select(ComputeBackend::Metal, &[]).is_err());
        let mut empty = gpu(0, "MTL");
        empty.memory_total = 0;
        assert_eq!(select(ComputeBackend::Auto, &[empty]).unwrap(), Plan::CPU);
    }

    #[test]
    fn auto_retries_load_or_probe_failure_and_preserves_both_errors() {
        let plan = Plan {
            provider: "metal",
            device: Some(0),
        };
        let cancelled = AtomicBool::new(false);
        let mut attempts = Vec::new();
        let (_, selected, reason) = load(ComputeBackend::Auto, plan, &cancelled, |p| {
            attempts.push(p);
            if p.device.is_some() {
                Err(native("load", "GPU allocation failed"))
            } else {
                Ok(())
            }
        })
        .unwrap();
        assert_eq!(attempts, [plan, Plan::CPU]);
        assert_eq!(selected, Plan::CPU);
        assert!(reason.unwrap().contains("GPU allocation failed"));
        let error = load::<()>(ComputeBackend::Auto, plan, &cancelled, |p| {
            Err(native(
                "load",
                if p.device.is_some() {
                    "GPU failure"
                } else {
                    "CPU failure"
                },
            ))
        })
        .unwrap_err()
        .to_string();
        assert!(error.contains("GPU failure") && error.contains("CPU failure"));
    }

    #[test]
    fn explicit_failure_and_cancellation_never_retry() {
        let plan = Plan {
            provider: "metal",
            device: Some(0),
        };
        for (requested, cancel) in [(ComputeBackend::Metal, false), (ComputeBackend::Auto, true)] {
            let mut attempts = 0;
            let result = load::<()>(requested, plan, &AtomicBool::new(false), |_| {
                attempts += 1;
                if cancel {
                    Err(InferenceError::Cancelled)
                } else {
                    Err(native("probe", "failed"))
                }
            });
            assert!(result.is_err());
            assert_eq!(attempts, 1);
        }
    }
}
