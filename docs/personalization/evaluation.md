# Evaluating automatic personalization

Behavioral correctness, recognition quality and user effort are separate questions. Tests exercise the implemented policy and storage boundaries; they do not show that ordinary users need fewer corrections over weeks of use.

The [learning/RSI coverage comparison](../research-rsi-2026-09.md#what-has-actually-been-tested-in-this-app) answers the flow-level distinction explicitly: application memory has real Rust/SQLite tests and mocked UI checks; recursive policy updates have an offline simulation; a combined live background RSI flow has not been implemented or tested. The September literature review rechecked existing evidence without claiming a new benchmark run.

The [self-improvement objective](roadmap.md#will-ordinary-use-produce-self-improvement) distinguishes adapting a user's memory from improving the learning procedure itself. Success means better later-session outcomes against a frozen-memory comparison, with independent feedback and preserved meaning; neither memory growth nor self-scoring proves improvement.

The separate [policy experiment](policy-experiment.md) tests bounded policy updates against fresh validation and hidden audits, with all six fixed-policy comparators. It remains offline and has not beaten the best fixed policies on the constructed word-error total or shown a public-corpus gain. Its [measurement report](../../benchmarks/asr/2026-09-17-policy-learning.md) retains adverse outputs and the difference between mechanical tests and deployment evidence.

The [expanded simulation](../../benchmarks/asr/2026-09-18-policy-learning-stress.md) adds 97 locked histories and 23,809 episodes, including misleading/sparse feedback, transitive retries, changing context/support requirements, language/project switches and storage pressure. Version 2 fixes hidden regressions inside correlated feedback groups and unnecessary provenance errors on disabled entries. All events contribute to reporting; independent components determine policy evidence. Results compare every trajectory with the starting policy and all six fixed alternatives, including held-out and final-only results. Synthetic labels are explicit simulated review feedback, not inferred user acceptance. The test does not measure microphone/ASR accuracy, accent adaptation, natural user effort or end-to-end latency.

The subsequent [failure-bank comparison](../../benchmarks/asr/2026-09-18-failure-bank-comparison.md) uses 96 fresh-seed histories plus the reused capacity control. The previous executable and rebuilt current mode match across all 97 histories. The proposed historical veto increases hidden word edits **2,599 → 2,625**, despite slightly improving the complete replay and final-only partition; the fixed context control scores **1,464**. All 304 cached Svarah held-out outputs remain identical. Twenty-three Rust evaluator tests, ten Python harness/report tests, 16 isolation reruns and 24 counterbalanced timing pairs verify this isolated experiment. It remains unqualified for live use. Public controls reuse acoustic outputs; no fresh ASR or native UI benchmark is implied.

The [scoped-policy refinement](../../benchmarks/asr/2026-09-18-scoped-policy-comparison.md) uses another locked set of 96 fresh-seed histories plus capacity, with a stronger fixed fallback and context-specific controllers. Held-out edits are **2,598 original adaptive, 1,468 fixed context, 1,412 same-start global adaptive, and 1,372 scoped adaptive**. Most gain is from initialization; scoping contributes the remaining improvement over the same-start global control. Six held-out episodes still regress versus that control, and cached public/accent outputs show no gain. The report preserves the initialization ablation, scoped/fallback coverage, repeated timing and frozen-source reproducibility checks. These are authored-feedback results, not native longitudinal user qualification.

The [real-audio accent evaluation](../../benchmarks/asr/2026-09-18-accent-personalization.md) uses 304 Svarah recordings, 16 per speaker-primary-language group, through the installed Nemotron and Cohere native adapters. Raw and frozen-dictionary outputs share the same acoustic decoding; 38 recordings also use paced streaming. The cached corpus's language-background labels are accent proxies, following Svarah's convention. They are not predictions from an accent detector, and the available metadata does not establish a speaker-disjoint longitudinal split. FLEURS US-English controls remain a separate corpus. Per-group WER/CER, failures, reference denominators, actual native thread counts and precisely defined latency are retained. This audio screen and the simulated policy experiment answer different questions; neither establishes that accent detection or learned acoustic adaptation is implemented.

## Reproducible checks

`make check` covers formatting, frontend types/build, Node helpers, native completion fixtures, strict Clippy and workspace tests. The existing model-download test needs a loopback HTTP listener; a restricted sandbox may require local socket access. No cloud inference is needed.

Focused checks:

```sh
cargo test -p transcribe-core learning --locked
cargo test -p transcribe-data learning --locked
cargo test -p transcribe-desktop automatic_ --lib --locked
cargo test -p transcribe-data desktop_ --lib --locked
```

These cover guarded edits, Unicode combining marks, source/episode deduplication, scoped and cross-model memory, permission epochs, stale results, disabled collection, review/export separation, undo, deletion, quota/expiry and injected database failure. The transaction failure test verifies that an attempted admission leaves neither its profile change nor its evidence behind.

Desktop-policy regressions cover first use without a settings visit, legacy schemas 1/2/3, idempotent persisted enabling, stale saves, disabled-entry preservation, old clients attempting to restore the retired switch, two independent ordinary edits, cross-model later use without installed weights, collection-off storage boundaries, and forget followed by rejected old feedback and accepted fresh evidence. These are application/data-flow checks with constructed text and audio fingerprints, not another acoustic benchmark.

The browser fixture mounts production components with mocked native commands. It exercises single-click Copy after editing, feedback display, undo, save failure/retry and learning controls. This is frontend evidence, distinct from the real Rust store tests and installed-model tests. Transient browser fixtures stay outside shipped code. The automatic desktop-policy verification passes 114 Rust tests (32 core, 30 data, 52 desktop; four existing opt-in tests ignored), strict targeted Clippy, frontend type/build checks and modified-file formatting. Browser checks use mocked native commands: the removed opt-in, one draft commit on Copy, pending-state disabling, undo, forget, keyboard navigation, profile save failure/retry and a 390-pixel layout. These do not replace the real SQLite tests or previous native accent measurements.

## Chronological replay

The [replay executable](../../crates/transcribe-data/examples/automatic_learning_replay.rs) consumes an ordered manifest of interactions. At each step it snapshots current memory, generates the output, scores against withheld references, and only then exposes the ordinary edit to memory. Source fingerprints and inferred correlated episodes prevent artificial independent support. No actual time sleeps or ASR inference occur.

```sh
cargo run --release --locked -p transcribe-data --example automatic_learning_replay -- \
  benchmarks/asr/fixtures/automatic-learning-episodes.json /tmp/automatic-learning-results.json
```

The portable [constructed manifest](../../benchmarks/asr/fixtures/automatic-learning-episodes.json) contains later uses, wrong context/language, repeated sources, quote/date/number/negation changes, retry/continuation cues and a model switch. Its expected outputs are withheld from automatic learning. The off, manually supplied-memory comparator, automatic-edits and automatic-with-usage modes see the same chronological inputs.

The manual comparator receives an explicit remember annotation and uses the broader existing suggestion policy. It does not reproduce a real user's approval decisions or equalize interaction effort. Its result must not be called a fair head-to-head usability trial.

A separate replay uses already cached public native hypotheses: 192 calibration recordings followed by 48 held-out recordings, with references simulating correction feedback only on calibration. These are ordinary corpus sentences, not natural repeat-dictation episodes. The first screen produced no automatically active memory and no held-out accuracy gain. That limits claims; it does not justify lowering admission thresholds based on the test set.

## Timing and release evidence

The [Z.ai-inspired evaluation proposal](../research-rsi-2026-09.md#zai-inspiration-diagnose-replay-and-verify) connects error attribution, controlled replay and native completion measurements. For a suspected harmful memory, compare the same hypothesis/profile with and without that entry before generalizing to the full audio path. A repeated recording alone does not identify which stage failed. Richer live attribution and queue/learning timing are proposed extensions; the current desktop stage timer covers transcription, cleanup and save.

Replay reports count, mean and nearest-rank p50/p95/p99 separately for in-memory final correction, feedback processing and observation processing. Mutation timings include opening SQLite, the transaction/commit and connection closure. They exclude frontend IPC/rendering, microphone capture, ASR, cleanup and native insertion. `LearningOutcome.elapsedMicros` measures its store mutation through commit and excludes opening the caller's connection.

These timings are not stop-to-final latency. Measure that separately from the recording stop request until visible final text, and measure stop-to-insert separately through confirmed native insertion. A model's real-time factor is another metric. See the [qualification protocol](../../benchmarks/asr/README.md) and [phase-A measurement report](../../benchmarks/asr/2026-09-16-automatic-learning.md) for provenance, exact local results and limitations.

The user requires automatic background learning as core product behavior, so the desktop no longer has an opt-in gate. That requirement does not change the measured efficacy limits or qualify broader admission thresholds. A future qualification needs multiple sessions per consenting user, independently labelled corrections/retry relations, grouped chronological splits, locked release gates, protected-meaning failures, correction time/burden and representative unflagged samples. Keep synthetic, public-corpus and real-user results separate. No existing test proves semantic style learning, accent adaptation or phase-B capability.
