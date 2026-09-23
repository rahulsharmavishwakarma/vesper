# Device-aware accelerator choices

Screenshots mentioned below are local validation captures excluded from source
control. The written results and reproducible test fixtures remain in the repository.

Models → Device and storage detects the native operating system, application
architecture, and GPU backends exposed by this app build. Automatic and CPU are
always available. Explicit GPU choices are offered only when the corresponding
backend was compiled, a usable GPU was detected, and the existing speech-model
and platform rules permit it.

| Choice   | Device/build evidence required                      | Existing model limits                                                                                                 |
| -------- | --------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| Metal    | Apple Silicon macOS build with a detected Metal GPU | Native LFM and Nemotron paths are available; optional MLX builds also support explicit English Parakeet/Qwen-ASR; Whisper Small remains evaluation-only |
| CUDA     | Windows/Linux GPU build with a detected CUDA device | Model-specific restrictions still apply                                                                               |
| Vulkan   | Windows/Linux Vulkan build with a detected GPU      | Model-specific restrictions still apply                                                                               |
| AMD ROCm | Windows/Linux ROCm build with a detected GPU        | Nemotron's existing profile uses Vulkan for speech; its package requires x86-64                                       |

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
when a separately downloaded speech runtime might support them.

LFM and cleanup share a native selection plan that chooses a detected device
index matching the requested backend. Automatic tries detected Metal, CUDA,
Vulkan and ROCm in that order; with none available it chooses CPU. Explicit GPU
selection fails if that backend is unavailable. CPU supplies an empty native
device list and disables layer/KQV/operator offload. LFM also requests GPU
execution for its mtmd audio projector, whose public API does not expose exact
device/operator placement.

Automatic may retry complete model/context initialization on CPU if GPU
initialization fails, preserving the fallback reason. Explicit GPU requests and
cancellation never silently retry on CPU. Nemotron retains its separate native
runtime/package selection. A selected GPU or successful context probe is not
proof that every operator executes there; reports retain unknown placement.
The [measured Metal screen](inference-performance.md#native-gpu-implementation-and-screening--23-september-2026)
records exact models, runtime pins, accuracy, cold/resident timings and limits.

The optional MLX application path is approved in [ADR-0020](adrs/0020-optional-mlx-speech-runtime.md). It shares the worker and architecture contracts, uses separately pinned GPU artifacts and supports explicit Metal with English Parakeet/Qwen-ASR. Whisper Small uses the same path in evaluation; its desktop picker remains gated. CPU/Automatic speech defaults are unchanged. Cohere and Whisper Turbo remain on CPU because their MLX screens included quality regressions. MLX is a runtime; Metal is its backend. CUDA support for the four Sherpa speech
families is also not established by the llama.cpp Cargo feature; it needs a
compatible Sherpa/ONNX Runtime build and model-specific operator evidence.

At pinned Sherpa revision `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`, GPU mode
[forces shared libraries](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/CMakeLists.txt#L173).
Its Linux x86-64 GPU package pins ONNX Runtime 1.28.2 with CUDA 12/cuDNN 9;
the application's current Sherpa script builds GPU off and static libraries.
The native [session factory](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/session.cc#L334)
can log CPU fallback when CUDA is unavailable, so a requested provider string
cannot serve as initialized-provider evidence.

Source and installed-graph inspection also found dynamically quantized UINT8
activations feeding `MatMulInteger` in the current CPU exports. The official
[ORT 1.28.2 CUDA kernel table](https://github.com/microsoft/onnxruntime/blob/v1.28.2/docs/OperatorKernels.md)
lists only INT8×INT8 for that operator and lacks `DynamicQuantizeLinear`;
Parakeet additionally uses integer convolution and dynamic LSTM operators without
matching CUDA kernels. This establishes a compatibility gap, not measured
placement or latency. Separate GPU-compatible exports must be pinned and tested.

Cohere/Whisper already have native CUDA I/O-binding code for attention caches;
Qwen's pinned CUDA branch [binds decoder outputs, including KV state, to CPU](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/offline-qwen3-asr-model.cc#L466).
Reusing the existing bindings is runtime integration; changing Qwen cache placement
is runtime-internal work. Neither requires training, and neither is enabled by
this Mac patch. One shared Sherpa GPU package/probe policy is the proposed reuse
boundary, with architecture-specific decoding and artifact checks retained.

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

The observation archived at `.cache/benchmarks/results/accelerator-capabilities/debug-summary.json` is local hardware
evidence. Windows/Linux/NVIDIA/AMD cases are synthetic rule tests, not native
cross-platform qualification. Tests also cover CPU-only builds, empty devices,
unsupported architectures, model restrictions, saved-choice preservation and
detection failure/retry. The menu screenshot
uses synthetic app settings with the native Mac observation as its device fixture.

## Building the optional MLX application

On Apple Silicon with full Xcode and the Metal compiler installed:

```sh
node scripts/mlx-runtime.mjs
# With the normal Rust/Sherpa development environment configured:
cargo build -p transcribe-eval --features mlx --release
cargo run -p transcribe-models --features mlx --example install -- models/downloads --asr-model Qwen/Qwen3-ASR-0.6B --backend metal
cd apps/desktop
npx tauri build --config src-tauri/tauri.mlx.conf.json -- --locked
```

The optional Tauri configuration includes the `mlx` Cargo feature, macOS 14 minimum and the native resources. The normal build stays feature-off. Select English and Metal for Parakeet or Qwen, save, then install the selected GPU files. Installation/removal follows the saved backend; CPU and Metal packages occupy separate directories. Changing back to Automatic selects the CPU export. This preserves the existing UI flow without introducing a second model identity or a runtime picker.

The build refuses stale native source/provenance. Runtime verification checks the library, kernels and preparation tool before use. Resources belong in `Contents/Resources/mlx-runtime`; a packaged application does not search downloaded model directories for executable code. Native signatures change binary hashes, so distribution signing must be incorporated before binding final native identities; the local optional bundle is not a signed/notarized release claim.

Model-source attribution is pinned per artifact. Parakeet MLX derives from NVIDIA Parakeet TDT v3 under CC-BY4.0 ([conversion card](https://huggingface.co/mlx-community/parakeet-tdt-0.6b-v3/blob/ed2b7e8c15f9aaa0b5772e2efb986255eaef7e15/README.md)); Qwen's conversion declares Apache2.0 ([card](https://huggingface.co/mlx-community/Qwen3-ASR-0.6B-8bit/blob/89e96d92ba34aca20b3e29fb10cc284097d1219f/README.md)). Whisper Small's [conversion](https://huggingface.co/mlx-community/whisper-small-fp16/blob/fa19eb05939653a9334d81bec7e053db81970170/README.md) and [pinned tokenizer card](https://huggingface.co/openai/whisper-small/blob/973afd24965f72e36ca33b3055d56a652f456b4d/README.md) declare Apache2.0; OpenAI's [source repository](https://github.com/openai/whisper/blob/v20250625/LICENSE) separately uses MIT. These cards name original model families but do not prove exact checkpoint equivalence with CPU exports. The project does not gain a license from its dependencies.
