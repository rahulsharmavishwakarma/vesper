# transcribe

A local macOS desktop dictation application with a **Rust backend, Svelte 5 frontend, multiple native speech adapters and Qwen3.5-0.8B cleanup**. No Python runtime or cloud inference service is required.

**Status:** working native inference integration. Cohere Transcribe is the default completed-recording ASR, paired with Qwen3.5 writing cleanup. New settings select English; the current Cohere adapter also supports explicit French, but not Auto/Hindi or live partials. Nemotron remains selectable for multilingual ASR and live partial captions, using a resident NeMo-Speech.cpp recognizer with a fresh stream per recording. Qwen weights remain resident while every cleanup gets a fresh context, and Parakeet remains the offline fallback. macOS quick capture supports Hold/Toggle and opt-in final-text insertion, with Copy fallback; external-app qualification is tracked in #4. Model training and automatic endpointing remain future work.

[LFM2.5 Audio 1.5B](docs/lfm-audio.md) is an optional English-only CPU model for completed recordings. Its Q4 backbone and audio projector total about 915 MB; weights stay resident between requests.

Choose speech and writing models from the [curated Models menus](docs/model-presets.md). Built-in adapters cover Nemotron through NeMo-Speech.cpp, NeMo transducer ONNX bundles, Qwen3-ASR and Cohere ONNX exports, LFM2.5 Audio through llama.cpp/mtmd, and instruction GGUF writing models; arbitrary Hugging Face architectures still need an architecture-specific adapter. Python is not required for these paths.

The Models menus now include [eight ASR and six cleanup choices](docs/model-presets.md),
including [Qwen3-ASR and Cohere](docs/offline-asr.md) for completed recordings,
with pinned presets for additional Parakeet/FastConformer exports, Qwen2.5,
SmolLM2, and Gemma 3. Choose a model, then save or explicitly download it. New
preset metadata and runtime contracts are checked; their inference quality has
not been benchmarked in this application.

Transcription has [no application time quota](docs/adrs/0011-segment-long-generative-asr.md). Qwen3-ASR and Cohere process longer audio in internal segments. Available memory, model context and recognition quality still apply.

## Download for Mac

