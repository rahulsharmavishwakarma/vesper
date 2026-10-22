# ADR-0026: Pick the speech engine automatically

Status: Accepted
Date: 2026-10-22
Related: [#293](https://github.com/rahulsharmavishwakarma/vesper/issues/293), [#143](https://github.com/rahulsharmavishwakarma/vesper/issues/143); amends ADR-0024

## Context and approval

ADR-0024 added a visible per-model **Speech engine** setting so Parakeet v3 could run as one GGUF on every device while
the current ONNX and MLX exports stayed the default. After trying the setting in the application, the owner asked why
users are offered engine choices such as GGUF or the Neural Engine at all, and approved on 22 October 2026 hiding the
choice behind an Automatic engine that picks the best measured path for the device and the files installed. The owner
also chose to download the set the language needs first and fetch the other set in the background.

## Decision

- `ModelSelection.asr_engine` gains `Automatic`, now the default. `Standard` and `UnifiedGguf` remain only as developer
  and support overrides (configuration file, `vesper-eval --asr-engine`, the installer example); Settings shows no
  engine control.
- Automatic resolves to the unified GGUF for models in `nemo_gguf_supported` (currently Parakeet v3) and to the existing
  adapters for every other model. NeMo-Speech.cpp then picks Metal, CUDA or CPU and retries a failed GPU load once on CPU
  with the same file (ADR-0024).
- Until the GGUF is installed, Automatic keeps using installed current-engine files, first on the configured backend and
  then on CPU, and reports the fallback reason instead of failing.
- Inventory specs reset an engine override the model does not support back to `Automatic`.
- The Apple Neural Engine path ([#143](https://github.com/rahulsharmavishwakarma/vesper/issues/143),
  [ADR-0025](0025-neural-engine-speech-engine.md), PR #301) joins this order on Apple silicon ahead of the GGUF.
  - **When it applies:** `effective_asr_engine` resolves Automatic to `NeuralEngine` for Parakeet v3 in English with
    Automatic or Metal processing, in MLX builds.
  - **Downgrade:** selection downgrades to the GGUF where the MLX bridge does not load.
  - **Missing files:** if the Neural Engine files are missing, Automatic uses the installed GGUF, then the standard
    files.
  - **Failed load:** a failed Neural Engine load retries once on the installed GGUF.
  - **Background fetch:** after the selected set installs, the other set (`companion_artifacts`) downloads in the
    background, outside the operation slot.

## Research and evidence boundaries

Application-only change to engine selection; no runtime, decoder, quantization or checkpoint change. The accuracy and
latency evidence for the GGUF is ADR-0024's [qualification record](../validation/parakeet-unified-gguf-2026-10-22.md).

## Consequences

- Users with only the current Parakeet files keep working; the GGUF becomes the Parakeet path once installed.
- Remaining GGUF qualification gaps (23 languages, long audio, CUDA and Vulkan) now apply to the default path rather than
  an opt-in, and are tracked on #293 and #287.
- New engines are added as entries in the Automatic order, not as Settings options.
