# Whisper qualification and native integration

Research checked 18 September 2026. [Issue #45](https://github.com/rahulsharmavishwakarma/vesper/issues/45) owns the investigation and approval status. One shared Whisper Small/Turbo adapter now runs through the application inference manager and verified installer under [ADR-0018](adrs/0018-shared-whisper-adapter.md). It remains unavailable in product selection because noise qualification fails. The development `whisper_probe` is a separate earlier path; its execution does not qualify an application preset. See the [adapter regression](validation/whisper-adapter-2026-09-18.md).

The [local 213-case screen](validation/whisper-2026-09-18.md) found better natural-English WER for small/Turbo than Cohere on this sample, but failed Hindi/noise gates and could not establish an uncontended latency comparison. No preset is promoted by those results.

The [25 September application holdout](inference-performance.md) adds 32 new
English read-speech clips per configuration: Small CPU, Small MLX and Turbo CPU
all complete, with WER 6.45%, 5.85% and 5.10% respectively. Subsequent ASR mean
latencies are 647.7, 191.0 and 1154.6 ms on the named M5 Pro; runtime/precision
and ownership differences are part of those configurations. Small MLX also
passes 100 repeated requests with bounded sampled RSS and the existing
cancellation/reset/long-coverage checks. This improves completed-speech and
resource evidence; it does not clear the retained non-speech hallucination,
French repetition, Hindi or live-streaming gates. Product selection remains gated.

## Checkpoints and published evidence

Whisper remains a useful established baseline, explicitly requested despite the earlier preference for checkpoints released within the last year. Turbo was released in October 2024. A recent runtime or model-card edit does not make its weights new. The multilingual candidates fit the requested ceiling: tiny 39M, base 74M, small 244M, turbo 809M and large-v3 approximately 1.55B. `.en` checkpoints and Distil-Whisper need separate English-only capability metadata. [OpenAI model table](https://github.com/openai/whisper/blob/86098128c0b4f24f0e2aa2994de830614b474227/README.md)

**Paper result:** [Whisper](https://arxiv.org/abs/2212.04356) studies robustness from large-scale weak supervision across datasets and languages. It does not establish a universal accuracy winner or current CPU latency. [Distil-Whisper](https://arxiv.org/abs/2311.00430) studies trained smaller checkpoints and speculative decoding; its reported acceleration uses a particular GPU/PyTorch evaluation. Distillation is not a configuration switch on downloaded Whisper weights.

**Official model capability:** Turbo retains the large-v3 encoder and reduces the decoder from 32 layers to 4, with additional multilingual transcription training. Translation was excluded. OpenAI's relative speed table uses English on an A100 and cannot predict CPU, Metal, quantized execution, or this application's two-stage latency. [Turbo release](https://github.com/openai/whisper/discussions/2363), [model table](https://github.com/openai/whisper/blob/86098128c0b4f24f0e2aa2994de830614b474227/README.md)

Distil-large-v3.5 is a separate 756M English candidate. Its card reports OOD short-form WER 7.08% versus Turbo 7.30%, but sequential long-form WER 11.39% versus 10.25%; neither comparison proves it wins CPU dictation. The card's speed table lacks enough hardware detail for a local comparison. Do not attribute those v3.5 measurements to the original Distil paper's v2 experiments. [Pinned v3.5 card](https://huggingface.co/distil-whisper/distil-large-v3.5/blob/728a7691f3ff1d3d971528d3203a6e9559165d41/README.md)

## Runtime benchmarks are separate evidence

The [faster-whisper maintainer benchmark](https://github.com/SYSTRAN/faster-whisper#benchmark) uses approximately 13 minutes of audio, Whisper small, beam 5, eight threads and an Intel i7-12700K. Its CPU results are:

| Implementation in that experiment | Precision | Transcription time | Reported RAM |
| --- | --- | --- | --- |
| OpenAI Whisper | FP32 | 6m58s | 2,335MB |
| whisper.cpp | FP32 | 2m05s | 1,049MB |
| whisper.cpp with OpenVINO | FP32 | 1m45s | 1,642MB |
| faster-whisper | INT8 | 1m42s | 1,477MB |
| faster-whisper, batch 8 | INT8 | 51s | 3,608MB |

Those are old comparison versions (OpenAI 20240930, whisper.cpp 1.7.2, faster-whisper 1.1.0). The batched row measures a different throughput/memory tradeoff. These results justify native runtime experiments; they do not rank current implementations on this laptop or measure stop-to-final latency.

Sherpa publishes a historical Raspberry Pi 4 tiny.en INT8 example with RTF 0.386 using four threads on 16.715 seconds of audio. It is useful feasibility evidence, with insufficient revision/thermal coverage for modern edge qualification. [Official tiny.en results](https://k2-fsa.github.io/sherpa/onnx/pretrained_models/whisper/tiny.en.html)

| Runtime | Official capability | Qualification needed here |
| --- | --- | --- |
| Existing sherpa-onnx 1.13.8 | CPU INT8 ONNX exports; resident recognizer and fresh stream objects | Completion/error reporting, token caps, long-audio coverage, language and reset tests |
| whisper.cpp | ARM NEON/Accelerate CPU, integer quantization, optional Metal and CoreML encoder | New native integration; exact GGML/llama.cpp coexistence; per-backend quality, RAM and latency |
| faster-whisper/CTranslate2 | CPU INT8 and NVIDIA CUDA; greedy/beam and batching APIs | Another runtime; advertised hardware support does not establish Metal/ANE execution |

Sources: [Sherpa exports](https://k2-fsa.github.io/sherpa/onnx/pretrained_models/whisper/export-onnx.html), [whisper.cpp](https://github.com/ggml-org/whisper.cpp), [CTranslate2 hardware](https://opennmt.net/CTranslate2/hardware_support.html), [precision matrix](https://opennmt.net/CTranslate2/quantization.html). INT8 weights do not mean every activation, residual operation or KV tensor executes in INT8.

Reviewed runtime revisions: Sherpa `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`; whisper.cpp `da54572229bcf64ba367d96c7ef15770376c4280`; faster-whisper `ed9a06cd89a93e47838f564998a6c09b655d7f43`; CTranslate2 documentation 4.8.2. Only the pinned Sherpa build is executed by this screen. Runtime inspection is not an installed backend claim.

## Constraints in upstream Sherpa and the earlier development probe

The [recognizer](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/offline-recognizer-whisper-impl.h) accepts greedy search, loops over multiple streams serially, and discards input beyond approximately 29.5 seconds. It catches ONNX Runtime exceptions and returns an empty result. The screen therefore rejects recordings above 29 seconds before calling native code, retains stderr and empty outcomes, and labels completion status unknown. Long-input rejection is a compatibility failure, not a successful long-form transcription test.

The [decoder](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/offline-whisper-greedy-search-decoder.cc) caps generation at the lesser of roughly six tokens per input second and half the text context. The public API cannot reliably distinguish EOS from that cap. Token count can help investigate suspicious tails but is not proof of complete decoding. The local Hindi screen reached the inspected cap on short natural inputs and had very high WER. That supports investigating the restriction; the accuracy recovered by a corrected decoder remains unverified.

Tail padding defaults to 1000 feature frames (about 10 seconds). The official configuration suggests 300 for multilingual exports. Reducing padding is a public configuration experiment that can affect both latency and termination; it must pass the same tail/noise/meaning tests before selection. The primary screen holds 1000 constant and does not claim an optimal thread count or padding policy.

The [model implementation](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/offline-whisper-model.cc) exposes fresh zeroed FLOAT32 self-KV state for a decode. Reusing model sessions need not reuse recording state. This supports a resident experiment, not automatic approval of a production lifetime change.

## Streaming and implementation categories

1. **Frozen-weight public-API work:** resident weights, fresh recording state, explicit language/task options, verified artifacts, bounded queues, completion accounting, and measured padding/thread choices. The bounded ownership/segmentation integration is implemented under ADR-0018; other ownership changes still require approval.
2. **Runtime-internal work:** reliable native EOS/error/token-limit reporting; changing Sherpa's generation cap; different decoding loops, quantized formats, fused operators or accelerator kernels. Architecture names alone do not make these available in the selected build.
3. **Training/checkpoint work:** Turbo's pruned/fine-tuned decoder, Distil checkpoints, causal encoder changes and streaming-specific training. None is a switch that converts the existing checkpoint into a native streaming transducer.

[Whisper-Streaming](https://aclanthology.org/2023.ijcnlp-demo.3/) applies LocalAgreement to repeated rolling-window decoding. This is a possible future application algorithm, with repeated encoder/decoder cost and its own latency/quality policy. Its paper result is not a CPU edge promise. A completed-file or rolling microphone example is not evidence of native causal streaming. See the [paper-to-engineering map](research-streaming-inference-papers.md).

## Approved integration and release gates

The user explicitly approved shared resident reuse, native reporting and the token-cap correction in issue #45. These are implemented under ADR-0018; product selection remains gated on validation. The development-only `whisper_probe` executed real Whisper inference for the [213-case screen](validation/whisper-2026-09-18.md), but that executable is separate from the application's model selection and inference adapter. Completed benchmark results do not imply that Whisper is available in the product.

The problem is a missing broad-language Whisper adapter plus native completion hazards. The bounded proposal is to add a selectable **completed-audio Whisper family** to the existing registry/manager, beginning with pinned multilingual small and Turbo artifacts. Keep Cohere plus the writing model as the default and retain the existing separate writing/fidelity stage.

Affected components are core capability/selection metadata, model artifact installation and verification, one architecture-specific inference adapter, manager ownership/invalidation, Models UI presets, and benchmark qualification. The adapter must advertise its tested runtime/backend/language/mode combination. Native no-speech/error/completion reporting and a validated noise-rejection policy are release gates; any new VAD model or broader frontend subsystem needs its own concrete proposal. Hindi remains disabled until the corrected path passes qualification. It must own selected resident weights and fresh per-recording decode state, process every long-audio segment, and discard cancelled, failed or incomplete output. It must not reinterpret a transducer importer as Whisper support.

Sherpa reuses an existing dependency but needs native completion and generation-cap fixes before this contract is adequate. whisper.cpp offers a broader accelerator/decoder path but adds a native dependency and requires a combined build with the exact llama.cpp revision. Its `talk-llama` example shows coexistence is possible for a selected pair; it does not verify ours. The first integration proposal uses the existing Sherpa CPU runtime with those completion fixes. A whisper.cpp integration would be a separate proposal after its own comparable native measurements and combined-build check. [Official combined example](https://github.com/ggml-org/whisper.cpp/blob/da54572229bcf64ba367d96c7ef15770376c4280/examples/talk-llama/CMakeLists.txt)

Validation before a preset is enabled: same pinned natural English/French/Hindi manifests and automatic-language checks; code-switching and singing hard filters; quiet/noise/negation/numbers/tail coverage; natural long recordings and synthetic boundary tests; 100-cycle reuse, A/B/A, language/model switches, cancellation, unload and invalidation; raw versus cleaned quality; cold/resident latency and RAM. Measure CPU first, then each accelerator/edge device separately. Live Whisper partials, training, automatic model promotion and default changes are outside this proposal.

### Architecture boundary and reuse

Use **one Whisper architecture/runtime adapter for both Small and Turbo**. Checkpoint variants select pinned artifact manifests and metadata within that adapter; they do not each introduce an implementation. English-only or distilled checkpoints can use this family only after their artifacts and language/mode combinations qualify. A checkpoint name, parameter count, or shared Transformer ancestry is insufficient evidence of runtime compatibility.

Keep the existing installer, integrity verification, worker queue, selected-model ownership, recording isolation, segmentation, cancellation and writing stage shared. Cohere and Whisper can share the resident Sherpa session wrapper while retaining their distinct native configuration and result policies. Avoid introducing a general plugin framework, a new dependency, or a shared implementation of incompatible decoding algorithms for this work.

The narrow Whisper-specific responsibilities are encoder/decoder/token artifact selection; Whisper language/task/padding configuration; validation of actual EOS and untruncated input; and the combined native no-speech/confidence policy. Native kernels and attention/KV operations remain owned by the pinned Sherpa/ONNX Runtime implementation. There is no proposed per-checkpoint kernel implementation. Kyutai remains removed.

### Native reporting and confidence contract

**Official implementation evidence:** the pinned Sherpa exporter returns FLOAT32 logits for all prefill token positions and stores the no-speech token in model metadata. The no-speech probability can be computed from the SOT-position row before sampling filters, without changing model weights. Average log probability includes the observed EOS score and divides by generated text-token count plus one. Sherpa's greedy sampler lacks OpenAI's full suppression/fallback behavior, so these scores must not be advertised as numerically equivalent to OpenAI decoding. [Sherpa exporter](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/scripts/whisper/export-onnx.py), [OpenAI decoder](https://github.com/openai/whisper/blob/86098128c0b4f24f0e2aa2994de830614b474227/whisper/decoding.py)

Preserve the strict shared completion-report v1 schema. Put Whisper confidence and detected language in separate stream options, since the current Rust binding omits the native result's language field. Reset all reporting state before every decode, reject malformed/nonfinite logits, retain actual EOS versus token/context exhaustion, and reject oversized native input instead of returning a prefix. Application segmentation must cover every input sample with fresh decode state.

**Paper/official policy:** Whisper §4.5 and the official transcription implementation combine no-speech probability above 0.6 with average log probability at or below −1 for silence skipping. High-confidence text overrides the no-speech probability; low confidence alone does not erase speech. Apply any candidate silence policy only after successful, nontruncated completion. This is a hypothesis for the INT8 Sherpa artifacts until the shared noise and quiet-speech manifests pass. [Whisper paper](https://arxiv.org/abs/2212.04356), [OpenAI transcription policy](https://github.com/openai/whisper/blob/86098128c0b4f24f0e2aa2994de830614b474227/whisper/transcribe.py)

The proposed decoder behavior change removes Sherpa's six-tokens-per-second assumption while retaining the model's half-context output bound and fixed KV bounds. OpenAI defaults to half the text context; this provides a research-grounded experiment, not evidence of recovered Hindi quality. Longer decoding can increase latency on ambiguous audio. The specific native change is approved and implemented. Adapter measurements are reported separately from the earlier development probe; product presets remain disabled because the noise gate fails. The metadata inspection confirms the correct SOT/no-speech IDs for both exports, so no token-remapping fix is justified. Keep thresholds fixed for this regression; low confidence alone must not silently delete quiet or accented speech.
