# Licenses and model terms

## Application code

Vesper's application code is licensed under the [Apache License 2.0](../LICENSE):

Copyright 2026 Rahul Sharma

See [LICENSE](../LICENSE) for the full text. The choice and its rationale are
recorded in [issue #127](https://github.com/rahulsharmavishwakarma/vesper/issues/127).
This is not legal advice.

## Speech models and runtimes use their own terms

Model checkpoints, runtimes and fonts are **not** covered by the license above.
Each follows its publisher's terms. The app downloads them from their recorded
upstream sources; the inventory lives in [models/catalog.json](../models/catalog.json)
and [models/presets.json](../models/presets.json).

| Publisher | Examples in the catalog | Terms |
| --- | --- | --- |
| NVIDIA | Parakeet TDT, Nemotron ASR | Original-weights terms apply; the Parakeet conversion used here is recorded as CC-BY-4.0 with NVIDIA as the original rights holder — attribution is required and preserved in the model profile (see [Hugging Face model workflow](huggingface-models.md)). The Parakeet unified GGUF engine downloads NVIDIA's own CC-BY-4.0 `parakeet-tdt-0.6b-v3.q8_0.gguf` directly from `nvidia/parakeet-tdt-0.6b-v3`; Vesper does not redistribute it |
| Qwen | Qwen3.5, Qwen3-ASR, Qwen2.5 presets | Upstream Qwen license terms apply |
| CohereLabs | Cohere Transcribe | Upstream Cohere terms apply |
| LiquidAI | LFM Audio | Upstream Liquid AI terms apply |
| OpenAI | Whisper Small / Large Turbo | Upstream OpenAI terms apply |
| Moondream | Parakeet Redux | CC-BY-4.0 weights derived from NVIDIA Parakeet TDT 0.6B v3. Vesper's lossless MLX repacking is hosted at `rahulsharmavishwakarma/vesper-parakeet-redux-mlx`, with attribution in its card |
| Fermion Research | Phonon-2 (English) | CC-BY-4.0 weights derived from NVIDIA Parakeet TDT 0.6B v3. The MLX conversion by mlx-community is installed unmodified; the packed-layer design is adapted from fermion-research (Apache-2.0) |
| IBM | Granite Speech 5.0 TurboCTC | Apache-2.0; files are installed unmodified from IBM's repository |
| Moonshine AI | Moonshine Base (English) | MIT for the English model. Non-English Moonshine models use the Moonshine Community License and are not offered |
| Google | Gemma 3 1B (`ggml-org/gemma-3-1b-it-GGUF`, from `google/gemma-3-1b-it`) | [Gemma Terms of Use](https://ai.google.dev/gemma/terms) (last modified 1 April 2026), including its Prohibited Use Policy. The app downloads the file directly from Hugging Face and does not redistribute it. Anyone who redistributes Gemma or a modified copy must pass on the use restrictions and a copy of the terms, mark modified files, and include a `NOTICE` file stating "Gemma is provided under and subject to the Gemma Terms of Use found at ai.google.dev/gemma/terms" (section 3.1) |
| Microsoft | VibeVoice ASR checkpoints (`microsoft/VibeVoice-ASR`, `-ASR-BitNet`, `-ASR-Streaming-1.5B`) | MIT according to each model card's license field (checked 22 October 2026). These are used only by the optional benchmark adapters under `benchmarks/asr/`, not offered in the app catalog |
| HuggingFaceTB | SmolLM2 presets | Upstream SmolLM terms apply |

Before redistributing any checkpoint or bundling weights with the app, confirm
the current upstream terms for that exact artifact — model terms change
independently of this repository.

## Fonts and native runtimes

Bundled fonts and the pinned native runtimes (sherpa-onnx, NeMo speech builds)
follow their own licenses. The bundled
`apps/desktop/src/assets/bricolage-grotesque.ttf` is Bricolage Grotesque from
[ateliertriay/bricolage](https://github.com/ateliertriay/bricolage), licensed under
the SIL Open Font License 1.1; the license text ships beside it as
`apps/desktop/src/assets/bricolage-OFL.txt`, as the OFL requires.
