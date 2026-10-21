# parakeet-ultra on Vesper's engines — 21 October 2026

Question ([#233](https://github.com/rahulsharmavishwakarma/vesper/issues/233)):
does `moondream/parakeet-ultra`, a post-trained Parakeet TDT 0.6B v3 with the
same architecture and tokenizer, beat the shipped Parakeet v3 on Vesper's speech
paths?

**Result: no. It is not added.**
- Publisher card: Open ASR 5.80% vs 6.26%; FLEURS-25 9.55% vs 11.62%.
- Through NeMo, ultra was better on these clips.
- On Vesper's two engines it matched or trailed v3:
  - Metal (MLX): 5.10% vs 5.25%, within noise.
  - CPU (sherpa-onnx FP32): 9.27% vs 8.28%.

These are local measurements of read English speech; they do not cover other
languages or the publisher's Photon runtime.

## Conversion (both exact)

- **Source:** `moondream/parakeet-ultra@73175eb7`, CC-BY-4.0, Hugging Face
  Transformers `ParakeetForTDT` tensor names.
- **NeMo:** the official v3 `.nemo` with ultra's tensors.
  - Names were mapped with the inverse of the Transformers `convert_nemo_to_hf.py`
    Parakeet map.
  - 723 tensors mapped and 651 differ from v3. The 6 `vad_head` tensors were
    dropped; there was no shape mismatch.
- **CPU:** the unmodified sherpa-onnx `export_onnx.py@51aaac5`. `tokens.txt` is
  byte-identical to v3's. A v3 export through the same pipeline matched the
  official INT8 within noise (17.98% vs 18.26% on the development clips).
- **Metal:** the `mlx-community/parakeet-tdt-0.6b-v3` layout.
  - The NeMo→MLX mapping was derived by exact value matching: 697 tensors,
    including the combined LSTM biases.
  - Applied to v3, it reproduced the shipped MLX file with zero difference.

Converted files were not published.

## Results

Apple M5 Pro, macOS 27.0.1, battery power, `vesper-eval` release build. The
Metal run used a local, uncommitted pin of the converted file; Metal Parakeet is
English-only.

| Path | Set | v3 | ultra | ultra better / worse |
| --- | --- | ---: | ---: | --- |
| NeMo PyTorch (reference) | 72 development clips | 7.35% | 6.30% | — |
| CPU sherpa FP32 | 72 development clips | 8.28% | 9.27% | — |
| CPU sherpa INT8 | 72 development clips | 18.26% | 16.17% | — |
| Metal MLX FP32 | 94 held-out FLEURS en-US clips | 5.25% | 5.10% | 7 / 8; CI [−0.72, 0.34] |

The NeMo row is pooled from the clean, +10 dB and 0 dB lanes. The published gain
reproduces in NeMo's own front end but not through sherpa-onnx's or
MLX-Audio-Swift's feature extraction. That cause is a hypothesis; the
front-end difference was not isolated.

Median Metal decode was 47–48 ms per clip for both models.
