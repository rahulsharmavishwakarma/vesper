# Inference performance across devices

The separate whisper.cpp path remains a candidate: the [18 September native screen](validation/whisper-2026-09-18.md) records quality gains on its English sample, Hindi/noise failures, and provisional CPU timings affected by other workloads. It does not qualify that runtime for production. The existing shared Sherpa Whisper adapter is a different implementation.

Updated 19 October 2026 (the llama.cpp pin and later measurements; earlier sections keep their dated evidence). The primary-paper basis and standardized streaming metrics are in [Research basis for streaming inference](research-streaming-inference-papers.md). Packaged accelerator selection and selected resident adapters are implemented; the default Cohere speech adapter now retains its recognizer in the existing worker with fresh segment streams (ADR-0017). Performance claims require measurements on each named device. Recommendations below are engineering proposals unless explicitly identified as observations.

The per-job Nemotron CLI and per-request cleanup-model loading have been replaced. CPU Qwen3-ASR and Parakeet now retain their verified ONNX recognizers on the existing owner, with fresh request streams, alongside the previously resident adapters. The next priorities are to instrument the complete live path, run the lifecycle/corpus gates, tune CPU budgets, and benchmark each packaged accelerator. Speech and cleanup ultimately need independent device controls. A GPU being present, a backend compiling, and the exact model running efficiently are three separate checks.

## Resources for live adapters

