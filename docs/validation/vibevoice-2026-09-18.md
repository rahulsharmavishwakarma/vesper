# VibeVoice reference CPU screen — 18 September 2026

**Streaming-1.5B is not a sub-2B model or a qualified low-latency CPU replacement.**
The verified family inventory found no complete Microsoft VibeVoice ASR variant
below two billion total parameters. The explicitly requested checkpoint was
tested as an out-of-budget reference. No application adapter, default, model
lifetime or writing pipeline was changed.

All **49 case executions** completed across three inference processes: one smoke,
43 completed-audio cases, and five streaming/meeting cases. There were no missing
results, process failures or token-cap failures. Quality and latency qualification
still fail; successful execution is not a model recommendation. Cases include
repeats and controls, not 49 independent natural recordings.

See the [machine-readable evidence](vibevoice-2026-09-18.json),
[primary-source research and full family inventory](../research-vibevoice.md)
and [issue #15](https://github.com/rahulsharmavishwakarma/transcribe/issues/15).
This is an isolated Microsoft Python-runtime experiment, not the application's
Rust inference path or a full benchmark-set evaluation.

## Completed recordings: measured quality

The 24 natural English recordings contain 610 reference words and last
3.840–23.640 seconds. The existing Rust
scorer was used, removing only the model's `Speaker N:` markers from ASR text.

| Executed checkpoint/runtime | Natural English word edits | WER |
| --- | ---: | ---: |
| VibeVoice Streaming-1.5B, PyTorch CPU FP32 | 70 / 610 | **11.48%** |
| Cohere INT8, earlier same-corpus screen | 59 / 610 | 9.67% |
| Whisper small INT8, earlier same-corpus screen | 45 / 610 | 7.38% |
| Whisper Turbo INT8, earlier same-corpus screen | 30 / 610 | 4.92% |

The comparator values are from the [Whisper screen](whisper-2026-09-18.md), not
new comparator runs. The natural recordings, references and scorer match;
these numbers do not establish a universal ranking or a latency comparison.
Whisper's own noise, Hindi and duration limitations remain applicable.

French's two natural clips had 16/46 edits (**34.78% WER**). Hindi had 63/64
(**98.44%**); it is an explicitly unsupported-language diagnostic, not evidence
against Hindi support in other VibeVoice checkpoints. Languages were recognized
without a forced language ID. Code-switching and singing were not evaluated.

The four synthetic meaning controls retained negation, self-correction and
spoken-instruction transcription. Numeric formatting differs from the written
references. The amount control omitted the spoken currency unit, so preserving
the two numbers alone does not establish complete meaning preservation. No
writing-model repair was applied or benchmarked.

On three natural-noise clips plus silence, three outputs contained only model
event annotations (`[Noise]`, `[Music]`, `[Silence]`), while one was empty. The
strict empty-reference output metric therefore reports 3/4 nonempty outputs and
five inserted annotation words. Removing only those known event markers for a
separate diagnostic reveals **0/4 lexical speech hallucinations**. Both metrics
must remain visible; labels are not invented spoken sentences, and the app has
no implemented policy for hiding them. The single utterance attenuated by 30 dB
was also labeled silence, missing all 12 reference words. This prevents treating
the model's silence output as a reliable quiet-speech filter.

Hotwords were tested on one synthetic name/amount control with a fixed seed.
The correct `Maya` hint left the baseline output unchanged; `Maia` changed its
spelling. Unrelated `Zorblax,Quuxcity` hints also changed the recognized name to
`Maia`, without inserting those unrelated hint words. These three cases show
prompt sensitivity, not a general hotword accuracy gain. Resetting to the same
initial recording after noise, language and hint changes reproduced its output;
one alternate seed also reproduced it. No continuous learning took place.

## Paced streaming and speaker attribution

The five additional cases were a 4.62-second reset control, the same 45-second
natural AMI meeting excerpt delivered as a completed file and as paced audio,
a three-minute continuous fixture, and the initial control repeated afterward.
Pacing makes each input window available at its corresponding recording time;
this is file replay through the publisher step API, not a microphone queue test.

| Recording and delivery | First nonempty partial | Stop-to-final | Compute RTF |
| --- | ---: | ---: | ---: |
| 4.62 s control, paced | 6.010 s | 3.315 s | 1.021 |
| 45 s AMI, completed file | 2.487 s | 44.669 s | 0.991 |
| 45 s AMI, paced | 6.338 s | 8.540 s | 1.106 |
| 180 s continuous fixture, paced | **6.382 s** | **13.539 s** | 1.014 |
| 4.62 s control after reset, paced | 6.893 s | 5.119 s | 1.396 |

First partial is measured from request start to a completed chunk containing
nonempty output after removing speaker labels. It is not a word-aligned emission
measurement. For paced delivery, stop-to-final is completion time minus audio
duration; for a completed file, the entire processing time remains after input
is available. These values exclude model construction, audio preparation, the
writing model and application delivery. No automatic endpoint was used.

The AMI excerpt is ES2004a mixed-headset, seconds 600–645: one development/validation
excerpt with four annotated speakers and 134 normalized reference words. The model
produced three speaker labels. Both delivery modes returned exactly the same raw
output. Ordinary WER was 40/134 (**29.85%**); exact concatenated minimum-permutation
speaker-attributed WER was 39/134 (**29.10% cpWER**). cpWER combines transcription
and speaker-assignment errors; it is not diarization error rate (DER), which was
not measured. Two executions of this excerpt are not two independent meetings.

The 180-second fixture advanced through all 62 windows, including its padded tail.
Maximum backlog before computing a window reached **10.322 seconds**. Input coverage
does not establish recognition of every spoken word. This fixture stitches clean,
noise-mixed and attenuated speech and has no aligned reference transcript. Its inherited
`scenario: noise` label belongs to an earlier contract fixture; it must not be scored
as pure non-speech. WER, CER and hallucination scoring are explicitly unavailable
for this case, and its aggregate is separate from the scored short reset controls.

The long recording ended with 2,373 decoder cache tokens. Fresh state reduced this
to 108 on the repeated short control, matching its initial final cache length and
exact output. The meeting's completed-file and paced runs each ended at 754 tokens.
These are limited reset/parity observations, not proof of cancellation isolation,
stable speaker identity across recordings or bounded long-session attention cost.

## CPU timing and memory envelope

Apple M5 Pro, 18 logical CPUs, 64 GiB RAM, macOS 27.0 (26A428), AC power, normal power
mode; CPU/FP32/SDPA, eight Torch threads and one interop thread. Other native
builds and macOS background activity overlapped the screen. The smoke process
had 14 contention observations; the completed-audio process had 100. **Timings are
diagnostic, not an isolated speed ranking, optimization gain or SOTA claim.**
The streaming preflight did not find a quiet slot within 120 seconds; a separate
run explicitly allowed and recorded contention, yielding 48 observations of
competing macOS/editor activity. No unrelated processes were stopped.

For the 23 natural English requests after the first, median processing time was
8.968 s and nearest-rank p95 was 16.530 s. Median compute RTF was 0.834. RTF compares
compute with audio duration; it is not stop-to-final latency or first-partial
latency. These completed recordings were already available when processing
began, and their processing time does not include microphone capture, disk
decode/resampling, writing, UI, persistence or insertion.

The two audio encoders accounted for 67.38% of measured encoder-plus-decoder
compute across the 24 natural English recordings, excluding prompt prefill.
This supports investigating encoder execution before expecting KV-cache-only
work to solve the whole latency problem. It does not measure a faster encoder.

The completed-audio process peaked at 12,887,900,160 bytes RSS (12.89 decimal GB).
Torch loaded 2,580,742,625 unique parameters, 10,322,970,500 bytes at FP32, including
the unused synthesis decoder. The stored checkpoint has 2,814,116,321 values;
tied embeddings explain the difference. Weight verification took 1.914 s and
model/processor loading 1.090 s in this process. These are warm-filesystem object
construction observations, not a disk-cold or boot-cold distribution.

The streaming process peaked at 12,888,522,752 bytes RSS. Its coordinator recorded
400.511 seconds wall time, versus 309.665 seconds of reported recording/preparation
stages and 2.987 seconds of verification/loading. The remaining **87.858 seconds
are unattributed**, including uninstrumented work and coordinator overhead. The
evidence retains this discrepancy; no cause or end-to-end startup claim is inferred.

## Provenance and reproduction

Model revision `4262d23d8a539a6530cf64fbd0b1751ef9a30853`; Microsoft source
`1541f590c7099820f10ea012f48d2399282df69f`. Every model file and selected source
file was hash-verified against the [pinned benchmark artifacts](../../benchmarks/asr/vibevoice-streaming-artifacts.json).
The separate [dependency lock](../../benchmarks/asr/vibevoice-reference-requirements.txt)
records Torch 2.8.0, Transformers 4.51.3 and the resolved development environment.
Native Torch library hashes and its Git revision are retained in the evidence.

Raw reports, per-chunk text, public corpus manifests and executed helper snapshots
live under `.cache/benchmarks/vibevoice-2026-09-18/`. No private audio or microphone
input was used. Public FLEURS and MS-SNSD sources retain the existing
[hard-regression corpus](../../benchmarks/asr/audio-regression.md) licenses and
hashes; the model does not access application history or personalization storage.

Prepare that corpus and the existing French/Hindi long-ASR manifests first.
The case preparer also requires the existing AMI mixed-headset audio and manual
annotations used by the audio-frontend regression. Use a fresh output root:

```sh
uv venv --python 3.12 .cache/vibevoice-eval-venv
uv pip install --python .cache/vibevoice-eval-venv/bin/python \
  -r benchmarks/asr/vibevoice-reference-requirements.txt
python3 scripts/prepare-vibevoice-model.py ROOT --download
python3 scripts/prepare-vibevoice-benchmark.py --root ROOT
mkdir -p ROOT/runs ROOT/binaries
cp scripts/probe-vibevoice.py ROOT/binaries/probe-vibevoice.py

OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 VECLIB_MAXIMUM_THREADS=1 \
MKL_NUM_THREADS=1 TOKENIZERS_PARALLELISM=false \
  .cache/vibevoice-eval-venv/bin/python ROOT/binaries/probe-vibevoice.py \
  --root ROOT --plan ROOT/plan-offline.json --output ROOT/runs/offline.json \
  --threads 8 --case-timeout 900
```

Execute `plan-smoke.json` and `plan-streaming.json` sequentially with distinct
output filenames and the same settings. Retain process wall time, exit status,
power and competing-process observations alongside each run. The original
coordinator and executed helper snapshots are retained in the local evidence
root; the commands above reproduce the inference probe, not that monitoring.
The model/source pins, report hashes and frozen runner hash are published. The
application HEAD was `592e0084fe5cf486b4d56c575423e9a4f219bfb8`; uncommitted benchmark
sources are identified by content hashes rather than implying that HEAD contains
them. Raw transcript-free scores can be republished using
`scripts/summarize-vibevoice.py ROOT OUTPUT`, with the canonical release-built
`transcribe-eval` `score_json` example copied to `ROOT/binaries/score-json` and the
recorded environment/provenance JSON files in `ROOT`.

The model preparer verifies existing files without `--download`; it never installs
an app preset. The probe rejects mismatched artifacts/audio and existing output
files. It uses seeded Gaussian acoustic sampling, greedy text decoding, the
trained 22+4 frame geometry, fresh recording state and one resident model per
process. It records token-cap exhaustion as failure; the native step API still
does not distinguish EOS from chunk-end. No VAD, denoiser, encoder replacement,
quantization or custom kernel was added.

The current probe's RSS units and signal timeout were exercised on macOS only.
MLX/Metal, CUDA, BitNet-native, other devices, energy/thermal behavior, 100-cycle
lifecycle tests and cancellation isolation remain unqualified. Do not infer
their results from this FP32 reference screen.

## Verification of the benchmark tooling

The Rust scoring example built in release mode and passed strict Clippy; the
17 evaluation-library tests passed. Python syntax and Ruff checks passed for
the probe, artifact/case preparers and publisher. Checks covered checksum
rejection before model loading, refusal to overwrite evidence, canonical
English/Hindi/non-speech scoring, and cpWER speaker permutation, merge and extra
speaker behavior. Recreating all three case plans and the AMI excerpt reproduced
their content/audio hashes. Model and source hashes were verified before every
inference process and again with the standalone artifact preparer.

Publication checks preserve every planned case and runtime failure, separate event
annotations from lexical noise hallucinations, leave the reference-free long
fixture unscored, and remove raw transcripts from the public JSON. Partial
stability, word-emission timing, endpointing, live queue drops and sustained
thermal behavior still require a dedicated application qualification run.
