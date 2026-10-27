# Parakeet v3 unified GGUF engine: application-path qualification — 22 October 2026

Question ([#293](https://github.com/rahulsharmavishwakarma/vesper/issues/293), [ADR-0024](../adrs/0024-unified-gguf-speech-engine.md)):
does Parakeet TDT 0.6B v3 as NVIDIA's one Q8_0 GGUF on NeMo-Speech.cpp keep accuracy against the current engine, with
the same results on CPU and GPU, through the application's own path?

**Result:** on the #251 held-out read-speech sets, the unified engine gave identical transcripts on CPU and Metal, was
**1.01 points more accurate on English** (significant) and 0.63 points more accurate on German (within noise) than the
current 4-bit ONNX engine, at about the same CPU speed and 4-5x lower latency on Metal. On the regression lanes it cut
quiet-speech WER from 64.05% to 7.25%, improved or matched clean and noisy speech, and produced no text on non-speech
audio. Other languages, long audio, CUDA and Vulkan are not measured yet. The engine was opt-in when measured; it is now
the Automatic default for Parakeet ([ADR-0026](../adrs/0026-automatic-speech-engine.md)).

## Method

- Application revision: branch `feature/parakeet-nemo-gguf` at `9967be5` (from `main` `d85fd75`).
- Files installed with the application installer (`crates/vesper-models/examples/install.rs --asr-engine unified_gguf`),
  which downloaded and hash-checked `nvidia/parakeet-tdt-0.6b-v3@541d1f99`
  `parakeet-tdt-0.6b-v3.q8_0.gguf` and the pinned `nemo-speech-0.1.0-macos-aarch64-{cpu,metal}` packages. The current
  engine used the pinned `vesper-parakeet-tdt-0.6b-v3-sherpa-onnx-wo4@8e178c8a` files.
- Runner: `vesper-eval run … --model nvidia/parakeet-tdt-0.6b-v3 --asr-engine unified_gguf|standard`, completed-audio
  mode, language from the manifest, environment backend `cpu` or `metal`. Each report's `initializedProvider` confirms
  the device used. Scoring is `vesper-eval`'s own normalized word error rate; the difference uses a 2,000-resample
  paired clip bootstrap against the current engine.
- Sets: `fleurs-en-us-100` (100 FLEURS en-US clips, 2,183 words, manifest `8dd6cabe…`) and `fleurs-de-de-50`
  (50 FLEURS de-DE clips, 1,118 words, manifest `7a13c048…`), the held-out manifests of
  [#251](parakeet-weight-only-2026-10-21.md).
- Machine: Apple M5 Pro, 18 logical CPUs, 64 GiB, macOS 27.0.1; NeMo uses 4 graph threads.

## Results

| Set       | Engine, device      |       WER | vs current [95% CI]      | Same transcript as current | Median ASR per clip |
| --------- | ------------------- | --------: | ------------------------ | -------------------------: | ------------------: |
| en-US 100 | Current ONNX, CPU   |     5.86% | —                        |                          — |              255 ms |
| en-US 100 | Unified GGUF, CPU   | **4.86%** | **−1.01 [−1.75, −0.31]** |                   59 / 100 |              284 ms |
| en-US 100 | Unified GGUF, Metal | **4.86%** | **−1.01 [−1.75, −0.31]** |                   59 / 100 |           **58 ms** |
| de-DE 50  | Current ONNX, CPU   |     6.62% | —                        |                          — |              358 ms |
| de-DE 50  | Unified GGUF, CPU   |     5.99% | −0.63 [−1.58, +0.33]     |                    25 / 50 |              354 ms |
| de-DE 50  | Unified GGUF, Metal |     5.99% | −0.63 [−1.58, +0.33]     |                    25 / 50 |           **74 ms** |

CPU and Metal produced the same scores on both sets. No clip failed. The current engine's 6.62% German result matches
its #251 record on the same set.

### Regression lanes

The [16 September regression corpus](audio-regression-2026-09-16.md) (`asr.json`: 28 FLEURS en-US sentences per
condition plus 8 non-speech clips), explicit English, same runner and build:

| Lane                      |   Current ONNX, CPU | Unified GGUF, CPU [vs current, 95% CI] | Unified GGUF, Metal [vs current, 95% CI] |
| ------------------------- | ------------------: | -------------------------------------: | ---------------------------------------: |
| Clean, 662 words          |               8.01% |                   6.34% [−4.47, +0.47] |                     6.04% [−4.74, +0.28] |
| Natural noise 0 dB        |              10.57% |                   9.52% [−3.62, +0.92] |                     9.37% [−3.71, +0.62] |
| Natural noise 10 dB       |               7.55% |                   6.95% [−2.30, +0.79] |                     6.95% [−2.30, +0.79] |
| **Quiet, −30 dB**         |          **64.05%** |             **7.25% [−72.91, −37.21]** |               **7.25% [−72.91, −37.21]** |
| Non-speech noise, 8 clips | 0 false transcripts |                    0 false transcripts |                      0 false transcripts |

The current engine loses most words on quiet speech, as its #251 record already showed (68.75% there); the unified
engine does not. The cause was not isolated: NeMo-Speech.cpp's feature front end may normalize level differently from
the sherpa-onnx export. Every other lane improved or stayed within noise, and neither engine produced text on
non-speech audio.

Supporting measurements outside the application path:

- NeMo-Speech.cpp CLI on 100 other FLEURS en-US test clips: 4.94% CPU and 4.89% Metal, the same transcript for 99/100
  clips; Q6_K/Q5_K/Q4_K conversions lost 0.14-0.46 points and were not faster (#287).
- A separate session measured the same GGUF on the 94-clip `qual-ultra/heldout-fleurs100.json` set (#143): 5.05% on
  both CPU and Metal, against 5.25% for the bf16 MLX build through the app; the MLX app path was about 28% faster per
  clip than NeMo Metal there (42.7 ms against 59 ms mean). Those runs used different timing scopes and are not
  combined with the table above.

### Contention with the writer (measured in #143)

A separate session measured how the speech engines and the Metal writer slow each other when they run at the same time
([#143](https://github.com/rahulsharmavishwakarma/vesper/issues/143)). Quiet machine on AC power, Apple M5 Pro, two
rounds in alternating order. Writer: llama.cpp at the pinned `d8123504`, Qwen3.5-0.8B Q4_0 on Metal (pp512/tg128; solo
248 tok/s generation). The speech engines ran as continuous loops; the NeMo lanes used `nemo-speech bench` on 94
held-out FLEURS clips with the CLI's default thread count, not the app's 4 graph threads.

| Speech engine running alongside       | Writer generation | Writer prompt processing |     Speech latency with writer running |
| ------------------------------------- | ----------------: | -----------------------: | -------------------------------------: |
| Core ML on the Neural Engine          |               −8% |                      −4% |                                  ×1.32 |
| Core ML on GPU                        |              −48% |                     −29% |                                  ×1.52 |
| MLX on GPU (current Apple GPU engine) |              −61% |                     −33% |                                  ×1.40 |
| **Unified GGUF, Metal**               |          **−25%** |                 **−17%** | **×2.84** (58.6 → 161-172 ms per clip) |
| **Unified GGUF, CPU**                 |           **−3%** |                  **−2%** |      ×1.96 (300 → 446-740 ms per clip) |

The unified engine on Metal slows the writer less than the current MLX engine, but its own latency is the most
sensitive to a concurrent writer; on CPU it barely affects the writer. Vesper's inference worker runs requests one at a
time, so this matters only when work overlaps (live previews during polishing, other GPU applications, or a future
pipelined writer), not for back-to-back requests.

### One download for CPU and GPU

On Apple silicon the installer now selects the Metal NeMo-Speech.cpp package for every device, since it also contains
the CPU backend. Re-running the en-US 100 set on CPU from a model folder holding only the Metal package gave the same
4.86% WER, with `initializedProvider` reporting CPU and a 287 ms median per clip. Nemotron shares this package
selection; its CPU path through the Metal package was not re-qualified here.

## Limitations and remaining gates

- English and German only. The other 23 declared languages, spontaneous speech and long recordings remain to be
  qualified.
- CUDA and Vulkan packages are pinned but were not run.
- Memory, cold load and sustained thermal behaviour were not recorded. Contention was measured with the CLI, not the
  application worker.
- Raw transcripts differ in punctuation and number formatting because NeMo-Speech.cpp normalizes text differently from
  sherpa-onnx; the scorer normalizes punctuation, but number formatting can still count as errors in either direction.
