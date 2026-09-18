# Device-aware accelerator choices

Screenshots mentioned below are local validation captures excluded from source
control. The written results and reproducible test fixtures remain in the repository.

Models → Device and storage detects the native operating system, application
architecture, and GPU backends exposed by this app build. Automatic and CPU are
always available. Explicit GPU choices are offered only when the corresponding
backend was compiled, a usable GPU was detected, and the existing speech-model
and platform rules permit it.

| Choice | Device/build evidence required | Existing model limits |
| --- | --- | --- |
| Metal | Apple Silicon macOS build with a detected Metal GPU | Explicit GPU selection is unavailable for built-in Parakeet, Qwen3-ASR and Cohere |
| CUDA | Windows/Linux GPU build with a detected CUDA device | Model-specific restrictions still apply |
| Vulkan | Windows/Linux Vulkan build with a detected GPU | Model-specific restrictions still apply |
| AMD ROCm | Windows/Linux ROCm build with a detected GPU | Nemotron's existing profile uses Vulkan for speech; its package requires x86-64 |

Imported speech profiles still run on CPU. With cleanup enabled, the accelerator
preference applies to cleanup; with cleanup off, their menu retains only Automatic
and CPU. An AMD CPU or an AMD vendor label alone never enables ROCm.

A saved unavailable preference stays visible as a disabled entry, accompanied by
an explanation. Detection never changes saved settings. Choose Automatic or CPU
and explicitly save to recover. Loading, failed detection and browser preview do
not invent devices; unknown saved GPU choices remain labelled not checked. Detection
errors have a retry action, and a slow request reports its status after five seconds
while the fallback choices remain usable.

## Scope and implementation

The existing `llama-cpp-2 = 0.1.156` dependency exposes
`list_llama_ggml_backend_devices()`. The app calls it once on a background worker
and caches the result for the process lifetime. It maps MTL/CUDA/Vulkan/ROCm registry
names to the existing preferences, retaining GPU/integrated-GPU records with a
nonempty description and positive total memory. Those checks reject Metal's empty
default-device slot when native detection fails.

Enumeration initializes compiled driver/device registries and can create native
queues/contexts or run tiny capability probes. It does not load model weights,
create another `LlamaBackend`, or change the inference worker's model ownership.
There is no periodic hardware scan or new dependency. Restart after changing GPU
drivers, hardware visibility, or the app build.

This is discovery through the bundled cleanup runtime, not a complete physical
hardware inventory. A CPU-only build cannot discover GPU backends that were not
compiled. The menu conservatively withholds unconfirmed explicit GPU choices even
when a separately downloaded speech runtime might support them. Automatic retains
its existing behavior, including the independent NVIDIA probe and CPU fallback.
These remain accelerator preferences, not device-routing or successful-inference
attestations. Cleanup currently controls layer offload and lets llama.cpp choose
its default device; a build containing several GPU backends does not explicitly
select a device index. This change does not alter routing, cache keys, runtime
packages, fallback behavior, models, or training.

Primary sources: binding revision
[`63e549708237b16b39018288582655539e0a9d5b`](https://github.com/utilityai/llama-cpp-rs/blob/63e549708237b16b39018288582655539e0a9d5b/llama-cpp-2/src/lib.rs),
its [backend initialization guard](https://github.com/utilityai/llama-cpp-rs/blob/63e549708237b16b39018288582655539e0a9d5b/llama-cpp-2/src/llama_backend.rs),
and bundled llama.cpp revision
[`e79e4bf660e19f2ad851e06c6913f7a8c5852621`](https://github.com/ggml-org/llama.cpp/blob/e79e4bf660e19f2ad851e06c6913f7a8c5852621/ggml/src/ggml-backend-reg.cpp).

## Validation

The explicit native probe reported macOS/aarch64, Metal compiled with device
Apple M5 Pro, and CUDA/Vulkan/ROCm not compiled. It confirmed the same cached
snapshot on repeat access and loaded no model. Reproduce with:

```sh
source scripts/dev-env.sh
cargo test --locked -p transcribe-inference capabilities::tests::observe_native_accelerators -- --ignored --exact --nocapture
```

The [observation](validation/accelerator-capabilities.json) is local hardware
evidence. Windows/Linux/NVIDIA/AMD cases are synthetic rule tests, not native
cross-platform qualification. Tests also cover CPU-only builds, empty devices,
unsupported architectures, model restrictions, saved-choice preservation and
detection failure/retry. The menu screenshot
uses synthetic app settings with the native Mac observation as its device fixture.
