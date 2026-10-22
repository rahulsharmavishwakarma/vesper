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

The GPU-contention and energy results come from the [spike record](ane-coreml-spike-2026-10-22.md). They apply
unchanged, because this is the same encoder program; its weights are byte-identical across conversions.

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
