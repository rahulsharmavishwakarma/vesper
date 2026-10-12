# Licenses and model terms

## Application code

Vesper's application code is licensed under the [Apache License 2.0](../../LICENSE):

Copyright 2026 Rahul Sharma

See [LICENSE](../../LICENSE) for the full text. The choice and its rationale are
recorded in [issue #127](https://github.com/rahulsharmavishwakarma/vesper/issues/127).
This is not legal advice.

## Speech models and runtimes use their own terms

Model checkpoints, runtimes and fonts are **not** covered by the license above.
Each follows its publisher's terms. The app downloads them from their recorded
upstream sources; the inventory lives in [models/catalog.json](../../models/catalog.json)
and [models/presets.json](../../models/presets.json).

| Publisher | Examples in the catalog | Terms |
| --- | --- | --- |
| NVIDIA | Parakeet TDT, Nemotron ASR | Original-weights terms apply; the Parakeet conversion used here is recorded as CC-BY-4.0 with NVIDIA as the original rights holder — attribution is required and preserved in the model profile (see [Hugging Face model workflow](huggingface-models.md)) |
| Qwen | Qwen3.5, Qwen3-ASR, Qwen2.5 presets | Upstream Qwen license terms apply |
| CohereLabs | Cohere Transcribe | Upstream Cohere terms apply |
| LiquidAI | LFM Audio | Upstream Liquid AI terms apply |
| OpenAI | Whisper Small / Large Turbo | Upstream OpenAI terms apply |
| Google | Gemma 3 presets | Upstream Gemma terms apply |
| HuggingFaceTB | SmolLM2 presets | Upstream SmolLM terms apply |

Before redistributing any checkpoint or bundling weights with the app, confirm
the current upstream terms for that exact artifact — model terms change
independently of this repository.

## Fonts and native runtimes

Bundled fonts and the pinned native runtimes (sherpa-onnx, NeMo speech builds)
follow their own licenses. The tracked `inter.ttf` ships with unverified terms
and must be resolved before the repository goes public
([issue #127](https://github.com/rahulsharmavishwakarma/vesper/issues/127)).
