# Inference performance across devices

Whisper remains a candidate: the [18 September native screen](validation/whisper-2026-09-18.md) records quality gains on its English sample, Hindi/noise failures, and provisional CPU timings affected by other workloads. It does not establish a latency improvement or a production adapter.

Updated 17 September 2026. The primary-paper basis and standardized streaming metrics are in [Research basis for streaming inference](research-streaming-inference-papers.md). Packaged accelerator selection and selected resident adapters are implemented; the default Cohere speech adapter now retains its recognizer in the existing worker with fresh segment streams (ADR-0017). Performance claims require measurements on each named device. Recommendations below are engineering proposals unless explicitly identified as observations.

The per-job Nemotron CLI and per-request cleanup-model loading have been replaced. Qwen3-ASR and Parakeet still own their ONNX models per completed job; this is distinct from the resident Qwen cleanup model. The next priorities are to instrument the complete live path, run the lifecycle/corpus gates, tune CPU budgets, and benchmark each packaged accelerator. Speech and cleanup ultimately need independent device controls. A GPU being present, a backend compiling, and the exact model running efficiently are three separate checks.

## What this application does today

The user-selected default is Cohere Transcribe INT8 through sherpa-onnx 1.13.8, with completed-recording CPU inference and Qwen writing cleanup. This preference is not a fastest-model claim. Selectable Nemotron 3.5 Q8 uses the official NeMo-Speech.cpp 0.1.0 C ABI and supports live partials. Parakeet v3 INT8 through `sherpa-onnx = 1.13.8` remains an offline CPU fallback. Cleanup uses Qwen3.5-0.8B Q4_0/Q8_0 through `llama-cpp-2 = 0.1.156`. Hugging Face profile selection changes compatible artifacts; it does not make an arbitrary architecture compatible with these runtimes.

| Component | Current behavior | Consequence |
| --- | --- | --- |
| Speech runtime | Default Cohere uses pinned CPU sherpa-onnx; selectable Nemotron uses CPU/Metal/CUDA/Vulkan NeMo-Speech.cpp with 20 ms capture packets | Cohere processes completed audio; NeMo owns cache-aware RNNT state and native partials |
| Cleanup runtime | Metal on Apple Silicon; CUDA/Vulkan/ROCm when compiled with the matching feature; otherwise CPU | All layers plus K/Q/V and operators are offloaded when usable; `Auto` retries CPU when accelerator initialization fails |
| CPU budget | Qwen available parallelism clamped to 1–8, fallback 2; NeMo runtime controls its own threading | Portable baseline, not a measured optimum |
| Object lifetime | One bounded worker retains at most one selected Cohere/Nemotron/LFM ASR and one Qwen writer; Qwen-ASR and Parakeet construct recognizers per job | Residency is adapter-specific; each recording/segment and Qwen pass starts isolated request state |
| Cleanup limits | Up to 4096 context tokens, bounded output up to 2048 tokens; logical batch 512, physical microbatch 128 | Memory and latency depend on actual prompt/output lengths |
| Integrity | Full role-specific verification on resident load; per-job speech adapters verify every request | Cohere verifies on native load and checks observed identity on reuse; Apple aarch64/macOS enables sha2's supported hardware dispatch while retaining all size/hash checks |

These observations come from [Nemotron inference](../crates/transcribe-inference/src/nemotron.rs), [cleanup inference](../crates/transcribe-inference/src/qwen.rs), [model/runtime selection](../crates/transcribe-models/src/lib.rs), and [desktop orchestration](../apps/desktop/src-tauri/src/runtime.rs).

Qwen's complete-prefix state experiment is **off by default**. `TRANSCRIBE_EXPERIMENTAL_PREFIX_CACHE=1` enables it; normal cleanup retains weights and creates fresh contexts. Its hybrid recurrent state and full-attention KV state require the exact runtime's complete-state API. A capability flag is not evidence that prefix reuse executes on default requests.

