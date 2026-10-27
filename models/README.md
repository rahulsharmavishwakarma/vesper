# Models

The native app downloads any supported catalog entry only on explicit request; downloading an unselected model does not change settings. No weights are committed. [artifacts.json](artifacts.json) is the built-in installation manifest; [presets.json](presets.json) supplies additional pinned profiles to the model menus. [catalog.json](catalog.json) is the display source of truth for picker names, creators, blurbs, and offer status, plus research metadata for the original built-ins; executable installation and runtime selection stay implemented in vesper-models and vesper-inference.

The menus contain ten ASR and six cleanup choices. Additional options include
Qwen3-ASR, Cohere, Parakeet v2, Parakeet Unified, FastConformer, Qwen2.5, SmolLM2, and Gemma 3.
See [preset files, compatibility evidence, and validation limits](../docs/model-presets.md).

| Role                                   | Installed runtime               | Files / status                                                                                  |
| -------------------------------------- | ------------------------------- | ----------------------------------------------------------------------------------------------- |
| NVIDIA Nemotron 3.5 Streaming ASR 0.6B | NeMo-Speech.cpp 0.1.0           | Official Q8 GGUF, 742 MB; pinned CPU/Metal/CUDA/Vulkan runtime selected per platform            |
| NVIDIA Parakeet-TDT-0.6B-v3 (fallback) | sherpa-onnx 1.13.8              | INT8 encoder, decoder, joiner, vocabulary; approximately 670 MB                                 |
| Qwen3-ASR 0.6B                         | Patched sherpa-onnx 1.13.8, CPU | Six pinned files, 987,015,347 bytes; long audio processed in segments                           |
| Cohere Transcribe (default)            | Patched sherpa-onnx 1.13.8, CPU | Four pinned files, 2,888,052,036 bytes; long audio processed in segments; select English/French |
| Liquid LFM2.5 Audio 1.5B               | llama-cpp-2/mtmd 0.1.156, CPU   | Q4 backbone and projector, 915,262,016 bytes; English-only completed audio; resident weights    |
| Qwen3.5-0.8B cleanup                   | llama-cpp-2 0.1.156             | Q4_0 563 MB by default; Q8_0 834 MB selectable                                                  |

Nemotron repository: [nvidia/nemotron-3.5-asr-streaming-0.6b](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b), pinned revision `ea30d66debe3740a08b573244286791d423d6b3e`, OpenMDW-1.1. Its Q8 GGUF is loaded by NVIDIA's [NeMo-Speech.cpp](https://github.com/NVIDIA/NeMo-Speech.cpp) release 0.1.0. The current app uses the versioned C ABI with a resident recognizer, fresh per-recording streams for live captions, and an offline path for imported files.

Parakeet export repository: [csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8](https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8), pinned revision `2bda32ec70b097a55adaa07d9a7173915b43cc78`. Original [NVIDIA weights](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3) use CC-BY-4.0; preserve attribution when redistributing. Parakeet supports English and its European-language set, not Hindi; `auto` does not add unsupported languages.

Qwen export repository: [ggml-org/Qwen3.5-0.8B-GGUF](https://huggingface.co/ggml-org/Qwen3.5-0.8B-GGUF), pinned revision `8fea620810c4afa23dd6443f999a48574c1611a3`, Apache-2.0. Text cleanup requires only `Qwen3.5-0.8B-Q4_0.gguf` or `Qwen3.5-0.8B-Q8_0.gguf`; no multimodal projector. The exact two-message text-only [chat template](https://huggingface.co/Qwen/Qwen3.5-0.8B/blob/main/chat_template.jinja) runs with thinking closed. Download size is not total application RAM.

Expected directory layout:

```text
<model-root>/
  nemotron-3.5-asr-streaming-0.6b/
    nemotron-3.5-asr-streaming-0.6b.q8_0.gguf
  nemo-speech-runtime/0.1.0/nemo-speech/
    bin/nemo-speech[.exe]
  parakeet-tdt-0.6b-v3/
    encoder.int8.onnx
    decoder.int8.onnx
    joiner.int8.onnx
    tokens.txt
  qwen3.5-0.8b/
    Qwen3.5-0.8B-Q4_0.gguf
```

The Models screen lists each ASR model and both Qwen quantizations with independent status and download controls. See the [language and recommendation matrix](../docs/language-support-and-recommendations.md) for the exact picker choices and qualification gaps. The browser preview shows the same catalog without native actions. Install the selected pipeline from the desktop app, or explicitly from this repository:

```sh
source scripts/dev-env.sh
cargo run -p vesper-models --example install -- models/downloads
```

Downloads are staged and hash-checked before replacement. Cancelled/failed staged files are removed. Subsequent installs verify existing files and fetch only missing or damaged ones. Full integrity checks also run before inference. The installer does not upload audio or transcripts. `modelDirectory` overrides the app-data default; `VESPER_MODEL_DIR` supplies an optional development default. The development helper selects existing `models/downloads` automatically.

No Python or model server is required. Nemotron runs through the local C ABI; cancellation is checked around native operations and streaming chunks. `Auto` chooses Metal on Apple Silicon, CUDA on Linux/Windows when `nvidia-smi` reports a visible GPU, and CPU otherwise. Explicit CPU, Metal, CUDA, and Vulkan packages are pinned where NVIDIA publishes them. Build the app with `--features cuda` to accelerate Qwen as well as ASR. The optional ROCm application build accelerates Qwen through llama.cpp while Nemotron uses Vulkan on AMD. MLX is a different Apple framework and is not used by this adapter. Qwen Q4_0 was smoke-tested; Q8_0 is pinned/selectable but has not been quality-tested. See [validation](../docs/validation.md) and [inference performance](../docs/inference-performance.md).

## Whisper evaluation support

Small and Turbo share one native CPU adapter and pinned INT8 installation entries in `artifacts.json` (375,485,327 and 1,036,613,791 bytes respectively). They can run through `vesper-eval` and the existing installer, but are not selectable product presets while noise/repetition qualification fails. This does not change the eight selectable ASR choices or the Cohere/Qwen default. English/French completed audio is the current evaluation contract; Hindi, code-switching, singing and live Whisper streaming are unqualified. See [the adapter design](../docs/adrs/0018-shared-whisper-adapter.md) and [validation](../docs/validation/whisper-adapter-2026-09-18.md).
