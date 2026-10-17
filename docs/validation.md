# Validation — 15 September 2026

Screenshots mentioned below are local validation captures excluded from source
control. The written results and reproducible test fixtures remain in the repository.

Pre-Vesper names in dated measurements and quoted commands identify the tested
revision. Current reproduction commands use `vesper-*` and `VESPER_*`; see
[naming compatibility](development.md#product-name-and-compatibility). Renaming
does not transfer old qualification claims to the current build.

## Vesper technical identity — 26 September 2026

The full project-owned namespace migration was checked on Apple-silicon macOS:

- Workspace/all-target compilation, strict Clippy, Rust tests/doctests and public
  documentation checks passed (348 tests passed, 36 explicit installed-model
  tests ignored), plus the new fixed-vector persisted audio-identity regression.
- Fifteen Python test suites, the three browser scenarios (including the
  existing mocked-native recovery suite), two Swift tests and native completion
  report tests passed. Fast checks include storage/override precedence, corrupt
  current-state handling and a current-namespace regression guard.
- A separate isolated browser loaded synthetic legacy French settings without
  creating a Vesper key. Explicit Save wrote the Vesper key and left the prior
  settings unchanged. No real user browser profile or recordings were used.
- Both pinned native runtimes were genuinely rebuilt. The debug bundle contains
  `com.vesper.desktop`, executable `vesper-desktop`, `libVesperMLX.dylib` and the
  six `vesper_mlx_*` exports. The microphone purpose string still names Vesper.
- Parsed comparisons of both Cargo locks, all three Python locks and the npm
  lock confirmed that only local package names/order changed: third-party
  versions, hashes, sources and dependency edges were preserved.

This is namespace/build/compatibility validation, not new ASR quality evidence,
permission-prompt qualification, a launched native UI check, notarization or a
published release. macOS may require new permissions for the new app identity.
Existing user data is reused in place; historical benchmark records remain
historical. See [compatibility and cache upgrades](development.md#product-name-and-compatibility).

The historical ten-second qualification statements below are superseded by
[long Qwen/Cohere support](#long-qwen3-asr-and-cohere-audio).

## Models, quick capture, and duration policy

[Issue #26](https://github.com/rahulsharmavishwakarma/vesper/issues/26) removes
the JSON profile import section and obsolete instructions. Saved custom profiles
and the curated speech/writing selectors retain their existing persistence,
validation, and explicit-download paths. The Models screenshot
shows the resulting page.

[Issue #27](https://github.com/rahulsharmavishwakarma/vesper/issues/27) reduces
the floating window to 440 × 96 logical pixels, replaces the large wordmark and
completion checkmark action with status text and Record/Stop/View, adds elapsed
time, and keeps the measured waveform in the compact signal area. View navigates
the main window to Dictate and loads/focuses the latest transcript. Completion
dismissal pauses while hovered or focused. State screenshots
cover idle, hover, focus, active, recording, quiet input, disabled/processing,
error, and completion. A disposable preview remains outside the repository.

[Issue #28](https://github.com/rahulsharmavishwakarma/vesper/issues/28) implements
[ADR-0009](adrs/0009-no-general-transcription-time-quota.md): no general recording
or import time quota. Old saved durations are readable but ignored. The UI removes
the duration editor and identifies the remaining Qwen3-ASR/Cohere ten-second
restriction as an adapter limit. Existing resource checks remain; the capture
budget reserves space for a WAV header within the archival file limit.

Verification used synthetic audio, settings, and native commands:

- `make check` passed: 120 Rust tests, 49 JavaScript/tool tests, native completion
  report checks, formatting, strict Clippy, and zero Svelte/TypeScript diagnostics.
  Nine opt-in native/model tests were not run for this change.
- All 13 actual-component browser scenarios passed, including preserved custom
  models, exact download routing, quick-capture burst/retry protection, View
  opening the main window, and loading the latest transcript from Settings.
- A synthetic 601-second WAV imported in full at 16 kHz with no quota, while the
  same file was rejected under a ten-second adapter cutoff. A callback test
  verified that a resource stop preserves complete captured frames and reports
  the reason; the WAV header fits the archival budget. All 17 audio tests also
  passed after the final header-budget adjustment.
- Quick capture has no horizontal overflow at 320, 375, 414, and 768 pixels;
  visible buttons have 44-pixel base targets and single-line labels. Keyboard
  focus uses a visible 2-pixel outline. Reduced motion keeps processing-wave
  geometry stable. Measured contrast is 6.75:1 for supporting text, 10.13:1 for
  Record/View, and 8.50:1 for Stop.
- `make build-debug` built the macOS app. Its recipe now sends `--debug` to Tauri
  and retains Cargo's `--locked` flag. A test-only runtime helper is compiled only
  in tests so strict Clippy remains clean.

No microphone or long-form model inference was used. Browser checks do not
qualify native window compositing or arbitrary-duration transcription accuracy.
macOS transparency uses Tauri's private API option; see the direct-distribution
constraint in [development](development.md). All edits remain unstaged and
uncommitted.

## Qwen3-ASR and Cohere integration

The approved [issue #14](https://github.com/rahulsharmavishwakarma/vesper/issues/14) integration was qualified on Apple M5 Pro, 64 GiB RAM, macOS 26.6.2 (25G83), CPU with eight native threads. The source-pinned Sherpa 1.13.8 patch/build, artifact hashes, outputs and individual timings remain in `.cache/benchmarks/results/qwen-cohere-native/debug-summary.json`; fixture attribution and reproduction commands are in [offline ASR](offline-asr.md).

| Adapter | Native load | Native decode | Peak process RSS | Post-stop backend pipeline |
| --- | ---: | ---: | ---: | ---: |
| Qwen3-ASR 0.6B INT8 | 0.90 s | 0.46 s | 1.53 GiB | 3.02 s |
| Cohere Transcribe INT8 | 1.63 s | 0.46 s | 4.14 GiB | 6.89 s |
| Parakeet v3 INT8 regression | 0.54 s | 0.15 s | 1.10 GiB | Not measured in this run |

These are separate single observations on the public 8.76-second English FLEURS fixture, with per-job native loads and possibly warm filesystem caches. Native probe timings exclude hash verification, archival, cleanup and UI/IPC. Post-stop timing calls the actual capture-processing backend with public PCM: it includes archive persistence, artifact verification, model load and decode, with cleanup disabled, and excludes microphone/device-stop/UI/IPC time. It is not a measured live user-visible latency. File-import pipeline times were 3.05 s and 6.91 s respectively. Peak RSS comes from a separate process for each native probe. Power mode and sustained thermal behavior were not measured.

All speech cases reached EOS without input-context loss: English/French for both models, Hindi and automatic-language English for Qwen, plus English padded to exactly ten seconds for both. One- and ten-second digital silence returns empty through the application's exact-zero guard. One extra sample beyond ten seconds is rejected before loading. Forced Qwen output/context exhaustion and a reused Cohere stream with an invalid language reject incomplete/stale results. Native helper tests cover final-boundary EOS, report isolation/reset, context-loss retention, all-nonfinite rows and Cohere mixed NaN/positive-infinity rejection while preserving negative-infinity masks. Not every native failure branch was induced with real weights; repetition/unknown report handling is additionally covered at the application gate.

The patched and original native builds return identical English text for Qwen, Cohere and Parakeet. Completing decoding does not establish accuracy: both new models misrecognized the German word “Sie” in the English sentence, and Qwen's French/Hindi samples contain substitutions. No general WER or quality claim follows from this small smoke set. Initial Qwen inference hallucinated words on exact digital silence, which prompted the explicit app guard; nonzero quiet/noisy input remains unqualified.

At this historical qualification checkpoint, the desktop integration test verified byte-identical archival of imported audio, raw prediction persistence, exact model/artifact/native build provenance, and exports without automatic ASR/cleanup training labels. `make check` passed 93 Rust tests, 33 JavaScript tests, native report tests, strict Clippy/formatting, Svelte/TypeScript and the production frontend build. Three opt-in model/pipeline tests passed separately. All seven browser recovery/model scenarios passed, including save-before-download and Cohere language restrictions. The menu then offered eight ASR choices; it now offers nine after LFM2.5 Audio integration. The former whole-recording Qwen ten-second limit has also been replaced by bounded long-audio segmentation. `make build-debug` produced the macOS arm64 app bundle with its microphone usage description; the final 13 native archive hashes were reverified.

Qualification is local macOS arm64 evidence. CI is configured to source-build and check macOS/Linux, but no remote CI or Windows qualification was run. The source-build helper does not yet cover Windows. Qwen3-ASR 1.7B and Microsoft integration remain separate work. No microphone audio was captured, and no files were staged or committed.

## Resident Nemotron streaming baseline

The Phase 1 lifecycle smoke uses the pinned 3.845-second public English fixture and NeMo-Speech.cpp 0.1.0 C ABI. It sends 320-sample (20 ms at 16 kHz) non-overlapping chunks through two consecutive fresh streams on one resident recognizer. Both operations returned the expected punctuated transcript; their event operation IDs remained isolated, sequence numbers were strictly increasing within each operation, and each last event was final. The test completed in 2.59 seconds on the development Apple M5 Pro using CPU after the sandbox could not create a Metal command queue. Audio was pushed as fast as accepted, so this is a lifecycle/throughput smoke—not a measurement of live first-partial or user-visible latency.

The full native smoke also verifies the resident recognizer's offline file call, resident Qwen weights with a fresh cleanup context, raw-before-cleanup persistence, original-audio byte equality, artifact provenance, dataset export, and control-token rejection. It passed in 14.22 seconds in this headless development environment; that aggregate includes builds, model loads, hashes, assertions, three separate test processes, and fallback retries, so it is not an interactive latency result. `Auto` now retries CPU when either native accelerator cannot initialize; explicit accelerator selections still fail rather than silently changing the request.

## Historical Nemotron-default and Metal smoke

Before the resident-worker migration, the installer downloaded and verified the official Nemotron Q8 GGUF (`742,090,464` bytes), NeMo-Speech.cpp 0.1.0 Apple Metal archive (`3,465,028` bytes), and existing Qwen Q4 GGUF (`563,036,064` bytes). The then-current complete release-mode pipeline ran on the Apple M5 Pro against the same 3.845-second public fixture:

| Measured operation | Time |
| --- | ---: |
| Nemotron Metal load + recognition | 1.09 s |
| Qwen Metal load + cleanup, including first-run Metal kernel compilation | 6.76 s |
| Complete native pipeline | 7.85 s |

Nemotron returned “Ask not what your country can do for you. Ask what you can do for your country.” Qwen's minimal cleanup preserved it exactly. Runtime provenance reported `metal` for both stages. An ASR-only rerun took 1.07 seconds including model load. These are historical single warm-filesystem observations, not distributions; the current resident C-ABI path requires new live-path distributions before making a performance comparison.

The first real run also found an adapter defect in the retired CLI path: NeMo's CLI rejected float WAV input. The current C-ABI path accepts float samples directly, streams microphone chunks at the device rate for runtime resampling, and still archives original captured audio as float WAV.

The native Parakeet → Qwen pipeline ran on an **Apple M5 Pro with 64 GiB RAM**, using CPU execution, Parakeet INT8, and Qwen3.5-0.8B Q4_0. No live microphone audio was captured during validation.

## Real model results

The pinned [public English speech fixture](https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/blob/2bda32ec70b097a55adaa07d9a7173915b43cc78/test_wavs/en.wav) is approximately 3.845 seconds, sampled at 24 kHz. Our decoder/resampler converted it to mono 16 kHz before inference. Its SHA-256 is `148b936b43ce7c546a866e64da059f0458aee2d65e617f16e9d94f06e8d99ed6`.

| Measured operation | Time |
| --- | ---: |
| Parakeet load + recognition | 0.76 s |
| Qwen load + cleanup | 0.70 s |
| Native model pipeline | 1.46 s |
| Desktop service integration test, including full model hashes, audio preservation, both models, SQLite and export verification | 3.63 s |

These are individual development-machine observations, not latency distributions. Native stage measurements exclude model download, microphone recording, UI/IPC, and per-request artifact hashing. They include per-job model loading; the filesystem cache may be warm. The integration measurement includes test assertions and export work. Peak process memory was not measured.

Both ASR and final minimal cleanup returned:

> Ask not what your country can do for you, ask what you can do for your country.

The raw and cleaned result plus inference settings remain in `.cache/benchmarks/results/native-pipeline/debug-summary.json`. The desktop test also checks original audio byte equality, pinned artifact provenance, and absence of machine-created ASR/cleanup training labels in its export.

Unoptimized Rust checksum code initially made the same integration test take 38.59 seconds. Development optimization for `sha2` and `transcribe-models` reduced that to 3.63 seconds while retaining full hash verification. Release builds already use optimization.

## Cleanup quality findings

Early temperature-0.7 output dropped the negative clause from the public sentence. Another balanced output changed an imperative into a statement. These were real failures, which led to a clearer task prompt, three punctuation examples, deterministic greedy decoding, output limits, and conservative fidelity checks.

On six subsequent synthetic cases not used as prompt examples:

- Minimal accepted five with the normalized word sequence unchanged; one repetition loop reached the token limit and retained raw text.
- Balanced accepted four; one changed number words and was rejected, and one repetition loop reached the token limit.
- Accepted examples preserved the tested names, amounts, time, negation, and uncertainty. Some minimally cleaned outputs still lacked useful punctuation.

Exact inputs, outputs, errors, and timings are preserved in [cleanup-cases.json](validation/cleanup-cases.json). This small development suite is **not a held-out quality benchmark**. The guards cannot establish semantic equivalence; balanced output still requires review. Q8_0, broad language coverage, noise, long recordings, and representative user dictation have not been evaluated.

### Guarded technical-tail regression — 15 September 2026

> **Historical; invalidated 14 October 2026.** This result depended on a
> fixture-specific prompt rule and a built-in `README.txt` fallback. Without them
> the model left the tail unchanged, and the pass was retired
> ([ADR-0008](adrs/0008-guarded-technical-tail-correction.md#retirement)).

The installed Qwen3.5 0.8B Q4 model passed the opt-in regression for a long
synthetic dictation ending with an ASR-like misspelling of `README.txt`. Context
correction changed only that nearby filename token and retained the trailing
words. Unit tests additionally reject multiple technical candidates, broader
model rewrites, changed numbers, and changed negation; an accepted correction is
reapplied if the subsequent full cleanup pass omits it.

One local CPU invocation of the cleanup example completed in 1.666 seconds and
returned `README.txt` for the reported long-form pattern. This is a single
one-shot observation that includes example-process setup, not a latency
distribution or live streaming measurement. Formatting-only cleanup remains the
default and does not run the preliminary technical-tail pass.

## Automated and UI checks

All 69 workspace tests passed, along with 16 JavaScript tests, strict Clippy and formatting checks. They cover configuration migration/validation, quick-capture preferences, synthetic compressed-audio decoding, sample limits/resampling/tail behavior, sidecar PCM16 staging, partial capture recovery, microphone RMS telemetry, immutable raw predictions, separate human labels, export integrity, pinned model/runtime inventory, Hugging Face profiles, cleanup guards, cancellation state, stale recording watchers, close preservation, and recovery WAV retention on archive failure. Native unit tests never open the microphone; the real model integration test is explicitly ignored in ordinary CI and was run separately with installed weights.

Frontend type/Svelte checks and Vite production build pass. Browser preview QA checked disabled native actions, model/language validation, settings persistence/reset, clean JavaScript console, and a 390-pixel mobile layout without horizontal overflow. This is browser-preview QA, not a live native microphone test.

macOS native linking and `.app` bundling were tested. The bundle must carry the microphone usage description. Linux CI is configured with ALSA/WebKit/native build prerequisites, but no remote CI run or Windows build was performed in this session. Microphone permissions, actual device disconnection, native window-close interactions, and capture quality still require deliberate on-device testing.

<a id="transcribe-integration-update"></a>

## Historical branding integration (before Vesper)

This record predates the Vesper identity. The old display names and bundle paths
below describe the tested build, not the current product; current branding is
documented in [UI design](ui-design.md#selected-identity).

After adopting the Studio UI, custom controls, RMS meter and then-current `transcribe` name, `make check` passed with 69 Rust tests,
16 JavaScript tests, no Svelte/TypeScript errors or warnings, formatting and strict
Clippy. `make build-debug` produced `target/debug/bundle/macos/transcribe.app`; its
Info.plist identified `transcribe`, `com.transcribe.desktop`, the ARM64 `transcribe-desktop` executable,
the lowercase text-only app icon, and the then-current microphone permission description.

The explicit real-file pipeline test also passed after the rename, in 3.81 seconds
for this one integration-test invocation. It checks ASR, cleanup, original audio,
archive and export provenance. This is a smoke observation, not a performance
distribution or a GPU benchmark. The existing local GGUF also passed the custom
embedded-template formatting/tokenization smoke; third-party model generation
quality remains unbenchmarked.

Browser checks covered all four application views at 320, 375, 414 and 768 px,
settings/profile drafts across navigation, legacy preview settings fallback,
new-key precedence and saving. No horizontal overflow or clipped controls was
found. Custom menus passed pointer selection, keyboard arrows/Home/End/typeahead,
Enter/Escape/Tab, focus retention, form change propagation, disabled states,
outside dismissal, scrolling and upward placement. The portal stays above cards
and within the viewport at the four tested widths.

Browser interaction checks confirmed the 220 ms press effect for real mouse and
native Enter activation, with no global transform on menu triggers/options.
Disabled fieldsets remained inert. Enabling reduced motion cancelled a running
press animation and suppressed subsequent ones; the cancellation check slowed
one browser-only animation so it remained active across automation commands.
Synthetic recording styles followed supplied level values, while processing
bars animated separately. Reduced motion disabled their movement. These are
interaction/style checks, not a live microphone or native WebKit recording test.

Recording bars now consume callback RMS from the existing microphone stream.
Synthetic audio tests verify amplitude, silence, integer PCM normalization,
nonfinite input handling, reset after stop/cancel, and preservation of original
finite PCM. Runtime tests ensure telemetry is finite, clamped, and zero outside
recording. These checks did not open a microphone. Native jobs retaining their
state across view changes was reviewed in code: runtime panels remain mounted.

## Quick capture, notifications, and lowercase rename

Historical record — 14 September 2026, before Vesper: the product metadata, Rust/npm package names, window titles, export prefixes, app identifier, wordmark, native text-only icon, and macOS bundle used lowercase `transcribe`. Existing `com.ukti.desktop` and `com.stt.desktop` data remained readable in place. Stale Ukti/STT bundles and brand-symbol references were removed. Vesper subsequently replaced the display name/artwork and then migrated current technical namespaces, retaining explicit legacy data lookup; see [product naming](development.md#product-name-and-compatibility).

The new quick-capture window was checked at its native 520 × 118 logical-pixel size in idle, recording, processing, and completion states. Its recording trace uses the same RMS value as the main recorder; the processing trace is explicitly indeterminate. A browser-only query preview was used for visual inspection and did not open the microphone. The native Tauri build includes the global-shortcut and notification plugins, a hidden always-on-top `quick-capture` window, permission-gated privacy-safe notifications, and three validated shortcut choices.

On 14 September 2026, `make check` completed with zero Svelte warnings, 16 frontend/tooling tests, strict Clippy, and 69 passing Rust tests. Two explicit model-file smoke tests remained ignored. `make build-debug` produced only `target/debug/bundle/macos/transcribe.app`; its generated Info.plist identifies lowercase `transcribe`, `com.transcribe.desktop`, and the ARM64 `transcribe-desktop` executable.

## Settings recovery and quick-capture regression checks

The browser regression page mounts the actual Svelte components with synthetic
Tauri commands. It covers delayed start/stop shortcut bursts, a failed preflight
followed by a successful retry, preview controls, and a settings-load error followed
by explicit Restore defaults. Run a development server:

```sh
npm run dev --workspace @transcribe/desktop -- --host 127.0.0.1 --port 1428
```

Open `http://127.0.0.1:1428/tests/recovery.html`. All checks should report
`passed: true`; automation can read `window.regressionResults` and
`document.documentElement.dataset.testStatus`. The page never calls a real native
backend or accesses saved user settings. It is a manual browser test and is not
part of `make check`.

Before the quick-capture fix, six shortcut events produced six start requests and
six stop requests in the delayed scenarios. With the action guard claimed before
the first native await, each burst produces exactly one request. All five browser
checks passed. Backend tests use temporary files to verify corrupt/invalid settings
preservation, startup fallback, explicit reset despite a shortcut conflict, valid
custom shortcut handling, and rollback after a failed settings save. Normal settings
reads remain strict, including those used by inference. `make check` passed with
82 Rust tests, 26 JavaScript tests, no Svelte/TypeScript diagnostics, formatting,
and strict Clippy. The five browser results are captured in
recovery-checks.png. `make build-debug` also
rebuilt the then-named `target/debug/bundle/macos/transcribe.app` successfully, before the Vesper identity. Live microphone and actual
operating-system shortcut testing remain tracked in
[GitHub issue #7](https://github.com/rahulsharmavishwakarma/vesper/issues/7).

## Expanded model menu validation

The [preset catalog](model-presets.md) adds three ASR and five cleanup profiles,
giving six choices in each menu. All eight presets were checked against immutable
Hub metadata; seventeen artifact entries matched, including locally hashed small
token tables. No new model weights were downloaded or used for inference.

`make check` passed with 83 Rust tests, 32 JavaScript tests, no Svelte/TypeScript
diagnostics, formatting, strict Clippy, and the production frontend build.
`make build-debug` rebuilt the macOS application. The shared preset JSON passes
both frontend and native profile validation, settings persistence, and exact
installer-path/hash checks.

All six browser regression scenarios at `/tests/recovery.html` passed. The added
scenario selects FastConformer and SmolLM2 360M in the real UI, confirms selection
alone sends no save/download request, exercises save-before-download ordering,
checks exact model IDs and fixed-file precision, and remounts the application to
verify saved choices. These use synthetic Tauri IPC. A separate browser-preview
check saved and reloaded both choices in the isolated preview's local storage.

The ASR menu and
cleanup menu screenshots show the new choices.
The existing decoder/template contracts support these candidate profiles, but
full inference, transcription accuracy, cleanup fidelity, and performance on
each new artifact remain unverified; see the catalog's evidence boundaries.

## Local engine status validation — 15 September 2026

The Models page now includes the [Local engine panel](application-status.md).
`make check` passed with 104 Rust tests and 38 JavaScript tests, no Svelte/TypeScript
diagnostics, formatting, strict Clippy, native completion-report tests, and the
production frontend build. The explicit installed-model desktop pipeline also
passed, verifying positive ASR, cleanup, and storage timings alongside archived
audio and provenance. The debug macOS app was rebuilt.

Eight browser regression scenarios passed using synthetic IPC. The status scenario
checks first CPU/RSS readings, activity and partial timings, missing models, cleanup
off, error/retry, collapse, navigation, document visibility, and disposal. Separate
checks verified preview behavior, bounded/stale history, stalled requests without
overlap, keyboard focus, reduced motion, contrast, and 320/375/414/768px layouts.
The screenshot uses labelled synthetic data.

Six native sampler observations were compared with macOS `ps`: 148–727 µs per
reading (512 µs median), with subsequent RSS readings within 32 KiB of `ps`.
These measurements exclude IPC/rendering and are an idle-process observation,
not an inference benchmark. See the local measurement data at `.cache/benchmarks/results/application-status-resources/debug-summary.json`
and [scope, sources, and platform limits](application-status.md).

## Model download progress — 15 September 2026

Model controls and the Models-page banner now show structured progress for built-in
ASR, cleanup precision variants, imported profiles, and selected-pipeline installs.
The bar includes percentage, current filename/file count, and bytes completed
across the selected artifacts. Already verified cached files count toward that
total; it is not a network-throughput measurement. Preparation is indeterminate,
and checking/verifying/finishing phases remain distinct from a successful install.
Reaching 100% bytes does not unlock the model until installation returns successfully.

Fixed individual downloads stopping their status polling when the initial status
request preceded the native job. Cancellation stays visible while pending, and
completion/failure/cancellation clears structured progress before another operation.
Installer errors during inference unload also release the operation gate.

`make check` passed with 106 Rust tests, 39 JavaScript tests, clean Svelte/TypeScript,
formatting, strict Clippy, native completion-report checks, and the frontend build.
The macOS debug app rebuilt successfully. Nine browser regression scenarios passed;
the new scenario covers an imported cleanup model and the selected pipeline,
indeterminate preparation, 25/50/100% readings, verification, cancel, failure and retry.
Native tests use tiny loopback HTTP fixtures to cover cached/multiple files,
checksum failure, cancellation, temporary-file cleanup and operation isolation.
They download no model weights and use no private audio.

The progress screenshot uses labelled
synthetic data. The control was checked at desktop and 320px widths without
horizontal overflow; progress has accessible numeric/phase attributes, cancel is
a native keyboard button, and reduced motion disables progress animation.

## Device-aware accelerators and recorder cleanup label — 15 September 2026

The accelerator menu now uses the [native device/build capability snapshot](accelerators.md)
and existing model restrictions. The local native probe found Apple M5 Pro/Metal
on macOS; synthetic tests cover Windows/Linux/CUDA/Vulkan/ROCm, CPU-only builds,
missing devices and unsupported saved preferences. The UI preserves saved choices
until an explicit save and supports detection failure/retry.

The recorder's large Cleanup on button is replaced by the saved cleanup model name
and a quiet Change action. Unsaved model selections do not change that label;
disabled cleanup displays Raw transcript and links to writing preferences.

`make check` passed with 112 Rust tests and 43 JavaScript tests, formatting, strict
Clippy, native completion-report checks and the frontend build. A subsequent
Svelte/TypeScript check passed with zero errors and warnings after resolving a
concurrent model-library warning. The macOS debug application rebuilt successfully.
Eleven browser scenarios passed, including device detection retry, keyboard
selection of Automatic to recover saved CUDA on a Mac, and cleanup label updates
only after saved model/off changes. Existing language-control tests were updated
to their current IDs.

The accelerator menu and
cleanup label were checked at desktop and
320px widths without horizontal overflow; long imported model names wrap. These
UI screenshots use synthetic settings. No microphone or model inference was used
for this change; Windows/Linux native GPU execution remains unverified.

## Recorder model summary — 15 September 2026

The follow-up to [issue #24](https://github.com/rahulsharmavishwakarma/vesper/issues/24)
shows both saved choices under the recorder: Speech model and Writing model.
Writing model also replaces Cleanup model on the Models selector. When writing
is disabled, its configured name stays visible with an Off label. Manage models
always opens Models. The duplicate summary below recent history was removed.

Svelte/TypeScript reports zero errors and warnings, the frontend and macOS debug
app builds pass, and all eleven browser scenarios pass. The recorder scenario
checks both built-in names, unsaved versus saved imported choices, writing off,
and navigation without saving or starting recording. Existing diagnostics mocks
and the resource-polling scenario now match the always-visible Local engine panel;
navigation, document visibility and disposal still stop sampling.

The recorder screenshot uses browser preview
defaults. Desktop and 320px layouts were checked, including synthetic long imported
names with writing off and an unbroken name. Both names wrap without horizontal
overflow. No native settings, model files or microphone were used for these checks.

## Model wording and saved selections across screens

[Issue #24](https://github.com/rahulsharmavishwakarma/vesper/issues/24) now uses
shared Speech model / Writing model role labels throughout the interface. Dictate,
Settings and History reuse one configured-model summary; Local engine on Models
reads the same saved settings. Settings, model imports/downloads, History review,
quick-capture progress and native progress/errors use writing/text-polishing terms.
Stored configuration fields, model IDs, pipeline stage keys and export formats
retain their existing names.

The browser regression checks both saved model names and writing on/off state on
all four screens after draft edits, saving imported profiles, disabling writing,
and restoring defaults. Unsaved edits never replace the configured summary, and
Manage models navigation does not save settings or start recording. All eleven
browser scenarios pass. `make check` passes with 112 Rust unit tests, one Rust doc
test, 47 JavaScript tests, clean Svelte/TypeScript, formatting, strict Clippy, native
completion checks and the frontend build. The macOS debug app also rebuilt.
The loopback download fixture needed localhost permission for the check run.

Browser preview screenshots show Settings,
History, and Models.
All four screens were also checked at 320px with synthetic long imported names and
writing off, without horizontal overflow. The checks used isolated preview
preferences and mocked native commands; no private recordings or live inference.

## Compact recorder layout

[Issue #25](https://github.com/rahulsharmavishwakarma/vesper/issues/25) groups
microphone selection and the recording limit in a toolbar, places Record and file
transcription together, and pairs a clear recorder status with a smaller waveform.
The shared model summary remains in the footer. Empty microphone selections now
have an explicit label. Audio sampling and capture behavior are unchanged.

The desktop screenshot uses synthetic desktop
responses. Idle, recording, processing and error layouts were checked at desktop
and 320/375/414/768px without horizontal page overflow. Button text stays on one
line; recording-ring decoration accounts for extra button scroll width. The
isolated state preview also covers hover, focus, active, disabled, loading and
completed states. Reduced motion holds the waveform steady while Stop remains
available. Measured contrast is 10.13:1 for the primary button, 6.75:1 for muted
text, and 9.34:1 for the focus ring against the panel.

All eleven browser regression scenarios pass. Additional mocked interactions
verify keyboard selection of an external microphone, the selected device passed
to Record, one Start/Stop call, the returned transcript and the file picker.
Svelte/TypeScript reports zero errors and warnings; frontend and macOS debug
builds pass. The desktop build needed network access to fetch an existing locked
dependency. No real microphone, audio files or model inference were used.

## Downloaded model management — 15 September 2026

The standalone library and disclosure below describe earlier UI validation. The current interface integrates download status and removal into each model card; see the follow-up validation below.

[Issue #23](https://github.com/rahulsharmavishwakarma/vesper/issues/23) puts Local engine at the top of Models and adds an inventory of known downloaded weights, including unselected shipped presets and separate Q4/Q8 packages. The model-library screenshot uses synthetic desktop responses and sizes, not a user's actual downloads.

Validation passed: 23 `transcribe-models` tests, the desktop runtime operation-boundary regression, strict Clippy for the model and desktop crates, Svelte/TypeScript with zero errors or warnings, and 43 JavaScript/tool tests. The macOS debug app rebuilt successfully. New temporary-directory tests cover actual file-byte accounting, incomplete files, precision isolation, selected-model readiness after removal, unknown IDs/roles/precisions, stale model folders, shared weights/runtime preservation, unselected preset inventory, and symlink preflight without deleting earlier valid files. These checks do not delete or download user model weights.

Browser QA used the actual App with synthetic Tauri responses. Checked initial inventory and section order, confirmation naming/size, initial focus on Keep model, cancel and Escape with no removal call, pending-action locks, failure/retry, Q8 removal preserving Q4, selected-model removal disabling Record, active-recording locks, refresh failure recovery, empty state, and reduced-motion confirmation (zero animations). The expanded Local engine and library had no horizontal overflow at 320, 375, 414, 768 and 1180 pixels; the confirmation also fit at 320 pixels. Native file safety is covered by Rust fixtures; browser mocking is not evidence of deleting real model weights through the desktop WebView.

The inventory is scoped to the saved model directory, shipped catalog and currently configured imported profiles. Re-import a replaced arbitrary profile to manage its declared files. Shared runtimes and unrecognized files remain untouched. Reported bytes are logical file sizes rather than physical space recovered by the filesystem.

### Panel disclosure refinement

Local engine now uses a static section heading and stays open at the top of Models. Its former disclosure hover/pressed background and collapse chevron are removed. Downloaded models has its own transparent native disclosure, visible package count, keyboard focus and chevron transition; it starts open and retains the user's choice when navigating between tabs in the current session.

Browser checks with synthetic data confirmed that clicking/hovering the engine header cannot collapse or fill it; Enter/Space toggle only the library; returning to Models preserves the collapsed library; resource sampling stops off-page and restarts on return; and refresh/removal confirmation remain functional when expanded. Both panels fit at 320/375/414/768/1180px without horizontal overflow. Library content has a brief entrance animation that is disabled with reduced motion. Frontend checks and the desktop rebuild passed. The updated panel screenshot shows the rebuilt frontend in browser preview, with Local engine open and Downloaded models collapsed; it contains no live telemetry.

### Integrated model selection and downloads

The separate library is removed. Speech/writing dropdowns identify downloaded, missing, incomplete and unavailable packages. Writing-model status names the installed built-in precisions, and the Q4/Q8 dropdown checks each separately. Remove shares the model card with Download/Verify and reuses the existing confirmation and native deletion checks. Browsing an unused package and removing it no longer requires saving or activating that draft selection; mismatched model folders and imported declarations cannot expose stale status or removal actions.

Validation: 47 JavaScript/tool tests and clean Svelte/TypeScript. The macOS debug app rebuilt successfully with `make check-web build-debug`. New helper cases distinguish Q4 from Q8, download progress, unavailable inventory, unsaved folder changes, imported profile changes and shipped preset previews. Browser QA with synthetic Tauri responses checked both speech and writing menus, cancel/failure/retry, deletion with zero settings-save calls, precision isolation, selected-model warnings and disabled recording after removal, focus restoration, folder mismatch, inventory failure/recovery, keyboard operation and responsive layouts. No real model files were deleted. The integrated dropdown screenshot and model controls screenshot use synthetic download state.

## Reproduce

Run all three installed-model checks with:

```sh
make smoke-native
```

This explicit target requires the existing pinned public English fixture at `.cache/smoke/en.wav`, the selected Nemotron runtime/weights, and Qwen Q4 weights. It verifies the fixture checksum, keeps Cargo offline, uses temporary dataset storage, runs both offline and two-stream resident Nemotron paths, and never opens a microphone or downloads models. Override the model directory with `MODEL_DIR`; `TRANSCRIBE_SMOKE_MODELS`, `TRANSCRIBE_SMOKE_AUDIO`, and `TRANSCRIBE_SMOKE_GGUF` provide individual input overrides. The previous `STT_SMOKE_*` names remain accepted as fallbacks. The fixture must still match the pinned public recording. Input paths are resolved before Cargo changes to each crate's directory.

### Issue 007 validation — 14 September 2026

Both explicit checks passed on an Apple M5 Pro with 64 GiB RAM, running macOS 26.6.2 (25G83):

| Check | Result | Observed test duration |
| --- | --- | --- |
| Desktop Nemotron → Qwen pipeline, original audio, model provenance, and dataset export | Passed; both stages reported Metal | 4.26 s |
| Existing GGUF embedded-template formatting/tokenization and control-token rejection | Passed; CPU model loading | 0.92 s |

The pipeline smoke now requires a nonempty cleanup result, no pipeline warnings, and equality between the returned and persisted cleanup. Previously it could pass when cleanup failed and raw text survived. On this run, the raw and cleaned public-fixture transcripts were identical. Both training targets remained null in the archive export, and the archived audio matched the source bytes.

These are individual test durations, including assertions, rather than latency distributions. Peak memory was not measured. The first sandboxed attempt could not create a Metal command queue; the successful run had local GPU access. No private audio or live microphone was used. Permission grant/denial, device disconnection, a real shortcut conflict, and native close/notification behavior remain manual checks in [GitHub issue #7](https://github.com/rahulsharmavishwakarma/vesper/issues/7).

### Individual commands

From the repository root, after installing prerequisites and the pinned weights:

```sh
source scripts/dev-env.sh
cargo run -p vesper-models --example install -- models/downloads
cargo run -p vesper-inference --example pipeline -- models/downloads .cache/smoke/en.wav
VESPER_SMOKE_MODELS="$PWD/models/downloads" \
VESPER_SMOKE_AUDIO="$PWD/.cache/smoke/en.wav" \
  cargo test -p vesper-desktop real_file_pipeline_preserves_audio_predictions_and_export_provenance -- --ignored --nocapture
cargo run -p vesper-inference --example cleanup -- models/downloads \
  --minimal 'please do not transfer 2,500 rupees to Meera' \
  --balanced 'could Ananya call Vikram at 10:30 tomorrow'
```

Download the linked fixture explicitly to `.cache/smoke/en.wav` and verify its hash first; it is not committed. The checked-in audio decoder fixtures are synthetic tones and need no external decoder at test time.

## Native menu bar — 15 September 2026

Historical record: [issue #29](https://github.com/rahulsharmavishwakarma/vesper/issues/29) added a native macOS menu bar entry with the then-current `transcribe` text title. Vesper now uses the selected Songbird logo alone, with **Open Vesper** and the Vesper status tooltip, as documented in [UI design](ui-design.md#selected-identity). The menu continues to show runtime activity, Start/Stop recording, History, Models, Settings and Quit through the same quick-capture controller. Status refresh reads runtime activity every 500 ms and only changes native items when needed; it does not inspect model files or load weights. The measurements below describe the original implementation, not a new qualification run for the logo.

Closing either window now hides it. Main-window drafts and ongoing work stay alive, and the macOS Dock reopen event restores the main window. Explicit Quit is disabled during active work. Cmd-Q / Dock quit also require idle and bring Dictate forward while busy; they no longer cancel a recording as a side effect. An accepted exit marks shutdown while holding the runtime state lock, preventing a new recording/download from starting between the idle check and exit.

Validation passed: 33 desktop Rust unit tests (three opt-in tests skipped), including four new menu/lifecycle regressions; strict Clippy for desktop/all targets; Rust formatting; Svelte/TypeScript with zero errors or warnings; and 49 JavaScript/tool tests. The macOS debug app rebuilt with `make build-debug`. The shared Makefile debug target now places Tauri's `--debug` before the separator and Cargo's `--locked` after it; the former npm argument ordering failed before this correction.

Browser QA mounted the actual App against synthetic native responses. A Settings navigation pending before mount was delivered; History, Models, Settings and Dictate navigation all selected the expected screen. An unsaved collection preference survived every transition. Unknown and empty destinations were ignored. Navigation made zero settings-save calls and zero recording calls, and the browser reported no page errors. Temporary browser fixtures were removed after validation; no real settings, models or audio were changed.

Native state tests cover Stop availability during recording, disabled capture/quit for every processing/download/removal/cancellation stage (including unknown busy stages), error recovery, stale recording-stage text after completion, menu ID isolation, and atomic idle shutdown. Quit rejection leaves busy work uncancelled and its stage unchanged; accepted idle shutdown prevents new recording or installation.

macOS reported UI automation disabled (`System Events` returned false). Actual status-item rendering/clicks, menu keyboard accelerators, Dock/window reopening, and recording via the native menu remain on-device checks under [issue #7](https://github.com/rahulsharmavishwakarma/vesper/issues/7). Browser event simulation and native unit tests are not evidence of completing those OS interactions. Linux and Windows tray behavior was not tested.

## History pagination — 15 September 2026

The pagination follow-up in [issue #12](https://github.com/rahulsharmavishwakarma/vesper/issues/12) limits the History list to 10 rows by default, with 10/25/50 page sizes, Previous/Next, current page and an exact visible result range. Search, date/review filters, stats and export continue to use the complete loaded history. Page changes preserve review drafts. Filter and size changes/imports reset to page one; refresh and review clamp pages when results shrink.

Validation passed: clean Svelte/TypeScript and 52 JavaScript/tool tests, including new pagination boundary/order, empty/shrinking-input and cross-page filtering tests. The macOS debug app rebuilt successfully. No Rust data or inference changes were needed for pagination.

Browser QA mounted the actual App with synthetic native responses and 53 recordings. It checked first/last/previous pages (including 51–53 of 53), focus restoration, search finding a record beyond page one, clearing search, review/date filters, 10/25/50 selection through the keyboard, per-record unsaved drafts, refresh shrinking six pages to two, refresh failure preserving the previous list, and reviewing the only remaining record on the final Needs review page. The last review correctly returned to page one with ten remaining matches. Imports and Recent thoughts both opened the intended record on page one. Empty history hid pagination. A search showing one match still exported all 53 saved examples when reviewed-only was disabled; the export command received no page/search restriction. No real recordings, labels, exports or settings were modified.

At 320/375/414/768/1180px, controls stayed within the viewport without horizontal overflow and retained at least 44px hit targets. Reduced motion produced zero-duration row animations and arrow transitions. The desktop and narrow layout screenshots contain synthetic records. Browser checks ended without page errors.

This is display pagination: the existing `list_sessions` command still loads the full history. The change bounds rendered rows, not database/IPC work or memory used by all loaded records. Database-level pagination and aggregate-stat queries remain outside this UI follow-up.

## UI wording cleanup

[Issue #31](https://github.com/rahulsharmavishwakarma/vesper/issues/31) removes the waveform caption, repeated recorder/setup instructions, benchmark boilerplate and model implementation explanations from normal controls. Language guidance, writing preferences, model status, History feedback and Quick Capture now use shorter user-facing text. Local engine keeps measurement scope and timing definitions; recording constraints, download progress, errors, unsaved changes and retention information remain visible. Control behavior and stored settings are unchanged.

Validation passed: Svelte/TypeScript with zero errors or warnings, 52 JavaScript/tool tests, all 13 existing browser regression scenarios, and the macOS debug app build. Browser preview checks covered Dictate, Models, Settings and History at 1280px and 320px without horizontal overflow. Visible controls had no missing label/description references. Quick Capture fit its 440 × 96 viewport. The Dictate and Settings screenshots use isolated browser preview preferences; regression scenarios use mocked native responses. No private audio or live inference was used.

At the time of issue #31, the Qwen/Cohere ten-second guard remained in place. It is now superseded by the approved [issue #32](https://github.com/rahulsharmavishwakarma/vesper/issues/32) implementation; see [long-audio validation](#long-qwen3-asr-and-cohere-audio).

## Long Qwen3-ASR and Cohere audio

The approved [issue #32](https://github.com/rahulsharmavishwakarma/vesper/issues/32)
and [ADR-0011](adrs/0011-segment-long-generative-asr.md) remove the remaining
whole-recording/import time cutoff. Qwen and Cohere now process contiguous
bounded parts through fresh streams on one recognizer per job. No samples are
overlapped, discarded, or inferred from transcript alignment. Every part must
pass the existing native completion check before the complete raw result can be
published. Sample coverage is reported as processing progress.

The local debug record at `.cache/benchmarks/results/long-asr/debug-summary.json` includes model/runtime
revisions and hashes, the evaluated executable fingerprint, fixture references
and hashes, all scores and hypotheses, process RSS, native isolation results,
and the initial failed long recording. All audio is public: Google FLEURS test
(CC-BY-4.0, Google/Conneau et al.) and MUCS 2021 Hindi-English test (CC-BY-SA-4.0,
Microsoft Research India/OpenSLR 104). No private audio or microphone was used.

On Apple M5 Pro, 64 GiB RAM, macOS 26.6.2 (25G83), CPU with eight native threads,
all 22 final benchmark samples completed: five four-sample FLEURS manifests plus
the eight-second and 413-second MUCS pair. FLEURS compares the original short
utterance, three continuous copies, those copies shifted by two seconds of
leading silence, and a separate natural utterance above ten seconds. Repeated
inputs are synthetic stress cases. The separate natural clips are 10.56 seconds
(English), 10.20 seconds (French), and 13.80 seconds (Hindi).

| Model / language | Original WER | Three copies WER | Shifted copies WER | Natural long WER |
| --- | ---: | ---: | ---: | ---: |
| Qwen / English | 9.52% | 9.52% | 9.52% | 5.26% |
| Qwen / French | 34.62% | 28.21% | 29.49% | 15.00% |
| Qwen / Hindi | 25.00% | 25.00% | 23.61% | 17.50% |
| Cohere / English | 9.52% | 9.52% | 9.52% | 21.05% |
| Cohere / French | 11.54% | 19.23% | 23.08% | 5.00% |

These small development measurements are not a general accuracy benchmark.
Cohere's French repeated/shifted cases worsen relative to the original: fresh
segment context and cuts can affect words. Nothing was deduplicated to improve
scores, and segment thresholds were not tuned on these fixtures. The long MUCS
output also contains substitutions, including a font-size number. Complete
native decoding does not establish meaning preservation. Different scripts for
English terms affect the mixed-language WER/CER comparison.

The first 413-second MUCS attempt failed at part 12 on Qwen's independent
128-output-token ceiling. It returned an error instead of a successful prefix.
That failure is preserved in the record. Qwen now sets output allowance equal
to its existing 512-token total context allowance through the public API; the
context guard, KV allocation, weights and native reporting patch are unchanged.
This structural correction was checked against the exact pinned source, and
the same manifest was rerun. The affected MUCS recording is an exploratory
regression after that correction, not untouched held-out evaluation. No further
segment-size or decoder threshold tuning was performed.
The final 413-second run completed its manager ASR stage in 57.78 seconds,
with 27.81% WER; its two-sample process peaked at 2.02 GiB RSS.

For the four-sample FLEURS processes, peak RSS was 1.75–1.92 GiB for Qwen and
4.18–4.24 GiB for Cohere. Per-sample manager ASR time was 3.23–7.21 seconds for
Qwen and 7.42–8.44 seconds for Cohere. This timer includes artifact verification,
per-job load and decode, and excludes import/capture, archive, writing and
UI/IPC. Process RSS includes input/scoring buffers and is not model-only memory.
Runs were sequential, filesystem caches may be warm, and the native libraries
use Release optimization with Rust debug orchestration. These are single
observations; power mode and sustained thermal behavior were not measured.

The native long qualifiers additionally passed 12 production-path cases,
including Qwen Hindi with automatic language detection, first-part cancellation,
fresh A/B/A state isolation, and a forced failed second part followed by a clean
retry. The short Qwen qualifier passed with the final output setting, including
forced output/context failures and exact-zero silence. The desktop test passed
26.28-second file and post-stop paths for both adapters, checking byte-identical
import archival, exact captured PCM, complete raw text, unreviewed exports, and
segment/decoder provenance. Post-stop tests supply public PCM directly; they do
not measure a real microphone or user-visible stop latency.

All 14 actual-component browser scenarios pass. The Dictate screen
shows Qwen selected with no duration hint (an adapter cutoff still surfaces an
"Up to N seconds" note). Processing progress is visible in
Dictate and Quick Capture.
Dictate has no horizontal overflow at 320 pixels; Quick Capture fits 440 × 96.
These use synthetic native responses and qualify rendering, not native window
compositing or inference performance.

Final `make check` passed: 143 Rust tests, 59 JavaScript/tool tests, native
completion-report tests, formatting, strict Clippy, and zero Svelte/TypeScript
errors or warnings. Thirteen opt-in tests were skipped by the ordinary suite;
four relevant installed-model/archive tests passed separately above. The
localhost download-progress test required running outside the network sandbox.
`make build-debug` produced the updated macOS app bundle.

Existing audio-buffer/import bounds and the writing model's context remain in
effect. A native call is noninterruptible; cancellation checks surround it and
run between parts. Speech crossing a cut, noisy/quiet nonzero input, other
languages, singing, other hardware and arbitrarily long audio remain outside
this measured envelope. No application duration quota is a separate property
from memory availability and model completion or accuracy.

## Adapter registry, reviewed preferences and updated defaults

The user-selected fresh/reset defaults are Cohere Transcribe plus Qwen3.5 writing cleanup with explicit English. Existing saved choices remain authoritative. See [the continuous-speech/completed-audio screen](../benchmarks/asr/2026-09-15-screen.md) for 150 Rust/62 tool checks, three native smoke tests, real-time Nemotron lecture/reset testing, completed-file performance, Cohere cleanup failures and remaining device/quality limits. [The separate prefix screen](../benchmarks/cleanup/README.md) records the opt-in two-pass cleanup measurement. Implementation and primary research boundaries are in [ADR-0012](adrs/0012-adapter-registry-and-reviewed-personalization.md).

## Native Hold/Toggle and final insertion — 16 September 2026

Issue [#4](https://github.com/rahulsharmavishwakarma/vesper/issues/4) and ADR-0014 track the approved macOS dictation integration. Existing settings migrate to Toggle and Keep in app; automatic paste is enabled explicitly in Settings → Quick capture.

Validation completed in this workspace:

- Required `make check` passed (Svelte/TypeScript, JavaScript tests, formatting, strict Clippy, native completion-report contract, and workspace Rust tests). After the final session-scoping fix, the desktop Rust suite passed 40 tests, with three existing opt-in tests ignored, and strict workspace/all-target Clippy passed again.
- Twenty browser scenarios passed using synthetic native IPC. New scenarios cover native state-driven startup/live preview, pending request deduplication/retry, Cancel, operation-scoped final Copy, persistent paste fallback, explicit permission requests and saved Hold/Paste preferences, and stale same-operation revision rejection. These do not exercise an OS hotkey or microphone.
- The floating recorder was inspected at 440 × 96; Settings at 1440 and 375 pixels, with no narrow-screen overflow and 44-pixel changed controls.
- `sh scripts/test-dictation-native.sh` passed on macOS: a real nonactivating NSPanel retains WebKit content across 20 show/hide cycles; showing it leaves the frontmost application unchanged; WebKit evaluation works after hosting; teardown restores the content view. An isolated pasteboard test preserves Unicode and multiple formats, restores empty content, and refuses to overwrite a newer copy. Current-layout Command-V resolution succeeds; delivery without a valid target leaves the general clipboard untouched.
- `cargo run -p transcribe-desktop --example dictation_overlay_smoke --locked` passed. Actual Tauri IPC works after native hosting and repeated panel show/hide. This example uses an isolated application identifier and synthetic page, and opens no microphone or inference models.

Limits: the test process reported native permission bits 9 (platform/clipboard access available; Accessibility and event posting unavailable). No system access was granted programmatically. Actual text insertion into TextEdit/browser/editor targets, secure-field/provider differences, fullscreen/Spaces/mixed-display interaction, OS shortcut press/release timing, and live microphone-to-paste latency remain unverified. Neither smoke injects text into another application. No inference latency or accuracy improvement is claimed. Windows/Linux automatic insertion remains outside this delivery.

### Separate Hold/Toggle shortcuts and release feedback

The shortcut controls use independent saved bindings and per-action pressed state. Core tests exercise every legacy binding/mode migration, distinct fallback selection, and duplicate rejection. Native controller tests cover mixed Hold/Toggle events, repeat/release isolation, early release during cold startup, and one finalization for a started microphone. Registration tests cover pair swaps, a second-binding conflict after partial registration, release failure, and rollback. Reset/settings-save tests preserve the previous binding pair on failure.

The browser regression page checks saving both bindings with the output preference, excludes duplicate choices, and confirms that native finalization immediately replaces Listening even while the runtime poll still says recording. The processing waveform and processing button have similar accessible labels; the regression targets the button explicitly. The shared controls were inspected at desktop and 375 px widths. Verification uses synthetic native IPC and does not establish physical OS key-release delivery or real external-field insertion.

Workspace Rust tests passed (211 passed, 16 opt-in tests ignored), strict workspace/all-target Clippy passed, 71 JavaScript tests passed, and Svelte/TypeScript reported zero errors/warnings. The model-download test uses a loopback HTTP server; its sandbox-denied bind required a rerun with local networking available. Cold model load still precedes microphone startup. The early-release guard prevents post-release empty capture, but cannot recover speech spoken before the microphone was ready. See [ADR-0014](adrs/0014-native-dictation-and-final-insertion.md) for the separately proposed inline-insertion extension and outstanding native qualification.

The final browser run passed all 35 scenarios, including the shortcut/release cases and the shared workspace's interaction checks. The checkbox focus fixture now establishes visible-focus inheritance before checking the ring; a real Tab/Shift+Tab browser check also confirmed `:focus-visible` and a solid outline. This avoids treating a synthetic label click as keyboard input. The final macOS debug bundle was rebuilt successfully. Live inline insertion remains approval-pending and was not exercised or enabled.

### Custom Hold/Toggle shortcut recording

The current direct recorder passed 51 desktop Rust tests (3 existing opt-in tests ignored), strict desktop/all-target Clippy with `--no-deps`, 74 JavaScript tests, and Svelte/TypeScript checks with zero errors or warnings. A macOS debug app bundle was rebuilt successfully. The earlier custom-binding core/parser checks remain in place: modifier identity, platform mapping, equivalent duplicates, valid keys, serialization and legacy fallback selection. New native tests cover redirecting registered shortcuts into the editor, repeat/release suppression, focus loss, and stale session cleanup. The dependency-inclusive lint attempt encountered an unrelated `needless_borrow` warning in `transcribe-inference/src/offline_asr.rs`; the strict desktop-only lint pass does not claim dependency lint coverage.

The browser suite passed all 42 scenarios present in the shared workspace. Recorder coverage includes physical key capture, unsupported and duplicate combinations, Escape restoring the exact pre-edit draft, saving both bindings, labels after navigation, registration-error feedback and retry, registered native combinations, stale events, window blur and delayed setup cancellation. Native event routing is mocked, so it establishes UI/state behavior rather than physical OS delivery. A delayed-setup regression led to rechecking focus/visibility after native readiness and refusing to start recording keys from a disabled, hidden or unfocusable field.

A separate browser interaction used real keyboard events for Ctrl + Option + K, verified Escape restoration, then saved and reloaded the binding in isolated browser-preview storage. The legacy customization panel is absent. The layout was inspected at 1440 × 1080 and 375 × 900 with entrance motion settled; a 320 px overflow check also passed. These checks do not register actual global OS shortcuts or open a microphone. Real keyboard layouts, OS-reserved/conflicting combinations, and physical press/release delivery remain part of native qualification in issues #4/#7. Fn/modifier-only capture and continuous external-field insertion are separate scope; the latter remains approval-pending.
