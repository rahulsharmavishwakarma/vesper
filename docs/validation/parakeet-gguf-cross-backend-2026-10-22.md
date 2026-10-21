# Parakeet v3 as one GGUF for CPU and GPU — 22 October 2026

Part of [#287](https://github.com/rahulsharmavishwakarma/vesper/issues/287). The owner asked for the same weights on
CPU and GPU. Today Parakeet TDT 0.6B v3 ships as two downloads: a 731 MB 4-bit ONNX build for CPU (sherpa-onnx) and a
2,509 MB float32 MLX build for Apple GPU. Research evidence only; the application is unchanged.

**Result:** NVIDIA's official Q8_0 GGUF (714 MB), run by the NeMo-Speech.cpp runtime the app already bundles for
Nemotron, produced the same transcripts on CPU and Metal for 99 of 100 clips, with 4.94% and 4.89% WER. One file would
replace both downloads; the same runtime ships CPU, CUDA and Vulkan packages for Windows and Linux, which were not run
here. Smaller K-quant files lost accuracy and were not faster, so Q8_0 is the candidate.

## Method

- Runtime: the pinned NeMo-Speech.cpp 0.1.0 package (`nemo-speech-0.1.0-macos-aarch64-metal`, source `4f967622`, from
  `models/artifacts.json`), whose `nemo-speech transcribe` command indexes `nvidia/parakeet-tdt-0.6b-v3` and selects
  `--device cpu` or `--device metal` for the same file. Default punctuation and text normalization; one utterance at a
  time.
- Official file: `nvidia/parakeet-tdt-0.6b-v3@541d1f99`, `parakeet-tdt-0.6b-v3.q8_0.gguf`, 713,975,456 bytes, size and
  SHA-256 verified by `nemo-speech pull`. Q8_0 covers 224 linear and 48 pointwise-convolution tensors; ggml treats a 1x1
  convolution as a matrix multiply, so no graph rewrite is needed.
- Own conversions: the pinned converter (`convert_model.py` at `4f967622`) from the same revision's `.nemo` checkpoint, in
  the locked `benchmarks/asr/python/nemo-convert` project. That project uses `gguf` 0.19.0 and PyTorch 2.14.1 instead of
  the converter's `torch~=2.6.0` pin, which has known advisories; `make audit-python-nemo-convert` reports none. K-quants
  use the runtime's own `libggml-base` (`NEMO_SPEECH_GGML_BASE`); 6 tensors whose inner dimension is not a multiple of 256
  fall back to F16, as the runtime's documentation describes.
- Audio: the same 100 FLEURS en-US test clips as the other records (`google/fleurs@70bb2e84`), written as 16 kHz mono
  16-bit WAV. Lower-case, punctuation-free micro WER over 2,166 words; paired clip bootstrap against the official Q8_0
  on CPU.
- Machine: Apple M5 Pro, macOS 27.0.1.

## Results

| File | Size | WER, CPU | WER, Metal | vs official Q8_0 on CPU [95% CI] | Clips identical to it | 100 clips, CPU / Metal |
| --- | ---: | ---: | ---: | --- | ---: | ---: |
| Official Q8_0 | 714 MB | 4.94% | 4.89% | — | — | 29.3 s / 6.6 s |
| Converted Q8_0 | 714 MB | 4.94% | 4.89% | 0.00 [0.00, 0.00] | 100 / 99 | 29.9 s / 6.6 s |
| Q6_K | 578 MB | 5.22% | 5.22% | +0.28 [+0.09, +0.56] | 95 / 95 | 31.5 s / 6.7 s |
| Q5_K | 498 MB | 5.08% | 5.08% | +0.14 [−0.18, +0.49] | 90 / 90 | 34.5 s / 7.2 s |
| Q4_K | 421 MB | 5.40% | 5.36% | +0.46 [−0.05, +1.00] | 79 / 79 | 34.0 s / 6.7 s |

Times are wall time for all 100 clips including model load, one pass each. The converted Q8_0 is not byte-identical to
NVIDIA's file (699 against 701 tensors; NVIDIA's metadata names an `oss-align` build) but produced the same transcripts.
The K-quant ordering is not monotonic (Q5_K beat Q6_K), so differences of a few words are noise at this sample size;
only Q6_K's loss excludes zero.

For comparison on the same clips: the 2,509 MB float32 MLX build scored 4.89% in Python mlx-audio, and the current
731 MB 4-bit ONNX build 5.91% in the sherpa-onnx Python wheel ([MLX record](parakeet-mlx-quantization-2026-10-22.md),
[CPU record](parakeet-conv-rewrite-2026-10-22.md)). Different runtimes normalize text differently, so these cross-runtime
numbers are indicative only.

## Limitations

- English read speech only; the other 24 Parakeet languages, noise, quiet speech and long audio were not scored.
- CUDA and Vulkan packages were not run; that needs Windows or Linux hardware (or a cloud GPU).
- Command-line runtime, not an application adapter. Moving Parakeet to NeMo-Speech.cpp changes which runtime owns the
  model and needs an approved proposal before any application change.
