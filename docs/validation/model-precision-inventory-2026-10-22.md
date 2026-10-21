# Precision inventory of shipped ONNX speech models, and Cohere pointwise rewrite — 22 October 2026

Part of [#287](https://github.com/rahulsharmavishwakarma/vesper/issues/287). Research evidence only; no application
artifact, catalog entry or default changed.

## Where the bytes are

`scripts/inspect-onnx-precision.py` downloads the files of an install folder from `models/artifacts.json` at their
pinned revisions, checks their hashes, and groups initializer bytes by the operator that consumes them and by element
type (convolutions split into pointwise, depthwise and other).

| Model file | Initializers | Largest groups |
| --- | ---: | --- |
| Cohere Transcribe `encoder.int8.onnx` (+ `.data`) | 2,629 MB | `MatMulInteger` int8 1,675 MB (64%); **pointwise `Conv` float32 945 MB (36%)** |
| Cohere Transcribe `decoder.int8.onnx` | 153 MB | `MatMulInteger` int8 134 MB; `Gather` uint8 18 MB |
| Qwen3-ASR 0.6B `decoder.int8.onnx` | 754 MB | `MatMulInteger` uint8 597 MB; `Gather` uint8 156 MB |
| Qwen3-ASR 0.6B `encoder.int8.onnx` | 177 MB | `MatMulInteger` int8 175 MB |
| Qwen3-ASR 0.6B `conv_frontend.onnx` | 44 MB | `MatMul` float32 28 MB; `Conv` float32 17 MB |
| Whisper Turbo `turbo-encoder.int8.onnx` | 674 MB | `MatMulInteger` int8 642 MB; `Conv` float32 22 MB |
| Whisper Turbo `turbo-decoder.int8.onnx` | 360 MB | **float32 token table through `Identity` 266 MB (74%)**; `MatMulInteger` int8 92 MB |
| Whisper Small `small-encoder.int8.onnx` | 112 MB | `MatMulInteger` int8 99 MB; `Conv` float32 8 MB |
| Whisper Small `small-decoder.int8.onnx` | 260 MB | **float32 token table through `Identity` 159 MB (61%)**; `MatMulInteger` int8 99 MB |

The Parakeet v3 CPU and MLX models are covered in [their](parakeet-conv-rewrite-2026-10-22.md)
[records](parakeet-mlx-quantization-2026-10-22.md). Moonshine ships ONNX Runtime `.ort` files, which this tool does not
read.

## Cohere Transcribe: quantizing the float32 pointwise convolutions

Only the int8 export (`csukuangfj2/sherpa-onnx-cohere-transcribe-14-lang-int8-2026-04-01@156a470c`) is published, so
`scripts/rewrite-onnx-pointwise.py` starts from it. It rewrites the 96 pointwise convolutions as `MatMul` (53 other
convolutions are left as they are) and quantizes only those new nodes, leaving every other tensor as published. Two
runs of the committed tool reproduced the scored files byte for byte.

Scored on the same 100 FLEURS en-US test clips as the Parakeet records (`google/fleurs@70bb2e84`), sherpa-onnx 1.13.8
Python wheel, `from_cohere_transcribe` with explicit English, punctuation and ITN on, 4 threads, Apple M5 Pro CPU;
lower-case, punctuation-free micro WER over 2,166 words; paired clip bootstrap against the shipped encoder:

| Encoder | Size | WER | Difference [95% CI] | Median decode per clip |
| --- | ---: | ---: | --- | ---: |
| Shipped int8 | 2,735 MB | 5.77% | — | 570 ms |
| Rewrite, new nodes dynamic int8 | 2,027 MB | 5.86% | +0.09 [−0.18, +0.40] | 446 ms |
| **Rewrite, new nodes 4-bit weight-only** | **1,938 MB** | **5.77%** | **0.00 [−0.42, +0.37]** | 595 ms |

Both cut 0.7-0.8 GB from the default speech model with WER unchanged within noise on this set. Decode times are from
one pass each and were not repeated.

## Limitations

- English read speech only; Cohere supports 14 languages and none of the others, noise or quiet lanes were scored.
- Python wheel, not the application's sherpa-onnx build or `vesper-eval`.
- 100 clips: differences under about 0.5 points are not significant.

## Next steps

- Qualify a Cohere rewrite build through the application path on the held-out lanes before proposing a repin, as for
  the Parakeet builds (#283, #289).
- Whisper decoders: quantize the float32 token table, which is both the input embedding and the output projection.
