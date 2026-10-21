//! One-time discovery of GPU backends exposed by the bundled runtime.
//! This initializes driver registries, never model weights or another LlamaBackend.
use llama_cpp_2::{list_llama_ggml_backend_devices, LlamaBackendDevice, LlamaBackendDeviceType};
use serde::Serialize;
use std::sync::OnceLock;
use vesper_core::ComputeBackend;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceleratorBackend {
    pub backend: ComputeBackend,
    pub compiled: bool,
    pub devices: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceleratorCapabilities {
    pub os: &'static str,
    pub arch: &'static str,
    pub backends: Vec<AcceleratorBackend>,
    pub speech_metal_models: Vec<&'static str>,
}

static CAPABILITIES: OnceLock<AcceleratorCapabilities> = OnceLock::new();
static DEVICES: OnceLock<Vec<LlamaBackendDevice>> = OnceLock::new();

pub(crate) fn native_devices() -> &'static [LlamaBackendDevice] {
    DEVICES.get_or_init(list_llama_ggml_backend_devices)
}

/// Call from a background thread. Native enumeration can initialize GPU driver
/// contexts; results are cached for this process and are not model attestation.
pub fn accelerator_capabilities() -> &'static AcceleratorCapabilities {
    CAPABILITIES.get_or_init(|| {
        let devices = native_devices();
        AcceleratorCapabilities {
            speech_metal_models: {
                #[cfg(feature = "mlx")]
                {
                    [
                        vesper_core::AsrModel::ParakeetTdt06bV3,
                        vesper_core::AsrModel::Qwen3Asr06b,
                        vesper_core::AsrModel::WhisperSmall,
                        vesper_core::AsrModel::GraniteSpeech5TurboCtc,
                        vesper_core::AsrModel::Phonon2,
                    ]
                    .into_iter()
                    .filter(|model| {
                        vesper_core::mlx_speech_supported(*model)
                            && vesper_models::mlx::runtime_ready()
                    })
                    .map(|model| model.id())
                    .collect()
                }
                #[cfg(not(feature = "mlx"))]
                {
                    Vec::new()
                }
            },
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            backends: [
                (
                    ComputeBackend::Metal,
                    "MTL",
                    cfg!(all(target_os = "macos", target_arch = "aarch64")),
                ),
                (ComputeBackend::Cuda, "CUDA", cfg!(feature = "cuda")),
                (ComputeBackend::Vulkan, "Vulkan", cfg!(feature = "vulkan")),
                (ComputeBackend::Rocm, "ROCm", cfg!(feature = "rocm")),
            ]
            .into_iter()
            .map(|(backend, name, compiled)| AcceleratorBackend {
                backend,
                compiled,
                devices: detected_devices(devices, name, compiled),
            })
            .collect(),
        }
    })
}

fn detected_devices(devices: &[LlamaBackendDevice], backend: &str, compiled: bool) -> Vec<String> {
    let mut names = Vec::new();
    for device in devices.iter().filter(|device| {
        compiled
            && device.backend == backend
            && device.memory_total > 0
            && !device.description.trim().is_empty()
            && matches!(
                device.device_type,
                LlamaBackendDeviceType::Gpu | LlamaBackendDeviceType::IntegratedGpu
            )
    }) {
        if !names.contains(&device.description) {
            names.push(device.description.clone());
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detection_excludes_cpu_empty_metal_slots_and_uncompiled_backends() {
        let gpu = LlamaBackendDevice {
            index: 0,
            name: "MTL0".into(),
            description: "Synthetic Apple GPU".into(),
            backend: "MTL".into(),
            memory_total: 1024,
            memory_free: 512,
            device_type: LlamaBackendDeviceType::IntegratedGpu,
        };
        let mut cpu = gpu.clone();
        cpu.device_type = LlamaBackendDeviceType::Cpu;
        let mut empty = gpu.clone();
        empty.description.clear();
        empty.memory_total = 0;
        let devices = [cpu, empty, gpu.clone(), gpu];
        assert_eq!(
            detected_devices(&devices, "MTL", true),
            vec!["Synthetic Apple GPU"]
        );
        assert!(detected_devices(&devices, "MTL", false).is_empty());
        assert!(detected_devices(&devices, "ROCm", true).is_empty());
    }

    #[test]
    #[ignore = "explicit native GPU registry observation; no model loading"]
    fn observe_native_accelerators() {
        let first = accelerator_capabilities();
        assert!(std::ptr::eq(first, accelerator_capabilities()));
        assert_eq!(first.os, std::env::consts::OS);
        println!("{}", serde_json::to_string(first).unwrap());
    }
}
