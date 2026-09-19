# Native learning qualification protocol

[#58](https://github.com/rahulsharmavishwakarma/transcribe/issues/58) qualifies the existing automatic spelling loop. The product outcome is fewer corrections and less editing time in later independent sessions, while preserving meaning. [Evaluation](evaluation.md) owns the measured results; this page defines the protocol before scoring. No new training, semantic analysis or live policy controller is enabled.

## Evidence boundaries

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

## Native session acceptance

Follow the [device checklist](../validation/native-device-checklist.md) for permissions, capture, shortcuts and insertion. Use an isolated evaluation profile, never an existing user's dataset. Record, correct a spelling, complete a later independent recording, restart the app, switch a supported installed model and repeat. Include wrong context, a rejected correction, undo, Forget and collection disabled. Preserve the user's draft even when its evidence has expired.

Timestamp the stop request and the first rendered final result in the same monotonic clock domain; report mean and nearest-rank p95 **stop-to-visible-final**. Measure insertion separately. Backend capture-processing entry to final runtime publication excludes device stop, IPC, WebView rendering and insertion and must retain that narrower name. Keep ASR, cleanup, persistence and edit-to-memory-ready durations separate; the existing save timer does not cover every learning/personalization write.

Before collecting the natural-user locked evaluation, fix participant/session sampling, independent label review, sample size/power and acceptance margins. At minimum, require a reduction in correction effort with uncertainty reported, no protected-meaning regression, and predeclared latency/resource limits. Natural-user gates remain unset until that design and participation exist; diagnostic simulation cannot silently fill them in. Record hardware, OS, power/thermal state, competing work, model/runtime revisions, artifact hashes, language/backend and failures. A missing natural-use or rendered-latency result remains unqualified.

## Reproduce the diagnostic runs

Use the repository's configured Rust/native toolchain and installed model artifacts. The test dependency on the existing workspace `sha2` crate verifies all public WAV bytes before any inference. Both tests use temporary datasets and refuse to overwrite an explicitly requested report. Generated reports belong in ignored cache directories or `/tmp`.

```sh
. ./scripts/dev-env.sh
TRANSCRIBE_LEARNING_REPORT=/tmp/learning-authored.json \
  cargo test --locked --offline -p transcribe-desktop \
  runtime::learning_qualification_tests::chronological_desktop_learning_against_frozen_memory \
  -- --exact --nocapture

TRANSCRIBE_MODEL_DIR="$PWD/models/downloads" \
TRANSCRIBE_LEARNING_AUDIO_ROOT="$PWD/.cache/benchmarks/accent-2026-09-18/corpus" \
TRANSCRIBE_LEARNING_REPORT=/tmp/learning-native.json \
  cargo test --locked --offline -p transcribe-desktop \
  runtime::learning_qualification_tests::native_public_capture_learning_against_frozen_memory \
  -- --exact --ignored --nocapture --test-threads=1
```

The public selection is pinned in [the native manifest](../../benchmarks/asr/manifests/native-learning-2026-09-19.json); audio remains outside source control. Its source revision, CC-BY-4.0 attribution, references and per-file hashes come from the existing [Svarah screen](../../benchmarks/asr/2026-09-18-accent-personalization.md). Missing or mismatched inputs fail preflight; the tests do not download models or audio. Run inference serially after compilation, record competing work and sample process RSS and thermal state. Record the exact binary hash and build profile: the initial diagnostic uses Cargo's test profile, not a release app latency benchmark.

The comparator freezes both vocabulary and eligibility evidence from the initial snapshot and disables observation in that test-only snapshot. Project/language selections still follow the input. Disabling observation here is an evaluation control, not a new application setting; normal desktop learning remains automatic. The first harness attempt omitted this observation isolation and failed an assertion when an observed candidate appeared in the frozen store. That run is retained as a harness failure; it supplied no comparative result and did not justify changing production admission rules.

A separate post-hoc wiring check can be run with the same model/audio variables and a new report path, selecting `runtime::learning_qualification_tests::native_public_identifier_memory_roundtrip` with `--exact --ignored --nocapture --test-threads=1`. It uses an authored project identifier preference after real recognition, then verifies later application, explicit reversal, Forget and stale-evidence isolation on repeated public audio. It does not add independent correction support or count as an accuracy test. Its actual initialized providers are recorded per request; keep CPU and Metal results separate even when both were requested as Auto.

The cleanup follow-up compares a rejected, test-only shorter prompt with the production prompt on fixed public hypotheses and authored safety controls:

```sh
TRANSCRIBE_SMOKE_MODELS="$PWD/models/downloads" \
TRANSCRIBE_CLEANUP_REPORT=/tmp/cleanup-candidate.json \
  cargo test --locked --offline -p transcribe-inference \
  qwen::qualification_tests::minimal_prompt_candidate_against_frozen_cases \
  -- --exact --ignored --nocapture --test-threads=1
```

This explicit CPU experiment is **expected to fail its candidate qualification gate** with the pinned artifacts. It writes the full report before failing and refuses to overwrite an existing report. The shorter prompt is not a production option. Keep its 58-case result separate from the native paired replay: it executes cleanup only, without audio recognition, learning feedback or human interaction. See [the decision and limitations](evaluation.md#cleanup-reliability-follow-up).

To check the integrated bounded recovery against the original Minimal path, use
`qwen::qualification_tests::bounded_recovery_against_frozen_cases` in the same
command with a fresh report path. Run it once without `TRANSCRIBE_CLEANUP_HOLDOUT`
for the development cases, and once with `TRANSCRIBE_CLEANUP_HOLDOUT=1` for the
disjoint qualification cases. Both are expected to pass their comparison gates;
remaining raw fallbacks are still recorded. The gate requires identical output
on every originally accepted case, plus recovery of the three known development
failures. It does not assert that every input can be cleaned successfully.
