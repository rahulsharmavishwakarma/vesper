# Development

The repository directory may be named `transcribe` or any other name: Cargo member paths and development scripts resolve relative to the repository. The root `Cargo.toml` is a virtual workspace manifest; the unpublished internal packages use `vesper-core`, `vesper-audio`, `vesper-inference`, `vesper-data`, and `vesper-models`.

The current supported desktop target is macOS on Apple silicon. Linux/Windows
code paths and prerequisites below are retained for future portability work;
they are not current release targets. Docker and Nginx are optional browser-preview
tools and are not part of the native application.

The repository contains a Rust workspace and a Tauri 2 desktop application with a Svelte 5 / Vite / TypeScript frontend. The native application connects model installation, microphone recording or file decoding, NVIDIA Nemotron, Parakeet, Qwen3-ASR, Cohere, or LFM transcription, Qwen cleanup, and local dataset review/export. Training and automatic model improvement are not implemented.

## Release installer

`CI=true make build` produces the optimized Mac application and a `.dmg` through
Tauri's macOS configuration. `make build-debug` remains `.app`-only. Versioned
build output stays under ignored `target/release/bundle/`; release collection
verifies the bundle and DMG, then copies the installer to
`release/vesper-macos-arm64.dmg`. The [release workflow](ci-and-releases.md#release-workflow-and-download-url)
attaches it to GitHub Releases for a stable direct download URL.

## Contributor checks

After `make setup`, install [uv](https://docs.astral.sh/uv/getting-started/installation/)
(0.12.15 or a compatible 0.12 release) and run `make setup-checks` and `make hooks`.
uv manages Python 3.12+ and the locked tools in `.venv`; Chromium lives in `.cache/playwright` (override with
`PLAYWRIGHT_BROWSERS_PATH`). The application still needs no Python runtime.
`make check` runs all language format/lint checks, Node/Python/browser regressions,
Rust all-target tests, doctests, strict Clippy/rustdoc and the native suite;
`make precommit` adds repository and workflow/security hygiene. See
[CI and releases](ci-and-releases.md) for setup and hosted limitations.
Automatic commits retain the existing format/lint/type and Node checks. Automatic
pushes run file hygiene and version consistency only; they do not compile Rust,
start Chromium or rerun the full suite while the remote connection waits. Use
`make prepush` to exercise those quick checks, and run `make check` explicitly
before pushing when you need full local validation. CI still runs the complete
portable, browser and macOS native checks on pull requests and `main`.
Rust updates change `rust-toolchain.toml` and must pass macOS native tests and app-bundle validation. The
workspace's minimum Rust metadata is not independently covered by CI.

## Python tooling policy

All Python work uses **uv** with an owning **`pyproject.toml`**. This includes
contributor checks, scripts, tests, benchmark preparation and research environments.
Python remains development/research tooling; the desktop application has no Python
runtime requirement.

- Declare dependencies, `requires-python` and supported tool settings in
  `pyproject.toml`. Put development/test tools in `[dependency-groups]`; use the
  existing owning project rather than creating a separate manifest for each script.
- Add/remove dependencies with `uv add` and `uv remove`. Keep the generated
  `uv.lock` in version control with dependency changes; do not edit it manually.
- Use `uv sync` to prepare the isolated project environment and `uv run` for Python
  scripts, formatting, linting and tests. Keep virtual environments and caches ignored.
- CI must use `uv sync --locked` and `uv run --locked` with the required dependency
  groups, so stale metadata fails instead of silently changing the resolution.
- Do not add parallel pip/pipx, manual virtualenv, Poetry/Pipenv or hand-maintained
  requirements workflows. If an external tool requires requirements format, generate
  it with `uv export` from the manifest/lockfile and document the export command.
  Independently installed Python CLI tools use `uv tool install` with a pinned version;
  shared project checks belong in the project's dependency groups.

Run from the repository root. The default `dev` group provides all contributor
checks; the optional `benchmark` group adds PyArrow for corpus preparation:

```sh
uv sync --locked
uv run --locked python scripts/run-python-tests.py
uv run --locked --group benchmark python scripts/prepare-accent-benchmark.py --help
make python-lock-check
make audit-python
# Apple-silicon macOS benchmark dependency audits:
make audit-python-audio-frontend
make audit-python-vibevoice
```

Use `uv add --dev PACKAGE` / `uv remove --dev PACKAGE` for contributor tools.
Ruff configuration lives in the root `pyproject.toml`. Make targets invoke
Python, Ruff, pre-commit, ShellCheck and shfmt through `uv run --locked`; old
`.tooling/ci-venv` installations are preserved but no longer selected. Set
`UV_PYTHON=3.12` to match CI. The Makefile keeps uv's download cache in ignored
`.cache/uv`; `UV_CACHE_DIR` can override it. The app's Rust/npm version metadata
is independent of the non-distributable Python tooling project's version.

Model probes use separate macOS Apple-silicon/Python 3.12 projects under
`benchmarks/asr/python/audio-frontend` and `benchmarks/asr/python/vibevoice`, each
with its own manifest and lockfile. Their incompatible NumPy/model dependencies
are isolated from contributor checks. See [benchmark Python setup](../benchmarks/asr/README.md#python-environments).
The retained benchmark requirements files are generated compatibility exports;
`make python-lock-check` verifies they match their locks. Dated validation reports
retain their original commands as historical evidence; use the current benchmark
setup for new work. Resolving a probe's dependencies does not qualify its model
or reproduce an earlier measurement.

The optional Apple-silicon `training/cleanup` project has its own manifest/lock
for adapter compatibility research. `make python-lock-check` also checks that
lock; the ordinary Ruff targets include its source without installing its model
dependencies. `make test-cleanup-bridge` explicitly runs its deterministic tensor
mapping tests in the separate environment. Its numerical gate currently fails;
see [cleanup adapter qualification](cleanup-adapter-qualification.md) for exact
pins, reproduction, measurements and integration limits.

The audits check every contributor group and each isolated benchmark environment.
Known upstream blockers and failing gates are documented in
[dependency security status](ci-and-releases.md#dependency-security-status-and-upstream-limits);
a successful contributor audit does not imply that optional inference packages
are free of advisories.

Run `make audit-rust` for Rust security checks. It verifies the maintained GLib
source and actual Cargo resolution, then audits a temporary registry-equivalent
lockfile with only the verified, repaired advisory excepted. Direct `cargo
audit` misses crates.io advisory coverage for this path dependency. The
Linux-only optimized regression is `make test-glib-backport`; install system
GLib development files and pkg-config first. See the [backport maintenance
procedure](../patches/glib/README.md) before updating vendor files or provenance.

The VibeVoice research probe uses a [checksum-verified compatibility
patch](../patches/vibevoice/README.md) for Transformers 5.10.1. After preparing
the pinned model assets and public corpus under an ignored `ROOT`, run:

```sh
uv sync --locked --project benchmarks/asr/python/vibevoice
uv run --locked --project benchmarks/asr/python/vibevoice python scripts/check-vibevoice-tokenizer.py --root ROOT --output ROOT/tokenizer-compat.json
uv run --locked --project benchmarks/asr/python/vibevoice python scripts/probe-vibevoice.py --root ROOT --plan ROOT/plan-offline.json --output ROOT/runs/offline.json
uv run --locked --project benchmarks/asr/python/vibevoice python scripts/probe-vibevoice.py --root ROOT --plan ROOT/plan-streaming.json --output ROOT/runs/streaming.json
```

Use fresh output filenames; existing reports are preserved. The compatibility
check covers tokenizer equivalence, tied and untied synthetic checkpoint
round trips, and separate publisher/built-in acoustic configuration dispatch.
The actual probe additionally verifies every downloaded checkpoint tensor and
per-chunk cache progression. Keep source patches, manifest/lock, generated
requirements and fresh benchmark evidence together when upgrading this stack.
These checks qualify the isolated reference probe, not an application adapter.

### Git worktrees and ignored caches

A worktree is a separate checkout with its own branch, index and working-tree
status. If one lives under ignored `.cache/`, its source changes are still
tracked relative to that checkout. Inspect it with
`git -C .cache/WORKTREE_NAME status --short` and `git worktree list`.
Its manifests, lockfiles and source changes can be committed on its branch;
the parent checkout does not stage the `.cache` directory. Python `.venv`, uv
caches, browser downloads and generated artifacts remain ignored in each
checkout. Commit authorization and signature requirements still apply.

Command references: Astral's [project guide](https://docs.astral.sh/uv/guides/projects/),
[dependency groups](https://docs.astral.sh/uv/concepts/projects/dependencies/#development-dependencies),
[locking and syncing](https://docs.astral.sh/uv/concepts/projects/sync/) and
[standalone tools](https://docs.astral.sh/uv/guides/tools/).

## Prerequisites

- Node.js 24 LTS is preferred. Supported versions are Node 22.13+ within the 22.x line, or Node 24 and newer; Node 23 is excluded. Use npm 10 or newer.
- Rust 1.98.1, selected by `rust-toolchain.toml`, with `rustfmt` and `clippy`.
- A C/C++ toolchain, CMake 3.15 or newer, and a discoverable libclang for the native inference bindings.
- Platform libraries listed below. Python, CUDA, ROCm, and FFmpeg are not prerequisites for the portable CPU build; accelerator toolchains are required only for their matching build profile.

Model weights are not needed to compile or run the unit tests. The first native build does need network access to download Rust dependencies and the pinned sherpa-onnx source and its hashed CMake dependencies; the desktop installer downloads model weights separately when requested.

If Rust is installed through rustup, add the development components with:

```sh
rustup component add rustfmt clippy
```

A project-local Rust toolchain can also be installed under `.tooling`. To use that existing installation without changing shell profiles, run this from the repository root before the development commands:

```sh
source scripts/dev-env.sh
```

The script enables the optional toolchain and an existing project-local CMake installation; it does not install Rust, CMake, or Node.js. It supplies `VESPER_MODEL_DIR` from an explicit nonempty override, earlier Transcribe/Ukti/STT aliases, or an existing `models/downloads`, in that order. `.tooling` is local development state and should not be committed.

On macOS, install the command line tools if needed:

```sh
xcode-select --install
brew install cmake
```

Xcode Command Line Tools supply Clang and the CoreAudio frameworks used by CPAL. If bindgen cannot find libclang, check the selected Xcode toolchain with `xcode-select -p`; set `LIBCLANG_PATH` to the `lib` directory of that toolchain only if discovery fails.

On Ubuntu 24.04 or a compatible Debian distribution:

```sh
sudo apt-get update
sudo apt-get install -y build-essential cmake clang libclang-dev pkg-config \
  libasound2-dev libwebkit2gtk-4.1-dev libxdo-dev libssl-dev \
  libayatana-appindicator3-dev librsvg2-dev
```

`libasound2-dev` is required for CPAL even when no microphone tests run. Other packages provide Tauri's GTK/WebKit shell and the inference compiler toolchain. Distribution equivalents and Windows prerequisites are described in the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) and [CPAL build instructions](https://github.com/RustAudio/cpal/blob/v0.16.0/README.md). Windows additionally needs MSVC C++ Build Tools, WebView2, CMake, and LLVM/libclang; Windows builds are not currently covered by CI.

### Native runtime builds and linking

- `sherpa-onnx` is pinned to `1.13.8` and built with the approved completion-reporting patch. Run `make sherpa-runtime` (also included in `make setup`) and `source scripts/dev-env.sh` before direct Cargo commands. The build validates the patch and 13 static-library hashes; ordinary upstream prebuilt libraries do not contain this report and cannot satisfy the build contract. See [offline ASR build/provenance](offline-asr.md).
- Nemotron uses the official NeMo-Speech.cpp 0.1.0 executable downloaded by the model installer. Platform archives are pinned for CPU, Apple Metal, NVIDIA CUDA, and Vulkan. The executable is a local, cancellable sidecar; no server or network request is involved during transcription.
- `llama-cpp-2` is pinned to `0.1.156`, with default features disabled. Its bundled llama.cpp builds from C/C++ source through CMake and links statically. Apple Silicon includes Metal; Linux/Windows accelerator builds use `cargo build -p vesper-desktop --features cuda`, `--features vulkan`, or `--features rocm`. The selected backend falls back to CPU for cleanup if its matching feature was not compiled. `CMAKE_BUILD_PARALLEL_LEVEL=2` can reduce peak build memory. See the [pinned binding build script](https://docs.rs/crate/llama-cpp-sys-2/0.1.156/source/build.rs).
- Keep `Cargo.lock` committed. `--locked` fixes Rust dependency resolution; native release downloads still require network access on an uncached build. Do not set `DOCS_RS` to bypass native linking: that upstream documentation option does not produce a working application. Switching to shared native libraries later requires packaging their runtime libraries and loader paths; the current manifests use static linking.

## Experimental speaker runtime

The local speaker stage uses GA Q8_0 weights independently of the chosen ASR.
Its C ABI comes from NVIDIA/NeMo-Speech.cpp
`97a15afa5caa9bce5baaa86c1184103877af4101`. Build that exact checkout's
`metal-diar` preset following the [sample report](validation/nemotron-diarization-2026-09-25.md#reproducibility),
including the pinned ggml patches and SentencePiece development files. The public
v0.1.0 ASR archive predates this model and cannot supply this stage.

Package the built dependency closure on Apple silicon with Xcode tools:

```sh
uv run --locked python scripts/package-nemotron-diarization.py \
  --source /path/to/pinned/NeMo-Speech.cpp \
  --model-root /path/to/selected/model-directory
```

Optionally pass `--model /path/to/Nemotron-3-Diarization.q8_0.gguf` to copy an
already downloaded hash-verified GA artifact. Otherwise use the app's speaker
model download. Use the **same model directory selected in the app**. The script
refuses to replace an existing runtime package; qualify a fresh model root when
rebuilding. It leaves the existing ASR installation intact. The separate bundle
contains renamed, ad-hoc-signed dylibs and a local file-hash manifest. No native
build, server, API call or model download runs during transcription.

The stage runs automatically on Apple silicon with Automatic, CPU or Metal when
the weight file, runtime manifest and all seven libraries are present. This cheap
availability check is shared by the desktop and convenience pipeline; full hashes
and path checks still run before native loading. Missing/partial components and
unsupported backends skip the stage without blocking ASR. Present but corrupt
components retain raw text and report the existing analysis warning.

`speakerDiarization` is retired: Rust ignores it on load and omits it on save;
the frontend migration removes it, including an old `false` value, while retaining
other preferences. The model remains independently manageable in the catalog;
ordinary ASR/cleanup installation does not require or install the speaker bundle.

Run the ignored real-model checks with `VESPER_DIAR_MODELS` pointing to an
isolated model root containing the speaker bundle, Cohere, Qwen3.5 cleanup and
Nemotron ASR with its macOS Metal release package, and
`VESPER_DIAR_CORPUS` pointing to the four prepared sample WAV files:

```sh
cargo test -p vesper-inference real_worker_speaker_samples_and_native_asr_coexistence -- --ignored --nocapture
VESPER_DIAR_FIRST=1 cargo test -p vesper-inference real_worker_speaker_samples_and_native_asr_coexistence -- --ignored --nocapture
cargo test -p vesper-desktop real_asr_speaker_pipeline_persists_and_exports_without_changing_words -- --ignored --nocapture
```

The first two commands verify the native load orders in independent processes,
CPU/Metal, fresh recording state, packet cancellation and stable before/after
ASR output. The desktop command exercises the
desktop ASR → automatic speaker stage → persistence/export path, including an old
disabled preference, unchanged text against a direct ASR-worker baseline,
collection-off results and coexistence with text cleanup. The baseline excludes
desktop persistence and is not an equivalent end-to-end latency measurement. They do not
establish published DER parity or language generalization.
See [ADR-0022](adrs/0022-local-speaker-timelines.md) for ownership, failure and
privacy contracts.

## Run the application

Run these commands from the repository root:

```sh
npm install
npm run dev
```

`npm run dev` starts the frontend in a browser. It is useful for UI development and saves preview settings in the browser's `localStorage`, separately from desktop settings. It does not create a Tauri runtime or provide native microphone/model functionality. Follow the URL printed by Vite.

To run the desktop shell:

```sh
npm run desktop:dev
```

The Tauri product name is `Vesper`, with application identifier `com.vesper.desktop`. Earlier settings and data remain accessible through the compatibility lookup below; the identity change does not move or delete data. macOS may request microphone, Accessibility or notification permission again for the new identity. The Models screen pairs each supported ASR/cleanup dropdown with its own download action; the complete selected pipeline must be ready before recording or transcribing a file. Merely launching the app or listing input devices does not start capture; recording starts only when Record is requested.

The macOS menu bar displays only the static **Songbird logo**, without text or a background tile. Its tooltip retains the Vesper name and current activity. Click it for current activity, Start/Stop recording, Open Vesper, History, Models, Settings and Quit. Menu recording uses the same quick-capture controller and default input as the global shortcut. The floating strip appears for menu preflight so missing models or microphones can be explained; after a successful start it respects the saved strip preference. Opening a screen never saves pending settings. Navigation requested before the main WebView is ready is retained until its listener mounts.

Closing a window hides it while keeping recording and unsaved drafts intact. Reopen the workspace from the menu or the macOS Dock. **Quit Vesper** is disabled during active work. Cmd-Q and Dock quit also refuse to exit during recording, inference or model operations and bring Dictate forward; finish or cancel the operation, then quit. An accepted idle quit blocks new operations atomically. Force termination is outside this protection.

`src-tauri/src/menu_bar.rs` uses [Tauri's native tray API](https://v2.tauri.app/learn/system-tray/) with OS-rendered menus. The macOS image is embedded at compile time and marked as an [AppKit template](https://developer.apple.com/documentation/appkit/nsimage/istemplate), allowing system tinting for light/dark and selected states. A 500 ms timer reads only the existing runtime activity and updates native items when their state changes; it does not animate or replace the icon, inspect model files, load weights or request microphone samples. Idle means no operation is running, not that models have been downloaded. Other desktop platforms retain the application bitmap; Linux and Windows tray behavior still needs platform qualification.

The desktop shell creates a hidden `quick-capture` webview window. Two independently registered global shortcuts drive the native dictation controller: Hold starts on press and finishes on release; Toggle starts/stops on successive presses. The WebView renders native state and existing partial captions; the same runtime and operation gate used by the main window prevent overlapping recording, installation, and inference jobs. The bar is always on top when enabled, uses the same curved waveform as Dictate, and hides shortly after completion. Wave height follows DC-free microphone RMS; wavelength follows a reliable 60–700 Hz pitch estimate from the existing capture worker. The lightweight `get_audio_features` command reads these values separately from model/status inspection. It samples sequentially at a requested 40 ms interval while recording; stale readings expire after 200 ms. Silence settles flat; unvoiced input retains its last visual spacing until silence/reset without inventing measured pitch. A new unvoiced recording starts neutral. Stroke heights morph directly between measured pitch shapes, and processing has a distinct travelling wave. Reduced motion keeps geometry fixed. Idle and recorder browser-preview modes do not simulate microphone activity. Settings → Quick capture has a separate, explicitly simulated nine-second design preview with Pause/Resume and Replay; it uses no microphone or native transcription calls and pauses offscreen or under Reduce Motion. See [motion design](motion-design.md) for the estimator and validation limits. Notification permission is requested only after a user enables notifications and saves Settings. Notification failures never fail transcription.

Quick capture uses a 440 × 96 logical-pixel window, with an 80-pixel bar inside its transparent margin. Its visible actions are Record, Stop, and View; View emits `show-transcript`, opens Dictate, refreshes the latest result, and focuses the transcript for copying. Completion remains visible for six seconds and pauses dismissal while hovered or focused. Close/Escape hide the bar. The elapsed timer has no general cutoff; a positive runtime duration is an adapter-specific restriction.

The rounded macOS window enables Tauri's `macos-private-api` feature and `app.macOSPrivateApi`. Both the document root and body are transparent, and the native rectangular shadow is disabled. Tauri documents that this private API use prevents Mac App Store acceptance; the current configuration is for direct desktop distribution. An App Store build needs a separate opaque-window design and must disable that feature. Browser screenshots validate web content, not native compositor behavior.

When the current `com.vesper.desktop` directory has no actual settings, dataset, or model files, the application checks prior sibling directories in this order: `com.transcribe.desktop`, `com.ukti.desktop`, `com.stt.desktop`, reusing the newest store with data in place. A populated `com.vesper.desktop` directory always wins. Nothing is moved, merged, or deleted. Empty platform-created directories do not hide an earlier store, and read failures are reported.

Nonempty overrides are checked in order: `VESPER_MODEL_DIR`, `TRANSCRIBE_MODEL_DIR`, `UKTI_MODEL_DIR`, `STT_MODEL_DIR`. Debug builds also discover an existing repository `models/downloads` folder, including when the debug `.app` is opened from Finder. Release builds without an override use the selected data directory's `models` folder. Browser preview settings read `vesper.browser-preview.settings.v1` first, then the legacy `transcribe.browser-preview.settings.v1`, `ukti.browser-preview.settings.v1` and `stt.browser-preview.settings.v1` keys. Reads never write. Only an explicit Save/Reset writes the Vesper key, leaving legacy bytes untouched. Invalid current settings are reported instead of silently falling back.

`VESPER_NEMO_SPEECH_LIB` selects a custom NeMo library, with `TRANSCRIBE_NEMO_SPEECH_LIB` retained as a fallback. The shared Rust environment resolver prefers a present Vesper value, even if invalid or non-Unicode, rather than hiding configuration errors behind an older value. Only directory overrides treat an empty value as absent. Rust developer/qualification overrides use this same resolver.

### macOS microphone permission and bundles

[apps/desktop/src-tauri/Info.plist](../apps/desktop/src-tauri/Info.plist) supplies `NSMicrophoneUsageDescription`. Tauri embeds it in development executables and merges it into macOS app bundles. Keep this file beside `tauri.conf.json`. If microphone access was denied, enable the application or development host in macOS System Settings → Privacy & Security → Microphone and relaunch it. See [Tauri's macOS configuration](https://v2.tauri.app/reference/config/#macconfig).

The macOS override `tauri.macos.conf.json` enables `.app` bundling and declares
macOS 10.15 as the minimum because the bundled llama.cpp runtime uses C++
filesystem APIs unavailable under Tauri's older default deployment target.
`make build-debug` creates
`target/debug/bundle/macos/Vesper.app`; verify its generated Info.plist and
microphone permission flow on the intended macOS release. Signing, notarization,
and sandbox entitlements for release distribution remain packaging work. CI does
not grant microphone permission or record audio.

### Brand assets

Edit only `apps/desktop/src/assets/vesper.svg` for the Songbird bird/wordmark
and `apps/desktop/src/assets/ember-bird.svg` for the in-app Ember lockup mark.
The frontend embeds those trusted, bundled SVGs without a runtime fetch or font.

The README's `docs/assets/vesper-banner.svg` is a generated, self-contained
1200 × 320 SVG. It preserves every canonical bird/call/letter path and embeds
the existing background, ink and ember tokens from `apps/desktop/src/tokens.css`.
Do not separately redraw its logo or edit the generated file. Regenerate it
after changes to those inputs:

```sh
node scripts/generate-readme-banner.mjs
node scripts/generate-readme-banner.mjs --check
node --test scripts/brand-identity.test.mjs
```

The banner and generator retain only project-relevant provenance comments.
Design-tool caches, prompts and self-assessment stamps are not product assets;
keep them out of tracked source. The local `.hallmark/` cache is Git-ignored and
is not needed to regenerate the banner. Brand regressions reject design-tool
stamps in both the generated SVG and its generator.

The banner has no script, animation, external font or image dependency. Its
relative README image path works in branches and clones; the image's `alt`
provides the Vesper name when images are unavailable. Keep the tagline,
platform and development status in real README text, so they remain readable
at narrow widths. GitHub owns the surrounding Markdown layout and typography.
Validate GitHub-rendered Markdown plus the local banner at 320/375/414/768 px
and desktop, in light and dark surroundings; changes are not visible on the
hosted repository until pushed. `make check-fast` includes banner freshness,
canonical-path and truthful-release-copy regressions. A banner does not imply
a published installer or selected open-source license.

Regenerate desktop icons using the existing locked Tauri CLI after `npm ci`:

```sh
node scripts/generate-brand-icons.mjs
node scripts/generate-brand-icons.mjs --check
node --test scripts/brand-identity.test.mjs
```

The generator extracts the canonical bird, resolves the existing ember
tokens to sRGB, and updates `icons/source.svg` plus the 18 existing PNG/ICO/ICNS
derivatives. It also generates `menu-bar-template.svg` and its transparent black
36 × 36 PNG from the same unchanged bird and call paths, without the tile or
wordmark. Tauri 2.11.5's locked tray-icon 0.24.2 macOS backend sizes this image
to 18 × 18 logical points, giving a 2× backing image with transparent padding.
The native regression checks its decoded pixels, padding and antialiasing;
the source regression guards icon-only/template wiring and the retained tooltip.
The generator normalizes ICNS entry ordering for reproducible bytes and does
not copy mobile assets or declare new platform support. `--check` regenerates
into an OS temporary directory and compares without changing tracked files.
Temporary rendering output remains there for diagnosis. If colour tokens change,
update the resolved palette and its test together. Browser checks do not qualify
native Dock rendering, microphone permission or Windows/Linux packaging.

Run the app from the checkout containing the integrated changes. A preview
server in a different Git worktree serves that worktree's files, not the shared
checkout. Rebuild with `make build-debug` and relaunch
`target/debug/bundle/macos/Vesper.app` after native display-name or icon changes;
frontend hot reload alone does not replace a running app's packaged icon.

### Product name and compatibility

The native Swift bridge is `VesperMLX` (`vesper_mlx_*` C symbols). Its
project-owned completion-report properties use the same Vesper prefix. These
are private build interfaces: rebuild the bridge and Rust consumers together;
do not load a renamed copy of an old binary. The pinned upstream Cohere
Transcribe architecture and ordinary transcription API names are unchanged.
The generated library is `libVesperMLX.dylib`; its real source, patch and
binary hashes are checked before packaging. `VESPER_MLX_BUILD_DIRECTORY` is
injected by the Rust build script, not a user override.

An existing Swift scratch checkout may contain the previous exact reporting
patch. The integrity check intentionally refuses to overwrite a modified
checkout. To upgrade such a cache, stop builds, move `.tooling/mlx-build` to a
new, unused backup path, and rebuild with `node scripts/mlx-runtime.mjs`.
Keep that backup until the new build is verified. This is generated source/build
state, not model weights or application data. Sherpa uses a patch-hash-specific
build directory, so the prior verified cache can coexist with the new one.

**Vesper** is the display name in the app, documentation, issue templates,
GitHub issues and project title/readme. Native startup messages and developer
qualification windows use the same name. The application identity is `com.vesper.desktop`; legacy state is reused in place,
not moved or merged. Repository renaming remains owner-controlled. Internal Rust packages and their directories
use `vesper-*`; the native executable is `vesper-desktop` and the library is
`vesper_desktop_lib`. Current commands and source links use those names.
The npm workspace is `vesper-workspace`, with frontend package
`@vesper/desktop`; root npm scripts, browser tests and container builds resolve
that same workspace. These packages remain private, with unchanged versions.
Contributor Python metadata uses `vesper-tooling`. The hook IDs are
`vesper-fast-check` and `vesper-push-check`; CI's explicit duplicate-check skip
uses the matching ID. The checks themselves and dependency pins are unchanged.
The isolated audio-frontend Python project is `vesper-audio-frontend-probe`.
Its generated requirements export continues to omit the local package while
retaining accurate dependency provenance comments; `make python-lock-check`
verifies each manifest, lockfile and export together.
The VibeVoice reference environment likewise uses `vesper-vibevoice-probe`;
its isolation, platform markers and pinned third-party packages are unchanged.
Temporary audit, benchmark-source, release/version-test and native-probe paths
use Vesper prefixes. Their creation/cleanup boundaries are unchanged; no
existing temporary directory or prior measurement is renamed by the app.

Current project-owned packages, build interfaces, container tags, hook IDs,
new exports and future installer filenames use Vesper. The following old names
are intentional compatibility/evidence, not unfinished product naming:

- The live repository slug and working GitHub links remain `transcribe` until
  the owner renames it. No code push or repository rename is implied by a local
  migration. Update clone/release/issue links and the remote after that action.
- Old Transcribe/Ukti/STT app/browser identifiers remain lookup aliases.
  Native saves stay in the selected existing store; browser saves write only
  the Vesper key. No existing data directory or export is moved or merged.
- `VESPER_*` runtime and qualification overrides accept corresponding
  `TRANSCRIBE_*` fallbacks. The launcher also accepts the old MLX switch.
  Build-injected provenance variables are internal and need no runtime alias.
- The persisted audio identity hash domain `transcribe-learning-pcm16k-v1`
  is immutable: changing it would break recognition of previously seen audio.
  Prior Linux benchmark RSS markers remain parseable.

`node --test scripts/naming-consistency.test.mjs` rejects stale current package,
bridge, artifact and native-report names while allowing dated evidence and
explicit storage/environment compatibility. Historical commands target their
recorded revision; current commands use `vesper-*` and `VESPER_*`.
The guard also checks environment assignments in current ASR/cleanup benchmark
instructions, so copied examples prefer the canonical namespace.

Do not rename the third-party **Cohere Transcribe** model, the `transcribe.cpp`
runtime, transcription actions such as **Stop and transcribe**, or API methods.
Preserve literal names, paths, hashes, measurements and citations in historical
validation records and quoted naming discussions; clearly label pre-Vesper
branding as historical and link to current behavior. Do not rewrite old
measurements as if they tested the current identity.

## Check changes

```sh
make fmt-check
make lint
make check
make workflows
make build-debug
```

`make fmt` applies rustfmt, Prettier, Ruff and shfmt; checks never auto-fix source.
Use `make check-portable` for frontend/Node/Python/shell checks, or targets such as
`make lint-web`, `make fmt-python-check`, `make test-python` and `make test-rust`
while developing. The [CI language matrix](ci-and-releases.md#language-coverage)
describes exact coverage, pinned tools and model/hardware exclusions.

The npm build creates frontend assets; it does not package a desktop installer. CI runs Rust checks and desktop linking on macOS, with portable tooling checks on Ubuntu. Tests exercise audio decoding/resampling, capture-buffer behavior without opening a microphone, configuration, model manifests, and dataset invariants. They do not download model weights or measure model accuracy. Synthetic compressed fixtures are checked in, so tests do not invoke FFmpeg. Report any checks skipped because a toolchain or dependency is unavailable.

## Configuration and models

Edit and save settings in the application. On desktop, the Tauri commands `get_settings`, `save_settings`, and `reset_settings` validate settings and persist them atomically as `settings.json` inside Tauri's `app_config_dir`. The exact directory depends on the operating system and application identifier. Browser preview uses separate, origin-specific `localStorage`.

The persisted JSON uses camelCase fields:

| Field                              | Default                                   | Accepted values / behavior                                                                                                        |
| ---------------------------------- | ----------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `schemaVersion`                    | `1`                                       | Version `1`                                                                                                                       |
| `models.asr`                       | `CohereLabs/cohere-transcribe-03-2026`    | Cohere, Nemotron, Qwen3-ASR, Parakeet, or LFM2.5 Audio                                                                            |
| `models.rewriter`                  | `Qwen/Qwen3.5-0.8B`                       | Current cleanup model                                                                                                             |
| `cleanupEnabled`                   | `true`                                    | Enable or disable the cleanup stage                                                                                               |
| `language`                         | `en`                                      | `auto`, `en`, `fr`, or `hi`; the selected adapter narrows valid choices and Parakeet rejects Hindi                                |
| `computeBackend`                   | `auto`                                    | `auto`, `cpu`, `metal`, `cuda`, `vulkan`, or `rocm`; unsupported platform/model pairs are rejected                                |
| `cleanupPreset`                    | `minimal`                                 | `minimal` preserves words; built-in Qwen `balanced` may add a guarded 24-word technical tail pass before full cleanup             |
| `customCleanupInstruction`         | Empty string                              | At most 4,000 characters; nonblank text replaces the preset instruction, otherwise the preset applies                             |
| `rewriteQuantization`              | `q4_0`                                    | `q4_0` or `q8_0`                                                                                                                  |
| `maxRecordingSeconds`              | `0`                                       | Legacy unsigned integer, retained but ignored; no general time quota                                                              |
| `globalShortcut` / `recordingMode` | `CommandOrControl+Shift+Space` / `toggle` | Legacy single-binding fields retained for migration                                                                               |
| `holdShortcut`                     | `null`                                    | Effective default Alt/Option + Shift + D; a saved legacy Hold binding is preserved                                                |
| `toggleShortcut`                   | `null`                                    | Effective default ⌘/Ctrl + Shift + Space; a saved legacy Toggle binding is preserved                                              |
| `floatingBarEnabled`               | `true`                                    | Show the always-on-top quick-capture strip when the shortcut is pressed                                                           |
| `captureDesign`                    | `split`                                   | `split`, `ribbon`, `underline`, `studio`, `margin`, or `counter`; chosen under Settings → Quick capture                           |
| `notificationsEnabled`             | `false`                                   | Send privacy-safe OS completion/failure notifications after permission is granted                                                 |
| `collectTrainingData`              | `true`                                    | Retain imported/captured audio and session data; disabling runs transcription without archiving and prevents archive-only imports |
| `improvementMode`                  | `review_required`                         | `review_required` or `automatic`; future training policy, not a running trainer                                                   |

Models owns the separate ASR and cleanup selectors, cleanup precision, inference accelerator and downloads. Dictate and Settings expose the same persisted transcription-language setting; its choices are filtered to the selected built-in model or imported profile. Settings also owns cleanup behavior and instructions, shortcuts, notifications and collection policy. All pages share a configuration draft; edits autosave the whole draft about a second after each change, confirmed by a transient sidebar pill. Local engine stays open at the top, followed by the speech and writing model cards. Download status is integrated into the model and precision dropdowns; there is no separate downloaded-model panel. Custom profiles appear as the active choice in their respective selectors.

Saving settings alone does not download weights or load a model. Each model dropdown has a management row with file size, status and Download/Verify files plus Remove when local files exist. With pending edits, Download (or Verify files for installed files) first flushes and persists the complete configuration draft and stops if saving fails. It then requests exactly the selected model ID and precision through the existing runtime controller; ASR and custom profiles send no built-in cleanup quantization. Q4 installation never implies that Q8 is installed. Previously saved custom profiles remain in the corresponding dropdown; new user selections come from the curated catalog. There is no JSON profile import section. Download progress and cancellation remain outside the disabled settings fieldset. The complete selected pipeline can still be installed together. Installation and inference verify pinned artifact sizes and hashes, while selected-pipeline readiness remains separate from the status of unselected downloads. Settings cannot be changed while a runtime operation is active. Imported originals and their reviews are stored locally. Disabling collection does not erase existing records. Automatic improvement is not implemented and does not bypass the dataset's human-label requirements.

The **model dropdowns** show download status for built-in packages, every shipped preset (including presets no longer selected), and currently configured imported profiles in the model directory. The writing-model menu identifies downloaded built-in precisions (for example, Q4 downloaded); the precision dropdown checks Q4 and Q8 independently. Each model card places Download/Verify and, when local files exist, Remove together. Choosing a downloaded package in the draft lets the user inspect or remove its files without saving it as the active model. Unrelated pending edits do not hide its installed status. An edited imported declaration cannot reuse the saved catalog status; save first to check the new files. Local engine’s Refresh also refreshes model inventory. File sizes are actual logical bytes of known model weights, including incomplete or truncated files; they exclude shared native runtimes and are not a measurement of physical filesystem space recovered. Status polling checks expected file sizes; full hashes are checked during installation and before inference. Other model directories and previously replaced arbitrary imported profiles are outside the catalog inventory. Unrecognized files remain untouched.

**Remove** in either model card opens an accessible confirmation naming the model, precision and removable bytes. A selected model stays selected after removal; any required missing weights block transcription until downloaded again or replaced. Removal is disabled during active operations and catalog checks, and unavailable for a draft profile that does not match the saved catalog. Deleting a previewed unused package does not call save_settings or activate that package. Cancelling retains focus on Remove; after successful removal, focus returns to the relevant model or precision dropdown. The UI updates the option to Not downloaded. The native `remove_model` command reuses installation's existing operation gate and unload boundary, checks the current model folder against the confirmed folder, resolves the model ID/role/precision from the native catalog, preflights descendant symlinks, and unlinks only declared weight files. Files shared with another catalog entry, runtime archives/executables, unknown files, recordings and settings are preserved. No recursive directory deletion or arbitrary frontend file paths are accepted. Readiness and inventory refresh after both success and failure; a filesystem error can leave a partially removed package, which stays unavailable until repaired.

[configs/default.toml](../configs/default.toml) documents the equivalent defaults. It is a reference template, not an imported configuration file; desktop persistence uses JSON. Models always resolve to the default directory (see above); the template carries no folder override:

```toml
schemaVersion = 1
cleanupEnabled = true
language = "auto"
computeBackend = "auto"
cleanupPreset = "minimal"
customCleanupInstruction = ""
rewriteQuantization = "q4_0"
maxRecordingSeconds = 0
globalShortcut = "CommandOrControl+Shift+Space"
floatingBarEnabled = true
captureDesign = "split"
notificationsEnabled = false
collectTrainingData = true
improvementMode = "review_required"

[models]
asr = "CohereLabs/cohere-transcribe-03-2026"
rewriter = "Qwen/Qwen3.5-0.8B"
```

Cohere Transcribe is the default completed-recording ASR, paired with Qwen3.5 cleanup and explicit English for new settings. The current Cohere adapter also supports French; Auto/Hindi remain unavailable. Select Nemotron 3.5 for Hindi, automatic language selection and live partials through NeMo-Speech.cpp. Parakeet remains an offline fallback and does not support Hindi. [models/catalog.json](../models/catalog.json) is informational; `vesper-models` contains the executable manifest with pinned revisions, byte sizes, and SHA-256 hashes. Model storage defaults to `models/` under Tauri's `app_data_dir`, or the configured override. Do not commit downloaded model weights.

[prompts/cleanup.txt](../prompts/cleanup.txt) documents the faithful-cleanup policy. The core configuration supplies the actual preset/custom instruction; the Qwen adapter applies its chat template with thinking disabled. Raw transcription is published and saved before cleanup so that a cleanup failure does not erase it.

## Dataset storage and export

This section is the operational summary. [Application internals](internals.md) is the canonical reference for the exact SQLite schema, session JSON fields, filesystem paths, write ordering, durability, privacy, limits, and recovery behavior; [ADR-0002](adrs/0002-split-persistence-by-data-shape.md) records why persistence is split this way.

Desktop dataset files live in Tauri's `app_data_dir` under `dataset/`: `sessions.sqlite3` holds records and `audio/` holds imported original files. SQLite uses WAL, so its companion `-wal` and `-shm` files may be present. Use the export function to produce a portable snapshot instead of copying a live database file alone.

History calculates 7-day/30-day activity from saved session dates in the device’s local timezone. The overview counts recordings, words in the preferred saved transcript, reviewed recordings and archived audio bytes. Empty days are included; invalid/future dates are excluded from activity totals. It does not infer recording duration from compressed file sizes. The line graph uses smooth cubic curves through the daily counts, with bounded tangents that prevent overshoot; empty days stay at zero and flat runs remain flat. Its secondary statistics show active days, average words per non-empty preferred transcript, and recordings with/without saved text in the selected period. With no text, the average displays an em dash. Clicking a graph point filters the recording list by local date, and switching periods clears that date filter. Arrow keys, Home/End and Enter work on the graph. Overview totals describe the selected period independently of list search/review filters.

History displays 10 recordings per page by default, with 10/25/50 choices and Previous/Next controls. The result range and page count follow the complete search, review and date-filtered set. Changed filters, page size and new imports reset to page one; refresh or review clamps a page that no longer exists. Page navigation focuses the result count and brings it into view. Per-record review drafts and the current page survive tab changes; Recent thoughts opens its selected recording on page one. Statistics and exports are independent of the displayed page.

Pagination currently limits rendered rows only. `list_sessions` still reads the complete saved history for browser-side search, filters and statistics. It does not yet bound database reads, IPC payloads or full-history memory; database paging and aggregate queries require a separate data-flow proposal.

History searches filenames and all generated/reviewed transcript fields, with separate All, Needs review and Reviewed filters. History uses one full-width list with inline recording details; only one recording is expanded at a time. Rows show a transcript preview and local date; files without generated text remain visible as Audio only or No transcript. Generated UUID filenames display as “Voice recording”; original filenames remain available in Audio file details and searchable. The transcript and copy action come first; **Review and remember** is collapsed until opened. Rows identify unsaved drafts without treating them as reviewed. Saving a label while filtering for Needs review removes that recording from the results and returns keyboard focus to the updated result count. Review drafts are kept in memory per recording and label purpose while navigating or filtering, but do not survive an app restart. **Use model text** only fills a draft. Empty model transcripts cannot receive a cleanup label through the UI. Export always uses saved data and its reviewed-only setting, independently of search and list filters. Browser preview contains no sample recordings.

Archival imports copy original bytes without transcoding, with a 512 MiB per-file limit and a nonempty regular-file requirement. Accepted archive extensions are `wav`, `mp3`, `m4a`, `flac`, `ogg`, `opus`, `aac`, `aif`, `aiff`, `webm`, and `mp4`. The transcription path archives the original first when collection is enabled, then decodes it with Symphonia. WAV, MP3, FLAC, AAC/MP4, ALAC, AIFF, and Vorbis are supported; a container such as Ogg/WebM may contain an unsupported codec. Opus decoding is not supported. Decoder errors and duration overflow are reported explicitly rather than silently truncating imported speech.

Capture uses a bounded lock-free callback buffer and preserves native-rate/channel PCM as a 32-bit float WAV before mono mixing and filtered 16 kHz resampling. Device sample formats are normalized to f32 during capture; inputs with more than 24 significant bits may lose that extra precision. A capture error retains usable complete frames with an early-end warning; downstream code must show and archive that warning. The capture worker itself does no inference. Recording limits apply both to captured sample count and elapsed time. Stopping processes the recording; cancelling explicitly discards the current capture.

The `vesper-data` API separates these operations:

- `set_transcripts` stores original raw/cleaned predictions and model-version metadata once, rejecting later overwrites.
- `approve_transcription` stores a human-verified **verbatim** transcript for ASR supervision.
- `approve_correction` stores human-approved **edited** text for cleanup supervision.
- Both review methods append a timestamped revision to `reviewHistory`. Text is limited to 100,000 Unicode characters; human labels must be nonblank.

An export contains `manifest.jsonl`, `metadata.json`, and `audio/`. Each JSONL record preserves the session and review metadata and adds explicit training fields:

| Field                            | Eligibility                                                       |
| -------------------------------- | ----------------------------------------------------------------- |
| `asrTarget`                      | Human-verified verbatim transcript                                |
| `cleanupInput` / `cleanupTarget` | Original raw prediction paired with an approved edited correction |
| `taskEligibility`                | `asr`, `cleanup`, both, or neither                                |

Training export uses `approved_only = true`, excluding records without any task-eligible label. An approved edited sentence with no raw prediction is preserved in archives but does not form a cleanup pair or ASR label. Archival export uses `false` to retain all sessions, including unreviewed predictions with null training targets. Export destinations must be new or empty directories outside the dataset store. Existing nonempty exports are never merged or overwritten.

No training runs automatically. The optional `training/cleanup` uv project tests checkpoint-specific adapter conversion and numerical compatibility; it is not an application training runner. Python is not needed for the current application or dataset storage. See [the self-improvement plan](self-improvement.md) for the evaluation and promotion workflow.

## Manual runtime checks

With the selected model files installed, use a short audio fixture to exercise both native runtimes. Verify repeated requests do not carry over prior text, raw results survive cleanup failure, and cancellation is reported after the current non-interruptible native call returns. Manual microphone tests require an explicit Record action: check permission denial, a removed device, manual stop, cancellation, adapter/resource stops, and app close during capture. These checks are not simulated by the unit-test pass.

Measure model load time, time from recording stop to final text, and peak memory across the speech and cleanup stages. Compare generated text against a verbatim reference and a separately approved cleaned reference. See [architecture.md](architecture.md) for the pipeline boundaries and remaining work.

### Separate Hold and Toggle shortcuts

Settings → Quick capture exposes both actions at once. Click the shortcut field beside Hold to talk or Press to toggle, press the desired key combination; the pair activates when it autosaves. Keyboard users can focus the field and activate it with Enter or Space. Escape restores the draft bindings present when editing began; Tab or clicking away ends key recording while keeping a valid draft, which autosaves. The existing Restore defaults action restores the default bindings. There is no separate customization panel or preliminary confirmation. See [UI interaction design](ui-design.md) for the shared design rule and official reference patterns.

Custom bindings require at least one of Ctrl, Command/Super, or Alt/Option, with optional Shift. Supported keys are A–Z, 0–9, Space, F1–F24, arrows, Home/End, Page Up/Down, and the supported punctuation keys. They refer to physical key positions understood by the existing native runtime; letter labels follow the A–Z key-code names. Fn, modifier-only bindings, Escape, and text-entry keys such as Enter/Tab are not offered. Escape remains the recording Cancel action. OS-reserved or unavailable combinations may fail registration; choose another binding when that happens. Physical delivery, keyboard layouts, and system conflicts still require native qualification on each OS.

Hold and Toggle must use different combinations. Checks compare modifier identities on the current platform, including CommandOrControl versus Command on macOS or Ctrl elsewhere, and ignore modifier order. Saving stores both explicit bindings. Old single-shortcut settings keep that shortcut for its previous action; the other action receives an unused default. Native registration canonicalizes both old and new pairs with the installed global-hotkey parser before comparing them, so an equivalent spelling cannot unregister a retained binding. Replacement is transactional: a conflict or release failure rolls back newly registered bindings and attempts to restore removed ones. A rejected save keeps the draft untouched and the sidebar pill stays on Saving… while it retries; choosing a registrable combination lets the next write land. Client-side duplicate checks catch most conflicts before they reach the backend.

While a field records keys, the existing global-shortcut callback routes registered combinations to that main-window editing session instead of starting dictation. Other supported combinations arrive through DOM keyboard events. The editor waits for native readiness, validates both paths, ignores stale session events and repeated keydown, and releases its guard on blur, navigation or unmount. Native window focus loss/close independently clears the guard; captured repeat/release events are consumed through key release. No new global keyboard hook or shortcut-registration pause is introduced.

Hold and Toggle maintain independent pressed states. Toggle release cannot finish a Hold session; unrelated Hold presses/releases cannot stop a Toggle or menu session. Repeated events cannot start another operation. Finishing a started recording remains operation-scoped.

With the current streaming adapter, a cold recognizer loads before microphone capture starts. Wait for **Listening** before speaking. Releasing during that load prevents the microphone from opening afterward; it does not transcribe an empty recording after the key was released. A release just after microphone startup still finalizes that recording once. No prewarming, capture-first buffering, or claim to capture speech before readiness is implied.

Live Nemotron captions currently appear in the recorder. Compatible external fields receive one final automatic paste when enabled; completed-recording adapters produce output after Stop. Continuous inline insertion has a separate approval-pending proposal in [ADR-0014](adrs/0014-native-dictation-and-final-insertion.md).

## Automatic Apple Silicon runtime packaging

The shared `scripts/desktop.mjs` launcher drives normal desktop dev/build and `make build-debug`. Native arm64 macOS builds include the existing pinned MLX library, kernels and tokenizer tool; install full Xcode with Metal support and Swift 6.2 or newer. A Swift build failure is not silently converted into a CPU build. `VESPER_MLX=0` (legacy `TRANSCRIBE_MLX=0` remains accepted) selects the portable build deliberately. Intel/universal and non-macOS targets remain feature-off. Pass custom targets as Tauri `--target` arguments; target settings after the Cargo separator are rejected. The launcher preserves the normal app/DMG release targets and only debug invocations request app alone. See [accelerator packaging](accelerators.md#building-the-apple-silicon-application) for provenance, signing and standalone evaluator constraints. `node --test scripts/desktop.test.mjs` exercises target/argument policy without invoking compilers.

### Missing Swift build/cache directories

An I/O error while reading `.tooling/swift-cache` can mean that a directory is
a dangling symlink into a deleted temporary checkout. Before invoking Swift,
the MLX build script checks its three generated directories: `mlx-build`,
`swift-cache` and `mlx-runtime` under `.tooling`. Missing directories are created
locally. Dangling symlinks are moved into a uniquely named
`.tooling/mlx-directory-recovery-*` folder and replaced with real local
directories; their deleted targets are not recreated.

Run `make dev` again after updating the build script. Rebuilding an empty cache
requires network access to the pinned Swift dependencies and takes longer than
an incremental build. Existing directories, valid directory symlinks and their
contents are retained. Files, non-directory link targets and other filesystem
errors still stop the build; this recovery is not a workaround for disk or
permission failures. No model files, recordings or application preferences are
removed. `node --test scripts/mlx-build-directories.test.mjs` covers recovery
and preservation without downloading dependencies or invoking a compiler.
