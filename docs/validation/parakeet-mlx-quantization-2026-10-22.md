# Parakeet v3 MLX (Apple GPU) weight quantization — 22 October 2026

Question ([#287](https://github.com/rahulsharmavishwakarma/vesper/issues/287)): the MLX Parakeet TDT 0.6B v3 artifact that
Automatic selects for English on Apple silicon is a 2.5 GB float32 download. How small can it get with the quantization
methods from the [quantization research](https://github.com/rahulsharmavishwakarma/vesper/issues/265) before accuracy
moves?

**Result (research evidence, not a qualification):** 8-bit affine quantization of the `Linear` layers cuts the weights
from 2,508 MB to **749 MB (−70%)** with the same word errors as float32 on every one of 100 FLEURS English test clips.
Lower bit widths cost about 0.1 to 0.3 points of WER on this set, and sensitivity-guided mixed precision did not beat
uniform 4-bit. Nothing in the application changed.

## Method

Tool: `scripts/quantize-mlx-parakeet.py` in the locked `benchmarks/asr/python/mlx-convert` project (mlx-audio 0.6.1).

- Source: `mlx-community/parakeet-tdt-0.6b-v3@ed2b7e8c`, the artifact pinned in `models/mlx-artifacts.json`.
- Quantized: the 220 `Linear` layers, with MLX affine group quantization. MLX `Conv1d` has no quantized form, so the 72
  one-dimensional convolutions (48 pointwise, 24 depthwise), 5 two-dimensional convolutions, 3 LSTMs and the embedding are
  stored as bfloat16 in every variant except float32. Unquantized quantizable layers are listed as `false` in
  `config.json`, so loaders do not apply the default to them.
- Mixed precision: each encoder `Linear` was fake-quantized to 4-bit on its own and ranked by the mean relative L2 change of
  the encoder output over 16 FLEURS en-US **dev** clips. The top 25% or 10% stayed at 8-bit; decoder and joint layers,
  which that probe cannot see, always stayed at 8-bit.
- Scoring: the first 100 unique 4-20 s sentences of the FLEURS en-US **test** split (`google/fleurs@70bb2e84`, archive and
  metadata hashes checked), the same clips as the [CPU rewrite record](parakeet-conv-rewrite-2026-10-22.md). Python
  mlx-audio `generate`, which computes in bfloat16; lower-case, punctuation-free micro WER over 2,166 words; 2,000-resample
  paired clip bootstrap against float32.
- Machine: Apple M5 Pro, macOS 27.0.1, Metal.

## Results

| Variant | Weights | WER | Difference from float32 [95% CI] |
| --- | ---: | ---: | --- |
| float32 (current artifact) | 2,508 MB | 4.89% | — |
| bfloat16 | 1,254 MB | 4.94% | +0.05 [0.00, +0.14] |
| **8-bit, group 64** | **749 MB** | **4.89%** | **0.00 [0.00, 0.00]** |
| 6-bit, group 64 | 614 MB | 5.03% | +0.14 [−0.09, +0.38] |
| 5-bit, group 64 | 547 MB | 5.08% | +0.18 [−0.27, +0.70] |
| 4-bit, group 64 | 479 MB | 5.12% | +0.23 [−0.20, +0.71] |
| 4-bit, group 32 | 513 MB | 5.12% | +0.23 [−0.19, +0.74] |
| Mixed 4/8-bit, top 25% sensitive at 8-bit | 584 MB | 5.17% | +0.28 [0.00, +0.64] |
| Mixed 4/8-bit, top 10% sensitive at 8-bit | 529 MB | 5.17% | +0.28 [0.00, +0.60] |

Median decode times were 40-68 ms per clip for every variant and moved by up to 40% between runs of the same files on this
machine, so they do not rank the variants. Peak-memory readings did not reset between models and are not reported.

Two smaller findings:

- The encoder-output sensitivity probe ranked the first feed-forward `linear1` of several blocks and the subsampling output
  projection highest, but keeping those at 8-bit did not improve WER over uniform 4-bit. Either the probe metric does not
  track WER, or 100 clips cannot resolve the difference.
- At 4-bit, about 151 MB of the 479 MB are the bfloat16 pointwise convolutions. The pointwise rewrite from the CPU record
  would let those be quantized too, but it changes module types, so the app's Swift loader would need a code change.

## Limitations

- English read speech only; no noise, quiet, other-language or long-audio lanes.
- Python mlx-audio, not the app's Swift mlx-audio-swift loader (`d302a5c6`). That loader reads per-layer quantization from
  `config.json`, but loading these files in the app is untested.
- 100 clips: differences under about 0.5 points are not significant.

## Next steps

1. Load the 8-bit variant through the app's Swift path and run the held-out lanes used for the MLX qualification
   (clean, quiet and noisy English), with latency and memory, before proposing a change to the pinned artifact.
2. Calibrated rounding (GPTQ-style) or learned scales (MLX DWQ-style) for 4-bit, and a WER-driven layer search, before
   concluding that 4-bit cannot match 8-bit.
3. The pointwise rewrite for MLX, only with an approved Swift loader change.
