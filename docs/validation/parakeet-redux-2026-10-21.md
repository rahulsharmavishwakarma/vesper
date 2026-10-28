# parakeet-redux on Vesper's Metal path — 21–22 October 2026

Question ([#232](https://github.com/rahulsharmavishwakarma/vesper/issues/232)):
should `moondream/parakeet-redux`, a 178 MB ternary student of Parakeet TDT 0.6B
v3, be offered as a small 25-language model?

**Result (22 October 2026): added as an optional Apple-GPU-only model, by owner
decision, for its size.** It is never a default.

It is the smallest multilingual option (~217 MB download, 0.94 GB GPU memory,
against ~2.5 GB and 3.80 GB for Parakeet v3 on Metal). It is measurably less
accurate than v3 on clean English and much less accurate in 0 dB noise. The
original 21 October evaluation recommended against adding it; that evidence
follows the application-path results below and is kept as history.

## Application-path results (22 October)

Measured through `vesper-eval` on Metal on an M5 Pro, on battery, against
Parakeet v3 on the same lanes.

| Lane                                                | Parakeet v3 | Redux  | Redux − v3 (95% CI) |
| --------------------------------------------------- | ----------- | ------ | ------------------- |
| FLEURS en-US held-out, 94 clips                     | 5.25%       | 6.35%  | [+0.36, +1.87]      |
| FLEURS en-US unseen, 100 clips                      | 5.68%       | 6.71%  | [+0.13, +2.03]      |
| Natural clean, 24                                   | 6.25%       | 6.91%  | [−0.96, +2.14]      |
| Natural +10 dB SNR, 24                              | 6.74%       | 8.22%  | [−0.36, +3.65]      |
| Natural 0 dB SNR, 24                                | 9.21%       | 13.32% | [+1.45, +7.37]      |
| Quiet (−30 dB), 24                                  | 6.91%       | 8.22%  | [0.00, +2.77]       |
| Joined FLEURS long-form, 6 × ~95 s                  | 10.50%      | 8.97%  | [−4.56, +0.76]      |
| FLEURS de-DE, 50 clips (explicit `de` or automatic) | 6.62%¹      | 7.16%  | [−0.84, +1.95]      |

¹ Parakeet v3's 4-bit CPU export: its Metal path is English-only.

| Resource (FLEURS en-US, Metal) | Parakeet v3 MLX | Redux      |
| ------------------------------ | --------------- | ---------- |
| Median / p95 ASR time per clip | 48 / 71 ms      | 51 / 80 ms |
| Peak MLX allocator memory      | 3.80 GB         | 0.94 GB    |
| Download                       | ~2.5 GB         | ~217 MB    |

The unseen set and the long-form lane are described in the
[Phonon-2 record](phonon2-2026-10-21.md#environment) and the
[Granite record](granite-speech5-turboctc-2026-10-21.md#environment).

## Adapter (22 October)

1. **Conversion.**
   - `scripts/convert-parakeet-redux-mlx.py` runs in the locked
     `benchmarks/asr/python/mlx-convert` project with mlx-audio 0.6.1. It
     repacks Moondream's base-3 codes into MLX affine 2-bit words (bias =
     −scale) without requantizing, keeps the fp16 scales, and drops the VAD head.
   - It writes the pinned `mlx-community/parakeet-tdt-0.6b-v3@ed2b7e8c` config,
     which has the same vocabulary, durations and features, with dither 0 and
     the ternary module list.
   - Two runs produce identical bytes. The output is hosted, unmodified, at
     [rahulsharmavishwakarma/vesper-parakeet-redux-mlx@47de12f2](https://huggingface.co/rahulsharmavishwakarma/vesper-parakeet-redux-mlx/tree/47de12f261ab05fe19dd211c0c98812eab46139e):
     `config.json` 351,148 bytes, `8461d141…9bda`; `model.safetensors`
     216,875,637 bytes, `40f4451e…d1cf`.
   - Loaded back into mlx-audio, the converted files reproduce the reference
     word for word.
2. **Swift** (`native/mlx/Sources/VesperMLX/ParakeetRedux.swift`).
   - The pinned Parakeet load hook replaces the 264 encoder projections with
     one native 2-bit matmul each, using float32 scales.
   - The model runs in float32, as mlx-audio runs Redux.
3. **Parity.** On three clips (synthetic speech and two FLEURS clips) the Swift
   model produces the same text as mlx-audio's `ParakeetRedux` in evaluation
   mode on Moondream's original files. Its ternary projections agree to about
   1e-6 relative on identical input.
4. **Known deviations from the reference.** Neither changed any parity output.
   - The upstream Swift graph does not apply Redux's valid-frame normalization
     and padding masks, which affect one trailing frame.
   - It uses the standard TDT loop rather than Photon's bounded loop.
5. **Packaging.** Apple GPU only (Automatic or Metal), with v3's 25 languages
   and automatic detection, in 30 s parts.

A measurement note: building mlx-audio's `ParakeetRedux` directly leaves it in
training mode, so BatchNorm uses batch statistics. Reference outputs must call
`eval()`, as `mlx_audio`'s own loader does.

## 21 October evaluation (historical)

The rest of this record is the original evaluation. Its quiet-lane figures are
superseded, as noted below.

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

| Set                             | v3 (MLX) |   redux | redux better / worse | Difference CI  |
| ------------------------------- | -------: | ------: | -------------------- | -------------- |
| FLEURS en-US, 94 held-out clips |    5.05% |   6.50% | 6 / 27               | [0.65, 2.29]   |
| Regression, clean (24)          |    6.58% |   7.07% | 5 / 8                | [−0.96, 1.78]  |
| Regression, +10 dB (24)         |    6.58% |   8.55% | 2 / 8                | [0.00, 4.44]   |
| Regression, 0 dB (24)           |    9.54% |  13.65% | 3 / 14               | [1.39, 7.25]   |
| Regression, −30 dB quiet (24)   |  57.73%² | 23.68%² | 15 / 2               | [−50.0, −18.4] |
| FLEURS de-DE, 50 clips          |    5.55% |   6.71% | —                    | —              |

Median decode was 51 ms for v3 and 79 ms for redux per clip on Metal.

² Superseded. These runs loaded audio through `load_audio`, which quantizes to
16 bits; on −30 dB speech that alone degrades both models. Through the
application on float input, the quiet lane is 6.91% for v3 and 8.22% for Redux.

These results match Moondream's own figures for noise and some European
languages; its card reports MUSAN 6.72% → 9.04% and French 4.81% → 7.71%.

## What adoption would have required (as assessed on 21 October)

- A converter to the mlx-community layout, with a NeMo-style config and a
  `quantization` block.
- Changes to the pinned MLX-Audio-Swift Parakeet model:
  - quantized pointwise convolutions, or dense copies at about +150 MB;
  - valid-frame normalization and padding masks;
  - an fp16 or fp32 compute dtype to keep the F16 scales exact.

Those changes were not made.

## Follow-up (resolved)

The 21 October record suggested that v3's preprocessing degraded on very quiet
input. That was the 16-bit audio loading in the Python runs, not the app: v3
scores 6.91% on the quiet lane through the application.
