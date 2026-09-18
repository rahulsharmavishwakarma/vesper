# VibeVoice-ASR-BitNet CPU benchmark — 18 September 2026

The official quantized BitNet checkpoint runs on the tested CPU and is useful
for short completed recordings. It does not yet qualify as this application's
default, native continuous-audio streaming model, or a low-memory long-recording
backend. The user explicitly selected the quantized candidate after reviewing
the family inventory; its parameter count still exceeds two billion.

See [primary-source family research](../research-vibevoice.md),
[issue #15](https://github.com/rahulsharmavishwakarma/transcribe/issues/15), and the
[machine-readable evidence](vibevoice-bitnet-2026-09-18.json).

The final evidence contains **172 planned instrumented case records: 169 native
executions and three explicit memory-budget blocks**, across 11 configurations.
There was also one stock-runtime parity execution and a fresh 24-case Cohere
comparison. The only native completion failure was the deliberately forced
one-token-budget test. Three JSON-format cases failed their output contract;
successful native EOS does not erase quality or feature failures.

## Native runtime and measurement contract

The verified model revision is `66e78021ab8f5f06133d1ab421ba4d348bda97c9`.
Its two specialized GGUF files total **1,695,957,664 bytes** (1.696 decimal GB):
I8_S acoustic/semantic encoders, I2_S decoder weights, and Q6_K embeddings/head.
Hashes are in the [artifact manifest](../../benchmarks/asr/vibevoice-bitnet-artifacts.json).
This is a separately trained checkpoint, not quantized Streaming-1.5B.

VibeASR.cpp `94cddff7216df8069cbf1b9095714b8327ffeedc` uses its required llama.cpp
fork `a2fdadc20285df2dce90402fca9264a93a8eb32f`. The current source head inspected
by the independent review differs in Windows configuration/documentation, not
the measured macOS path. The local build is Release, Clang, ARM NEON with Apple
Accelerate, static libraries; Metal, GGML_BLAS, OpenMP and optional TL1 are off.
The decoder's default KV precision is FP16. This is CPU execution, not Metal.

Hardware: Apple M5 Pro, 18 logical CPUs, 64 GiB RAM, macOS 27.0 (26A428), AC power,
power mode 0. Main runs use eight native threads, context 16,384, batch 2,048,
greedy decoding and a 1,024-token budget. No warm-up flag, writing model, VAD,
denoiser, learned personalization or private audio was used.

Audio is the same public reference corpus used for the earlier
[Streaming-1.5B screen](vibevoice-2026-09-18.md). FFmpeg converts it to the same
24 kHz mono PCM16 before timing; PCM hashes are retained. The BitNet runtime
then applies its default RMS normalization to −25 dBFS. Streaming-1.5B did not
normalize audio, so this comparison includes checkpoint/runtime/preprocessing
differences and cannot isolate a quantization effect.

Two official executables were exercised. `asr_infer` starts a fresh process
and loads models for each recording. `asr_stream_server --no-token-stream`
retains models and clears decoder KV state per completed file. It has no native
incremental-waveform state: emitting tokens after encoding a complete file is
not the Streaming-1.5B continuous-audio interface.

The [measurement patch](../../scripts/native-tests/vibeasr-benchmark.patch)
records raw detokenized output, actual ending token, EOS/token-limit/decoder-error,
encoder frame counts and stage timing. It does not alter decoding. This is
necessary because the stock CLI can return exit zero after errors and treats
any output beginning `[` as JSON, potentially hiding raw annotations. Its server
also lacks reliable completion status. Stock versus instrumented output matched
on the recorded parity control. The deliberately one-token-budget test was
correctly retained as an incomplete decoding failure.

`requestMs` is native audio loading/normalization, encoding, prompt construction,
prefill and decoding, excluding model loading and final process teardown.
External CLI wall time includes startup, model loading and teardown; artifact
verification and FFmpeg preparation remain outside it. Resident response time
measures file submission to response delimiter, with model construction reported
separately. First-generated-token timing can refer to an assistant header and
is not a first-word or live microphone latency. No endpoint latency is measured.

## Baseline quality and feature gates

Both the fresh-process and resident main plans contain 46 cases. **45 actually
executed per plan**, with matching raw text; the three-minute fixture was blocked
before native allocation by the explicit benchmark memory budget. Repeats,
controls, delivery comparisons and ablations are not independent natural clips.

| Test | Baseline BitNet result |
| --- | --- |
| 24 natural English clips | **58/610 edits, 9.51% WER** |
| Two natural French clips | 5/46, 10.87% WER |
| Two unsupported-Hindi diagnostics | 46/64, 71.88% WER |
| Four synthetic meaning controls | 4/52, 7.69% WER |
| Three natural-noise clips plus silence | **0/4 nonempty raw outputs** |
| One utterance attenuated by 30 dB | 4/12, 33.33% WER |
| 45 s AMI meeting, plain text | 59/134, 44.03% WER |
| Same AMI meeting, JSON prompt | Empty output; invalid requested format, 100% WER |

The invoice control preserved the currency unit, amount contrast and negative
instruction, although its grammar differed. Date/time, correction and spoken
instruction controls remained transcription content. A low control WER is not
proof of general intent or meaning preservation.

All three hotword prompts (`Maya`, `Maia`, and unrelated names) returned
`Non-Speech` on the invoice speech control. This is a failed feature test, not
a silence-detection success. The reset recording matched after the context was
cleared. Greedy repeats use the native model's deterministic path; the inherited
Python-reference seed labels do not create a separate BitNet sampling experiment.

The AMI clip is the same ES2004a mixed-headset 600–645-second excerpt, with four
reference speakers. Plain text has no useful speaker attribution; the shared
runtime's JSON option is documented in source as the 7B prompt format. These
results do not establish BitNet diarization, cpWER or DER. Hindi is absent from
the card's seven explicit language tags. Other languages, code-switching,
singing and accent coverage remain unqualified.

An explicit normalization-off ablation made this small noise/quiet-speech screen
worse: two of four noise inputs produced invented speech, and the quiet utterance
became empty (100% WER). Keep the native normalization choice visible; its result
is not evidence for a general VAD or denoising policy.

## Timing, memory and existing default comparison

Across the 24 natural English fresh-process requests, median native processing
was **1.513 s**, nearest-rank p95 **2.641 s**, median RTF **0.138**. Median external
process wall time was **1.809 s**. No competing activity meeting the monitor's
threshold was observed during this CLI screen; sampling cannot exclude all
background activity or establish thermal isolation.

The resident main run's warm 23 natural requests had median processing **1.459 s**
and median RTF **0.127**. External process-to-ready time was 0.678 s and the first
request took 0.754 s. These are warm-filesystem measurements, not a disk-cold
distribution. Six competing-activity observations were retained for this run.

A fresh 24-clip run of the existing frozen Cohere evaluator produced **59/610 edits
(9.67% WER)** and warm median ASR time **0.571 s**. Its first ASR call includes
artifact verification and model loading, so it is not compared directly with
BitNet's verification-excluded cold metric. The writer was disabled. Cohere's
process had one background-activity observation; both pipelines retain their own
model-specific preprocessing and segmentation. The one-word accuracy difference
does not establish superiority, and these timings do not establish an isolated
speed ratio. They provide no reason to replace the selected Cohere default.

BitNet peak RSS grew from about **3.21 GB** on the 4.62-second smoke clip to
**7.07 GB** across the natural clips and **11.43 GB** after the 45-second meeting
in the resident run. The fresh Cohere process peaked at **4.53 GB**. These are
process measurements for the stated duration envelopes, not model file sizes.

The native I8_S encoder reserves `samples × 10,240 + 512 MiB` bytes and retains
its enlarged arena for later requests. At 24 kHz, the 180-second fixture would
request **44.774 GB for that arena alone**. It was not executed: the development
benchmark limits estimated arena size to 16 GiB and observed process RSS to
20 GiB. This is a source-derived allocation bound, not measured long-input RSS
or a new product recording limit. Whole-file memory growth blocks an edge-ready
long-recording claim. Segmentation would be a separate pipeline experiment.

The thread sweep used one 4.62-second clip and one 23.64-second clip, each repeated
three times per setting. The table uses the last two repetitions after each
shape's first request. It does not describe a broad hardware scaling study.

| Native threads | Short-clip median | Long-clip median | Contention observations |
| --- | ---: | ---: | ---: |
| 1 | 3.388 s | 13.958 s | 6 |
| 2 | 1.734 s | 6.998 s | 4 |
| 4 | 1.072 s | 4.467 s | 0 |
| 8 | 0.722 s | 2.593 s | 0 |

## Separate decoder-position correction

Source inspection found that the encoder can return `floor(samples/3200)` frames
while the prompt reserves `ceil(samples/3200)` placeholders. Prefill actually
writes only the available frames, returns the next position, and appends the
suffix contiguously. Both original callers instead start generation from the
nominal prompt count, skipping one logical position for affected lengths. The
pinned llama.cpp API preserves that supplied position in KV/RoPE; it does not
repair the gap. This is runtime-internal correctness work, not a model setting
or training change. [Prefill helper](https://github.com/microsoft/VibeASR.cpp/blob/94cddff7216df8069cbf1b9095714b8327ffeedc/utils/prompt_builder.h),
[native callers](https://github.com/microsoft/VibeASR.cpp/blob/94cddff7216df8069cbf1b9095714b8327ffeedc/src/asr_server.cpp)

The [separate position patch](../../scripts/native-tests/vibeasr-prefill-position.patch)
retains the successful prefill return value and uses it for the next decode
position. It does not right-pad audio, change kernels, modify weights or alter
sampling. It was built separately, preserving the original benchmark binaries,
and ran the same 45 executable cases. The 4.62-second control now records actual
prefill count 80 versus nominal 81.

Eight raw outputs changed, but aggregate English WER remained **58/610, 9.51%**.
French, Hindi, quiet-speech and meeting WER also stayed the same. The synthetic
control score changed from 4/52 to 6/52 edits through different date/time rendering.
Hotword and JSON-format failures remained. This does not demonstrate an accuracy
or speed gain; the patch stays a separately identified development experiment.

A further A / 45-second meeting / A sequence through the corrected resident
runtime reproduced A's exact output. This checks reset after the larger encoder
allocation in that sequence, not general cancellation or long-session isolation.

## Reproduction and limits

Use [the model preparer](../../scripts/prepare-vibevoice-bitnet.py) to download
and verify only the pinned GGUF pair. Fetch the pinned Microsoft source and its
declared llama.cpp fork revision, then build the stock CLI/server in Release.
Retain those binaries, apply the measurement patch and rebuild. Source archive,
source file, build-option and binary hashes are recorded in the evidence.

The measured CMake options were `-DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF
-DGGML_OPENMP=OFF -DLLAMA_CURL=OFF`; build targets were `asr_infer` and
`asr_stream_server`, with four compile jobs. The runtime's CMake disables Metal
and GGML_BLAS itself. Apply the position patch only for the separately named
position-correction experiment, after the measurement patch.

```sh
python3 scripts/prepare-vibevoice-bitnet.py ROOT --download
python3 scripts/prepare-bitnet-benchmark.py ROOT REFERENCE_ROOT
mkdir -p ROOT/runs ROOT/binaries
# Copy the built, instrumented asr_infer and asr_stream_server to ROOT/binaries.
python3 scripts/benchmark-vibevoice-bitnet.py ROOT ROOT/plan-main.json \
  ROOT/runs/main-cli.json --mode cli --threads 8
python3 scripts/benchmark-vibevoice-bitnet.py ROOT ROOT/plan-main.json \
  ROOT/runs/main-resident.json --mode resident --threads 8
```

The executed Python version was 3.12.13. Corpus preparation requires the prior
VibeVoice reference manifests and their verified public audio. Run timed native
processes sequentially. The harness waits up to 60 seconds for a quiet initial
slot unless explicitly run in recorded-contention mode. Output files are created
exclusively; do not overwrite earlier runs. Model/audio verification precedes
inference. Raw reports and executed helper snapshots remain in
`.cache/benchmarks/vibevoice-bitnet-2026-09-18/`; publication removes transcripts.

No new application adapter, downloadable preset, model lifetime or default was
introduced. Native microphone streaming, endpointing, cancellation/recovery,
100-cycle lifetimes, energy/thermal behavior, other hardware and writing-pipeline
costs remain unqualified. The original BitNet adapter proposal in issue #15
remains separate from these development measurements.

Verification included stock and instrumented native builds, five protocol tests
(missing completion despite exit zero, raw bracket/newline preservation, token
cap retention, marker-without-result rejection, and stderr/stdout arrival order),
Python syntax/Ruff checks, whole-artifact SHA verification, PCM parity with the
earlier reference, all planned-row accounting, stock/instrumentation output
parity, reset checks, and transcript-free publication validation. The protocol
tests exposed unclosed parent pipes; explicit pipe cleanup was added and the
tests passed with resource warnings treated as errors. Executed benchmark helper
snapshots retain the earlier hash; this post-run parent cleanup does not change
the recorded native inference or retrospectively rerun the measurements.