One build detail matters: `default-features = false` does **not** mean Metal is absent. The pinned `llama-cpp-2` manifest has an Apple ARM target dependency enabling `metal`. Application features forward `cuda`, `rocm`, and `vulkan`; there is no direct `sycl` feature in this release. Selecting a backend without compiling its feature falls back to CPU cleanup and reports CPU provenance. [Pinned Rust binding manifest](https://docs.rs/crate/llama-cpp-2/0.1.156/source/Cargo.toml).

NeMo-Speech.cpp 0.1.0 publishes CPU, CUDA, Vulkan, and Metal release packages. transcribe pins their bytes and hashes rather than treating a requested device string as evidence of availability. NVIDIA does not publish an MLX or ROCm Nemotron package: Apple uses Metal; the AMD profile uses Vulkan for speech and can use ROCm/HIP for Qwen cleanup.

## Existing measurements, with their limits

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

| Pack and configured language | Nemotron median WER / RTF | Qwen3-ASR median WER / RTF | Balanced result |
| --- | ---: | ---: | --- |
| Hindi `hi-IN`, explicit `hi` | 9.35% / 0.045 | 12.13% / 0.411 | Nemotron |
| Hindi `hi-IN`, Auto | 9.35% / 0.047 | 11.44% / 0.420 | Nemotron |
| US English `en-US`, explicit `en` | 7.18% / 0.046 | 5.42% / 0.361 | Nemotron; Qwen wins accuracy-only |
| US English `en-US`, Auto | 7.18% / 0.045 | 7.50% / 0.358 | Nemotron |
| Hinglish `hi+en`, Auto | 47.73% / 0.047 | 51.89% / 0.568 | No qualified model |

The example balanced policy requires median WER at most 20% for a monolingual pack, 30% for Hinglish, median RTF at most 1.0, and failure rate at most 2%. A comparable locally measured candidate that fails a requested ceiling is excluded; it is not downgraded to unmeasured official support. Nemotron also does not claim code-switching capability, while Qwen's measured Hinglish WER exceeds the ceiling. The safe answer for the current Hinglish pack is therefore “no qualified model,” not a forced recommendation. Exact mixed-script WER penalizes Latin reference words transliterated into Devanagari, so the raw hypotheses still need qualitative analysis.

Four-sample wall-clock streaming subsets exercised continuous packet delivery and final drain. The wall-clock RTF is near 1.0 by construction because replay sleeps to audio time; stop-to-final and first-partial latency are the interactive measurements.

| Pack | Median WER | Median wall RTF | p95 stop-to-final | Warm first nonempty partial |
| --- | ---: | ---: | ---: | ---: |
| Hindi `hi-IN`, explicit `hi` | 10.60% | 1.011 | 91.9 ms | 1.32–1.96 s |
| US English `en-US`, explicit `en` | 9.01% | 1.010 | 89.0 ms | 1.32–1.64 s |
| Hinglish `hi+en`, Auto | 56.06% | 1.014 | 90.9 ms | 0.68–1.65 s |

The first streaming sample in each process was cold and reached its first nonempty partial in 3.24–4.54 seconds. The sample count is deliberately small and does not support a production p95 claim. Indian English was later screened on 48 Svarah recordings in a separate personalization experiment: raw WER was 18.50% for Nemotron and 9.83% for Cohere, with 48/48 executions successful for each. Because that report belongs to a personalization profile envelope, it is retained as local evidence but excluded from automatic base-model recommendation aggregation. It is not an all-model `en-IN` qualification.

Qwen3-ASR's current adapter verifies and creates its ONNX model per job, which explains much of its 0.36–0.57 RTF on these short clips. Nemotron retains weights and creates fresh utterance streams. A prior instrumented eight-second Qwen run peaked at 1,613,037,568 bytes RSS. A 413-second MUCS recording also completed as one uninterrupted accelerated resident Nemotron stream with 20,555 partial updates, 0.27 RTF, and about 87 ms stop-to-final; that is a Hinglish robustness observation, not a Hindi accuracy result. The long run peaked at 1,232,601,088 bytes RSS and used 222.65 user CPU seconds over 112.62 seconds wall time. These are whole-process peaks, including the runner and decoded audio.

The runs used an Apple Silicon headless sandbox with the explicit NeMo CPU runtime. The exact chip/memory, power mode, sustained thermal state, and per-run peak RSS were unavailable, so this evidence must not be presented as a complete hardware qualification or Metal result. The [reproduction procedure and metric definitions](../benchmarks/asr/README.md) keep raw machine reports gitignored.

The recorded development-machine smoke used an **Apple M5 Pro with 64 GiB RAM**, CPU inference, Parakeet INT8 and Qwen Q4_0 on approximately 3.845 seconds of public English speech:

| Operation | Observed elapsed time |
| --- | ---: |
| Parakeet loading plus recognition | 0.76 s |
| Qwen loading plus cleanup | 0.70 s |
| Both native stages | 1.46 s |
| Desktop integration test, including hashes, archive, SQLite and export assertions | 3.63 s |

These are single observations with potentially warm filesystem caches, not p50/p95 values, cold-start measurements, or release-build comparisons. Peak memory and GPU execution were not measured. The derived ASR real-time factor for this one load-inclusive sample is approximately `0.76 / 3.845 = 0.20`; it says nothing about sustained performance on other devices. The integration test includes export work that ordinary dictation need not perform. See the [recorded validation and reproduction commands](validation.md); the native debug result remains in `.cache/benchmarks/results/native-pipeline/debug-summary.json`.

The six synthetic cleanup cases also exposed repetition and meaning changes. An optimization cannot be accepted solely because it produces text sooner. Preserve the existing raw-output fallback and review behavior during performance work. [Cleanup observations](validation.md#cleanup-quality-findings).

## Device and backend strategy

The paths below are selectable and packaged where noted, but still require successful execution and comparison against CPU on the exact target. Existing observations are single-fixture development checks, not cross-device qualification.

| Device family | Speech candidate | Cleanup candidate | Recommended order |
| --- | --- | --- | --- |
| Apple Silicon macOS | Packaged NeMo Metal | Built-in llama.cpp Metal | Current `Auto`; benchmark against explicit CPU |
| Linux/Windows with NVIDIA GPU | Packaged NeMo CUDA, selected automatically after a successful `nvidia-smi` probe | llama.cpp CUDA build | Build with `--features cuda`; qualify driver/toolkit/device |
| AMD GPU | Packaged NeMo Vulkan | llama.cpp Vulkan, or ROCm build | Vulkan works across more devices; ROCm profile combines Vulkan ASR with HIP cleanup |
| Intel integrated/discrete GPU | Packaged NeMo Vulkan | llama.cpp Vulkan build | Build with `--features vulkan`; qualify driver and memory behavior |
| Other ARM/x86 systems | Native CPU with suitable instruction support | Native CPU | Smaller compatible models and bounded workload; measure on actual hardware |
| iOS/Android, future | Platform CPU first, then qualified native accelerator paths | Platform CPU/Metal or other supported native backend | Treat as a separate packaging, capture and lifecycle project |

**Apple.** `Auto` selects the packaged NeMo Metal runtime and offloads Qwen through llama.cpp Metal on Apple Silicon. MLX is a separate array/model framework; the selected Nemotron GGUF and official runtime do not expose an MLX backend. Compare against explicit CPU because GPU dispatch can lose on a very short cleanup. [llama.cpp backend build guidance](https://github.com/ggml-org/llama.cpp/blob/master/docs/build.md).

Core ML can target Apple CPU, GPU and Neural Engine. Its ONNX provider supports only eligible graph portions; dynamic shapes can affect performance, and newer MLProgram/compute-unit options differ from the legacy API. Profile coverage and compilation time for the exact Parakeet export. A CPU INT8 bundle is not proof of ANE compatibility or acceleration. [Core ML execution provider](https://onnxruntime.ai/docs/execution-providers/CoreML-ExecutionProvider.html), [ONNX provider graph assignment](https://onnxruntime.ai/docs/execution-providers/).

**NVIDIA.** Select CUDA and compile transcribe with `--features cuda`. The installer chooses NVIDIA's matching NeMo release package, while the feature builds llama.cpp CUDA for Qwen. Device transfer, workspace, driver compatibility, and allocations remain part of the benchmark.

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

| Metric | Definition and reporting |
| --- | --- |
| Cold process start | New process with no resident model; report filesystem and compiled-backend cache state separately |
| Cold artifact/compile start | First use with backend compilation cache absent; never label an ordinary restart as a cold disk-cache test |
| Warm latency | Resident models, fresh utterance state; separate any warmup samples |
| ASR RTF | Recognition elapsed seconds / audio seconds; report load-exclusive and load-inclusive forms explicitly |
| Cleanup TTFT | Cleanup request to first generated token; report whether loading and prompt evaluation are included |
| Cleanup generation rate | Generated tokens / decode time, with prompt/output token counts and tokenizer/model ID |
| Final text latency | Recording stop or imported-file submission to accepted final text or explicit raw fallback |
| Distribution | Per-device/per-workload p50 and p95, sample count and failures; do not pool different clip lengths into a misleading single number |
| Resource use | Peak RSS, GPU/shared allocations, idle residency, CPU load, power mode and sustained thermal state |
| ASR quality | WER `(substitutions + deletions + insertions) / reference words`, with declared normalization; CER where appropriate; punctuation/case assessed separately |
| Meaning preservation | Human-reviewed changes to entities, numbers, negation and intent; guard rejection, truncation, repetition and raw-fallback rates |

As an initial screening protocol, use at least 30 timed warm requests per configuration and several independent process starts. For a publishable p95 claim, expand to a representative corpus with substantially more independent requests and report uncertainty; repeated copies of one fixture do not establish a population latency distribution. Alternate configuration order to reduce cache/thermal bias. Report failures in the denominator rather than timing only successful runs.

Use monotonic Rust timers for application spans. ONNX Runtime profiling can expose operator timing and execution providers. Upstream `llama-bench` separates prompt processing and generation and supports machine-readable output, making it useful for backend screening; it does not include our actual prompts, guards, audio path or UI. Use the llama.cpp revision corresponding to the bundled binding when comparing results. [ONNX profiling](https://onnxruntime.ai/docs/performance/tune-performance/profiling-tools.html), [llama-bench](https://github.com/ggml-org/llama.cpp/blob/master/tools/llama-bench/README.md).

Store one JSONL record per run with hardware/OS, driver versions, application revision, runtime build/features, model revision and hashes, requested/actual device, thread/offload settings, input duration, token counts, stage timings, memory observations, outputs and errors. Publish aggregate results alongside this manifest. Preserve raw ASR independently from cleaned output, and keep machine predictions separate from human training labels.

Promote a performance profile only after it passes existing tests, the held-out quality review, repeated latency/memory measurements, cancellation/recovery tests, and package validation on its named devices. There are no measured GPU speedups or universal hardware defaults to report yet.
