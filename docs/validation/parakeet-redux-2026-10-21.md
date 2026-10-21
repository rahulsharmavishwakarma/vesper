# parakeet-redux on Vesper's Metal path — 21 October 2026

Question ([#232](https://github.com/rahulsharmavishwakarma/vesper/issues/232)):
should `moondream/parakeet-redux`, a 178 MB ternary student of Parakeet TDT 0.6B
v3, be offered as a small 25-language model?

**Result: no. It is not added.** It is about 14× smaller than the shipped v3 MLX
files, but it was less accurate on clean speech, noise and German. Only very quiet
audio improved.

## Model facts

Checked at `moondream/parakeet-redux@2bf12860`, CC-BY-4.0.

- **Training.** `ternary.json` names `nvidia/parakeet-tdt-0.6b-v3` as teacher.
  Moondream's runtime source calls it a distilled ternary student. The training
  recipe is not published.
- **What is ternary.** 264 encoder modules: per layer, the FFN, attention,
  relative-position and pointwise-conv projections.
- **Storage.**
  - Codes are packed five base-3 digits per byte.
  - There is one F16 scale per output row and per group of 128 inputs:
    `w = scale × (code − 1)`.
  - The subsampler, depthwise conv, decoder, joint and a 213k-parameter VAD head
    stay F16; norms and biases stay F32.
- **MLX fit.** These weights map exactly onto MLX affine 2-bit quantization
  with group size 128: codes {0,1,2}, bias = −scale.
- **Photon runtime.** Moondream's fast kernels (`kestrel-kernels`) are
  proprietary. The published accuracy figures were measured on its CUDA path,
  where weights are expanded to dense.

## Measurement

The same reference code, Python mlx-audio 0.6.1 (`ParakeetRedux` and the
mlx-community v3 files the app ships), ran on an Apple M5 Pro on battery power.

| Set | v3 (MLX) | redux | redux better / worse | Difference CI |
| --- | ---: | ---: | --- | --- |
| FLEURS en-US, 94 held-out clips | 5.05% | 6.50% | 6 / 27 | [0.65, 2.29] |
| Regression, clean (24) | 6.58% | 7.07% | 5 / 8 | [−0.96, 1.78] |
| Regression, +10 dB (24) | 6.58% | 8.55% | 2 / 8 | [0.00, 4.44] |
| Regression, 0 dB (24) | 9.54% | 13.65% | 3 / 14 | [1.39, 7.25] |
| Regression, −30 dB quiet (24) | 57.73% | 23.68% | 15 / 2 | [−50.0, −18.4] |
| FLEURS de-DE, 50 clips | 5.55% | 6.71% | — | — |

Median decode was 51 ms for v3 and 79 ms for redux per clip on Metal.

These results match Moondream's own figures for noise and some European
languages; its card reports MUSAN 6.72% → 9.04% and French 4.81% → 7.71%.

## What adoption would have required

- A converter to the mlx-community layout, with a NeMo-style config and a
  `quantization` block.
- Changes to the pinned MLX-Audio-Swift Parakeet model:
  - quantized pointwise convolutions, or dense copies at about +150 MB;
  - valid-frame normalization and padding masks;
  - an fp16 or fp32 compute dtype to keep the F16 scales exact.

Those changes were not made.

## Follow-up

Redux's large gain on very quiet input suggests v3's preprocessing on the Metal
path degrades at very low levels. The likely cause is normalization over padded
frames, which is a hypothesis and was not isolated. That may deserve its own
issue for the shipped v3.