The current accelerator scope is Apple Silicon MLX/Metal, retaining CPU paths.
NVIDIA CUDA and AMD/Intel Vulkan/ROCm expansion and hardware qualification are
deferred to [issue #76](https://github.com/rahulsharmavishwakarma/vesper/issues/76)
in the GitHub Project backlog. Existing backend code is not hardware qualification;
non-Apple GPU work is not a completion gate for the current Apple live-adapter
scope. See [the current routing table](live-transcription.md#device-and-memory-behavior)
for supported model/language combinations and remaining Apple validation limits.

The approved [live adapter implementation](live-transcription.md) reuses the
existing inference worker and qualified backend selection. It adds bounded
preview audio, coalesced scheduling, refreshed llama.cpp device memory readings,
CPU execution headroom and MLX allocator/cache limits. Only supported runtime and
artifact combinations can use acceleration; CPU Sherpa does not acquire CUDA
support from GPU detection alone.

CPU worker counts reserve two logical slots where possible, with a 1–8 clamp and
fallback of two. The selected model remains resident; preview passes use fresh
request state. Buffered sessions sample system memory and process RSS through
sysinfo, defer previews under pressure, and can release idle writer ownership
before speech admission. Capture and the authoritative final audio are retained.
These heuristics reduce contention; they are not an optimal-device benchmark or a
hard global memory admission guarantee.

The [pinned MLX allocator API](https://github.com/ml-explore/mlx-swift/blob/0.31.4/Source/MLX/Memory.swift)
sets a recycled-cache cap and an allocation limit based on Metal's recommended
working set. Active/cache/peak memory and configured limits are reported. These
do not bound the whole process or other runtimes sharing unified memory. Apple
CPU/GPU memory is one pool. NeMo's current C ABI provides no application-wide
CPU-thread or memory-budget control. No wired-memory setting changes.

Automatic initialization fallback and explicit-backend errors remain intact.
Selected device free memory is refreshed at load; within a selected llama.cpp
backend the device with the most free memory wins. No per-packet device migration,
new kernels or quantization change is introduced. CUDA/Vulkan/ROCm execution,
sustained thermals and power qualification require matching hardware; unavailable
telemetry remains unknown. See [verification and evidence](live-transcription.md#verification-and-evidence)
for current measurements and their scope.

## What this application does today

The user-selected default is Cohere Transcribe INT8 through sherpa-onnx 1.13.8, with completed-recording CPU inference and Qwen writing cleanup. This preference is not a fastest-model claim. Selectable Nemotron 3.5 Q8 uses the official NeMo-Speech.cpp 0.1.0 C ABI and supports live partials. Parakeet v3 INT8 through `sherpa-onnx = 1.13.8` remains an offline CPU fallback. Cleanup uses Qwen3.5-0.8B Q4_0/Q8_0 through `llama-cpp-2 = 0.1.159`. Hugging Face profile selection changes compatible artifacts; it does not make an arbitrary architecture compatible with these runtimes.

| Component       | Current behavior                                                                                                                                                                                                                                               | Consequence                                                                                                                                                                                                                                            |
| --------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Speech runtime  | Default Cohere uses pinned CPU sherpa-onnx; selectable Nemotron uses CPU/Metal/CUDA/Vulkan NeMo-Speech.cpp with 20 ms capture packets                                                                                                                          | Cohere uses buffered live drafts plus a completed-audio final; NeMo owns cache-aware RNNT state and native partials                                                                                                                                    |
| Cleanup runtime | Metal on Apple Silicon; CUDA/Vulkan/ROCm when compiled with the matching feature; otherwise CPU                                                                                                                                                                | Requests layer, K/Q/V and operator offload; `Auto` retries the complete initialization on CPU after GPU model-load or context-probe failure and records the reason. Explicit GPU selection fails if unavailable; operator placement remains unverified |
| CPU budget      | Available parallelism minus two, clamped to 1–8, fallback 2; NeMo runtime controls its own threading                                                                                                                                                           | Portable baseline, not a measured optimum                                                                                                                                                                                                              |
| Object lifetime | One bounded worker retains at most one selected Cohere/Whisper/Nemotron/LFM ASR and one Qwen writer; CPU Qwen-ASR and Parakeet also retain verified recognizers; MLX retains the selected Qwen/Parakeet/Whisper Small model in compatible Apple Silicon builds | Residency is adapter-specific; each recording/segment and Qwen pass starts isolated request state                                                                                                                                                      |
| Cleanup limits  | Up to 4096 context tokens, bounded output up to 2048 tokens; logical batch 512, physical microbatch 128                                                                                                                                                        | Memory and latency depend on actual prompt/output lengths                                                                                                                                                                                              |
| Integrity       | Full role-specific verification on resident load; per-job speech adapters verify every request                                                                                                                                                                 | Cohere verifies on native load and checks observed identity on reuse; Apple aarch64/macOS enables sha2's supported hardware dispatch while retaining all size/hash checks                                                                              |

These observations come from [Nemotron inference](../crates/vesper-inference/src/nemotron.rs), [cleanup inference](../crates/vesper-inference/src/qwen.rs), [model/runtime selection](../crates/vesper-models/src/lib.rs), and [desktop orchestration](../apps/desktop/src-tauri/src/runtime.rs).

Each cleanup pass has a wall-clock limit of 5 s plus 50 ms per input token
([#141](https://github.com/rahulsharmavishwakarma/vesper/issues/141)), about ten
times the measured warm cost. Exceeding it returns `TimeLimit`, keeps the raw
transcript and does not retry. In 416 measured rewrites of 52 public
transcripts (CPU and Metal, both presets) the slowest pass took 1.0 s, so the
limit only stops runaway generations.

**Prompt-lookup speculative decoding: measured, not adopted (19 October 2026, #141).**
The cleanup output is mostly a copy of the transcript, so tokens following the
latest output n-gram can be verified in one batch. llama-cpp-sys-2 0.1.156 can
roll back Qwen3.5's recurrent state through the experimental `n_rs_seq` context
setting (`llama.h`, `llm_arch_supports_rs_rollback`). A decode-only spike on the
52 public transcripts accepted 98% of drafts and halved generation time. In the
production writer, however, setting `n_rs_seq = 8` on the whole context raised
context creation from about 6 to 38 ms and uncached CPU prefill from about 243
to 650 ms; it also slowed each verification decode. Median CPU rewrite became
412 → 958 ms uncached and 201 → 375 ms cached. 6 of 416 Balanced outputs
changed through different rounding. The change was reverted. Reconsider it only
if a runtime allows rollback without slowing prefill, for example a separate
verification context or a per-batch rollback setting. Compare candidates with
the ignored `dump_cleanup_outputs` test against a saved baseline.

Qwen's complete-prefix state cache is **on by default** since 18 October 2026
([#141](https://github.com/rahulsharmavishwakarma/vesper/issues/141)).
`VESPER_PREFIX_CACHE=0`, or the older `VESPER_EXPERIMENTAL_PREFIX_CACHE=0`,
disables it. Normal cleanup retains weights and creates fresh contexts. Its
hybrid recurrent state and full-attention KV state require the exact runtime's
complete-state API. The prompt is always prefilled in two batches split at the
immutable prefix boundary, whether or not the cache is enabled. A restored
snapshot is therefore the same state as fresh prefill. Without that split, a
single-batch prefill changed one greedy Balanced output through
floating-point accumulation order.

Local measurement on 52 public Cohere transcripts from the
[polishing study](validation/polishing-wer-2026-10-16.md) (Apple M5 Pro,
Qwen3.5-0.8B Q4_0, warm resident weights, alternating order). Cached output
equals uncached output in all 208 pairs, and every cached request restored
the prefix.

| Backend, preset        | Median rewrite, uncached → cached |          P90 | Median prefill |
| ---------------------- | --------------------------------: | -----------: | -------------: |
| CPU, Minimal (default) |                      412 → 201 ms | 587 → 371 ms |    243 → 41 ms |
| CPU, Balanced          |                      549 → 218 ms | 730 → 398 ms |    368 → 42 ms |
| Metal, Minimal         |                      194 → 155 ms | 324 → 286 ms |     55 → 16 ms |
| Metal, Balanced        |                      222 → 167 ms | 375 → 317 ms |     72 → 16 ms |

Generation time is unchanged; the gain is prompt prefill only. Prompts that
contain retrieved personal examples and custom GGUF templates still bypass the
cache. Reproduce with the ignored `measure_prefix_cache_on_transcripts` test
and `VESPER_WRITER_INPUTS` (a JSON array of public transcripts).

Current supported-request, Cohere-residency, application-SHA and Whisper
qualification runners strip both Vesper and legacy Transcribe process overrides
before applying their explicit `VESPER_EXPERIMENTAL_PREFIX_CACHE=0` baseline,
which still disables the cache.
Frozen reports retain the environment names actually used for those runs.
The denoising, accent and frontend-integration runners also write Vesper
baseline overrides. Where a runner sanitizes inherited settings it removes
both namespaces; recorded prior runtime environments remain historical input.
Automatic-level qualification writes the same Vesper baseline switch.

One build detail matters: `default-features = false` does **not** mean Metal is absent. The `llama-cpp-2` manifest had an Apple ARM target dependency enabling `metal` as reviewed for 0.1.156; not re-checked for the pinned 0.1.159. Application features forward `cuda`, `rocm`, and `vulkan`; there is no direct `sycl` feature in this release. Explicitly selecting an unavailable backend fails with a device-selection error. Automatic selection uses a detected native GPU or CPU; failed GPU initialization retries CPU and records its reason. [Pinned Rust binding manifest](https://docs.rs/crate/llama-cpp-2/0.1.156/source/Cargo.toml).

NeMo-Speech.cpp 0.1.0 publishes CPU, CUDA, Vulkan, and Metal release packages. Vesper pins their bytes and hashes rather than treating a requested device string as evidence of availability. NVIDIA does not publish an MLX or ROCm Nemotron package: Apple uses Metal; the AMD profile uses Vulkan for speech and can use ROCm/HIP for Qwen cleanup.

### Raw text visibility during local postprocessing

The shared completion path publishes an operation-scoped, read-only raw text
preview after the existing raw save/publication attempt and before speaker
analysis or writing cleanup. The Dictate screen reads it through runtime status;
it does not accept it as an editable final result or feed it into learning.
Completed recordings, successful native-stream finalization, completed fallback
after a failed stream, and imported files use the same completion path.
Cancellation, completion and a new request clear the preview. Delayed UI status
responses from before a new action/final result are ignored. Raw-result recovery
after cancellation and final editing/copy/insertion retain their existing paths.

This is a frozen-weight application change: no additional inference, model
lifetime changes, decoder changes, checkpoint changes or concurrent GPU work.
Status polling during postprocessing remains 500 ms, so a short job can complete
without an intermediate render. It can reduce the wait to _see recognized text_
when postprocessing is slow; it does not reduce recognition time or stop-to-final
latency. It is not a new streaming decoder and does not make Cohere live-capable.

The engineering pattern is supported by the local preview/final distinction in
[WhisperKit's streaming implementation](https://github.com/argmaxinc/argmax-oss-swift/blob/f4e5d6be37ec820614fb0d72037e76c22d4c16f7/Sources/WhisperKit/Core/Audio/AudioStreamTranscriber.swift).
The [WhisperFlow paper (v2)](https://arxiv.org/abs/2412.11272v2) studies on-device ASR
scheduling and per-word latency; its decoder/scheduling speedups are separate
from this display-only change and are not claimed here. Local deterministic
runtime and delayed-IPC browser tests check ordering, cancellation, request
isolation and final-draft behavior. They are not model benchmarks or a native
WebView latency measurement. CPU/Apple Silicon model qualification, sustained
load and the existing audio-quality gates remain independent requirements.

## Existing measurements, with their limits

### Native GPU implementation and screening — 23 September 2026

The shared llama.cpp backend selector is now used by the writer and LFM. It
selects a detected native GPU index at model load, requests layer/KQV/operator
offload, and keeps model-specific decoding and cache state in the native runtime.
LFM also requests GPU execution for its mtmd audio projector. Automatic mode may
retry CPU after either model loading or context initialization fails; explicit GPU
selection never silently changes to CPU. Cancellation does not trigger fallback.
A background semantic request with no loaded writer returns before GPU discovery.

This is a frozen-weight runtime integration using supported APIs. It adds no
attention kernels, Flash Attention implementation, quantized format or training
technique. CUDA/Vulkan/ROCm require the corresponding native build features; this
Mac run does not qualify those devices. Native logs confirm Metal backbone and
projector initialization; the mtmd public API does not expose exact operator
placement, which remains `unknown` in execution evidence.

**Local measurement.** On Apple M5 Pro (18 logical CPUs, 64 GiB), macOS 27.0
(26A428), LFM2.5 Audio with the pinned Q4 backbone/projector and 8 native threads
completed the 84 en-US cases in the existing
`frontend-integration-2026-09-18/quality.json` manifest on both CPU and Metal.
Audio preprocessing and writing cleanup were disabled in this comparison.
First-request time includes integrity verification/loading; resident ASR time
measures the completed-recording recognition call, not microphone, UI or
stop-to-final latency.

| Condition             | Cases per backend | CPU WER               | Metal WER             | Resident median CPU / Metal |
| --------------------- | ----------------- | --------------------- | --------------------- | --------------------------- |
| Clean speech          | 24                | 69/689 = 10.01%       | 70/689 = 10.16%       | 407.28 / 205.25 ms          |
| Quiet speech (−30 dB) | 24                | 485/689 = 70.39%      | 485/689 = 70.39%      | 355.42 / 125.55 ms          |
| Noise (0 dB)          | 24                | 142/689 = 20.61%      | 138/689 = 20.03%      | 384.02 / 193.67 ms          |
| Controls              | 12                | Speech controls: 8/52 | Speech controls: 8/52 | 137.19 / 61.38 ms           |

All 168 requests completed without runtime failure. Outputs matched exactly for
79/84 paired cases. Both backends hallucinated one word on 4/8 non-speech controls;
GPU execution does not fix that existing model behavior. Very quiet speech is
poor on both paths. Clean speech gained one word error while noisy speech lost
four errors; do not call these numerically identical or claim a quality gain.

Across 83 resident requests, median ASR time was 344.25 ms CPU and 167.58 ms Metal.
The earlier six-clean-clip screen had exact parity and resident medians of
318.36/151.09 ms, but its first Metal request took 9.012 s versus CPU 1.003 s.
After native shader caches were warm, the broader run first request took
0.676 s Metal and 0.879 s CPU. The broader run used Metal then CPU; the initial
screen used CPU then Metal. This was a shared, uncontrolled host, not randomized
paired qualification. Power, thermal behavior, sustained VRAM and live speech
latency are unmeasured. No SOTA or whole-pipeline speed claim follows.

The separate real Metal lifecycle regression passed A→B→A isolation, token-limit
rejection, cancellation and subsequent recovery, silence/wrong-rate admission,
segment cancellation, writing cleanup, and unload/reload. The core/inference
unit suites and shared backend fallback tests also passed. Raw outputs remain
ignored under `.cache/benchmarks/results/gpu-adapters-2026-09-23/`; no benchmark
result JSON is part of this change. The benchmark binary SHA-256 was
`bd577902c2e62d8417d36894ab3937b6bc3539d041d7c63d483019cd29d27f09`, built before the integration commits from
`c8358b4` plus the then-unstaged native GPU change. Runtime/model revisions and artifact
hashes are recorded in those results and the canonical artifact registry.
The original 84-case envelope inherited a six-clip note from the initial screen;
its manifest hash and 84 recorded requests identify the actual corpus. This
metadata defect is retained here rather than silently rewriting the raw result.
Medians average the two middle values for even-sized groups.

The existing Nemotron Metal streaming adapter also passed its three-recording
resident-weight/reset regression. A separate six-clean-clip replay ran at real
audio pace, with Silero VAD, DPDFNet4 denoising and the Qwen writer enabled. All
six ASR requests completed; cleanup passed fidelity checks on four and preserved
raw text on two rejected rewrites. Thus this is not six successful rewrites.
Warm first-partial latency was 1.03–1.67 s; the cold request was 7.65 s.
Stop-to-final ASR was 34.05–56.88 ms; cleanup added 137.69–848.38 ms.
These definitions exclude cleanup from ASR stop-to-final. Partial updates were
observed, but this replay does not measure microphone/UI latency, endpointing,
word-level emission latency, queue-drop instrumentation, or sustained power and
thermal behavior. Raw replay and partial-stability metrics are retained in
`nemo-metal-streaming.json` in the ignored result directory.
The same six clips also completed in the recorded-audio path with those stages
enabled: warm ASR calls took 0.856–1.138 s and cleanup 0.132–0.811 s. Both stages
reported Metal; the small audio frontend reported CPU and preserved sample
coverage. Again four rewrites were accepted and two retained raw text. Raw ASR
WER was 16/128 offline versus 15/128 in paced streaming, so these observations do
not establish identical transcripts between modes. The streaming environment
file's original descriptive note is shared by this offline screen; each run's
recorded `mode`/`pace` fields identify the executed path.

### MLX speech qualification boundary

`native/mlx` and `vesper-mlx` provide one native boundary with upstream loaders
for Cohere, Whisper, Qwen3-ASR and Parakeet. MLX-Audio-Swift is pinned to
`d302a5c6080d2bb97bae38c7418f82abb76013b6`, MLX Swift to 0.31.4
(`dc43e62d7055353c7f99fa071a4e71d29dfddc44`) and MLX Swift LM to 3.31.4
(`bd4b7434e6bdb588c7ef55706ff8904cb7fd4c57`), with the full transitive lockfile
in `native/mlx/Package.resolved`.

Build with `node scripts/mlx-runtime.mjs` using Xcode and its Metal compiler
component. The script verifies the source revision, applies the narrow
[runtime qualification patch](../patches/mlx-audio-swift/README.md), and packages
the library with `mlx.metallib` and provenance under ignored `.tooling/mlx-runtime`.
The isolated Rust `probe` example accepts the library, a verified local model
directory, architecture, explicit language and public audio paths. It does not
download models or alter application defaults.

The qualification artifacts are pinned independently from the CPU exports:

| Model                  | Converted repository and revision                                                                                                                                       | Weight SHA-256                                                     |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| Whisper Small FP16     | [`mlx-community/whisper-small-fp16`](https://huggingface.co/mlx-community/whisper-small-fp16/tree/fa19eb05939653a9334d81bec7e053db81970170)                             | `7408174e70bffbff6a189cadc3621512be0d1d26bb8c9122d3eeace1652d1b54` |
| Qwen3-ASR 0.6B 8-bit   | [`mlx-community/Qwen3-ASR-0.6B-8bit`](https://huggingface.co/mlx-community/Qwen3-ASR-0.6B-8bit/tree/89e96d92ba34aca20b3e29fb10cc284097d1219f)                           | `b5bfe4abc1b4c6e58b633096682ec2b6297298add1527119936107d211adf0e8` |
| Parakeet TDT 0.6B v3   | [`mlx-community/parakeet-tdt-0.6b-v3`](https://huggingface.co/mlx-community/parakeet-tdt-0.6b-v3/tree/ed2b7e8c15f9aaa0b5772e2efb986255eaef7e15)                         | `05e01c7f396c298cf7d23f61da7b504adeab698f0aaeafd9c82d198625464592` |
| Cohere Transcribe FP16 | [`beshkenadze/cohere-transcribe-03-2026-mlx-fp16`](https://huggingface.co/beshkenadze/cohere-transcribe-03-2026-mlx-fp16/tree/06bbe0d853a93cbbf7c1f40cffccd4c5481459ef) | `1ec6ba9ee27da02b21b3ffdb5183b77020351d3331d05a74ad8d58a09394a2b8` |
| Whisper Turbo FP16     | [`mlx-community/whisper-large-v3-turbo`](https://huggingface.co/mlx-community/whisper-large-v3-turbo/tree/a4aaeec0636e6fef84abdcbe3544cb2bf7e9f6fb)                     | `951ed3fc1203e6a62467abb2144a96ce7eafca8fa77e3704fdb8635ff3e7f8a6` |

These hashes identify candidate bytes, not application support. Whisper uses
OpenAI tokenizer repositories at Small revision
`973afd24965f72e36ca33b3055d56a652f456b4d` and Turbo revision
`41f01f3fe87f28c78e2fbf8b568835947dd65ed9`. Configuration/tokenizer file hashes
and the actual bridge/probe hashes accompany each ignored benchmark envelope.
Parakeet stores float32 weights and the pinned loader defaults to bfloat16 compute;
the original artifact and actual compute dtype must not be conflated. Conversion
cards do not establish runtime compatibility or original-checkpoint equivalence.

After downloading and verifying the pinned Qwen inputs, prepare its tokenizer
explicitly (other loaders require their already prepared local tokenizer files):

```sh
swift run --package-path native/mlx --scratch-path .tooling/mlx-build \
  --cache-path .tooling/swift-cache --force-resolved-versions \
  -c release prepare-qwen-tokenizer VERIFIED_QWEN_DIRECTORY
cargo run --release -p vesper-mlx --example probe -- \
  .tooling/mlx-runtime/libVesperMLX.dylib VERIFIED_QWEN_DIRECTORY \
  qwen3_asr English PUBLIC_AUDIO.wav
```

The resulting Qwen tokenizer in this screen has SHA-256
`1c98aeb375d2cb5b386bad460bc2a87bf39b9debbda44d49115377d1de8cb2ed`.
Use the manifest's declared ordering and canonical `vesper-eval` scorer for
quality comparisons. Retain all failed requests and keep the model-load line
from stderr; a single successful probe is only a smoke test.

The bridge forces the MLX GPU device, joins native work before releasing audio or
cancellation pointers, pads short inputs for safe STFT framing and discards
non-EOS generative output. Parakeet uses bounded audio/frame decoding; it does not
honor the generative token ceiling. Cancellation is cooperative between decoder
steps, not inside an encoder/kernel call. Whisper/Cohere require explicit English
or French on this path; automatic-language behavior is not equivalent to Sherpa.
Tokenizers must be local, prepared and verified. No loader may download missing
files or generate a tokenizer during inference. MLX peak memory is a process-life
allocator peak including resident weights.

Whisper exposes the native SOT-row no-speech probability and accumulated greedy
token log probability. Both CPU and MLX reuse the existing combined OpenAI
silence policy; missing/nonfinite scores and incomplete decodes fail closed.
This reduced false transcripts from 7/8 to 2/8 non-speech controls in the isolated
Small screen, without changing its speech WER. That is a local patch result,
not a claim about unmodified MLX-Audio. The pinned reference is OpenAI Whisper
[decoding.py](https://github.com/openai/whisper/blob/v20250625/whisper/decoding.py)
and [transcribe.py](https://github.com/openai/whisper/blob/v20250625/whisper/transcribe.py).

The probe uses shared complete-audio energy boundaries: 29 seconds for Whisper,
10 seconds for Cohere/Qwen, and a qualification-only 30-second bound for Parakeet.
The CPU Parakeet adapter accepts the whole recording, so geometry matches only
for inputs within that bound. This English manifest's longest clip is 23.76 s.
All samples remain covered, with fresh decoder state for each segment. Model
loading is reported separately; probe ASR time includes the first native call's
compilation if needed. CPU Qwen/Parakeet still load per job, so comparing their
application timings with a resident MLX probe is not a GPU-only speedup.

On the matched English screen, Whisper Small's 84 CPU and 84 MLX requests all
completed. The final verified bridge produced the following results:

| Condition       | Sherpa INT8 WER  | MLX FP16 WER     | Resident median CPU / MLX |
| --------------- | ---------------- | ---------------- | ------------------------- |
| Clean           | 54/689 = 7.84%   | 42/689 = 6.10%   | 730.23 / 219.06 ms        |
| Quiet (−30 dB)  | 227/689 = 32.95% | 79/689 = 11.47%  | 650.03 / 217.24 ms        |
| Noise (0 dB)    | 120/689 = 17.42% | 115/689 = 16.69% | 740.23 / 221.54 ms        |
| Spoken controls | 9/52             | 8/52             | —                         |

CPU had false transcripts on 5/8 non-speech controls; MLX had 2/8. The MLX process
allocator peak reached 1.644 GB. Its first model load took 765 ms, separately from
request timings. Both lanes use the same 29-second boundary policy and confidence
thresholds, but different precision, converted weights and native implementations.
These results support further integration qualification, not a backend-only
quality gain or a production/default promotion. This is 24 distinct English
utterances in three conditions plus 12 controls, not 84 independent speech
recordings. Automatic language and Hindi are rejected by this bridge's current
Whisper/Cohere language contract. French, code-switching, singing, long audio,
bundled-library deployment and sustained thermals remain unqualified. The earlier
CPU file carried an incorrect LFM precision label; the final CPU rerun uses the
correct Whisper INT8 envelope and reproduces the same transcripts.

Whisper Turbo also completed all 84 requests on each path. MLX clean/quiet/noisy
WER was 33/689 (4.79%), 66/689 (9.58%) and 53/689 (7.69%), versus CPU INT8's
35/689 (5.08%), 45/689 (6.53%) and 65/689 (9.43%). Both had 8/52 errors on
spoken controls and false transcripts on 7/8 non-speech controls. Thus the GPU
conversion's faster resident clean median (356.23 ms versus 1,315.30 ms) comes
with a quiet-speech regression and unresolved silence behavior. Separate MLX load
was 209.01 ms, first call 387.99 ms with previously used native shader caches,
and process allocator peak 3.091 GB. This is not a cold-compilation result or a
reason to promote Turbo over Small. The two Whisper variants share one bridge;
their quality outcomes must still be qualified separately.

The initial matched 84-case Qwen screen completed every request, but quiet WER worsened
from 66/689 (9.58%) on Sherpa INT8 to 112/689 (16.26%) on MLX 8-bit, and noisy WER
from 92/689 (13.35%) to 139/689 (20.17%). Clean WER was 57/689 versus 50/689;
spoken controls were 0/52 on both. MLX hallucinated on 5/8 non-speech controls
versus CPU 7/8. Its 213 ms resident clean median excludes model loading; the
CPU's 1,808 ms includes per-job initialization. The quiet/noisy regression blocks
promotion for that implementation, regardless of its timing difference. Earlier
fixed-boundary trials remain separate from this matched-boundary result.

Source comparison then found two Qwen-specific port errors: the generic frontend
used HTK mel spacing, symmetric Hann and an extra centered frame; audio-token
length calculation used MLX floating division for complete 100-frame groups.
Official Qwen uses a Whisper frontend and integer floor division. The correction
reuses the runtime's existing `WhisperAudio.logMelSpectrogram` without fixed-window
padding and fixes only that length term. Primary references are the
[Qwen processor at 7c6daf77](https://github.com/QwenLM/Qwen3-ASR/blob/7c6daf77a2421100f5fb066495372c00129d39ff/qwen_asr/core/transformers_backend/processing_qwen3_asr.py)
and its pinned [Transformers 4.57.6 extractor](https://github.com/huggingface/transformers/blob/v4.57.6/src/transformers/models/whisper/feature_extraction_whisper.py).
Native tests check fixed official token counts at chunk boundaries and
160,000 samples → 1,000 frames → 130 audio tokens. They passed on Metal.

The corrected 84-case rerun retained the same weights, tokenizer, language,
segmentation and decoding policy, and completed every request. Clean/quiet/noisy
WER became 47/689 (6.82%), 52/689 (7.55%) and 85/689 (12.34%), compared with
CPU's 57/689, 66/689 and 92/689. Spoken controls remained 0/52; non-speech false
transcripts fell to 4/8, still a limitation. Resident clean median was 204.66 ms,
model loading separately took 1.185 s, and process allocator peak was 1.811 GB.
The result supports the combined official-frontend correction; it does not isolate
each change's contribution or establish general multilingual quality. No threshold
was fitted to this holdout. The earlier failed screen and its native library/patch
remain preserved under the ignored result directory. Subsecond inputs still use
the bridge's one-second safety padding and were not part of the quality corpus.

Cohere's MLX conversion completed all 84 cases, with clean/quiet/noisy WER of
45/689 (6.53%), 60/689 (8.71%) and 47/689 (6.82%), versus CPU INT8's 39/689
(5.66%), 460/689 (66.76%) and 65/689 (9.43%). Both had 8/52 errors on spoken
controls and no false transcripts on the eight non-speech controls. The quiet/noisy
improvement carries six additional clean-word errors. Resident clean median was
502.76 ms MLX versus 610.14 ms CPU; MLX load was separately 270.54 ms, first call
2.395 s and process allocator peak 5.368 GB. Conversion lineage and precision
differ, so this does not isolate hardware effects or qualify a default change.

Parakeet completed all 84 requests on both paths. MLX clean/quiet/noisy WER was
29/689 (4.21%), 35/689 (5.08%) and 45/689 (6.53%); CPU INT8 was 55/689 (7.98%),
472/689 (68.51%) and 105/689 (15.24%). Both had 8/52 word errors on spoken
controls and no false transcripts on the eight non-speech controls. MLX resident
clean median was 53.43 ms, versus 954.12 ms for CPU with per-job model loading.
That is an ownership/precision/runtime comparison, not an 18× GPU acceleration
claim. MLX model loading took 259.94 ms separately, its first recognition call
took 1.991 s including cold native work, and its process allocator peak reached
3.804 GB. The two lanes consumed the same complete clips without segmentation
because all inputs fit below the isolated bridge's 30-second bound. This supports
further qualification, with automatic-language behavior, multilingual requests,
longer recordings and application ownership still outside the measured scope.

All five variants passed the final native cancellation, A→B→A isolation, bounded
completion and unload/reload regression. Strict Rust Clippy, core/inference/audio
unit tests, frontend configuration tests and the two Swift geometry tests passed.
The final integrity audit rechecked 34 downloaded model files, the prepared Qwen
tokenizer, all 84 corpus hashes and both measured native-library/kernel pairs.
Original benchmark envelopes are retained; `qualification-integrity-audit.json`
also corrects generic downloader license labels against the pinned model cards.
These research inventories are not production installation manifests.

The measurements above used the isolated bridge. The approved optional application integration is now described in [ADR-0020](adrs/0020-optional-mlx-speech-runtime.md), with application validation recorded separately below. English Parakeet, Qwen and Whisper Small are the initial gated variants; CPU/Automatic defaults remain unchanged. Cohere and Whisper Turbo remain excluded from application Metal selection. The small
audio frontends retain CPU execution; GPU dispatch overhead must be measured
before moving their per-frame work.

Application registration, optional cross-crate feature wiring, pinned model
installation and packaging are a separate architectural integration. Automatic
approval review rejected the attempted registration patch under the explicit
approval rule in `AGENTS.md`; none of that patch was applied. The concrete scope
was subsequently approved explicitly, as recorded in [issue #35](https://github.com/rahulsharmavishwakarma/vesper/issues/35#issuecomment-5788299497). The historical rejection did not block the later approved implementation.
No MLX model becomes selectable and no default changes merely because this
isolated bridge compiles or produces promising measurements.

### Blocked TDT decoding on Metal — 22 October 2026

[#286](https://github.com/rahulsharmavishwakarma/vesper/issues/286) profiled
Vesper's MLX speech adapters on a 12.6 s FLEURS clip (release build, warm, M5
Pro). For the three Parakeet TDT models, features took under 0.5 ms. Decoding
took roughly 45% of each request:

| Model                    | Total | Encoder | Decode steps | Decode per step |
| ------------------------ | ----- | ------- | ------------ | --------------- |
| Parakeet v3              | 65 ms | 36 ms   | 92           | 0.33 ms         |
| Phonon-2                 | 75 ms | 36 ms   | 116          | 0.34 ms         |
| Parakeet Redux (float32) | 78 ms | 26 ms   | 109          | 0.48 ms         |

Each greedy TDT step does about 12M multiply-adds. The upstream serial loop
waited for the GPU after every step to read the token and duration, so
synchronization rather than arithmetic set the cost.

The pinned patch `patches/mlx-audio-swift/parakeet-blocked-tdt.patch` keeps the
loop state on the GPU: frame index, symbol count, last token and LSTM state. It
fuses the step with the serial loop's reductions into one compiled call and
reads back 16 steps per synchronization. The arithmetic and its order are
unchanged.

`ParakeetBlockedDecodeTests` shows the decode trace of every step is identical
to the serial loop for Parakeet v3, Phonon-2 and Redux on 31 clips. Through the
application on the 94-clip FLEURS en-US held-out set, transcripts were identical
on 94/94 for each model:

| Model          | Median before → after | p95 before → after |
| -------------- | --------------------- | ------------------ |
| Parakeet v3    | 48.2 → 40.8 ms        | 70.5 → 63.1 ms     |
| Phonon-2       | 54.5 → 43.9 ms        | 86.3 → 68.0 ms     |
| Parakeet Redux | 51.4 → 43.7 ms        | 79.7 → 68.4 ms     |

Longer recordings gain more: 24–27% on the 12.6 s profiling clip.

Block sizes of 8, 16 and 32 performed alike; 16 is used. Three experiments were
not adopted:

- **Decoding on the CPU** was 2× slower for the bf16 models, because bf16 math
  on the CPU is slow.
- **Stacking Phonon-2's two code planes** into one matmul left its encoder
  unchanged at about 37 ms. Five-value weights inherently cost two 2-bit planes
  of work, while Redux's ternary weights cost one.
- **MLX's fused attention for Granite** (26.7 → 26.3 ms, identical text) is
  within noise. Granite's time is in its dense encoder layers.

These are single-machine measurements on battery power, not general claims.

### Historical Mac GPU audit — 22 September 2026

Source audit of the pre-change application `c8358b4061876f1bb3bafed39a8694afb5bc9e6c`; the implementation above supersedes its LFM and writer-fallback findings.
The practical target is the fastest **qualified model/runtime/device combination**
for each stage. MLX is a framework; Metal and CUDA are execution backends.
Current MLX also documents a Linux CUDA backend, so MLX and CUDA are not mutually
exclusive choices. That upstream capability does not establish MLX-Audio model
portability, Windows support, or application integration. [MLX installation](https://ml-explore.github.io/mlx/build/html/install.html).

| Application component                | Current Apple path                                                                    | Available opportunity and boundary                                                                                                                           |
| ------------------------------------ | ------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Qwen writing                         | llama.cpp Metal in `Auto`; all offloadable layers, KQV and operator offload requested | Measure existing Metal first. The native defaults already request Flash Attention AUTO and F16 K/V; enabling a flag is not automatically a new optimization. |
| Nemotron streaming/completed audio   | Packaged NeMo Metal in `Auto`; native cache-aware streams                             | Compare the same Q8 artifact and right context on CPU/Metal. RNNT control remains host-side and the scheduler retains CPU fallback.                          |
| Cohere, Whisper, Qwen3-ASR, Parakeet | Sherpa ONNX CPU, including when `Auto` is selected                                    | MLX implementations are candidates. Current artifacts and loaders cannot become MLX by changing a provider string.                                           |
| LFM2.5 Audio                         | llama.cpp backbone and mtmd audio projector explicitly on CPU                         | Existing native APIs allow separate backbone/projector GPU requests; exact model execution and quality remain unqualified.                                   |
| Small audio frontends                | Current shared CPU path                                                               | Keep as the baseline until stage timings justify moving them; small per-frame GPU dispatches may lose latency. This is a hypothesis to test.                 |

The first two rows are existing requests for acceleration, not evidence that every
operator executes on the GPU. `qwen.rs` derives `initialized_provider` from the
requested layer count and context probe. Its automatic CPU retry covers a failed
context probe but not an earlier `load_model` error. Both execution attestation
and the load-error boundary remain focused follow-ups under [#30](https://github.com/rahulsharmavishwakarma/vesper/issues/30).
An explicit backend should surface failure; an automatic fallback should report
its reason and resulting backend. Neither should mislabel CPU work as GPU work.

**Local observation, not a model benchmark.** The existing ignored
`capabilities::tests::observe_native_accelerators` test passed on macOS 27.0
(26A428), reporting Apple M5 Pro through Metal. Native logs reported unified
memory, bfloat and tensor support, shared buffers and residency sets. CUDA,
Vulkan and ROCm were not compiled in this test build. Running the same executable
inside the restricted agent sandbox returned no Metal devices; outside that
sandbox it found the GPU. Device absence in a restricted process must not be
generalized to native application availability.

The reused test executable SHA-256 was
`a7880b747bf0521d390fa3a86e95b1796db2d8cfebf6d3eb702bfc82f78baa6b`.
It was not rebuilt for this audit, so its build revision is not newly attested.
No weights were loaded, no ASR/cleanup latency was measured, and no model placement
or thermal qualification follows from this discovery test.

There is earlier bounded performance evidence: the [19 September writer screen](personalization/evaluation.md#semantic-shadow-qualification)
records mean resident foreground cleanup of **239.82 ms CPU versus 73.18 ms Metal**
for its short authored fixture, with Qwen3.5-0.8B Q4_0 and 20 pairs per provider.
Those pairs compare foreground cleanup with/without interrupted shadow work;
they are not randomized CPU/Metal pairs. The shared host, power changes and absent
thermal/memory qualification limit inference. This is not a new audit measurement,
an ASR speedup, or whole-application latency. The separate semantic classifier
failed its quality gate on both providers.

#### Exact native boundaries

The current pins are Sherpa 1.13.8/source
`11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`, llama-cpp-2 0.1.159 / llama-cpp-sys-2 0.1.160 (llama.cpp
`d8123504`), and NeMo-Speech.cpp 0.1.0/source
`4f9676226f667d14608487df744f375db87127f8` (ggml
`c03b4e2bcece5134827881af90242086daf75be5`). Model revisions and complete hashes
remain owned by [the artifact registry](../models/artifacts.json).

The local static Sherpa build has `SHERPA_ONNX_DISABLE_COREML` in its actual
compiler definitions. Its pinned [Apple ARM static packaging](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/cmake/onnxruntime-osx-arm64-static.cmake)
selects ONNX Runtime 1.28.2, archive SHA-256
`bf6e35dc9440d2899c3d21f52b1045fb99176a6ae822281b7245c5418cc338ab`.
Observed runtime build ID:
`67ce39e3334e098b428e0e94284c3b6a0846e014a5c08000c02ae28f393ebc7b`.
This is stronger evidence than interpreting `SHERPA_ONNX_ENABLE_GPU=OFF`, which
does not by itself describe CoreML availability. The application also explicitly
sets Sherpa's provider to CPU. CoreML would require a compatible runtime build,
error-checked provider registration and exact-graph coverage measurements.

Upstream Sherpa's CoreML branch uses the legacy registration API with zero flags
and discards its returned status. Modern ORT supports MLProgram, compute-unit
selection, model caching and compute-plan diagnostics. Those options are not
automatically exposed by this branch. Dynamic shapes, graph partitions, CPU
fallback and first-use compilation must be measured. CoreML/ANE and Metal GPU
are separate execution claims. [ORT CoreML contract](https://onnxruntime.ai/docs/execution-providers/CoreML-ExecutionProvider.html),
[pinned Sherpa session code](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/session.cc).

The bundled llama.cpp already has an eligible Metal tensor path and native
quantized kernels. Verify shape/dtype dispatch before proposing new M5 kernels.
LFM's Conformer projector explicitly constructs relative-position attention;
generic Flash Attention configuration does not fuse that graph.
[Metal implementation](https://github.com/ggml-org/llama.cpp/blob/e79e4bf660e19f2ad851e06c6913f7a8c5852621/ggml/src/ggml-metal/ggml-metal-device.m),
[LFM Conformer implementation](https://github.com/ggml-org/llama.cpp/blob/e79e4bf660e19f2ad851e06c6913f7a8c5852621/tools/mtmd/models/conformer.cpp).

On NVIDIA, Qwen needs the existing compiled `cuda` feature and a usable device;
Nemotron needs its matching pinned CUDA package. NeMo's fused relative attention,
depthwise convolution and FastConformer graph optimizations are CUDA-gated.
Portable Metal execution does not inherit those kernels. CUDA-specific planar
Q8 layouts must not replace portable artifacts indiscriminately.
[Pinned CUDA gates](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/v0.1.0/ggml-patches/README.md).
No NVIDIA hardware was available for execution in this audit; CUDA remains a
separate native-host validation lane.

#### MLX candidates and integration proposal

MLX-Audio Python [v0.5.5](https://github.com/Blaizzy/mlx-audio/releases/tag/v0.5.5)
pins `cd605ecfcc266ccf6ea3077586c373101be982c6`; MLX-Audio-Swift
[v0.1.3](https://github.com/Blaizzy/mlx-audio-swift/releases/tag/v0.1.3) pins
`d302a5c6080d2bb97bae38c7418f82abb76013b6`. They contain implementations of
the above speech families. These are community model ports built on Apple's MLX;
they are not Apple/NVIDIA/Cohere qualification of this application.

For Cohere, the Swift implementation documents
[`beshkenadze/cohere-transcribe-03-2026-mlx-fp16`](https://huggingface.co/beshkenadze/cohere-transcribe-03-2026-mlx-fp16/tree/06bbe0d853a93cbbf7c1f40cffccd4c5481459ef)
at `06bbe0d853a93cbbf7c1f40cffccd4c5481459ef`. Its conversion records original
revision `494db8a1d34a3aeb28e9ecf61bae9e7cdef455b9`; match the CPU checkpoint
lineage before attributing differences to hardware. An arbitrary “MLX 8-bit”
repository is not interchangeable: the observed `mlx-community/cohere-transcribe-03-2026-mlx-8bit`
card targets mlx-speech and did not supply the expected `config.json`.
Start with a documented loader/artifact pair and inventory every required file
and hash before downloading. FP16 versus INT8 is a deployment comparison, not a
controlled backend-only comparison.

The [Swift manifest](https://raw.githubusercontent.com/Blaizzy/mlx-audio-swift/d302a5c6080d2bb97bae38c7418f82abb76013b6/Package.swift)
requires Swift **6.2** and macOS 14, despite older requirements in its README.
Its ranged dependencies require a committed resolution for reproducible builds.
Python is a useful isolated reference benchmark; shipping it adds an interpreter
and dependency environment. A Swift bridge avoids that interpreter but adds a
native packaging/FFI boundary. MLX C/Rust tensor bindings alone do not implement
the speech architectures. Neither bridge was integrated at the 22 September audit; the later approved Swift integration is covered by ADR-0020.

Historical proposed sequence from 22 September, before the subsequent isolated experiments and explicit MLX application approval:

1. Qualify current Qwen and Nemotron Metal/CUDA routes through the existing
   benchmark harness, with CPU controls and honest backend evidence. Fix focused
   fallback/provenance defects under #30 without changing inference ownership.
2. Test LFM's existing native APIs in an isolated four-way experiment:
   CPU/CPU, GPU backbone/CPU projector, CPU backbone/GPU projector, GPU/GPU.
   Keep prompts, segments, weights and output limits fixed; do not expose an
   unqualified backend in the application menu.
3. Add a development-only, locked MLX reference lane, starting with default
   Cohere completed audio and a documented compatible checkpoint. Use the
   existing held-out manifests/results contract. Qualify Qwen3-ASR and Whisper
   subsequently; retain Nemotron's existing live runtime until a replacement
   demonstrates a benefit.
4. Only if that comparison passes, propose one shared native MLX bridge beneath
   the existing adapter registry. Preserve the current Rust inference owner,
   bounded work, cancellation, artifact verification and fresh recording state.
   Reuse framework loaders with architecture-specific processors/decoders;
   do not create a new service or adapter per model name. Independent speech and
   writing placement needs a separate configuration proposal if it changes the
   current global preference.

At that audit, steps 3–4 added dependencies/runtime boundaries and were proposals, not approved
implementation. The benefit hypothesis is reduced speech-stage compute time;
costs include another artifact format, build toolchain, compatibility surface and
shared-memory contention with the writer. Do not change defaults before paired
quality and latency gates pass. Live input requires actual incremental state;
streaming generated tokens from a complete waveform does not satisfy that gate.

Use the existing [qualification protocol](../benchmarks/asr/README.md): freeze
source/executable/runtime/artifact hashes; compare cold process, first compilation
and resident requests separately; synchronize lazy/asynchronous GPU work; run
configurations sequentially in alternating order. Report prefill/decode, total
ASR+writer time, failure and fallback counts, WER/CER and meaning preservation.
For live lanes record first partial, word emission, endpoint, stop-to-final,
queue drops and reset/cancellation isolation. Record power/thermal state and
unified-memory pressure; do not double-count shared RAM as separate VRAM.
Device traces establish placement separately from timed runs. The hypothesis
that GPU offload improves short requests remains subject to these measurements.

### Resident default Cohere

The [Cohere residency comparison](validation/cohere-residency-2026-09-17.md)
measures the implemented [ADR-0017](adrs/0017-resident-cohere-recognizer.md).
On the M5 Pro CPU, 30 English cases/build reduce ASR+writer p50
**3.279→.805 s**, with exact output/error parity. The matched 28 requests after
first use reduce p50 **3.267→.776 s**. First requests still pay verification/load.
Peak pipeline RSS is similar; retaining weights increases idle memory. The
100-cycle test records RSS and cancellation/invalidation behavior separately.
This is a CPU inference-stage result, not full UI latency or edge qualification.

### Shared request-path overhead and cancellation

The shared manager rejects already-cancelled requests before copying inputs or
queueing work, and checks cancellation before replacing an existing resident
handle. A stream also has an independent cancellation flag: cancel/drop wakes an
idle worker and bypasses queued audio, with checks around native push, polling,
finalization and update callbacks. Cancelling partial transcription preserves the
application operation flag so complete-audio fallback can still run. A native
call already executing remains synchronous; this does not promise instant abort.

Capture mixes packets directly from its pending input, avoiding an intermediate
interleaved allocation. Imported/captured 16 kHz audio transfers the owned mono
buffer directly instead of cloning it through the resampler bypass. Packet
boundaries, resampling for other rates, original-audio archival and final tails
retain their existing behavior.

Qwen cleanup no longer calls `accept` after `sample`: the pinned
[llama-cpp-2 0.1.156 API](https://docs.rs/llama-cpp-2/0.1.156/llama_cpp_2/sampling/struct.LlamaSampler.html#method.sample)
already accepts the sampled token. This removes one FFI call per token; the
current greedy sampler is stateless, so a material generation speedup is not
assumed. These are frozen-weight application changes. They introduce no kernel,
checkpoint or cross-recording state reuse. Existing adapter model lifetimes and
the disabled-by-default prefix experiment remain as described above.

The [paired CPU regression](validation/supported-request-optimization-2026-09-17.md)
records 89 executed evaluations across five ASRs plus Nemotron streaming, exact
output/error parity on 43 pairs, and a three-minute real-time stream. The latency
differences are small and mixed; these changes do not establish a material
end-to-end speedup. Missing-model attempts and existing writer rejections remain
visible in the evidence.

### Default-pipeline verification optimization

The [17 September paired application test](validation/application-sha-2026-09-17.md) qualifies Apple ARM hardware SHA with complete integrity checks intact. On one M5 Pro CPU on battery, 30 paired recordings/build reduce ASR p50 **7.246→3.319 s** and ASR+writing p50 **7.718→3.793 s**. Outputs and rejection outcomes match exactly. A separate five-case probe attributes the change to verification (**5.130→1.035 s**); load/decode remain similar. These are inference stages, not UI latency. Other CPU/edge targets are not qualified by this result. The subsequent [resident Cohere decision](adrs/0017-resident-cohere-recognizer.md) retains verification on load and adds observed-file invalidation; its paired measurements are separate from this SHA result.

### Phase-one held-out CPU run

On 15 September 2026, the release runner exercised deterministic 4–20 second held-out samples through the application's actual adapters and the explicit pinned NeMo CPU package. Each row contains 16 successful samples and no failed samples. FLEURS supplies monolingual Hindi and US English; MUCS supplies Hindi-English code switching. Locale and configured-language variants are separate evidence groups.

| Pack and configured language      | Nemotron median WER / RTF | Qwen3-ASR median WER / RTF | Balanced result                   |
| --------------------------------- | ------------------------: | -------------------------: | --------------------------------- |
| Hindi `hi-IN`, explicit `hi`      |             9.35% / 0.045 |             12.13% / 0.411 | Nemotron                          |
| Hindi `hi-IN`, Auto               |             9.35% / 0.047 |             11.44% / 0.420 | Nemotron                          |
| US English `en-US`, explicit `en` |             7.18% / 0.046 |              5.42% / 0.361 | Nemotron; Qwen wins accuracy-only |
| US English `en-US`, Auto          |             7.18% / 0.045 |              7.50% / 0.358 | Nemotron                          |
| Hinglish `hi+en`, Auto            |            47.73% / 0.047 |             51.89% / 0.568 | No qualified model                |

The example balanced policy requires median WER at most 20% for a monolingual pack, 30% for Hinglish, median RTF at most 1.0, and failure rate at most 2%. A comparable locally measured candidate that fails a requested ceiling is excluded; it is not downgraded to unmeasured official support. Nemotron also does not claim code-switching capability, while Qwen's measured Hinglish WER exceeds the ceiling. The safe answer for the current Hinglish pack is therefore “no qualified model,” not a forced recommendation. Exact mixed-script WER penalizes Latin reference words transliterated into Devanagari, so the raw hypotheses still need qualitative analysis.

Four-sample wall-clock streaming subsets exercised continuous packet delivery and final drain. The wall-clock RTF is near 1.0 by construction because replay sleeps to audio time; stop-to-final and first-partial latency are the interactive measurements.

| Pack                              | Median WER | Median wall RTF | p95 stop-to-final | Warm first nonempty partial |
| --------------------------------- | ---------: | --------------: | ----------------: | --------------------------: |
| Hindi `hi-IN`, explicit `hi`      |     10.60% |           1.011 |           91.9 ms |                 1.32–1.96 s |
| US English `en-US`, explicit `en` |      9.01% |           1.010 |           89.0 ms |                 1.32–1.64 s |
| Hinglish `hi+en`, Auto            |     56.06% |           1.014 |           90.9 ms |                 0.68–1.65 s |

The first streaming sample in each process was cold and reached its first nonempty partial in 3.24–4.54 seconds. The sample count is deliberately small and does not support a production p95 claim. Indian English was later screened on 48 Svarah recordings in a separate personalization experiment: raw WER was 18.50% for Nemotron and 9.83% for Cohere, with 48/48 executions successful for each. Because that report belongs to a personalization profile envelope, it is retained as local evidence but excluded from automatic base-model recommendation aggregation. It is not an all-model `en-IN` qualification.

Qwen3-ASR's current adapter verifies and creates its ONNX model per job, which explains much of its 0.36–0.57 RTF on these short clips. Nemotron retains weights and creates fresh utterance streams. A prior instrumented eight-second Qwen run peaked at 1,613,037,568 bytes RSS. A 413-second MUCS recording also completed as one uninterrupted accelerated resident Nemotron stream with 20,555 partial updates, 0.27 RTF, and about 87 ms stop-to-final; that is a Hinglish robustness observation, not a Hindi accuracy result. The long run peaked at 1,232,601,088 bytes RSS and used 222.65 user CPU seconds over 112.62 seconds wall time. These are whole-process peaks, including the runner and decoded audio.

The runs used an Apple Silicon headless sandbox with the explicit NeMo CPU runtime. The exact chip/memory, power mode, sustained thermal state, and per-run peak RSS were unavailable, so this evidence must not be presented as a complete hardware qualification or Metal result. The [reproduction procedure and metric definitions](../benchmarks/asr/README.md) keep raw machine reports gitignored.

The recorded development-machine smoke used an **Apple M5 Pro with 64 GiB RAM**, CPU inference, Parakeet INT8 and Qwen Q4_0 on approximately 3.845 seconds of public English speech:

| Operation                                                                         | Observed elapsed time |
| --------------------------------------------------------------------------------- | --------------------: |
| Parakeet loading plus recognition                                                 |                0.76 s |
| Qwen loading plus cleanup                                                         |                0.70 s |
| Both native stages                                                                |                1.46 s |
| Desktop integration test, including hashes, archive, SQLite and export assertions |                3.63 s |

These are single observations with potentially warm filesystem caches, not p50/p95 values, cold-start measurements, or release-build comparisons. Peak memory and GPU execution were not measured. The derived ASR real-time factor for this one load-inclusive sample is approximately `0.76 / 3.845 = 0.20`; it says nothing about sustained performance on other devices. The integration test includes export work that ordinary dictation need not perform. See the [recorded validation and reproduction commands](validation.md); the native debug result remains in `.cache/benchmarks/results/native-pipeline/debug-summary.json`.

The six synthetic cleanup cases also exposed repetition and meaning changes. An optimization cannot be accepted solely because it produces text sooner. Preserve the existing raw-output fallback and review behavior during performance work. [Cleanup observations](validation.md#cleanup-quality-findings).

## Device and backend strategy

The non-Apple GPU rows below describe deferred qualification targets tracked in
[issue #76](https://github.com/rahulsharmavishwakarma/vesper/issues/76).

The paths below are selectable and packaged where noted, but still require successful execution and comparison against CPU on the exact target. Existing observations are single-fixture development checks, not cross-device qualification.

| Device family                 | Speech candidate                                                                 | Cleanup candidate                                    | Recommended order                                                                   |
| ----------------------------- | -------------------------------------------------------------------------------- | ---------------------------------------------------- | ----------------------------------------------------------------------------------- |
| Apple Silicon macOS           | Packaged NeMo Metal                                                              | Built-in llama.cpp Metal                             | Current `Auto`; benchmark against explicit CPU                                      |
| Linux/Windows with NVIDIA GPU | Packaged NeMo CUDA, selected automatically after a successful `nvidia-smi` probe | llama.cpp CUDA build                                 | Build with `--features cuda`; qualify driver/toolkit/device                         |
| AMD GPU                       | Packaged NeMo Vulkan                                                             | llama.cpp Vulkan, or ROCm build                      | Vulkan works across more devices; ROCm profile combines Vulkan ASR with HIP cleanup |
| Intel integrated/discrete GPU | Packaged NeMo Vulkan                                                             | llama.cpp Vulkan build                               | Build with `--features vulkan`; qualify driver and memory behavior                  |
| Other ARM/x86 systems         | Native CPU with suitable instruction support                                     | Native CPU                                           | Smaller compatible models and bounded workload; measure on actual hardware          |
| iOS/Android, future           | Platform CPU first, then qualified native accelerator paths                      | Platform CPU/Metal or other supported native backend | Treat as a separate packaging, capture and lifecycle project                        |

**Apple.** `Auto` selects the packaged NeMo Metal runtime and offloads Qwen through llama.cpp Metal on Apple Silicon. MLX is a separate array/model framework; the selected Nemotron GGUF and official runtime do not expose an MLX backend. Compare against explicit CPU because GPU dispatch can lose on a very short cleanup. [llama.cpp backend build guidance](https://github.com/ggml-org/llama.cpp/blob/master/docs/build.md).

**Neural Engine spike (22 October 2026):** a Core ML fp16 Parakeet v3 encoder ran 99.98% on the ANE. Through Vesper's mel and MLX decoder it matched today's WER (5.25% held-out) and was about 10% faster per clip. It used about 4.6× less encoder energy, and a concurrent Metal writer slowed by 8% instead of 61%. It needs encoder windows of at most 15 s and an 8–11 s first load per macOS update. See the [spike record](validation/ane-coreml-spike-2026-10-22.md). It now ships as the **Neural Engine** speech engine for English: same accuracy as the MLX path on recordings up to 15 s, with a few more errors on longer ones from 15 s parts. See [ADR-0025](adrs/0025-neural-engine-speech-engine.md) and the [qualification record](validation/parakeet-neural-engine-2026-10-22.md).

Core ML can target Apple CPU, GPU and Neural Engine. Its ONNX provider supports only eligible graph portions; dynamic shapes can affect performance, and newer MLProgram/compute-unit options differ from the legacy API. Profile coverage and compilation time for the exact Parakeet export. A CPU INT8 bundle is not proof of ANE compatibility or acceleration. [Core ML execution provider](https://onnxruntime.ai/docs/execution-providers/CoreML-ExecutionProvider.html), [ONNX provider graph assignment](https://onnxruntime.ai/docs/execution-providers/).

**NVIDIA.** Select CUDA and compile Vesper with `--features cuda`. The installer chooses NVIDIA's matching NeMo release package, while the feature builds llama.cpp CUDA for Qwen. Device transfer, workspace, driver compatibility, and allocations remain part of the benchmark.

**AMD.** Vulkan selects NeMo and llama.cpp Vulkan when built with `--features vulkan`. The ROCm profile selects NeMo Vulkan because no official ROCm speech package exists, and llama.cpp HIP when built with `--features rocm`. Match the supported GPU, OS, and driver combination. [llama.cpp HIP/Vulkan build guidance](https://github.com/ggml-org/llama.cpp/blob/master/docs/build.md), [AMD compatibility matrix](https://rocm.docs.amd.com/en/latest/compatibility/compatibility-matrix.html).

For the fallback ONNX model, do not start a new integration around the old ONNX Runtime ROCm provider: upstream removed it starting with 1.23 and directs users to MIGraphX. This does not affect the default NeMo Vulkan path or llama.cpp HIP cleanup. [ROCm provider retirement](https://onnxruntime.ai/docs/execution-providers/ROCm-ExecutionProvider.html), [MIGraphX provider](https://onnxruntime.ai/docs/execution-providers/MIGraphX-ExecutionProvider.html).

**Intel.** Vulkan is the implemented Nemotron/cleanup route for compatible Intel GPUs. OpenVINO remains a possible future fallback-model or Neural Processing Unit route, not a backend for the current Nemotron adapter. [OpenVINO provider](https://onnxruntime.ai/docs/execution-providers/OpenVINO-ExecutionProvider.html).

For cleanup, upstream SYCL targets Intel GPUs and requires matching toolchain/runtime components. It is not exposed as a direct feature by our pinned Rust wrapper; additional build/link integration would be needed. Vulkan is another candidate available through the wrapper. Neither route is currently verified for Qwen3.5 in this app. [llama.cpp SYCL documentation](https://github.com/ggml-org/llama.cpp/blob/master/docs/backend/SYCL.md).

**Pinned Sherpa details.** Version 1.13.8 includes CUDA, Core ML, DirectML, OpenVINO and other provider branches, often falling back to CPU when unavailable. Its Core ML branch uses the legacy flags API. OpenVINO defaults to NPU unless `device_type` is provided. Its TensorRT branch requires online-model configuration and is not a drop-in setting for our offline Parakeet path. Record actual execution, including fallback, rather than displaying the requested provider as an attestation. [Versioned Sherpa session implementation](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.8/sherpa-onnx/csrc/session.cc).

**Mobile.** Tauri supports native Kotlin/Java and Swift plugin implementations alongside shared Rust logic. Mobile capture permissions, audio interruptions, suspension, native library packaging, memory limits and background behavior need their own implementation and device tests. Desktop microphone integration and a responsive web page do not establish mobile support. Keep recording state and audio recovery independent of model lifetime. [Tauri mobile plugin development](https://v2.tauri.app/develop/plugins/develop-mobile/).

## Changes to prioritize

1. **Add stage timing before changing defaults.** Measure archive, queue residence/drops, first partial, final drain, verification, model/session loading, ASR, prompt evaluation, token generation, fidelity checks, persistence and final UI delivery separately. Make the final accepted transcript the primary interactive latency metric.
2. **Complete resident-lifecycle qualification.** The bounded owner, cache keys, fresh NeMo streams, fresh Qwen contexts and explicit unload on model/backend changes are implemented. Run at least 100 finish/cancel/reset cycles, memory measurements and mixed-language state-leakage tests before treating the lifecycle gate as complete. An idle eviction timer remains deliberately unimplemented until memory pressure is measured.
3. **Separate the two CPU budgets.** Benchmark ASR threads and cleanup generation/prompt threads independently. Sweep 1, 2, 4, 6, 8 and additional device-appropriate counts; these are experiment values, not new defaults. Preserve UI responsiveness and capture reliability under load. Do not overlap full-core ASR and cleanup jobs by default: cleanup depends on the transcript, while idle session pools can still consume resources.
4. **Benchmark the implemented Metal path.** On the available Mac, compare the same audio, Q4 model, prompts, and guard behavior with explicit CPU. Record backend logs, memory, cold load, and warm-equivalent stage timing.
5. **Qualify CUDA, Vulkan, and ROCm packages.** Test named GPU/driver/OS combinations and package loading, cancellation, output equivalence, sustained thermals, and memory pressure.
6. **Split device controls and add probing.** The first implementation uses one pipeline backend. A later control should report actual devices independently per stage and let `Auto` choose only measured profiles, with a visible fallback.

Caching preserves integrity by performing full role-specific verification when a resident handle is created and unloading handles before settings/model replacement. Session inference metadata describes the selected provider; because `Auto` can fall back during native initialization, it is not yet an attestation of the device that executed every operator. Runtime-observed provider provenance and visible fallback reporting remain measurement work. Training or candidate evaluation should use a separate idle-work budget and must not compete with interactive recording.

ONNX Runtime distinguishes intra-op parallelism from inter-op graph parallelism. Its default graph execution is sequential; an inter-op count alone does not enable parallel graph execution. Spinning may reduce wakeup latency while increasing power use, and multiple session pools can contend. Compare spinning disabled against enabled for desktop latency and battery modes. [ONNX threading guidance](https://onnxruntime.ai/docs/performance/tune-performance/threading.html).

Sherpa 1.13.8 maps `num_threads` to both intra/inter counts and forwards `SessionConfig.*` entries from a provider configuration file. That offers an experimental route for supported ONNX session keys and profiling without inventing new Rust binding APIs; independent pool controls may still require upstream changes. Validate every key against the linked runtime version. [Versioned session implementation](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.8/sherpa-onnx/csrc/session.cc).

## Precision, memory and sustained use

Treat precision as part of each model/backend profile. For speech, compare the existing INT8 export with a compatible higher-precision reference before proposing further quantization. For cleanup, compare Q4_0 and Q8_0, then other supported quantizations only as separate candidates. Record changed outputs, rejection rates and human corrections. Quantization performance depends on hardware instructions and graph representation; smaller files do not guarantee lower latency. [ONNX quantization guidance](https://onnxruntime.ai/docs/performance/model-optimizations/quantization.html), [GGUF representations](https://huggingface.co/docs/hub/gguf).

Measure process peak resident memory, accelerator allocations, context/KV or recurrent state, compute buffers, and concurrent model residency. Artifact size is only one component. On unified-memory Macs, report shared-memory pressure rather than adding CPU and GPU figures as if they were independent physical pools. Do not use all 64 GiB simply because it is installed. Start with bounded context, one interactive job and an explicit unload policy; investigate larger batches or flash attention only when the chosen model/backend supports them and profiling justifies it.

Repeat a realistic dictation workload for 10–15 minutes, on mains and battery where applicable, and record thermal/power mode, latency drift and capture failures. This duration is a proposed test condition. Apple exposes a thermal-state signal that can inform a future conservative scheduling policy. No thermal throttling or battery measurements have yet been collected here. [Apple thermal-state API](https://developer.apple.com/documentation/foundation/processinfo/thermalstate-swift.property).

## Reproducible benchmark protocol

Use release builds and a frozen corpus of human-referenced speech covering short commands, ordinary paragraphs, longer recordings, supported languages, accents, noise and silence. Do not use the six development cleanup cases as a held-out benchmark. Keep an additional review set for names, numbers, dates, negation, uncertainty, quotations and prompt-like dictated content. Record dataset version, consent scope and reference provenance.

| Metric                      | Definition and reporting                                                                                                                                   |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Cold process start          | New process with no resident model; report filesystem and compiled-backend cache state separately                                                          |
| Cold artifact/compile start | First use with backend compilation cache absent; never label an ordinary restart as a cold disk-cache test                                                 |
| Warm latency                | Resident models, fresh utterance state; separate any warmup samples                                                                                        |
| ASR RTF                     | Recognition elapsed seconds / audio seconds; report load-exclusive and load-inclusive forms explicitly                                                     |
| Cleanup TTFT                | Cleanup request to first generated token; report whether loading and prompt evaluation are included                                                        |
| Cleanup generation rate     | Generated tokens / decode time, with prompt/output token counts and tokenizer/model ID                                                                     |
| Final text latency          | Recording stop or imported-file submission to accepted final text or explicit raw fallback                                                                 |
| Distribution                | Per-device/per-workload p50 and p95, sample count and failures; do not pool different clip lengths into a misleading single number                         |
| Resource use                | Peak RSS, GPU/shared allocations, idle residency, CPU load, power mode and sustained thermal state                                                         |
| ASR quality                 | WER `(substitutions + deletions + insertions) / reference words`, with declared normalization; CER where appropriate; punctuation/case assessed separately |
| Meaning preservation        | Human-reviewed changes to entities, numbers, negation and intent; guard rejection, truncation, repetition and raw-fallback rates                           |

As an initial screening protocol, use at least 30 timed warm requests per configuration and several independent process starts. For a publishable p95 claim, expand to a representative corpus with substantially more independent requests and report uncertainty; repeated copies of one fixture do not establish a population latency distribution. Alternate configuration order to reduce cache/thermal bias. Report failures in the denominator rather than timing only successful runs.

Use monotonic Rust timers for application spans. ONNX Runtime profiling can expose operator timing and execution providers. Upstream `llama-bench` separates prompt processing and generation and supports machine-readable output, making it useful for backend screening; it does not include our actual prompts, guards, audio path or UI. Use the llama.cpp revision corresponding to the bundled binding when comparing results. [ONNX profiling](https://onnxruntime.ai/docs/performance/tune-performance/profiling-tools.html), [llama-bench](https://github.com/ggml-org/llama.cpp/blob/master/tools/llama-bench/README.md).

Store one JSONL record per run with hardware/OS, driver versions, application revision, runtime build/features, model revision and hashes, requested/actual device, thread/offload settings, input duration, token counts, stage timings, memory observations, outputs and errors. Publish aggregate results alongside this manifest. Preserve raw ASR independently from cleaned output, and keep machine predictions separate from human training labels.

Promote a performance profile only after it passes existing tests, the held-out quality review, repeated latency/memory measurements, cancellation/recovery tests, and package validation on its named devices. The dated screens above provide local CPU/Metal observations; they do not establish universal hardware defaults or qualify untested backends.

### Approved optional MLX application validation — 23 September 2026

[ADR-0020](adrs/0020-optional-mlx-speech-runtime.md) records explicit approval and the implemented scope. The optional Tauri `.app` built successfully. Its library, `mlx.metallib` and tokenizer tool matched the Rust build's expected hashes. A qualification executable using the application `InferenceManager`, placed in a copy of the built bundle, ran against those resources. This validates native resource resolution and the shared recording/import inference path; it is not a microphone/UI end-to-end test. Missing kernels failed readiness, and same-size corrupted kernels failed integrity before native loading. Neither case fell back to developer files.

All 252 completed-audio requests succeeded: the same 84 English clean/quiet/noise/control cases for each model below. Every transcript exactly matched its final isolated MLX baseline. Exact parity preserves the earlier quality limitations rather than erasing them.

| MLX variant               | Clean / quiet / noisy word errors, each /689 | False transcripts on eight non-speech controls | Warm clean median ASR | First ASR request |
| ------------------------- | -------------------------------------------- | ---------------------------------------------- | --------------------- | ----------------- |
| Parakeet v3, BF16 compute | 29 / 35 / 45                                 | 0/8                                            | 55.20 ms              | 4.762 s           |
| Qwen-ASR 0.6B, 8-bit      | 47 / 52 / 85                                 | 4/8                                            | 234.45 ms             | 3.380 s           |
| Whisper Small, FP16       | 42 / 79 / 115                                | 2/8                                            | 232.50 ms             | 1.736 s           |

Warm clean medians use 23 clean requests after excluding the first request. First requests include integrity verification, model load and initial native execution. Separate verification/load timings in microseconds were Parakeet 973147/797215, Qwen 394444/479056 and Whisper 210109/163760. The host remains M5 Pro/64 GiB/macOS27, fixed request order, uncontrolled power/thermal state and previously used shader caches. These are ASR request timings, excluding capture/frontends/writing/UI; they are neither live latency nor backend-only speedups against differently quantized CPU exports. MLX uses its runtime thread defaults. The existing evaluator's positive `threads=8` field describes the other native runtime worker setting; the environment notes explicitly distinguish it from an MLX graph-thread count.

A further 148.68-second diagnostic concatenated the first 12 clean held-out clips without overlap or added silence, retaining all float32 samples and concatenating their references. All three models delivered all 2,378,880 samples. Parakeet, Qwen and Whisper Small had respectively 20, 28 and 24 word errors out of 383; their cold application ASR requests took 1.873, 3.724 and 3.349 seconds with already-used shader caches. This tests segmentation and long-file coverage; concatenated utterances are not a natural continuous-speech corpus.

Eighteen completed-recording pipeline requests (six per variant) enabled advisory Silero, DPDFNet4 and the writer. All 18 ASR requests completed and all 18 rewrites passed the existing fidelity guard. Raw word errors per 128 reference words were Parakeet 5, Qwen 4 and Whisper 9. Both speech and writing reported Metal; the small frontends stayed CPU. This small clean set does not establish a denoising quality improvement or replace the broader frontend regressions.

Real application lifecycle tests covered all three variants: twelve repeated A/B/A requests each, more-than-minute input, monotonic full sample coverage, native/progress cancellation, recovery, rejected live-stream requests, exact silence, unload/reload and model switches. The same selected speech weights remained resident until replacement or unload. Encoder/kernel work remains non-preemptible. Installer tests covered deterministic Qwen tokenizer repair, corrupt-source rejection and cancellation; an unlisted weight-shard test protects the upstream directory enumeration boundary. Hashes precede every native load and observed file identities govern reuse.

Raw results remain ignored under `gpu-adapters-2026-09-23/application-mlx-*`, including before/after executable/manifest hashes, exact runtime/model artifacts, environment, errors and long-audio derivation. The 84-case manifests and original isolated reports remain unchanged. A first lifecycle attempt used an incorrect fixture path and stopped before inference; the corrected run passed. A local HTTP fixture initially hit sandbox restrictions and passed outside the sandbox. Automatic approval review timed out on one benchmark launch; the permitted retry succeeded.

The production GPU catalog records Parakeet's CC-BY4.0 conversion attribution, Qwen's Apache2.0 card and the pinned Whisper conversion/tokenizer cards' Apache2.0 metadata. OpenAI's Whisper source repository separately uses MIT. Runtime build metadata does not assign a license to this project's bridge: the project license remains unselected and dependencies retain their upstream licenses. Earlier measured runtime inventories used a generic MIT label; that label is corrected in the implementation without altering measured binaries or results.

Cohere/Whisper Turbo GPU variants, other languages/automatic language detection, code-switching, singing, natural long-form corpora, sustained thermal/power behavior, distribution signing/notarization and CUDA hardware execution remain unqualified here. CPU/Automatic speech defaults are unchanged; Whisper Small remains outside the desktop picker.

Final validation after the directory-integrity test and runtime-license metadata correction: feature-on core 39, inference 61 and models 24 unit tests; feature-off core 39, inference 60 and models 22; 28 frontend tests; Svelte with zero warnings/errors; strict Rust Clippy, ESLint, formatting and diff checks. The final app bundle rebuilt, all native resource hashes matched again, and the three-variant real Metal lifecycle plus tokenizer preparation tests passed against the final Rust build. This validation preceded the local integration commits. The shared checkout's unrelated user edits were preserved; these measurements must not be relabeled as post-commit reruns.

### Reading the CPU versus GPU comparison

The same 23 warm clean English cases give the following ASR medians on the named
M5 Pro. CPU values are the earlier pinned-runtime baselines; MLX values are the
packaged application screen above. These are completed-audio ASR calls, excluding
capture, denoising, writing and UI. The LFM row uses its paired 84-case native
screen; all rows exclude their first request. Earlier Qwen/Parakeet CPU clean
medians included all 24 cases; the table below recalculates both CPU paths over
the same 23 cases as the packaged MLX medians.

| Model         | CPU median | Metal median | Comparison boundary                                            |
| ------------- | ---------- | ------------ | -------------------------------------------------------------- |
| LFM2.5 Audio  | 407.28 ms  | 205.25 ms    | Same Q4 native artifacts, resident weights on both paths       |
| Whisper Small | 730.23 ms  | 232.50 ms    | Resident Sherpa INT8 versus resident MLX FP16                  |
| Qwen-ASR 0.6B | 1816.46 ms | 234.45 ms    | Sherpa INT8 with per-job load versus resident MLX 8-bit        |
| Parakeet v3   | 953.33 ms  | 55.20 ms     | Sherpa INT8 with per-job load versus resident MLX BF16 compute |

Only LFM keeps the runtime/artifacts/ownership comparable, and even that screen
has uncontrolled host state and small quality differences. The other rows compare
whole application configurations: residency, runtime, export and precision also
change. They cannot establish GPU-only speedup. Cold load and shader compilation
can dominate a first request. The held-out WER and non-speech failures recorded
above remain part of the verdict. CUDA performance was not measured because this
host has no NVIDIA GPU. Local commit identifiers and merge state belong to #35;
raw benchmark binary identities remain attached to their original measurements.

### Cold exact-zero MLX recordings

The existing worker shortcut for cold exact-zero Sherpa recordings now also covers the qualified MLX variants. It validates the configuration, sample rate, model-directory layout, model hashes and packaged runtime hashes, reports complete sample coverage, then returns empty text without constructing speech weights. Verification success is not cached on this path. An already-selected resident model stays resident. Every finite nonzero sample, including extremely small positive/negative values, follows normal transcription; no amplitude threshold or additional speech/noise classifier was introduced. Cancellation and missing/corrupted installation checks still apply.

A real Metal regression failed before this change because all three variants reported resident weights after an all-zero request; it passes afterward with `residentWeights=false` and no initialized speech provider. The existing repeated-speech, >60s coverage, cancellation/recovery and model-switch lifecycle also passes. Unit cases cover tiny nonzero input, incorrect sample rate, cancellation and missing installation. This is frozen-weight application/runtime work (category 1), with no native kernel or training change. It reuses the previously established exact-zero segment policy instead of claiming a new paper technique.

One unloaded, one-second digital-zero request per variant gave these observations on the same M5 Pro/macOS27 host. The test binary uses the Rust development profile with the unchanged release Swift runtime. These are single observations, not latency distributions:

| Model         | Before     | After     | Speech weights loaded afterward |
| ------------- | ---------- | --------- | ------------------------------- |
| Parakeet      | 3970.22 ms | 855.04 ms | No                              |
| Qwen-ASR      | 732.79 ms  | 373.55 ms | No                              |
| Whisper Small | 391.13 ms  | 195.84 ms | No                              |

Timings include integrity verification. The first request can also include worker/driver startup, and processes ran sequentially with uncontrolled host state and existing shader caches. Therefore the Parakeet difference cannot be attributed solely to weight loading. Avoiding speech weight construction is directly observed; general speech latency, ordinary microphone silence, noise hallucinations and model accuracy are not changed or requalified by this diagnostic. Raw before/after logs, binary identities, generated-input description and environment are retained in the ignored `application-mlx-zero-*` results.

A separate source audit found no kernel-resource precedence defect: pinned MLX `ce45c52505c8158ea48d2a54e8caae05efd86bfe` [loads colocated `mlx.metallib` before SwiftPM bundles](https://github.com/ml-explore/mlx/blob/ce45c52505c8158ea48d2a54e8caae05efd86bfe/mlx/backend/metal/device.cpp#L139). The application verifies that file before loading the native model; all existing kernel copies matched. This source fact does not attest individual operator placement.

Follow-up verification: 62 feature-on inference tests and 60 feature-off inference tests passed, along with both real-Metal lifecycle tests, strict Clippy, Rust formatting and diff checks. The optional `.app` rebuilt successfully with the shortcut. The earlier 252-case speech-parity screen is retained as earlier evidence; this focused change was validated with exact-zero, tiny-nonzero and lifecycle regressions rather than presented as a new full quality benchmark.

### Automatic artifact/runtime selection — 25 September 2026

[ADR-0021](adrs/0021-automatic-accelerator-artifacts.md) qualifies the new selection path separately from the September23 CPU/MLX comparison. This is frozen-weight application orchestration; no kernel, decoder, training or checkpoint change was made. The saved preference stays Automatic while speech resolves its compatible export, and the writer resolves independently.

Local release evaluator at `8db742410ed538ca644bc31859238817d3f2d7fb` plus the unstaged automatic-acceleration implementation; source/binary digests and native build identities are retained in ignored `automatic-acceleration-2026-09-25/build-identity.json`. Hardware: Apple M5 Pro, 18 logical CPUs, 64GiB, macOS27.0(26A428), system Metal. Runtime pins and all model hashes are unchanged from ADR-0020. MLX has no explicit thread override; environment threads8 describes the other application runtimes. The host was shared and power/thermals were uncontrolled. Some compilation overlapped this screen, so timings are descriptive and do not establish selection-overhead significance or a new speedup.

The exact same 84-case English manifest has SHA256 `7b32ed77e4481c9d901200c8d5b10ecae95915e8f142c9dbb07444307940f451`: 24 distinct FLEURS utterances in clean, −30dB and noise0dB conditions plus12 controls. Automatic and explicit Metal each ran all 84 cases for each model, for 504 completed ASR requests with zero execution failures. Automatic transcripts matched explicit Metal exactly in all 252 paired cases and also matched the retained September23 explicit outputs. Actual per-request provider was Metal and matched the preferred artifacts. Whisper Small remains evaluation-only.

| Model           | Warm clean Automatic median | Warm clean explicit Metal median | First Automatic request |      Clean WER |       Quiet WER |        Noise WER | Non-speech false transcripts |
| --------------- | --------------------------: | -------------------------------: | ----------------------: | -------------: | --------------: | ---------------: | ---------------------------: |
| Parakeet TDT v3 |                     73.40ms |                          76.60ms |               5772.70ms | 29/689 (4.21%) |  35/689 (5.08%) |   45/689 (6.53%) |                          0/8 |
| Qwen3-ASR 0.6B  |                    293.90ms |                         292.90ms |               4830.65ms | 47/689 (6.82%) |  52/689 (7.55%) |  85/689 (12.34%) |                          4/8 |
| Whisper Small   |                    252.52ms |                         223.84ms |               1700.35ms | 42/689 (6.10%) | 79/689 (11.47%) | 115/689 (16.69%) |                          2/8 |

Warm medians use 23 clean cases after the first case, not 23 independent cold starts. `asrMs` is completed-audio manager wall time from submission to returned raw text, excluding file decode, installation and writer time. The first includes model verification/loading; the device probe can already have run during readiness. First-request shader/OS caches and fixed run order differ, so cold differences are not attributable to Automatic. Process maximum RSS from `time -l` for Automatic was about 2.61GiB/1.13GiB/0.64GiB respectively; these are process measurements, not complete GPU/unified-memory accounting. No power or sustained thermal measurements were taken. The unchanged non-speech errors remain visible and are not converted into successes by backend retries.

Additional application checks: a real Qwen CPU-only installation remained ready with an optimized update available, produced the same text as explicit CPU, then used verified MLX artifacts after ordinary cached installation. Automatic and explicit Metal reused the same resident load and produced the same text; preferences were unchanged. The real native lifecycle, cancellation/reset, complete coverage and cold exact-zero tests passed. Both promoted models completed the 148.68-second imported-audio fixture with all 2,378,880 samples delivered. Six public clips per model also completed with Silero+DPDFNet4 and the independently accelerated writer; all 12 ASR and cleanup passes succeeded. Frontend work remains part of those pipeline timings and can dominate raw ASR latency; the table above intentionally uses the frontend-off comparison.

Logs/manifests/environment records remain ignored local evidence, not new committed benchmark JSON. This establishes agreement with the existing qualified English paths, not broad language/edge-device qualification, a SOTA claim, a default-model change or a new CPU-versus-GPU measurement. The prior CPU comparison retains its own date, precision and ownership differences.

The streaming regression used the installed Nemotron Metal package with Automatic, Silero+DPDFNet4 and the writer: six completed-file plus six 20ms real-time-paced streaming cases all returned raw ASR; four of six cleanup results passed in each mode and two fidelity failures preserved raw text. Streaming first partials were 1022–1716ms from replay start, and explicit finish-to-final was 33.87–54.41ms. These are replay and finalization measurements, not acoustic endpoint or physical microphone latency. The attempted CPU run failed readiness because the local CPU runtime package was absent; its failure log is retained, and it is not counted as a successful CPU comparison. No injected queue-overflow, word-emission alignment or sustained thermal/power qualification was performed.

Normal `npm run desktop:build -- --debug --bundles app` produced `Vesper.app` with all three native resource hashes matching the Rust-bound build identities. A relocated evaluator using copies of that bundle's resources completed Automatic transcription. Removing or corrupting the copied runtime failed readiness; neither silently loaded developer resources or retried CPU. Native fixture tests passed for real CPU-to-MLX migration, cancellation/reset/coverage and exact-zero recordings. Portable core/inference/models tests passed 39/60/24; MLX core/inference/models passed 39/65/27, with fixture-dependent tests reported separately. Strict Clippy and the normal fast checks passed. Hosted CI, signed/notarized distribution and CUDA hardware execution remain unverified.

#### Verification after integration into local main

The complete seven-commit implementation was replayed from clean committed main at `59c6d21a75cf0b4bd5d5c6ad288280867025f58e`, using a fresh release evaluator and native runtime rebuilt from that checkout. The same hardware, pinned models/runtime revisions, manifest and latency definitions above apply. Ignored `automatic-acceleration-main-2026-09-25/build-identity.json` records this revision and the new source, evaluator and native artifact hashes; the earlier unstaged evidence remains unchanged. Task builds finished before sequential inference measurements, but the shared host's power and thermals were still uncontrolled.

All 252 Automatic ASR requests completed, reported Metal, and matched both the previous Automatic and explicit Metal transcripts exactly. WER and non-speech errors were unchanged. Warm clean medians over the same 23 resident cases were **55.96ms Parakeet, 206.27ms Qwen and 233.94ms Whisper Small**. First requests were 1174.18ms, 945.83ms and 580.40ms respectively, with previously exercised OS/shader caches; these are fresh-process requests, not cache-cold system measurements. The timing differences from the earlier shared-host run do not establish a code speedup.

Both 148.68-second imports again processed all 2,378,880 samples. All 12 Parakeet/Qwen Silero+DPDFNet4+writer cases passed with identical raw and cleaned outputs. Nemotron's six completed-file and six real-time-paced streaming cases all returned identical raw ASR; the same two cleanup fidelity failures in each mode preserved raw text. Streaming first partials were 1028.49–1712.22ms and explicit finish-to-final was 34.88–49.91ms. Microphone endpoint timing, word-emission alignment, queue-overflow injection and sustained thermal/power qualification remain outside this replay.

Post-integration checks passed: 126 Node tests and the remaining fast checks; portable core/inference/models 39/60/24; MLX core/inference/models 39/65/27; three real-Metal migration/lifecycle/silence fixtures; and strict Clippy. Normal desktop packaging rebuilt `Vesper.app`; all three native resource hashes matched. A relocated evaluator completed six further cases using the bundled resources, while missing/corrupt copies failed readiness. No benchmark JSON, audio or weights were committed. This remains local verification, with hosted CI, release signing/notarization and CUDA hardware execution unverified.

### Resident resource qualification procedure

The installed LFM and MLX lifecycle tests now run 100 alternating public-audio
requests before their existing cancellation, reset, full-coverage and unload
checks. They observe process RSS every ten requests and enforce a predeclared
128 MiB growth ceiling relative to the first ten-request checkpoint, matching
the existing Sherpa lifecycle ceiling. Missing process observations fail rather
than become zero usage. This is a bounded repeated-request memory test; process
RSS is not total GPU memory, repeated recordings are not independent quality
samples, and passing does not establish an hours-long thermal equilibrium.

Run installed-model tests serially with explicit fixture/model paths after
compilation. Record fresh-process versus resident state, actual backend, the
native/model hashes and the fixed input identities. Sample power, thermal warning
state and competing processes outside the model process; unavailable counters
must remain unavailable. For actual native-panel timing, use the existing
[public-PCM WebView replay](personalization/native-qualification.md#native-webview-replay)
and retain its narrower stop-to-render-opportunity definition. Public replay
cannot replace deliberate microphone or natural independent correction sessions.

### Fresh public holdout qualification — 25 September 2026

The release evaluator at `8a2c27b0765fea034fb0c0b8614e58f8e1f20f14`, plus
qualification-only test/tool changes, ran **448 completed-audio evaluations**:
11 English configurations and three Hindi configurations, each on 32 frozen
FLEURS test recordings. These are new selections relative to the frozen scan of
cached/versioned experiment history, with unique sentence IDs within each locale;
they are not proof of exclusion from model training or unavailable history.
The [preparer](../scripts/prepare-qualification-holdout.mjs) excludes previously
used audio IDs, normalized references and matching sentence groups before inference.
No accuracy threshold, model default, checkpoint or production decoder changed.

The same M5 Pro/64 GiB/macOS 27.0 host ran jobs serially after all builds finished.
Application threads were eight; MLX has no equivalent thread override. CPU Sherpa
exports use INT8, MLX Parakeet stores float32 with the native BF16 default, MLX
Qwen uses 8-bit weights, MLX Whisper Small uses float16, LFM uses Q4_0 and Nemotron
uses Q8_0. Per-request artifacts and runtime revisions are retained. These are
comparisons of complete runtime/artifact/ownership configurations, not isolated
GPU kernel speedups. CPU Parakeet and Qwen retained per-job model loads in this historical run; the later live-adapter implementation changes that ownership.

| English configuration / actual provider | Failures / 32 | WER, failure as empty | Subsequent ASR mean / p95 |
| --------------------------------------- | ------------: | --------------------: | ------------------------: |
| Cohere / CPU                            |             1 |                11.09% |          550.2 / 806.8 ms |
| Parakeet / CPU                          |             0 |                 9.15% |         911.3 / 1015.2 ms |
| Parakeet Automatic / Metal MLX          |             0 |                 5.55% |            48.2 / 67.6 ms |
| Qwen-ASR / CPU                          |             0 |                 6.30% |        1724.3 / 1934.3 ms |
| Qwen-ASR Automatic / Metal MLX          |             0 |                 3.15% |          182.0 / 288.3 ms |
| LFM / CPU                               |             0 |                 7.35% |          336.3 / 514.7 ms |
| LFM Automatic / Metal                   |             0 |                 6.90% |          168.1 / 267.4 ms |
| Whisper Small / CPU                     |             0 |                 6.45% |          647.7 / 999.4 ms |
| Whisper Small Automatic / Metal MLX     |             0 |                 5.85% |          191.0 / 282.1 ms |
| Whisper Turbo / CPU                     |             0 |                 5.10% |        1154.6 / 1615.4 ms |
| Nemotron / Metal                        |             0 |                12.29% |           92.7 / 154.2 ms |

All English rows use the same 667 reference words. Cohere's `duration_token_limit`
failure discarded an incomplete result; its 31 scored outputs have 42 edits / 635
words (6.61%). The failure-as-empty diagnostic counts the missing 32-word output
as deletions, giving 74/667. Both views and the execution failure are retained;
the scoring convention does not turn a failed request into a qualified one.

| Hindi configuration / actual provider | Failures / 32 |    WER | Subsequent ASR mean / p95 |
| ------------------------------------- | ------------: | -----: | ------------------------: |
| Qwen-ASR / CPU                        |             0 | 25.17% |        2573.1 / 3653.4 ms |
| Qwen-ASR Automatic / CPU              |             0 | 25.17% |        2553.0 / 3613.6 ms |
| Nemotron / Metal                      |             0 | 14.42% |          107.5 / 189.4 ms |

Hindi has 735 reference words. Automatic Qwen selects the compatible CPU path;
all 32 outputs match explicit CPU exactly. The original requested-lane environment
description said MLX; actual execution and artifact records establish CPU INT8.
The ignored metadata clarification preserves this distinction instead of editing
the raw report. Neither language is pooled with the other.

Latency is manager submission to returned raw ASR, excluding file decoding,
writer, capture, persistence and UI. Means and nearest-rank p95 use requests 2–32,
including failures, with the first request reported separately in the artifacts.
The evaluator's `residentRun` ordinal does not itself prove resident weights.
Fresh-process first calls still had previously exercised OS/shader caches.
Frontend and cleanup were disabled in this comparison.

Qwen MLX had the lowest observed English WER and Parakeet MLX the lowest latency
in this small read-speech screen. Nemotron was stronger on the sampled Hindi.
Exploratory 10,000-resample paired sentence bootstrap intervals, without multiple
comparison correction, give Qwen MLX minus Cohere −18.19 to −1.37 WER percentage
points and Nemotron minus Qwen CPU Hindi −14.45 to −7.24 points. These post-hoc
descriptive comparisons do not establish natural speaker independence, noise,
code-switching, singing, all language modes or a default-model promotion.
Earlier noise hallucinations and long/French Whisper losses remain failed gates.

Reproducibility records remain ignored under
`.cache/benchmarks/results/performance-qualification-2026-09-25/`: reports,
`runner.mjs`, `summarize.mjs`, per-job observations, source/binary identities and
metadata clarifications. The frozen inputs are under
`.cache/benchmarks/performance-qualification-2026-09-25-r1/`.

- English manifest SHA-256: `bfec5f6ce5e38e3f64bcb9aabbf9c64736b7c9420f5482cc527295a61a21cc2f`.
- Hindi manifest SHA-256: `340a80dcc23716c1a48212be4a7c07c7b5f5e69170b8396f7a00593ff26e9306`.
- Frozen exclusion SHA-256: `8c392d7162bf71deaa11e71de712bbdb811fb9c4b2782bf7859ed4eca25a6441`.
- Evaluator SHA-256: `a65cf85dbe0bad0219a66774d974eb5510d5efb17fda0594f2a6b40b42118538`.
- Before/after source and binary identities match; the identity record SHA-256 is
  `d494e75cd4b0c6351863d7efe0671d3d0371989e4298ce34d6dbbdc6bc11daf1`.

The installed native checks also passed 100 requests each for MLX Parakeet,
Qwen and Whisper Small, plus LFM CPU and Metal. Maximum sampled RSS growth after
request ten was respectively 1840, 416, 400, 0 and 816 KiB, below the predeclared
128 MiB limit. Existing cancellation/reset/unload and long-audio assertions pass;
these were 100 completed repeats plus separate cancellation checks, not 100 mixed
cancellation cycles. The frontend test passes 100 reset cycles per off/DPDFNet4/
DPDFNet8 configuration, including packetization tails, rate changes and cancellation.
The separate Nemotron stream reset/finalization fixture also passes.

Power and Foundation thermal state were sampled every ten seconds. The host was
on AC with Low Power Mode disabled and nominal state throughout these runs;
ordinary desktop/Codex activity remained present. Apple's
[thermal-state API](https://developer.apple.com/documentation/foundation/processinfo/thermalstate-swift.property)
reports a coarse OS category. This is not a temperature, power-consumption or
GPU-memory measurement, nor proof of no throttling or performance on another device.

#### Continuous-stream and completed-file soak

Nemotron Metal with Silero and DPDFNet4 completed the same **583.28-second**
synthetic concatenation in both modes. It repeats the 32 English recordings
twice, without claiming natural long-form or independent repeated accuracy.
All 9,332,480 input samples reached the frontend output, finalization succeeded,
and no intervals were omitted. Both transcripts match exactly: 193 edits / 1334
reference words, or 14.47% WER. There was no writer in this pair.

The 20 ms wall-clock-paced stream emitted its first nonempty partial at
2371.83 ms from stream start, including initial setup, and finished **58.57 ms**
after the explicit `finish` call. It produced 29,067 update events; cumulative
successive-partial word edit distance was 1659, which includes text growth and
is not a count of corrections to already committed words. Manager elapsed
time including pacing was 587.87 seconds. The completed-file manager call took
**81.39 seconds** (RTF 0.140), including 61.80 seconds of native frontend work.
The frontend therefore remains a material cost even with accelerated ASR;
these results do not measure microphone stop, acoustic endpoint latency or
word-aligned emission delay.

Whole-process maximum RSS was 1.24 GiB streaming / 1.37 GiB completed-file.
All 60 streaming and ten completed-file thermal samples were nominal, with
Low Power Mode off and AC attached. No push/finish error was observed; the
evaluator does not independently count native queue drops or inject queue
overflow. No watts, GPU-memory total or hours-long thermal equilibrium was
measured. This single paired soak qualifies the observed finite replay, not
unlimited recording or another device.

The synthetic manifest SHA-256 is
`9242ced4bb94dee9922277b0da85a25116641baad95b4f75d6935d4208d1e3f0`;
`nemo-sustained-realtime` and `nemo-sustained-offline` reports retain the exact
frontend metrics, source/artifact identities, logs and power/thermal samples.

Final qualification-tool validation passes 131 Rust library tests (66 desktop,
65 inference; 33 opt-in tests listed separately), 127 Node tests, eight Python
harness tests, strict targeted Clippy, formatting, ESLint and diff checks.
Installed-model runs are reported separately above. The native-panel diagnostic
[retains its failed rendering gate](personalization/evaluation.md), and the
recurring-learning diagnostic retains its quality and latency failures.
These results apply to the frozen `8a2c27b` baseline plus the recorded test-only
changes; later speaker-timeline integration in the shared checkout was not part
of this run. The qualification tooling was subsequently integrated with local
main at `0f1d762`, preserving the newer speaker-timeline implementation. Its
combined core/models/inference/desktop regression passes 203 tests, with 36
opt-in tests excluded; the localhost download fixture passes with socket access
after the restricted sandbox denied its listener. Strict Clippy and
`make check-fast` pass. These integration checks do not relabel the earlier
timings as measurements of the newer complete application pipeline.
