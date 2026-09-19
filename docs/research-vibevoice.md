# VibeVoice variants, streaming contract, and CPU qualification

The complete Microsoft VibeVoice inventory was checked on 18 September 2026.
**No complete ASR model in this inventory satisfies a strict limit of fewer than
two billion total parameters.** The specifically requested Streaming-1.5B remains
an out-of-budget reference experiment, not a production model recommendation.
The existing [VibeVoice issue](https://github.com/rahulsharmavishwakarma/transcribe/issues/15)
tracks research and the separate, unapproved BitNet integration proposal.

The [local CPU screen](validation/vibevoice-2026-09-18.md) records actual execution, quality, memory and timing limits.
The subsequent [quantized BitNet CPU benchmark](validation/vibevoice-bitnet-2026-09-18.md)
records the user's selected quantized candidate, a fresh Cohere comparison,
thread/normalization tests and a separate decoder-position correction experiment.

## Inventory and size evidence

The [pinned inventory](../benchmarks/asr/vibevoice-inventory.json) records all eight
official Microsoft repositories, model revisions, file sizes and publisher
checksums. Counts below describe stored tensor values, not file bytes or peak RAM.
Streaming-1.5B and BitNet were independently counted from tensor shapes in the
safetensors headers; the other counts come from the publisher's HF metadata.

| Variant | Stored parameters | Function | Under 2B total? |
| --- | ---: | --- | --- |
| ASR-Streaming-1.5B | 2,814,116,321 | Streaming ASR with speaker labels | No |
| ASR-BitNet | 2,814,116,321 | Specialized quantized CPU ASR | No |
| Realtime-0.5B | 1,017,626,722 | Streaming text-to-speech | Yes; cannot replace ASR |
| VibeVoice-1.5B | 2,704,021,985 | Long-form text-to-speech | No |
| AcousticTokenizer | 687,392,001 | Audio codec component | Yes; not a complete recognizer |
| ASR-Streaming-7B | 8,674,021,857 | Streaming ASR | No |
| ASR | 8,674,021,857 | Completed-audio ASR | No |
| ASR-HF | 8,330,325,888 | Transformers-format completed-audio ASR | No |

The names describe backbone scale. Streaming-1.5B's three BF16 shard files total
5,628,388,290 bytes. Its stored synthesis decoder contributes 343,695,969 values
and is not called by the ASR encoder path. Even subtracting that decoder and
233,373,696 potentially duplicated tied output-embedding values leaves
2,237,046,656: still above the cutoff. This subtraction is tensor arithmetic,
not an observed memory saving or validation of a modified model.
[Pinned checkpoint](https://huggingface.co/microsoft/VibeVoice-ASR-Streaming-1.5B/tree/4262d23d8a539a6530cf64fbd0b1751ef9a30853)

BitNet has contradictory publisher metadata: the index's `total_parameters` is
322,592,829, while its tensor shapes contain 2,814,116,321 F32 values. Its index
`total_size` of 11,256,465,284 bytes agrees with the latter. Therefore the displayed
0.3B summary is not evidence that this is a sub-2B recognizer. The deployable
specialized GGUF pair totals 1,695,957,664 bytes: 1.696 decimal GB or 1.579 GiB.
Quantization reduces storage, not the number of model parameters.
[Pinned BitNet index](https://huggingface.co/microsoft/VibeVoice-ASR-BitNet/blob/66e78021ab8f5f06133d1ab421ba4d348bda97c9/model.safetensors.index.json)

## Exact streaming capabilities

The Streaming-1.5B card specifies streaming speaker-attributed transcription and
optional names/domain hotwords. Its ten listed languages are Chinese, English,
French, German, Italian, Japanese, Korean, Portuguese, Russian and Spanish.
Hindi is absent. Language coverage from the separate long-form ASR checkpoint
must not be transferred here. Code-switching, singing and hotword accuracy still
need checkpoint-specific qualification.
[Official card](https://huggingface.co/microsoft/VibeVoice-ASR-Streaming-1.5B)

The released checkpoint uses 24 kHz mono audio, 3,200 samples per latent frame,
22-frame advances and four-frame lookahead. This means advancing 70,400 samples
(2.933333 seconds) while reading an 83,200-sample window (3.466667 seconds).
End-of-input pads the last window and drains all remaining advances. Changing
these numbers does not reproduce a separately trained smaller-chunk model.
[Official streaming usage](https://github.com/microsoft/VibeVoice/blob/1541f590c7099820f10ea012f48d2399282df69f/docs/vibevoice-asr-streaming.md)

The September 2026 paper's **2.00-second latency is an expected steady-state
algorithmic delay**, excluding compute. First output requires roughly 3.5 seconds
of input plus processing. Released sessions target at most eight minutes.
Speaker labels are session-local ordinals; prolonged overlapping speech is a
known weakness. The 1.5B/22-frame five-set mean is 29.35 recognition WER/CER and
44.31 speaker-attributed cpWER/cpCER. These mix different languages and metrics;
they are not an English WER estimate. Headline comparisons and the reported
RTF at or below 0.104 belong to 7B, with the latter measured at 15 frames on an
A100 80GB PCIe GPU using vLLM/BF16/batch one. They do not predict local CPU speed.
[Technical report v1, Tables 5 and 8](https://arxiv.org/html/2609.02812v1)

## Official runtime versus application support

The inspected Microsoft source revision is
`1541f590c7099820f10ea012f48d2399282df69f`; the streaming checkpoint revision is
`4262d23d8a539a6530cf64fbd0b1751ef9a30853`. The direct publisher ASR class supports
a CPU/FP32/SDPA test without FlashAttention, Triton or vLLM. Its dependency range
is Transformers `>=4.51.3,<5.0.0`; the test must record the exact resolved stack.
The generated HF pipeline snippet is not a substitute for verifying the class
and custom tokenizer used by this source revision.
[File demo](https://github.com/microsoft/VibeVoice/blob/1541f590c7099820f10ea012f48d2399282df69f/demo/vibevoice_asr_streaming_inference_from_file.py),
[dependencies](https://github.com/microsoft/VibeVoice/blob/1541f590c7099820f10ea012f48d2399282df69f/pyproject.toml)

The application has no VibeVoice adapter or downloadable executable manifest
entry. An isolated Python probe establishes only its measured reference-runtime
envelope. It does not qualify the Rust application, the writer pipeline, an
accelerator, or a production streaming lifetime.

The maintained isolated probe environment pins Torch 2.13.0, Transformers 5.10.1,
Hub 1.5.0, Diffusers 0.38.0, Safetensors 0.8.0 and Tokenizers 0.22.2.
[The reviewed compatibility patch](../patches/vibevoice/README.md) adapts the
publisher's tokenizer, initialization and tied-weight APIs; it is a local port,
not upstream support for Transformers 5. The checkpoint and its weights remain
unchanged. Each probe run verifies source/patch provenance, all checkpoint
tensors, tied-parameter identity and the non-persistent acoustic sampling buffer
before decoding. Accelerate remains absent. The [dependency compatibility
validation](validation/dependency-security-compat-2026-09-19.md) records the local
port checks and their limits.

Run the tokenizer comparison with the recorded 4.57.6 baseline and replay the
same public plans before accepting changes. This checks the existing manual
streaming API, not TTS, alternate checkpoints, direct tokenizer construction,
or application integration. The dated 18 September results retain their
original environment and are not overwritten by dependency requalification.
See [dependency security status](ci-and-releases.md#dependency-security-status-and-upstream-limits)
for the audit policy and exact scope.

The official file generator pre-encodes every window before yielding text.
Paced tests must instead use `init_streaming_state`, `encode_speech`, and
`streaming_generate_step`. The probe retains the required lookahead, creates
fresh state for each recording, and records each emitted chunk. The first-token
callback can report a speaker label, so it is not a word-emission timestamp.
The step API omits EOS-versus-chunk-end reporting; cache-length deltas establish
token-budget exhaustion, which must be retained as failure. Acoustic encoding
samples Gaussian noise even with greedy text decoding; seeded repeats must not
silently replace it with mean features.
[Pinned ASR implementation](https://github.com/microsoft/VibeVoice/blob/1541f590c7099820f10ea012f48d2399282df69f/vibevoice/modular/modeling_vibevoice_asr.py)

Speaker labels identify turns within the supplied conversation, not an enrolled
person across recordings. Hotwords are initial-prompt context, not learned
weights or preference memory. The public methods do not supply VAD, endpointing,
user enrollment, cancellation callbacks or continuous self-improvement. The
publisher's postprocessor derives timestamps from chunk positions; this does
not establish accurate learned word boundaries.
[Output postprocessor](https://github.com/microsoft/VibeVoice/blob/1541f590c7099820f10ea012f48d2399282df69f/vllm_plugin/asr_streaming.py)

## Community runtime and quantization candidates

MLX Audio now documents direct loading of the same Streaming-1.5B checkpoint and
its state/step API on Apple Silicon. This is a separate community implementation,
not Microsoft's PyTorch path or an application integration. Source revision,
precision, latent sampling, device synchronization and output parity need their
own qualification. Its availability does not change the full-model parameter
budget. [MLX Audio documentation](https://blaizzy.github.io/mlx-audio/models/stt/vibevoice-asr/)

The inspected MLX Audio revision is `aef6ebc20ddc775ee62455f383bba141338b7c1c`.
It drops the unused synthesis decoder and uses acoustic latent means. Streaming
feature/cache dtypes need inspection rather than assuming BF16 from the files;
non-strict loading also requires checking missing/unexpected keys. Its optional
quantization predicate covers language-model layers, leaving audio encoders and
connectors floating-point. The declared Transformers>=5.14/Hub>=1 requirements
need a separate environment from the Microsoft reference runtime. No local MLX
performance or quality qualification is claimed.
[Pinned model implementation](https://github.com/Blaizzy/mlx-audio/blob/aef6ebc20ddc775ee62455f383bba141338b7c1c/mlx_audio/stt/models/vibevoice_asr/vibevoice_asr.py),
[declared dependencies](https://github.com/Blaizzy/mlx-audio/blob/aef6ebc20ddc775ee62455f383bba141338b7c1c/pyproject.toml)

The community ONNX export uses an FP16 encoder, INT8 decoder weights and a shared
FP16 GQA cache. Its author explicitly requires ONNX Runtime's CUDA provider;
these graphs cannot simply run through the application's CPU Sherpa adapter.
It also replaces sampled acoustic features with their mean, so it is a distinct
numerical experiment. The author's RTX3090 speed/memory claims are not local
measurements. Its cache-capacity estimate of roughly 68 minutes does not extend
Microsoft's eight-minute checkpoint qualification, and its card reports weak
speaker separation on one clip. Neither fixed cache allocation nor model-file
quantization proves constant attention cost or preserved diarization accuracy.
[Export author's card](https://huggingface.co/christopherthompson81/vibevoice-asr-streaming-1.5b-onnx/blob/578d45b8fabe56d7182f32e932a8b9c06a785ec4/README.md)

## Optimization research and implementation boundaries

1. **Frozen-weight public-API experiments:** resident verified weights, fresh
   recording state, incremental delivery, bounded queues/finalization, selected
   CPU threads and measured stage timing. Keeping decoder KV state reuses prior
   context. Its full-history attention still grows with recording length; the
   custom step API does not expose a quantized-cache selection switch. Removing
   the unused synthesis decoder is a possible memory experiment requiring exact
   output parity and separate peak-load/resident measurements.
2. **Runtime-internal work:** architecture-specific exports, completion/cancel
   hooks, quantized caches, fusion and accelerator kernels require changes to
   the chosen runtime. BitNet's VibeASR.cpp kernels cannot simply be enabled for
   Streaming-1.5B. A lower download size alone does not establish compatible
   stream-state or speaker attribution behavior.
3. **Training/checkpoint changes:** BitNet's progressive quantization-aware
   training and GELU-to-ReLU conversion, distillation, smaller audio encoders,
   and reduced chunk/lookahead training are model changes. They are not safe
   settings to toggle on downloaded streaming weights.

BitNet is relevant CPU research: heterogeneous encoder/decoder quantization,
specialized SIMD and operator fusion. Its published hardware/corpus envelope
is separate from both the streaming paper and any local test.
[BitNet technical report v1](https://arxiv.org/html/2607.21075v1),
[official CPU runtime](https://github.com/microsoft/VibeASR.cpp)

The local quantized benchmark now verifies that the specialized GGUF pair runs
through the pinned CPU runtime. Its small English screen scored 9.51% WER versus
9.67% for the fresh Cohere baseline, but did not demonstrate a lower resident
request latency. Native memory grows substantially with audio duration; hotword
and requested JSON-speaker output failed their local feature checks. These
measurements do not qualify production streaming or a default change.

Two source details matter for future adapters. The stock CLI can hide text
beginning `[` during display formatting and can exit zero after inference errors;
raw text plus an explicit stop reason are needed for reliable evaluation. Also,
native prefill can process fewer frames than the nominal prompt count, while
both callers advance decoding from that nominal count. The benchmark includes a
separate correction using the actual prefill return position. Its overall
English WER did not improve. These are runtime-internal issues; neither is a
downloaded-weight configuration switch.
[Pinned CLI](https://github.com/microsoft/VibeASR.cpp/blob/94cddff7216df8069cbf1b9095714b8327ffeedc/demo/asr_infer.cpp),
[prefill contract](https://github.com/microsoft/VibeASR.cpp/blob/94cddff7216df8069cbf1b9095714b8327ffeedc/utils/prompt_builder.h)

The research direction is useful for reducing a separate diarization stage.
Its value for this product depends on measured accuracy and end-to-end latency,
including the retained writing model. Neither diarization nor hotword prompting
provides personalized intent learning. Any production adapter, new runtime or
training/promotion design still requires a concrete proposal and explicit
approval under [the project workflow](project-workflow.md).