The release download is an Apple-silicon **DMG**. Open it and drag `transcribe.app`
to Applications. The stable website link is
[Download for Mac](https://github.com/rahulsharmavishwakarma/transcribe/releases/latest/download/transcribe-macos-arm64.dmg).
It becomes available after the first stable release succeeds and the repository
is public. Developer ID signing/notarization is not yet configured; see the
[release setup and current limits](docs/ci-and-releases.md#release-workflow-and-download-url).

## Run

From this directory:

```sh
make setup
make dev
```

The Makefile loads `scripts/dev-env.sh` automatically, using the project-local Rust/CMake installations when present. Setup installs locked npm dependencies and fetches Rust crates; install the platform prerequisites separately. If `models/downloads` exists, the helper also selects those weights as the development model directory. A saved model-directory preference takes precedence. Otherwise use **Available downloads** in the app or `make models`. Default Cohere INT8 and Qwen Q4 model files total approximately 3.45 GB; every file is verified against a pinned SHA-256 hash.

`make web` opens a browser preview with isolated settings and disabled native actions. Use the Tauri application for recording, model installation, transcription, and datasets. Run `make help` for all targets. Override the download directory with `make models MODEL_DIR="/path/to/models"`, then select that folder in the app. The existing npm and Cargo commands remain available. See [development prerequisites](docs/development.md).

The current desktop CI and release target is macOS on Apple silicon. Browser
regression tests run through Vite. An existing Docker/Nginx preview is an optional
manual development tool; it is not needed to build or run the Mac application and
is never automatically published. See [CI and releases](docs/ci-and-releases.md).

## Use

1. In **Models**, select **Speech to text** (ASR) and **Text cleanup** (rewriter) independently. Use the download button under each dropdown. **Save & download** applies pending settings before fetching that model; cleanup precision selects the exact Q4 or Q8 file. You can also install the complete selected pipeline. Device acceleration, cleanup precision and the model folder are managed here. **Dictate** exposes the model-aware transcription language selector; **Settings** also contains it with recording, writing and capture preferences.
2. Choose a microphone and press **Record**, then **Stop & transcribe**, or select an audio file. You can also use the configured global shortcut in Hold or Toggle mode; the compact capture strip follows the real microphone level and processing state. There is no general recording time quota.
3. Compare raw ASR text with cleanup output. Formatting-only cleanup is the default and preserves the normalized word sequence. Context correction can run a guarded technical-only pass over the final 24 words before cleaning the complete transcript; it needs review and can add latency. Rejected or failed cleanup retains the raw text.
4. In **History**, view 7-day/30-day recording activity, transcript word totals, reviewed counts and stored audio size. The daily line graph filters recordings when you select a point. See active days, average words per transcript, and counts with or without saved text. Search transcript text or filenames, filter by review status, and expand a recording inline to read or copy its transcript. Browse 10, 25 or 50 recordings per page with Previous/Next; search and statistics still cover the full history. Open **Review and remember** to review exact speech or edited wording, then explicitly remember a recurring correction. Review verbatim speech for ASR training and edited wording separately for cleanup training. Expand **Export training data** to export reviewed examples or a complete archive. Search filters do not limit exports.

Original imported files are copied unchanged. Microphone audio is stored as native-rate/channel float WAV before resampling. Raw predictions, cleanup, settings, model artifact revisions/hashes, processing events, and human revision history are separate. Collection can be disabled; transcription then runs without dataset archiving. Nothing is uploaded.

Quick capture has two separate shortcuts while the desktop app is running: **Alt/Option + Shift + D** for Hold to talk (release to finish), and **⌘/Ctrl + Shift + Space** for Press to toggle. Change either in **Settings → Quick capture**; both remain available. Existing users keep their saved shortcut for its previous action. Wait for **Listening** before speaking, especially while the streaming model loads for the first recording. On macOS, choose **Paste into the original text field** and enable the requested Accessibility/clipboard access, then save. Focus a supported text field before recording. If focus changes or automatic paste is unavailable, the floating bar keeps a **Copy** action. Existing output preferences are preserved. **Capture design** offers Split (default), Ribbon, Underline, Studio, Margin and Counter, with a preview before saving. Every design uses the same voice-reactive waveform. Drag the waveform or panel background to move the floating controls; macOS remembers that position between recordings while the app stays running. Copy confirmation appears in the existing status line, and Split keeps longer feedback inside its waveform panel. Settings can also hide the floating strip while keeping shortcut capture active and enable privacy-safe completion/failure notifications. Notification bodies never contain transcript text.

On macOS, click **transcribe** in the menu bar for recording status, Start/Stop, History, Models, Settings and Quit. Closing the window keeps the app available there and preserves active work. Finish or cancel an operation before quitting.

**Personal memory stays on device and survives model changes.** In Settings → Your vocabulary and writing, manage spellings, recurring mishearings, project/context scope and writing preferences. Confirmed corrections work even with text cleanup off; raw output stays available and remembered edits can be undone. History → Learning on this device shows useful reviews, actual uses/undos and comparisons against later reviewed speech, including regressions. Output-form suggestions are based on remembered examples; automatic use is optional.

The app automatically learns eligible scoped spellings from ordinary edits in **Your text** for subsequent recordings, using its existing background workers. Edits save on leaving the field or copying; recurring corrections need no extra Remember action. Usage helps rank existing vocabulary, while possible retries remain uncertain evidence. Inspect, pause, undo or forget automatic memory locally. Learning needs no enable step; broader personal benefit still needs qualification. See the [detailed personalization guide](docs/personalization/README.md) and [measured results and limits](benchmarks/asr/2026-09-16-automatic-learning.md).

The improvement setting still stores a future model-promotion policy. **No cloud learning API, weight training or new-model activation is introduced.** Acoustic adaptation and accent detection remain deferred until an own-model training project. See the [local memory decision](docs/adrs/0015-model-independent-local-memory.md), [continuous improvement contract](docs/self-improvement.md), and [recent research with implementation boundaries](docs/research-personal-dictation.md).

## Structure

```text
apps/desktop/                Svelte / TypeScript / Vite frontend
apps/desktop/src-tauri/      Tauri commands, settings, job lifecycle, dataset integration
crates/transcribe-core/      Shared configuration and types
crates/transcribe-audio/     CPAL capture, Symphonia decoding, filtered 16 kHz resampling
crates/transcribe-inference/ Nemotron, Parakeet, Qwen3-ASR, Cohere, LFM and cleanup adapters
crates/transcribe-nemo/      Safe Rust boundary over the NeMo-Speech.cpp C ABI
crates/transcribe-models/    Pinned artifacts, downloads, integrity checks, provenance
crates/transcribe-data/      SQLite, original audio, reviews, JSONL exports
models/artifacts.json        Runtime artifact revisions, sizes, hashes, and sources
configs/                     Reference configuration
prompts/                     Cleanup policy
scripts/                     Development helpers
benchmarks/asr/              Reproducible ASR scoring and recommendation inputs
```

## Validate and build

```sh
make check
make smoke-native           # Explicit installed-model checks; requires the public speech fixture
node --test scripts/*benchmark*.test.mjs # Corpus and aggregation contracts
make build-debug
make build                  # Optimized release desktop app
make version-check          # Cargo, npm, lockfile and Tauri versions agree
```

`make check` runs Rust/Python/frontend/shell formatting and linting, Svelte/TypeScript checks, Node and Python tooling tests, mocked-native Chromium regressions, native reporting tests, all Rust target tests and doctests, and strict Clippy/rustdoc. First run `make setup-checks` with Python 3.12+ to install the development-only tools and browser. `make hooks` installs pre-commit and pre-push validation. `make test` and `make lint` also build the frontend assets required by Tauri. `make fmt` formats Rust, Python, frontend and shell source; `make fmt-check` verifies formatting without edits. The debug macOS build creates `target/debug/bundle/macos/transcribe.app`; release distribution still needs signing/notarization. Unit tests do not open the microphone. A separate explicit smoke test exercises actual installed models and dataset export; see [validation](docs/validation.md).

Real-model comparisons use the [ASR qualification benchmark](benchmarks/asr/README.md). It pins public held-out data, runs the actual native adapters in offline or real-time streaming mode, records artifact provenance and latency definitions, and produces evidence-backed recommendations. The [language and recommendation matrix](docs/language-support-and-recommendations.md) distinguishes picker choices, model compatibility, benchmark locales, and qualified results. Model recency alone never changes a default.

Nemotron's published language tiers include Hindi `hi-IN`. MUCS now supplies a dedicated `hi+en` Hinglish screen, but neither locally tested candidate passes the current phase-one quality ceiling; Auto must not be presented as qualified Hinglish support. Imported Opus is archival-only because its decoder is unavailable. Packaged Nemotron backends are CPU, Apple Metal, NVIDIA CUDA, and Vulkan for compatible AMD/Intel GPUs. The AMD ROCm build profile uses Vulkan for Nemotron and ROCm for Qwen cleanup. MLX is not a NeMo-Speech.cpp backend; Apple acceleration here uses Metal.

Read [research](docs/research.md), [streaming inference papers](docs/research-streaming-inference-papers.md), [architecture](docs/architecture.md), [application internals and storage](docs/internals.md), [architecture decisions](docs/adrs/README.md), [language support and recommendations](docs/language-support-and-recommendations.md), [model details](models/README.md), [Hugging Face profiles](docs/huggingface-models.md), [CPU/GPU optimization](docs/inference-performance.md), [NVIDIA and historical Kyutai streaming research](docs/research-streaming-speech.md), [Whisper, Cohere, Microsoft and other models](docs/research-speech-landscape.md), [CI, containers, and releases](docs/ci-and-releases.md), [motion and interaction design](docs/motion-design.md), and [development](docs/development.md).

## Contributing and security

Every repository job is tracked in [GitHub Issues](https://github.com/rahulsharmavishwakarma/transcribe/issues) using the GitHub CLI (`gh`), and completion includes an update to the relevant canonical documentation under `docs/`. See the [project workflow](docs/project-workflow.md) and agent rules in [AGENTS.md](AGENTS.md).

Contributions are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) for setup, Git hooks, tests, and pull-request expectations; [SUPPORT.md](SUPPORT.md) and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) describe support and community expectations. Report vulnerabilities privately according to [SECURITY.md](SECURITY.md), and never post private audio, transcripts, or credentials in an issue.

This repository does not yet declare an open-source license. Add an explicit license before accepting or distributing third-party contributions.
