# ADR-0025: Parakeet v3's encoder on the Apple Neural Engine as a speech engine

Status: Accepted
Date: 2026-10-22
Related: [#143](https://github.com/rahulsharmavishwakarma/vesper/issues/143), [#300](https://github.com/rahulsharmavishwakarma/vesper/pull/300); extends ADR-0021 and ADR-0024

## Context and approval

Parakeet v3's MLX path runs the encoder and decoder on the Apple GPU. The writing model also runs on the GPU, and
the [Neural Engine record](../validation/ane-coreml-spike-2026-10-22.md) measured how much they slow each other:

- With MLX speech running alongside, the writer generated 61% slower.
- A Core ML float16 encoder on the Neural Engine slowed the writer by 8%.
- That encoder used about a fifth of the MLX encoder's energy per window.
- Its English accuracy matched the MLX path.

Between them they found 99.98% of the encoder's estimated cost scheduled on the Neural Engine.

The owner approved shipping it on 22 October 2026 as another value of ADR-0024's per-model speech engine setting,
built on that setting's plumbing. The approval came after being told the tradeoffs: Apple only, an extra Core ML
file, and the risk of failing the converter's dependency audit.

## Decision

- `ModelSelection.asr_engine` gains `NeuralEngine`. Only models in `neural_engine_supported`, Parakeet v3 in builds
  with the MLX bridge, accept it. Since [ADR-0026](0026-automatic-speech-engine.md), Automatic selects it for English
  and the explicit value is a developer override. Validation rejects it elsewhere. Inventory specs, and the Settings model picker,
  reset it for other models.
- A `NeuralEngineTdt` registration (`parakeet_neural_engine`, MLX-Audio-Swift `d302a5c`) declares English only, the
  qualified language, and Automatic or Metal. Like ADR-0024's engine it is completed-audio only, so live dictation
  uses buffered previews.
- `uses_mlx_speech` is false for this engine and Automatic's MLX preference does not apply. A new
  `uses_mlx_runtime` predicate covers both paths that load through Vesper's MLX bridge.
- The bridge's `parakeet_neural_engine` architecture runs Parakeet's own log-mel frontend and decoder in MLX and the
  encoder through Core ML with `.cpuAndNeuralEngine`.
  - A pinned MLX Audio patch exposes `vesperLogMel` and `vesperDecode`.
  - The encoder takes one fixed 15 s window, so the recognizer delivers parts of at most 15 s instead of 30.
  - Neural Engine output is read through its padded strides.
  - The encoder loads with CPU and Neural Engine, then CPU and GPU, then CPU only. A failed load or a failed probe
    window falls through once, with no extra files.
- Artifacts use `engine: coreml_ane`, `backend: neural_engine` and the directory `neural-engine/parakeet`:
  - a compiled encoder with int8 per-channel weights and float16 compute (`ParakeetEncoder15s.mlmodelc`);
  - a decoder-only MLX checkpoint: decoder, joint and subsampling tensors copied unchanged from the pinned bfloat16
    file, with `encoder.n_layers` 0 so MLX never loads the encoder Core ML replaces.

  They are hosted at `rahulsharmavishwakarma/vesper-parakeet-tdt-0.6b-v3-coreml-ane@4dc405d6` (CC-BY-4.0, with
  attribution and a modification notice). The total download is 640 MB. The owner approved int8 weights on
  22 October 2026 after they matched the float16 encoder within noise at half the size.
- `scripts/convert-parakeet-v3-coreml-ane.py` builds those files.
  - It runs in the locked, audited `coreml-convert` project.
  - It builds the encoder from NVIDIA's Transformers-format checkpoint rather than NeMo, whose dependency stack
    failed the audit.
  - It applies NeMo's input masking and its -10000 attention mask, so the float32 export matches NeMo's encoder at
    every tested length.
- The directory check admits only pinned files at every depth, because Core ML reads the whole compiled model
  directory.
- Installing these files loads the encoder once, so Core ML's per-process specialization (about 7 s) happens during
  the download rather than on the first recording.

## Research and evidence boundaries

1. **Frozen-weight application change.** It uses the published checkpoint, a public Core ML conversion and the
   pinned upstream decoder.
2. **Runtime-internal work: none.** The patch only exposes existing upstream functions; it adds no kernel or
   decoding change.
3. **Training or checkpoint changes: none.**

Local measurements are in the [qualification record](../validation/parakeet-neural-engine-2026-10-22.md) and the
earlier spike record. They cover English read speech on one M5 Pro. Other languages, and M1 to M4 or lower-memory
Macs, are not measured.

## Consequences

- With this engine, Parakeet v3 leaves the GPU to the writing model and uses less energy per window. Its accuracy
  equals the MLX path on recordings up to 15 s.
  - Longer recordings are cut every 15 s instead of every 30 s.
  - The extra cuts cost a few word errors on clips of 15–20 s.
- Users do not choose it: Automatic ([ADR-0026](0026-automatic-speech-engine.md)) runs English Parakeet v3 on it and
  every other language on the unified GGUF, with the other set fetched in the background.
- A macOS update clears Core ML's specialization cache, so the first recording after an update pays that cost again.
- The converter depends on Transformers' Parakeet port and coremltools. Both are pinned, and coremltools 9.0
  requires NumPy 1.
