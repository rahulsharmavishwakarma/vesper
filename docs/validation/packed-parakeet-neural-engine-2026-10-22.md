# Phonon-2 and Parakeet Redux on the Neural Engine: spike — 22 October 2026

Question ([#304](https://github.com/rahulsharmavishwakarma/vesper/issues/304)): can Phonon-2 and Parakeet Redux run
their encoders on the Apple Neural Engine like Parakeet v3
([ADR-0025](../adrs/0025-neural-engine-speech-engine.md)), and keep their small downloads?

**Result: don't ship either as a Neural Engine engine.**
- **What works:** both encoders convert cleanly, keep 99.92% of estimated cost on the Neural Engine, and match their
  MLX paths in accuracy.
- **Size:** exact compression of their packed weights brings them to 252 MB (Phonon-2) and 585 MB (Redux).
- **Cost of that compression:** a macOS 15 deployment target, 3.5× slower encoding, and a 90 s first load.
- **Comparison:** at the same placement, Parakeet v3 with int8 weights is more accurate and faster at 640 MB.
- **Decision:** Phonon-2 and Redux stay on the Apple GPU through MLX.

All results are local measurements: an M5 Pro, macOS 27.0.1, AC power, coremltools 9.0, `.cpuAndNeuralEngine`.

## Method

- **Dense weights.** MLX itself unpacked the pinned packed files (`mx.dequantize`): 264 projections per model, to
  float32.
  - Phonon-2: w = lo(q−1) + (hi−lo)(s−1).
  - Redux: w = scale·(code−1).
- **Name and layout map.** Derived by matching values between v3's MLX bfloat16 checkpoint and NVIDIA's
  Transformers checkpoint, 684 of 684 tensors.
- **Conversion.** The weights were loaded into Transformers' `ParakeetEncoder` and converted with the
  `scripts/convert-parakeet-v3-coreml-ane.py` wrapper: NeMo masking, a 15 s window, float16.
- **Decoder.** Each model's own decoder-only MLX checkpoint, with zero encoder layers.
- **Accuracy.** `ParakeetNeuralEngineTests` with a `baseline` of `phonon2` or `parakeet_redux`. It ran on the 94
  held-out FLEURS en-US clips against each model's current MLX path. Phonon-2's digit-spacing repair is applied to
  both sides.

## Dense float16 (macOS 14 target)

| Model | Placement | Weights | NE WER | MLX WER | Δ [95% CI] | Same text | Median per clip, NE vs MLX |
| --- | ---: | ---: | ---: | ---: | --- | ---: | --- |
| Parakeet Redux | 99.92% | 1,186 MB | 6.69% | 6.69% | [−0.15, +0.15] | 89/94 | 33.2 vs 39.8 ms |
| Phonon-2 | 99.92% | 1,186 MB | 5.79% | 5.89% | [−0.25, 0.00] | 92/94 | 34.1 vs 41.1 ms |

Against float32 PyTorch, every length had cosine of at least 0.9992 and finite output.

## Keeping the packed size (macOS 15 target)

| Encoding | Phonon-2 | Redux | Outcome |
| --- | --- | --- | --- |
| int4 symmetric per 128-input block (`constexpr_blockwise_shift_scale`) | — | 327 MB | Exact for Redux's ternary values, but the Neural Engine compiler ran over 28 minutes without finishing; float16 takes about 7 s. #316's synthetic tests found this op falls back to the CPU. Not usable. |
| Per-row lookup table of each row's unique values (`per_grouped_channel`, group size 1) | 252 MB (3-bit; ≤ 5 values per row) | 585 MB (6-bit; ≤ 17 values per row) | Exact (see below). |

**Exactness:** the lookup-table encoders' outputs are bit-identical to the dense float16 encoders, with maximum
difference 0.0 on both the CPU and the Neural Engine. Accuracy is therefore that of the dense float16 encoder.

Timing, with each model alone on the machine:

| Encoder | Placement | Median per 15 s window | First load | Cached load |
| --- | ---: | ---: | ---: | ---: |
| Phonon-2 dense float16 | 99.92% | 20.3 ms | 11.7 s | 0.05 s |
| Phonon-2 3-bit table | 99.92% | 72.5 ms | 90 s | 0.06 s |
| Redux dense float16 | 99.92% | 21.6 ms | 10.5 s | 0.04 s |
| Redux 6-bit table | 99.92% | 72.6 ms | 88 s | 0.06 s |

The compute plan reports the same placement for the lookup-table encoders, yet they run 3.5× slower. Per-row tables
(group size 1) appear costly on the Neural Engine. Grouping rows would make the tables inexact, because each row has
its own scales.

The value-based selector matched 240 of the 264 projections; the other 24 stayed float16. That is why Redux is
585 MB, not about 450 MB.

## Compared with what ships

| Option | Download (encoder) | Median per 15 s window | WER, 94 held-out | First load | Minimum macOS |
| --- | ---: | ---: | ---: | ---: | --- |
| Parakeet v3, int8 (shipped, #301) | 594 MB | 24 ms | 5.34% | 7 s | 14 |
| Phonon-2, 3-bit table | 252 MB | 72.5 ms | 5.79% | 90 s | 15 |
| Redux, 6-bit table | 585 MB | 72.6 ms | 6.69% | 88 s | 15 |

Only Phonon-2 offers a smaller download, and it costs:
- accuracy (0.45 WER points);
- speed (3×);
- first-load time (90 s, again after each macOS update);
- a higher minimum macOS.

Phonon-2 and Redux keep their packed MLX paths on the Apple GPU (217 MB and 350 MB downloads). The Neural Engine
remains Parakeet v3's engine.

## Limitations

- **Hardware:** one machine.
- **Writer contention and energy:** not measured for these models.
- **Lookup tables:** grouped-channel tables (for example group 16, as in #316's research) were not tried. They would
  not be exact for these models, and would need their own accuracy lanes.
