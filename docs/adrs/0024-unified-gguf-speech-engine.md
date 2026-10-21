# ADR-0024: One GGUF for every device as a Parakeet v3 speech engine

Status: Accepted
Date: 2026-10-22
Related: [#293](https://github.com/rahulsharmavishwakarma/vesper/issues/293), [#287](https://github.com/rahulsharmavishwakarma/vesper/issues/287); extends ADR-0006 and ADR-0021

## Context and approval

Parakeet TDT 0.6B v3 ships as two exports with two runtimes: a 731 MB 4-bit ONNX build for CPU (sherpa-onnx) and an
Apple-GPU build for English (MLX). The owner asked for the same weights on CPU and GPU. NVIDIA publishes a Q8_0 GGUF of
Parakeet v3 that the NeMo-Speech.cpp 0.1.0 runtime, already pinned for Nemotron, runs on CPU, Metal, CUDA and Vulkan.
The owner approved the [proposal](https://github.com/rahulsharmavishwakarma/vesper/issues/293) on 22 October 2026 and
chose to keep one Parakeet model entry with an engine setting whose default stays the current exports until the new
engine qualifies.

## Decision

- `ModelSelection.asr_engine` selects `Standard` (default) or `UnifiedGguf`. Only models in `nemo_gguf_supported`,
  currently Parakeet v3, accept `UnifiedGguf`; validation rejects it elsewhere, and inventory specs reset it for other
  models.
- A new `NemoTdt` registration (`nemo_fastconformer_tdt`, NeMo-Speech.cpp `4f967622`) declares Parakeet's 25 languages
  on every backend, completed audio only, no contextual phrases. Live dictation therefore uses buffered previews; the
  completed-audio final stays authoritative.
- With `UnifiedGguf`, `uses_mlx_speech` is false and Automatic MLX preference does not apply; NeMo's own backend
  resolution chooses the device (Metal on Apple Silicon, CUDA when an NVIDIA GPU is visible, else CPU) and retries a
  failed automatic GPU load once on CPU with the same file.
- Artifacts gain an optional `engine` field. The pinned file is
  `nvidia/parakeet-tdt-0.6b-v3@541d1f99c6b0c3cd0b11a95167540bb8edefd82b` `parakeet-tdt-0.6b-v3.q8_0.gguf`
  (713,975,456 bytes, SHA-256 `e3880d0a…`), marked `engine: nemo_gguf`. NeMo runtime packages are selected for any NeMo
  model instead of being tied to Nemotron's model id.
- The resident NeMo recognizer loads the Parakeet GGUF without a streaming configuration and decodes completed audio
  in parts of at most 30 seconds through the shared planner, the same limit as the ONNX Parakeet path, with automatic
  language detection.

## Research and evidence boundaries

1. Frozen-weight public-API change: a published GGUF and the pinned runtime's offline C ABI. No new runtime, kernel,
   quantization format or training.
2. Runtime-internal work: none. The pinned runtime rejects Parakeet v3 streaming, so no native streaming is claimed.
3. Training/checkpoint changes: none.

Local measurements are in the [qualification record](../validation/parakeet-unified-gguf-2026-10-22.md). They cover
English and German read speech on Apple silicon CPU and Metal; CUDA and Vulkan are packaged but not yet measured.

## Consequences

- One 714 MB download can replace the ONNX and MLX exports once the engine passes the remaining lanes; until then both
  remain and `Standard` is the default.
- Parakeet gains CUDA and Vulkan, and non-English languages on Metal, only through the unified engine.
- Text normalization comes from NeMo-Speech.cpp rather than sherpa-onnx, so raw transcripts can differ in punctuation
  and number formatting.
