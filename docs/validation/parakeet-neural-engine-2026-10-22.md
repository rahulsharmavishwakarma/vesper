# Parakeet v3 on the Neural Engine: application-path qualification — 22 October 2026

Question ([#143](https://github.com/rahulsharmavishwakarma/vesper/issues/143),
[ADR-0025](../adrs/0025-neural-engine-speech-engine.md)): does the shipped Neural Engine engine keep Parakeet v3's
English accuracy against today's MLX path through the application's own path? The engine runs the encoder as Core
ML on the Neural Engine and the decoder in MLX.

**Result.**
- Recordings up to 15 s: same accuracy as the MLX path.
- Longer recordings: slightly worse, because they are cut every 15 s instead of every 30 s.
- Placement: 99.92% of the encoder's estimated cost runs on the Neural Engine.
- Load: Core ML's one-time preparation now happens during the download.
- Shipped encoder: int8 per-channel weights since 22 October 2026, at half the download (640 MB in total). It
  matches the float16 encoder within noise on every lane; see [Shipped int8 encoder](#shipped-int8-encoder).

The sections up to [Compressed encoders](#compressed-encoders) qualify the float16 encoder first pinned
(`@4dc405d6`). The GPU-contention and energy results come from the [spike record](ane-coreml-spike-2026-10-22.md),
measured on the float16 program; they are not re-measured for int8 weights.

## Method

- **Application revision:** branch `feature/parakeet-neural-engine` at `c2c8261`, on #295's engine plumbing and
  `main` `215aaa1`.
- **Machine:** Apple M5 Pro, 18 logical CPUs, 64 GiB, macOS 27.0.1, AC power.
- **Files:** installed with the application installer (`examples/install.rs --asr-engine neural_engine`). It
  downloaded and hash-checked the six files of
  `rahulsharmavishwakarma/vesper-parakeet-tdt-0.6b-v3-coreml-ane@4dc405d6` (1,231,812,753 bytes):

  | File | Bytes | SHA-256 |
  | --- | ---: | --- |
  | `ParakeetEncoder15s.mlmodelc/weights/weight.bin` | 1,185,862,912 | `1eb96ba1…` |
  | `ParakeetEncoder15s.mlmodelc/model.mil` | 712,021 | `294b38f4…` |
  | `ParakeetEncoder15s.mlmodelc/coremldata.bin` | 654 | `0c6918c9…` |
  | `ParakeetEncoder15s.mlmodelc/analytics/coremldata.bin` | 243 | `1e3a5766…` |
  | `model.safetensors` (decoder only) | 44,918,508 | `90c527af…` |
  | `config.json` | 318,415 | `60fff77a…` |

  The MLX lane used the pinned bfloat16 files (`vesper-parakeet-tdt-0.6b-v3-mlx-bf16@5668bb2d`).
- **Runner:** `vesper-eval run … --model nvidia/parakeet-tdt-0.6b-v3 --asr-engine neural_engine|standard
  --language en`, in completed-audio mode.
  - Scoring is `vesper-eval`'s normalized word error rate.
  - Differences use a 2,000-resample paired clip bootstrap.
- **Sets:** the held-out FLEURS en-US manifests from the bf16 qualification.
  - `heldout-fleurs100`: 94 clips, 2,016 words, manifest `347fd8a6…`.
  - `heldout-long48`: 48 clips, 1,171 words, manifest `72ba306a…`.
  - Each set has 4 clips of 15–20 s.

## Conversion

- **Converter:** `scripts/convert-parakeet-v3-coreml-ane.py`, in the locked `coreml-convert` project.
  - Versions: coremltools 9.0, PyTorch 2.14.1, Transformers 5.20.2, NumPy 1.26.4.
  - Audit: `pip-audit --strict` reports no known vulnerabilities.
  - Source: the encoder is built from NVIDIA's Transformers-format checkpoint at `541d1f99`. The NeMo stack the
    spike used left 18 advisories.
- **Float32 parity against NeMo's own encoder** (`.nemo` `3cbdc858…`), on the same 15 s FLEURS mel:

  | Mel frames | 1501 | 1200 | 901 | 457 | 120 |
  | --- | ---: | ---: | ---: | ---: | ---: |
  | Encoder frames (both) | 188 | 150 | 113 | 58 | 15 |
  | Max abs diff | 4.4e-7 | 2.4e-7 | 2.3e-6 | 2.3e-7 | 3.4e-7 |

  Two adjustments make the export match:
  - **Input masking.** NeMo zeroes mel frames past the length before the first convolution. Without that,
    lengths not divisible by 8 differed by up to 0.085.
  - **Attention mask.** Transformers' float32-minimum mask becomes -inf in float16, which turns fully padded rows
    into NaN. The converter clamps it to NeMo's -10000.
- **Core ML result.**
  - On the same speech mel, the Neural Engine output had cosine 0.99944 against NeMo float32, the same as the
    spike's encoder (0.99942).
  - The converter's own gate uses a synthetic voiced signal: cosine 0.989–0.999, finite at every length, exact frame
    counts. It catches broken exports, not accuracy.
  - The Core ML compute plan schedules 99.92% of estimated cost on the Neural Engine.

## Results through the application

| Set | Neural Engine WER | MLX WER | Δ [95% CI] | Same text | Median per clip, NE vs MLX |
| --- | ---: | ---: | --- | ---: | --- |
| `heldout-fleurs100` (94) | 5.26% | 5.16% | +0.10 [−0.25, +0.42] | 82/94 | 40.5 vs 39.7 ms |
| `heldout-long48` (48) | 6.75% | 6.40% | +0.35 [0.00, +0.78] | 41/48 | 42.6 vs 45.4 ms |

Neither lane had a failed clip.

- **The whole difference comes from the clips over 15 s.** They gained 3 word errors in each set. Clips up to 15 s
  changed by +1 and −1.
- **Why those clips differ:** the engine cuts them into two parts, while the MLX path decodes them whole within its
  30 s window.
- **Timing:** median per-clip time is about the same through the application. The engine's benefit is the free GPU
  and lower energy, not lower latency.

A Swift test of the production class (`ParakeetNeuralEngineTests`, `VESPER_ANE_PARITY`) gave the same picture with
a plain word normalizer:

| Set | Neural Engine WER | MLX WER | Same text |
| --- | ---: | ---: | ---: |
| 94 held-out | 5.34% | 5.49% | 84/94 |
| 100 unseen FLEURS clips | 5.77% | 5.82% | 95/100 |

## Load and preparation

- **First load in a process:** Core ML specializes the program for the Neural Engine on the first load of a model
  path, which took 7.3 s here. Later loads of that path in the same executable took 129–153 ms.
- **The cache is per executable as well as per path.** Measured with the install example and the XCTest runner:

  | Run | Time |
  | --- | ---: |
  | Install example, fresh path (hash checks plus preparation) | 8.8 s |
  | Install example, same path again | 1.0 s |
  | XCTest runner's first load of that path afterwards | 7.3 s |

  The application downloads and transcribes in one process, so loading the encoder at the end of the install keeps
  that time off the first recording.
- **macOS updates:** Apple documents that an update clears the cache
  ([WWDC23 10049](https://developer.apple.com/videos/play/wwdc2023/10049/)). The first recording after an update
  pays the cost again.

## Compressed encoders

Asked on #301: can a palettized encoder shrink the 1.19 GB download? The variants below were compressed from the same
float16 program with coremltools 9.0 at the app's macOS 14 target:
- per-tensor k-means palettization, the only kind that target supports;
- int8 per-channel linear quantization, for comparison.

All four schedule 99.92% of estimated cost on the Neural Engine.

| Encoder | Weights | First load | Median per 15 s window | Cosine vs NeMo fp32 (speech mel) |
| --- | ---: | ---: | ---: | ---: |
| float16 (shipped) | 1,186 MB | 7.1 s | 24.8 ms | 0.99944 |
| 8-bit palettized | 593 MB | 6.7 s | 24.0 ms | 0.99570 |
| int8 linear, per channel | 594 MB | 7.1 s | 24.8 ms | 0.98935 |
| 6-bit palettized | 445 MB | 10.5 s | 23.3 ms | 0.93553 |

Word error rates come from the Swift parity test (`ParakeetNeuralEngineTests`, plain word normalizer, the same MLX
decoder). The 94-clip set is the held-out FLEURS set used above. The regression lanes are #251's 48-clip
`heldout-regression` set: 4 clean clips, 4 at 10 dB natural noise, 4 at 0 dB, 28 at −30 dB, and 8 non-speech clips.
The bracketed interval is a 2,000-resample paired bootstrap against float16.

| Encoder | 94 held-out | Δ vs float16 [95% CI] | Same text as float16 | Clean | 10 dB | 0 dB | −30 dB | Non-speech clips with text |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| float16 | 5.34% | — | 94/94 | 15.38% | 15.38% | 19.23% | 8.48% | 0/8 |
| 8-bit palettized | 5.44% | +0.10 [0.00, +0.26] | 88/94 | 15.38% | 15.38% | 19.23% | 8.64% | 0/8 |
| int8 linear | 5.24% | −0.10 [−0.25, 0.00] | 82/94 | 15.38% | 15.38% | 19.23% | 8.48% | 0/8 |
| 6-bit palettized | 5.59% | +0.25 [−0.31, +0.91] | 70/94 | 15.38% | 15.38% | 19.23% | 8.33% | 0/8 |

The all-MLX path scored 5.49% on the same 94 clips, with identical regression lanes.

- **8-bit is within noise.** Both 8-bit encoders halve the download, keep Neural Engine placement and speed, and
  change WER by at most 0.10 points.
- **6-bit is too lossy.** Per-tensor 6-bit loses fidelity and changes a quarter of the transcripts, so it is not a
  candidate at this target.
- **Not adopted yet.** The shipped engine still uses float16. Switching to an 8-bit file needs its own hosted
  artifact, a manifest change and the application-path lanes.

## Compute-unit fallback

The shipped compiled model is one directory. It needs no extra files to run on other Core ML compute units. Measured on
the installed files with the 15 s speech mel:

| Compute units | First load | Median per 15 s window | Cosine vs NeMo fp32 |
| --- | ---: | ---: | ---: |
| CPU and Neural Engine (the app's setting) | 8.8 s | 24.8 ms | 0.99944 |
| CPU only | 0.6 s | 84.9 ms | 0.99992 |
| CPU and GPU | 0.9 s | 14.3 ms | 1.00000 |
| All | 1.5 s | 14.1 ms | 1.00000 |

The application requests CPU and Neural Engine. Within that setting, Core ML runs on the CPU any operation the Neural
Engine cannot take. The decoder always runs in MLX on the Apple GPU.

## Shipped int8 encoder

The owner approved int8 linear weights on 22 October 2026 (#143). The converter now quantizes every weight of
2,048+ elements to symmetric per-channel int8, with float16 compute; the decoder files are unchanged.

- **Hosted:** `@c81d0368`. `weight.bin` is 594,214,208 bytes, `2b4c00b4…`, byte-identical to the int8 variant
  above. The total download is 640,274,531 bytes, against 1,231,812,753.
- **Installed:** with the application installer, every hash verified.
- **Measured:** through `vesper-eval` with the same method, sets and machine as the float16 lanes above.

| Set | int8 | float16 | MLX | int8 − float16 [95% CI] | Same text as float16 |
| --- | ---: | ---: | ---: | --- | ---: |
| `heldout-fleurs100` (94) | 5.11% | 5.26% | 5.16% | −0.15 [−0.34, 0.00] | 83/94 |
| `heldout-long48` (48) | 6.49% | 6.75% | 6.40% | −0.26 [−0.58, 0.00] | 39/48 |

Regression lanes (`heldout-regression`, 48 clips) were identical for int8 and float16:

| Lane | Clips | WER |
| --- | ---: | ---: |
| Clean | 4 | 15.38% |
| Natural noise, 10 dB | 4 | 15.38% |
| Natural noise, 0 dB | 4 | 19.23% |
| Quiet, −30 dB | 28 | 8.16% |

No non-speech clip produced text (0/8), and no clip failed.

The median application time per clip was 41.3 ms for int8 against 40.5 ms for float16.

Encoder on the installed int8 files, with the 15 s speech mel:

| Compute units | Median per 15 s window | Cosine vs NeMo fp32 |
| --- | ---: | ---: |
| CPU and Neural Engine (99.92% placement; first load 6.8 s) | 24.1 ms | 0.98935 |
| CPU only | 83.3 ms | 0.97896 |
| CPU and GPU | 14.6 ms | 0.98023 |

The lower cosine against NeMo fp32 did not change word error rates.

The model now tries those compute units in that order at load, falling through on a failed load or probe window.
- Loaded alone, each unit gave identical text on a held-out clip (`testEveryComputeUnitRunsTheInstalledEncoderWhenProvided`).
- Rerunning the regression lane through `vesper-eval` recorded `encoderComputeUnits: cpu_and_neural_engine` for every
  segment, with all 48 transcripts unchanged.

## Limitations

- **One machine.** M1–M4 Macs and lower-memory Macs are not measured. The owner decided on 22 October 2026 that
  per-model Mac measurements are not required to ship this engine.
- **English only.** Parakeet's other 24 languages need their own lanes before the engine offers them.
- **15 s parts.** Longer recordings get twice as many cuts as on the MLX path.
  - Follow-up: one compiled model with enumerated 15 s and 30 s shapes would share weights and avoid the extra cuts.
  - That needs its own placement, load-time and accuracy check.
- **Not measured on the shipped engine:** writer contention, energy and the first load inside the packaged
  application.
  - The first two are expected to match the spike, which used the same encoder program.
  - The third rests on the per-executable measurement above.
