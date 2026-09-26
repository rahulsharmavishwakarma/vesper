# Native learning qualification protocol

[#58](https://github.com/rahulsharmavishwakarma/transcribe/issues/58) qualifies the existing automatic spelling loop. The product outcome is fewer corrections and less editing time in later independent sessions, while preserving meaning. [Evaluation](evaluation.md) owns the measured results; this page defines the protocol before scoring. No new training, semantic analysis or live policy controller is enabled.

## Evidence boundaries

Qualification-only app identifiers use `com.vesper.learning-qualification` and
`com.vesper.dictation-smoke`, separate from the user's application store.
All Rust qualification variables use `VESPER_*` with corresponding historical
`TRANSCRIBE_*` read fallbacks. Existing dated results retain their original
environment/source identities; renaming does not requalify those measurements.

Use three separately reported layers:

1. **Authored desktop lifecycle replay:** constructed ASR text, simulated runtime restarts and ordinary edits exercise the production completion, retry correlation, persistence, selection and draft-edit paths. A frozen-memory arm starts with the same empty memory. References are reporting-only; only explicit feedback rows may teach. Distinct synthetic audio fingerprints are mechanism controls, not independent human recordings.
2. **Native public-audio replay:** pinned cached Svarah recordings run through the real post-capture completion path and installed models in temporary stores. Corpus references simulate edits on calibration rows only; later rows cannot teach. Preserve separate language-background cohorts. These already-used public clips are diagnostic reuse, not a new untouched acoustic test or a consenting user's longitudinal history. The metadata cannot establish speaker identity.
3. **Natural multi-session use:** consenting users dictate and correct naturally in the native application. Independently label repair relations and a sample of unchanged outputs; retain source identities, user/session grouping and chronological boundaries locally. Human editing actions/time and actual stop-to-visible-final latency require this layer. Do not estimate either from word edit distance or backend timers.

Archive each input manifest's SHA-256 before execution, alongside source revision and changed-source hashes. Do not modify labels, admission rules or numerical gates after inspecting final scores. If a defect requires a fix, retain the failing result, describe the fix, and distinguish the rerun from a fresh holdout. Do not put private audio or transcripts in GitHub.

## Paired controls and gates

The authored fixture covers normal and duplicate-source edits, immediate retries, later independent-session support, model identity changes, project/language exclusion, quoted text, paraphrases, changed dates/numbers, unchanged outputs, contradictory feedback, undo, Forget and relearning. Later-session audit labels never enter the learner. An unflagged contradictory use deliberately tests whether an active memory can be wrong before feedback arrives; report the harm even when this is expected behavior.

Predeclared diagnostic gates are zero pipeline failures, zero changes to protected negative controls, zero automatic training labels, no collection-off audio/session archive, and unchanged vocabulary/eligibility evidence in the frozen-memory comparator. A positive score on these controls establishes mechanics only. Report all audit rows, total word edits, outputs needing correction, improvements and harms; these counts are proxies, not observed editing actions or seconds. Any harmful audit substitution prevents a general no-regression claim.

For the public native replay, preserve original manifest ordering and select the first two clips per available cohort without looking at recognizer output. The first clip is simulated calibration, the second an audit; use independent temporary profiles per cohort. Run both arms on each identical input, alternate arm order, and freeze the comparator's profile/evidence for every request. Use one resident inference manager serially; record first-load requests separately from requests with resident models. Full cleanup is enabled so both application stages execute. Report warnings and errors even when raw text remains available.

The diagnostic native quality margin is **no increase in audit word edits or harmed audit cases** relative to frozen memory. The exploratory timing margin is **no more than 10% higher mean or p95 backend completion time** for learning on matched resident requests; report sample size and order effects and do not treat a small shared-host screen as a release decision. No timing exclusion may depend on which arm was faster. Cold requests remain in their own reported group. These diagnostic margins do not replace a powered natural-user qualification.

## Learning opportunity coverage

The fixed two-clip-per-cohort public replay is an execution/admission diagnostic,
not a sufficient learning-efficacy design. Its current policy-3 run has one
teaching row per cohort and no later reuse of either admitted alias. Keep that
historical result; do not lower the independent-support threshold or move audit
labels into feedback to make it improve.

Before running an efficacy batch, freeze chronological episode membership and
feedback/audit roles. Include at least two genuinely independent recordings
with eligible corrections for each ordinary target, followed by a distinct
audit recording that requires the same mapping in a supported context. Include
the existing one-edit project-identifier path separately. Use real speaker/user
identity when available; language background alone does not establish one user.
Synthetic episodes must be labelled synthetic. A new filename, duplicated audio,
or a restarted runtime cannot establish natural independence. Immediate retries
must still pass through the application's episode-correlation rules.

Predeclare recurring target/context strata and selection rules using corpus
metadata and references. Do not select successful recognizer errors after
inspecting the final holdout. Report realized opportunities from the frozen
baseline alongside full-corpus results, including clips where ASR already spells
the term correctly, admission abstains, support never activates or selection
rejects a memory. A dataset with zero realized opportunities is insufficient for
an efficacy conclusion; it can still qualify execution checks.

For each arm, report feedback edits, eligible pairs, independent support,
activations, later alias occurrences, context-eligible retrievals, actual memory
applications and correction effort/word edits. Keep true human editing time
separate from simulated edits. Preserve failures and non-learning rows in the
full aggregate; opportunity-conditioned results are an additional view.

Include provisional edit retraction, correction of an already learned target to
a third spelling, new teaching contexts without a selected project, wrong
context/language, quotes, unchanged outputs, duplicate recordings, restart, Undo,
Forget and model changes. The [pipeline review](evaluation.md#pipeline-review-and-learning-opportunities)
documents three lifecycle failures reproduced under policy 3 and their policy-4
regressions. Audit references must
remain reporting-only, and the frozen comparator must never receive feedback.

## Native session acceptance

Follow the [device checklist](../validation/native-device-checklist.md) for permissions, capture, shortcuts and insertion. Use an isolated evaluation profile, never an existing user's dataset. Record, correct a spelling, complete a later independent recording, restart the app, switch a supported installed model and repeat. Include wrong context, a rejected correction, undo, Forget and collection disabled. Preserve the user's draft even when its evidence has expired.

Timestamp the stop request and the first rendered final result in the same monotonic clock domain; report mean and nearest-rank p95 **stop-to-visible-final**. Measure insertion separately. Backend capture-processing entry to final runtime publication excludes device stop, IPC, WebView rendering and insertion and must retain that narrower name. Keep ASR, cleanup, persistence and edit-to-memory-ready durations separate; the existing save timer does not cover every learning/personalization write.

Before collecting the natural-user locked evaluation, fix participant/session sampling, independent label review, sample size/power and acceptance margins. At minimum, require a reduction in correction effort with uncertainty reported, no protected-meaning regression, and predeclared latency/resource limits. Natural-user gates remain unset until that design and participation exist; diagnostic simulation cannot silently fill them in. Record hardware, OS, power/thermal state, competing work, model/runtime revisions, artifact hashes, language/backend and failures. A missing natural-use or rendered-latency result remains unqualified.

## Reproduce the diagnostic runs

The recurring-term synthetic diagnostic creates six isolated scenarios using
three installed macOS English voices and two authored name-spelling preferences.
Its 72 chronological rows include duplicate audio, two teaching contexts, later
use, changed targets, wrong context, no alias and Forget. Voice locales are
synthesis settings, not verified human accents. Runtime restarts simulate
independent episodes; they do not establish natural independence. All references
and roles are fixed before recognition, and failed or unlearnable rows remain
in the report. Joined/underscore spellings represent user preferences, not an
assertion that a spaced ASR transcript misrepresents the generated speech.

```sh
node scripts/prepare-recurring-learning.mjs .cache/benchmarks/recurring-learning-new
VESPER_MODEL_DIR="$PWD/models/downloads" \
VESPER_LEARNING_AUDIO_ROOT="$PWD/.cache/benchmarks/recurring-learning-new" \
VESPER_LEARNING_REPORT="$PWD/.cache/benchmarks/results/recurring-learning-new.json" \
cargo test --locked -p vesper-desktop \
  runtime::learning_qualification_tests::native_recurring_spelling_learning_against_frozen_memory \
  -- --exact --ignored --nocapture --test-threads=1
```

Use a new output directory/report path and an existing report parent directory.
The generator writes WAVs without playback or microphone use, records the system
voice/OS configuration and hashes every file. The test runs both arms through
installed ASR and cleanup, checks duplicate support and collection-off storage,
and records actual providers. A diagnostic pass requires no pipeline failures,
no harmed audit rows, a realized reuse opportunity and at least one improvement.
The report also enforces the predeclared 10% mean/p95 resident backend timing
margin in `exploratoryTimingGate`; missing timing coverage fails that gate.
Both arms of the first model-loading pair are excluded together. Earlier reports
whose `gatesPassed` omitted this timing check must be read with their separate
timings, not as evidence that the latency margin passed.
Report all six scenarios individually; an aggregate pass alone cannot establish
that every scenario activated or benefited. Backend timing excludes native UI
and microphone latency. Generated audio, manifests and result JSON stay ignored.

Use the repository's configured Rust/native toolchain and installed model artifacts. The test dependency on the existing workspace `sha2` crate verifies all public WAV bytes before any inference. Both tests use temporary datasets and refuse to overwrite an explicitly requested report. Generated reports belong in ignored cache directories or `/tmp`.

```sh
. ./scripts/dev-env.sh
VESPER_LEARNING_REPORT=/tmp/learning-authored.json \
  cargo test --locked --offline -p vesper-desktop \
  runtime::learning_qualification_tests::chronological_desktop_learning_against_frozen_memory \
  -- --exact --nocapture

VESPER_MODEL_DIR="$PWD/models/downloads" \
VESPER_LEARNING_AUDIO_ROOT="$PWD/.cache/benchmarks/accent-2026-09-18/corpus" \
VESPER_LEARNING_REPORT=/tmp/learning-native.json \
  cargo test --locked --offline -p vesper-desktop \
  runtime::learning_qualification_tests::native_public_capture_learning_against_frozen_memory \
  -- --exact --ignored --nocapture --test-threads=1
```

The public selection is pinned in [the native manifest](../../benchmarks/asr/manifests/native-learning-2026-09-19.json); audio remains outside source control. Its source revision, CC-BY-4.0 attribution, references and per-file hashes come from the existing [Svarah screen](../../benchmarks/asr/2026-09-18-accent-personalization.md). Missing or mismatched inputs fail preflight; the tests do not download models or audio. Run inference serially after compilation, record competing work and sample process RSS and thermal state. Record the exact binary hash and build profile: the initial diagnostic uses Cargo's test profile, not a release app latency benchmark.

The comparator freezes both vocabulary and eligibility evidence from the initial snapshot and disables observation in that test-only snapshot. Project/language selections still follow the input. Disabling observation here is an evaluation control, not a new application setting; normal desktop learning remains automatic. The first harness attempt omitted this observation isolation and failed an assertion when an observed candidate appeared in the frozen store. That run is retained as a harness failure; it supplied no comparative result and did not justify changing production admission rules.

A separate post-hoc wiring check can be run with the same model/audio variables and a new report path, selecting `runtime::learning_qualification_tests::native_public_identifier_memory_roundtrip` with `--exact --ignored --nocapture --test-threads=1`. It uses an authored project identifier preference after real recognition, then verifies later application, explicit reversal, Forget and stale-evidence isolation on repeated public audio. It does not add independent correction support or count as an accuracy test. Its actual initialized providers are recorded per request; keep CPU and Metal results separate even when both were requested as Auto.

The cleanup follow-up compares a rejected, test-only shorter prompt with the production prompt on fixed public hypotheses and authored safety controls:

```sh
VESPER_SMOKE_MODELS="$PWD/models/downloads" \
VESPER_CLEANUP_REPORT=/tmp/cleanup-candidate.json \
  cargo test --locked --offline -p vesper-inference \
  qwen::qualification_tests::minimal_prompt_candidate_against_frozen_cases \
  -- --exact --ignored --nocapture --test-threads=1
```

This explicit CPU experiment is **expected to fail its candidate qualification gate** with the pinned artifacts. It writes the full report before failing and refuses to overwrite an existing report. The shorter prompt is not a production option. Keep its 58-case result separate from the native paired replay: it executes cleanup only, without audio recognition, learning feedback or human interaction. See [the decision and limitations](evaluation.md#cleanup-reliability-follow-up).

To check the integrated bounded recovery against the original Minimal path, use
`qwen::qualification_tests::bounded_recovery_against_frozen_cases` in the same
command with a fresh report path. Run it once without `VESPER_CLEANUP_HOLDOUT`
for the development cases, and once with `VESPER_CLEANUP_HOLDOUT=1` for the
disjoint qualification cases. Both are expected to pass their comparison gates;
remaining raw fallbacks are still recorded. The gate requires identical output
on every originally accepted case, plus recovery of the three known development
failures. It does not assert that every input can be cleaned successfully.

## Native WebView replay

The opt-in `learning_webview` example measures the production transcription panel
in macOS WKWebView with actual Tauri IPC, installed inference and the Rust/SQLite
learning path. Its test adapter supplies preverified public PCM when the panel
requests Stop. It never creates a microphone recorder, registers shortcuts,
opens TextEdit, reads the ordinary application profile or inserts text elsewhere.
Learning and frozen-memory arms use separate temporary stores per cohort.
The window closes after the replay; ordinary application defaults are unchanged.

This reuses the fixed 38-recording Svarah manifest with 76 alternating requests.
Only the learning arm's calibration rows receive scripted reference edits through
the production editable field. Audit references remain scorer-only. Unchanged
text supplies no edit event. The real runtime also schedules its bounded shadow
work after an edit, as the production command does. This is a public replay with
scripted feedback; it does not establish independent personal support, natural
editing effort, process restart or acoustic model-switch qualification.

One WebView `performance.now()` clock measures the Stop click through the first
observed matching final DOM value (polled with a requested 5 ms timer), and
separately through two animation-frame callbacks with a
visible result field. The test scrolls the result into view if necessary. Callbacks
run before repaint and can pause when hidden, so the second duration is named
**Stop-to-render-opportunity**, not physical pixel presentation or microphone
stop-to-visible latency. See the [animation-frame contract](https://developer.mozilla.org/en-US/docs/Web/API/Window/requestAnimationFrame).
Hidden/offscreen results, absent frame callbacks and mismatched result identities
fail the measurement. Backend completion and scripted edit-submit durations are
reported separately; microphone-stop and human-editing measurements remain null.
The test also requires two frame callbacks before starting inference. If the
native host cannot render, it exits with an incomplete/failed report rather than
spending the full replay on unavailable presentation measurements. A visible
document flag alone does not prove that animation callbacks are running.

Build first, then run inference without competing compilation. From the repository
root, start the test-only frontend server in a separate terminal:

```sh
npm run dev --workspace @vesper/desktop -- --host 127.0.0.1 --port 5198
```

Then run the explicit native example with a new output path:

```sh
. ./scripts/dev-env.sh
cargo build --locked -p vesper-desktop --example learning_webview
mkdir -p .cache/benchmarks/results
node scripts/run-native-learning.mjs \
  "${CARGO_TARGET_DIR:-target}/debug/examples/learning_webview" \
  "$PWD/models/downloads" \
  "$PWD/.cache/benchmarks/accent-2026-09-18/corpus" \
  .cache/benchmarks/results/native-learning-webview
node scripts/summarize-native-learning.mjs \
  .cache/benchmarks/results/native-learning-webview/requests.jsonl \
  .cache/benchmarks/results/native-learning-webview/environment.json \
  .cache/benchmarks/results/native-learning-webview/summary.json
```

The example validates all WAV hashes/durations and installed artifacts before
replay, and refuses to overwrite a report. It uses the local development URL
through Tauri's [App URL variant](https://docs.rs/tauri/2.11.5/tauri/enum.WebviewUrl.html#variant.App).
The append-only JSONL retains completed requests if the window closes or a later
request fails. A complete native run means every request was reported; the
summarizer separately returns failure if diagnostic gates fail, after writing the
full report. The launcher records the actual process exit code/signal independently:
even 76 successful rows and a completion marker cannot pass after a shutdown crash.
It also records source/binary hashes, OS/hardware, power/thermal samples and Darwin
process resource totals. The harness explicitly unloads resident models on their
existing inference owner, releases its runtime workers and removes temporary
stores before exiting the native event loop. Tauri's `run_return()` lets the
example join its watchdog and verify cleanup before returning success. These
changes belong to the test host; they do not change production inference ownership.
Never replace an incomplete or failed run with a successful-only log. The
summarizer also retains valid JSONL rows when a crash truncates a later row and
marks that report incomplete.

The summarizer validates pinned ordering, source identities, arm pairing, exact
request coverage, unchanged frozen evidence, collection-off isolation, final DOM
identity and finite durations. It retains warning/error rows and their available
timings. Both rows of the first model-loading pair stay outside resident summaries;
no exclusion depends on measured speed. Missing timing coverage fails the margin,
and incompatible actual providers/thread counts prevent combined timing estimates.
The predeclared exploratory margin is at most 10% higher mean and nearest-rank p95
for learning in both backend and render-opportunity duration; every audit pair must
be scored with no harmful change. These are diagnostic gates, not a powered release
decision or evidence of natural-user benefit. Run `node --test
scripts/summarize-native-learning.test.mjs` for report integrity checks.

Keep raw JSONL/JSON and source/executable hashes in the ignored results directory.
Record power, thermals, OS/hardware, build mode, actual native providers/threads and
competing work with each run. Keep failures, earlier harness attempts and measured
results in the [canonical evaluation](evaluation.md), separate from the remaining
natural-session acceptance requirements above.

To diagnose why this public replay admitted no spelling, run the recorded-text
transaction check without models or a window:

```sh
. ./scripts/dev-env.sh
VESPER_LEARNING_REPORT=/tmp/public-admission-diagnostic.json \
  cargo test --locked -p vesper-desktop \
  runtime::learning_qualification_tests::public_correction_admission_diagnostic \
  -- --exact --nocapture
```

Use a fresh report path. It reuses the existing pinned hypotheses and public
references, checks each draft revision and persisted edit identity, and reports
the first admission barrier separately from later activation. It does not run
ASR or treat these scripted edits as natural feedback. See the [diagnosis](evaluation.md#admission-diagnosis)
before interpreting zero active memories as a broken storage or retrieval path.

The companion test
`runtime::learning_qualification_tests::ordinary_correction_learning_reuses_names_acronyms_and_formatted_edits`
runs authored correction-to-later-use lifecycles with duplicate-source, context,
quote, reversal, Undo and Forget controls. Use it in the same command with a
different report path. It supplies constructed raw text and PCM identities;
it is not an ASR, microphone or natural-user accuracy test.

The WebView summary separately reports expected/acknowledged calibration edits,
provisional outcomes, activations and maximum active memory. Missing scripted
feedback now fails the request gate even if final rendering succeeds. This
prevents a no-feedback run from appearing to qualify the learning path.
