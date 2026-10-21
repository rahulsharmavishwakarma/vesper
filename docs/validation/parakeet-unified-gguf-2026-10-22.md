# Parakeet v3 unified GGUF engine: application-path qualification — 22 October 2026

Question ([#293](https://github.com/rahulsharmavishwakarma/vesper/issues/293), [ADR-0024](../adrs/0024-unified-gguf-speech-engine.md)):
does Parakeet TDT 0.6B v3 as NVIDIA's one Q8_0 GGUF on NeMo-Speech.cpp keep accuracy against the current engine, with
the same results on CPU and GPU, through the application's own path?

**Result:** on the #251 held-out read-speech sets, the unified engine gave identical transcripts on CPU and Metal, was
**1.01 points more accurate on English** (significant) and 0.63 points more accurate on German (within noise) than the
current 4-bit ONNX engine, at about the same CPU speed and 4-5x lower latency on Metal. Noise, quiet speech, other
languages, long audio, CUDA and Vulkan are not measured yet, so `Standard` stays the default.

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

| Set | Engine, device | WER | vs current [95% CI] | Same transcript as current | Median ASR per clip |
| --- | --- | ---: | --- | ---: | ---: |
| en-US 100 | Current ONNX, CPU | 5.86% | — | — | 255 ms |
| en-US 100 | Unified GGUF, CPU | **4.86%** | **−1.01 [−1.75, −0.31]** | 59 / 100 | 284 ms |
| en-US 100 | Unified GGUF, Metal | **4.86%** | **−1.01 [−1.75, −0.31]** | 59 / 100 | **58 ms** |
| de-DE 50 | Current ONNX, CPU | 6.62% | — | — | 358 ms |
| de-DE 50 | Unified GGUF, CPU | 5.99% | −0.63 [−1.58, +0.33] | 25 / 50 | 354 ms |
| de-DE 50 | Unified GGUF, Metal | 5.99% | −0.63 [−1.58, +0.33] | 25 / 50 | **74 ms** |

CPU and Metal produced the same scores on both sets. No clip failed. The current engine's 6.62% German result matches
its #251 record on the same set.

Supporting measurements outside the application path:

- NeMo-Speech.cpp CLI on 100 other FLEURS en-US test clips: 4.94% CPU and 4.89% Metal, the same transcript for 99/100
  clips; Q6_K/Q5_K/Q4_K conversions lost 0.14-0.46 points and were not faster (#287).
- A separate session measured the same GGUF on the 94-clip `qual-ultra/heldout-fleurs100.json` set (#143): 5.05% on
  both CPU and Metal, against 5.25% for the bf16 MLX build through the app; the MLX app path was about 28% faster per
  clip than NeMo Metal there (42.7 ms against 59 ms mean). Those runs used different timing scopes and are not
  combined with the table above.

## Limitations and remaining gates

- Read speech in English and German only. Noise, quiet (−30 dB), spoken controls and non-speech controls, the other 23
  declared languages and long recordings remain to be qualified.
- CUDA and Vulkan packages are pinned but were not run.
- Memory, cold load and sustained thermal behaviour were not recorded.
- Raw transcripts differ in punctuation and number formatting because NeMo-Speech.cpp normalizes text differently from
  sherpa-onnx; the scorer normalizes punctuation, but number formatting can still count as errors in either direction.
