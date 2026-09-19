# Evaluating automatic personalization

Behavioral correctness, recognition quality and user effort are separate questions. Tests exercise the implemented policy and storage boundaries; they do not show that ordinary users need fewer corrections over weeks of use.

The [learning/RSI coverage comparison](../research-rsi-2026-09.md#what-has-actually-been-tested-in-this-app) records the earlier distinction: application memory has real Rust/SQLite tests and mocked UI checks, while recursive policy updates have an offline simulation. The application now also runs isolated semantic and policy shadow evaluation, qualified separately below. It does not promote policies or implement a live recursive improvement loop. The September literature review rechecked existing evidence without claiming a new benchmark run.

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

## Application lifecycle checks, 19 September 2026

The [desktop lifecycle suite](../../apps/desktop/src-tauri/src/runtime_learning_tests.rs) runs the real `WorkContext` completion path, runtime draft editing and SQLite learner with constructed ASR text and distinct synthetic audio fingerprints. It verifies learning across a new runtime and different model identity, language exclusion, no audio/session archival with collection disabled, no automatic admission from unedited usage, duplicate-audio support deduplication, undo suspension, Forget epoch isolation with fresh relearning, and storage-failure retry without duplicate feedback. It does not execute acoustic inference or establish real-user learning benefit. Old-epoch edits remain usable drafts but supply no learning evidence; rejection of stale evidence does not mean rejecting the user's text.

Actual-component browser checks reproduce and fix a Copy race: repeated clicks while a draft save was pending could queue multiple clipboard writes. The pending guard now covers save and Copy together. Tests also verify save failure/retry, no extra label from unchanged Copy, undo preserving authored text, and Forget pending/error/retry behavior with manual-entry preservation and unsaved-preference protection. Browser IPC is mocked; Rust lifecycle tests independently exercise real persistence. The deterministic core/data/desktop run passes **119 tests**, with four existing opt-in tests omitted from that run; targeted desktop Clippy passes.

All **45 browser scenarios** pass, including the three permanent learning scenarios. The Copy failure/retry case was strengthened and rechecked after the full run. Svelte/TypeScript, targeted ESLint/formatting, the frontend production build and the macOS debug application bundle also pass. The local artifact at `.cache/benchmarks/results/2026-09-19-application-learning/debug-summary.json` retains outcomes, the reproduced UI failure, current source/bundle hashes, native artifact/runtime provenance, hardware and exact timings. This is working-tree validation rather than a frozen-source benchmark. The bundle is built, but native WebView interaction and live microphone-to-text behavior were not automated.

The installed-model desktop file pipeline separately passes on the pinned 3.845-second public English fixture, using Cohere Transcribe and Qwen3.5 0.8B with temporary storage. One observed run records **3,204.30 ms transcription**, **2,285.77 ms cleanup** and **19.40 ms save-stage time**. These are one run's backend stage measurements, including model work, not average latency or microphone-stop-to-visible-text timing; they do not measure the new scoped controller. Fixture/model/runtime provenance and native assertions remain in the existing [native smoke procedure](../validation.md#reproduce).

The initial combined smoke command passed that pipeline and then failed because its model root did not contain the requested Nemotron CPU runtime. Re-running the unchanged stream reuse/reset/cancellation test against the existing `.cache/benchmarks/model-roots/nemotron-cpu` cache passes; the Qwen embedded-template test also passes separately. No model was downloaded or user profile modified. This prerequisite failure is retained in the validation record rather than reported as a fully passing first run.

At this pre-integration checkpoint, the scoped controller and stronger context selector were still offline candidates. The [integration proposal in issue #34](https://github.com/rahulsharmavishwakarma/transcribe/issues/34#issuecomment-5735762281) distinguishes the first fixed-context integration from later live policy promotion. These initial application checks validate automatic learning and its UI fix. The subsequent approved fixed-context integration is qualified below; the adaptive controller remains offline.

## Integrated teaching-context guard, 19 September 2026

The user approved the fixed-context integration in [issue #34](https://github.com/rahulsharmavishwakarma/transcribe/issues/34#issuecomment-5735896074). The desktop now snapshots profile and learning evidence together, selects automatic vocabulary from raw text before cleanup, and reuses that selected profile for final replacement. [Dictionary learning](automatic-dictionary.md#context-required-for-post-recognition-use) and [storage](storage-and-privacy.md) specify eligibility, schema-1 compatibility, schema-2 metadata and bounds. There is no new model, remote service or adaptive-policy promotion.

Frozen-source verification passes **127 core/data/desktop tests** (35/35/57), **28 offline-controller tests**, strict Clippy, frontend checks/build and **45 actual-component browser scenarios**. Four existing opt-in tests are excluded from the deterministic library total. New checks cover schema migration without invented context, unknown/oversized evidence, duplicate source/episode handling, bounded legacy-context backfill, transactional rollback, cue deletion on undo/forget, raw-only selection, snapshot isolation and later context learning after an ordinary correction. The desktop test verifies the latter across a model switch. Browser IPC remains mocked; real SQLite and native inference are separate checks.

The new [application context replay](../../crates/transcribe-data/examples/application_context_replay.rs) calls the production selector and real SQLite snapshot API on **64 authored text controls across eight topics**. The comparator uses the same learned profile with the new filter bypassed. Total word edits are **48 without the added filter and 16 with it**: **24 improved, eight regressed, 32 tied**. All eight regressions are deliberately retained unseen-paraphrase cases where lexical context does not match. These are mechanism controls with authored desired outputs, not an independent real-user accuracy benchmark or a rerun of the earlier 7,300-episode adaptive study. No thresholds were tuned to remove these failures.

The release replay repeats each output comparison five times and requires exact agreement. Mean component measurements with one to eight active entries are:

| Component | Observations | Mean |
| --- | ---: | ---: |
| SQLite/profile/evidence snapshot | 320 | 0.0220 ms |
| Raw-text eligibility selection | 320 | 0.0044 ms |
| Final deterministic application | 320 | 0.0034 ms |
| Learning edit transaction, including commit | 8 | 0.1905 ms |

The SQLite connection is already open in these measurements. They exclude connection opening, ASR, cleanup, frontend IPC/rendering and insertion. They do not qualify the theoretical maximum metadata size or native resource interference.

The frozen native desktop public-file pipeline passes in three fresh test processes on the same pinned 3.845-second English recording, with Cohere initialized on CPU (eight native graph threads) and Qwen initialized on Metal. Mean transcription is **3,393.24 ms**, cleanup **2,552.39 ms**, and save-stage time **19.05 ms**; the mean sum of these stages is **5,964.67 ms**. These are first requests with fresh model managers and potentially warm filesystem caches, not resident steady-state or microphone-stop-to-visible latency. The small sample on a shared host cannot establish a speedup or regression versus the earlier single run. Native Nemotron stream/reset/cancellation and Qwen template checks also pass using installed/cached artifacts. No microphone or private recording is used.

The local report at `.cache/benchmarks/results/2026-09-19-context-guard/debug-summary.json` retains every authored row, timing sample summaries, browser outcomes, frozen input hashes, native artifact/runtime/execution provenance and limitations. Frozen sources and executable are retained in `.cache/context-guard-2026-09-19/`; no benchmark query can open a live user store. Reproduce the text controls with:

```sh
. ./scripts/dev-env.sh
cargo run --release --locked -p transcribe-data --example application_context_replay -- \
  /tmp/application-context-replay.json
```

The output path must be new. Keep semantic/longitudinal benefit, true acoustic accent adaptation and scoped policy promotion as separate qualification work. The fixed guard is implemented and tested; these checks do not establish recursive discovery of better learning algorithms.

## Native multi-session qualification

The [native qualification protocol](native-qualification.md) freezes paired controls, diagnostic gates and the separate requirements for natural-user evidence. The [desktop qualification suite](../../apps/desktop/src-tauri/src/runtime_learning_qualification_tests.rs) exercises chronological constructed interactions and an explicit installed-model public-audio run; neither drives microphone hardware or the native WebView. The first authored run failed because the benchmark still allowed observations to update its frozen control. The harness now disables observation in its test-only snapshot; production learning is unchanged and the failed log is retained.

The frozen authored fixture contains **32 interactions across 13 simulated sessions**, with **22 later-session audit rows**. Learning reduces audit word edits **10 → 4** and outputs needing correction **6 → 2**: five outputs improve and one is harmed. The harm is a deliberately unflagged reversal of a previously taught spelling in matching lexical context; the subsequent explicit reversal suspends that memory. An unseen paraphrase remains uncorrected. Protected negative controls, duplicate-source/retry correlation, undo, Forget, collection-off isolation and frozen-control checks pass. These counts are authored diagnostic proxies, not observed human correction actions or time; the no-harm claim fails.

The **native public run** executes 38 cached Svarah recordings across 19 language-background cohorts in both arms (**76 requests**), using Cohere INT8 on CPU (eight native graph threads) and Qwen3.5 0.8B Q4_0 on Metal. Both use English and minimal cleanup. Nineteen calibration references simulate ordinary edits; the other 19 clips are audit-only. Each cohort has an isolated store. No memory activates, and audit word edits tie at **46**, with **15/19** outputs needing correction in each arm, zero paired improvements and zero paired harms. Only one feedback clip is supplied per cohort; most automatic spellings require repeated independent support. This diagnostic provides no evidence of recurring personal benefit and cannot justify relaxed admission.

The native no-warning gate **fails**: three clips each produce the same fallback in both arms (**6/76 requests**). One cleanup changes words despite formatting-only policy; two exhaust their output budgets (106 and 64 generated tokens). All six preserve raw text exactly, are scored, and remain in timings. The fidelity and truncation guards behaved as intended. No output limit or fidelity rule was relaxed to obtain a pass. Cleanup reliability remains a qualification issue separate from memory benefit.

| Native backend completion, matched resident pairs | Frozen memory | Automatic learning |
| --- | ---: | ---: |
| Requests per arm | 37 | 37 |
| Mean | 1,718.08 ms | 1,580.81 ms |
| p95, nearest rank | 4,016.99 ms | 3,703.07 ms |
| Mean ASR stage | 1,457.06 ms | 1,308.38 ms |
| Mean cleanup stage | 247.40 ms | 246.18 ms |

The first model-manager request is retained separately at **33,226.97 ms**; its entire pair is excluded from the resident comparison by the predeclared rule. These times start at backend capture-processing entry and end after final runtime publication. They exclude microphone stop, IPC, visible rendering and insertion. The exploratory 10% mean/p95 margin passes numerically, but the difference does not establish a speedup on this small shared-host run. Both aggregates retain warning-bearing requests. Collection is off, so archival save time is absent; total time includes personalization/observation work not isolated by the existing stage timer. Nineteen simulated draft submissions average **2.53 ms** to return (p95 **3.49 ms**); this is not human editing time.

The clips total **311.50 seconds**, ranging from 4.13 to 15.84 seconds. Environment: Mac17,8, 64 GiB RAM, 18 logical CPUs, macOS 27.0 (26A428), AC attached at 7% battery, Cargo's unoptimized test profile and the existing native runtimes. Maximum resident set size was **4.55 GiB** from Darwin `time -l`; 16 ten-second snapshots reported no thermal/performance warning. This roughly 161-second diagnostic is not sustained thermal qualification. Competing host activity was not controlled. Do not combine results across build profiles, backends or corpora.

Audit errors by primary-language background (one audit clip each; **not** speaker or accent classification):

| Background | Frozen / learning word edits | Cleanup fallback per arm |
| --- | ---: | --- |
| Assamese | 2 / 2 | No |
| Bengali | 4 / 4 | No |
| Bodo | 2 / 2 | Yes, fidelity |
| Dogri | 4 / 4 | No |
| Gujarati | 0 / 0 | No |
| Hindi | 1 / 1 | No |
| Kannada | 1 / 1 | No |
| Kashmiri | 1 / 1 | No |
| Konkani | 11 / 11 | No |
| Maithili | 4 / 4 | No |
| Malayalam | 1 / 1 | No |
| Marathi | 4 / 4 | No |
| Nepali | 1 / 1 | No |
| Odia | 0 / 0 | No |
| Punjabi | 5 / 5 | No |
| Sindhi | 0 / 0 | No |
| Tamil | 2 / 2 | No |
| Telugu | 3 / 3 | No |
| Urdu | 0 / 0 | Yes, output limit |

The third fallback is a calibration clip. All recordings are English; a Hindi-background speaker is not a Hindi-language qualification.

A **separate post-hoc native wiring diagnostic** supplies the scripted project identifier edit `application ID → application_id` after transcribing one public clip. Four real Cohere/Qwen requests verify activation, reuse after recreating the desktop Runtime, suspension after an explicit reverse edit, and rejection of stale learning evidence after Forget while retaining the user's draft. Reusing the same recording never adds independent support. This passes with no warnings using **CPU for both ASR and cleanup** under Auto; its timings are not pooled with the Metal comparison. It restarts Runtime objects, not the application process, and does not qualify microphone interaction, different acoustic models or natural-user benefit.

The core/data/desktop regression run passes **128 tests**; five opt-in tests were omitted in that run. After adding the supplemental native diagnostic, the targeted authored test and strict desktop test Clippy/formatting checks pass again. The native pair comparison retains its failing no-warning gate; the four-request identifier diagnostic passes separately. Production inference and learning policies are unchanged; the added dependency is test-only access to the existing workspace SHA-256 crate.

Detailed local artifacts are under `.cache/benchmarks/results/native-learning-2026-09-19/`: frozen manifest/protocol identities, executable/source hashes, the initial harness-failure log, authored rows, all native rows/warnings, model/runtime/artifact provenance, resource snapshots and the separate CPU identifier round trip. Base revision is `7407270` plus recorded unstaged test sources. These identities describe working-tree diagnostics, not release qualification. Reproduction commands are in the [protocol](native-qualification.md#reproduce-the-diagnostic-runs).

**Initial decision:** mechanical checks and the native identifier round trip passed; general improvement remained **unqualified**. Authored harm, six cleanup fallbacks, absent natural longitudinal feedback and missing stop-to-visible measurements were the visible gates. The subsequent bounded-recovery run below resolves this corpus's cleanup warnings; it does not qualify the offline adaptive controller, broader admission or acoustic accent adaptation. Native process/UI restart, real user correction effort and an independent multi-session evaluation remain required in [#58](https://github.com/rahulsharmavishwakarma/transcribe/issues/58).

### Cleanup reliability follow-up

A direct Qwen CPU diagnostic reproduced the three public failures using their recorded ASR text. The Bodo-background sentence expanded **it's** to **it is**, which the unchanged Minimal guard rejects. The two budget failures generated the system instruction/example text rather than the transcript. Raising the output budget cannot make that instruction echo faithful dictation. The diagnostic captured only public/authored text in an opt-in test; temporary generation logging was removed from the implementation.

Before comparing a candidate, the [58-case fixture](../../benchmarks/cleanup/native-learning-safety.json) was frozen at SHA-256 `e405e606ff62b58093d9b7060c50bf2dc76d88d88b23ac26534c6ac2d9c2f2e2`: all 38 previous public hypotheses plus 20 authored controls for lists, repetitions, contractions, quotations, unfinished thoughts, instructions spoken as data, identifiers, numbers, negation, Hindi, French and code-switching. These are development/regression cases, not a new acoustic holdout or independently labelled user feedback.

The candidate retained the existing preset instruction and official nonthinking wrapper, removed the additional system prose/examples, and added a request to preserve contractions and repetition. It used the same installed Qwen3.5-0.8B Q4_0, CPU, eight threads, greedy selection, fresh 4096-token contexts and unchanged fidelity/output-budget rules. Prefix caching was off. Baseline always ran first; these per-pass timings do not establish a latency comparison.

| CPU cleanup result | Existing prompt | Shorter candidate |
| --- | ---: | ---: |
| Public hypotheses rejected / 38 | 4 | 8 |
| Authored controls rejected / 20 | 5 | 8 |
| Total rejected / 58 | 9 | 16 |

The candidate resolves one rejection but introduces **eight new rejections** and leaves two of the original three failures unresolved. **Reject the candidate.** It remains inside an ignored experiment and is never selected by the application. Acceptance means passing lexical/number/negation checks, not independent semantic certification. The CPU result must remain separate from the earlier Metal cleanup run; even the existing prompt has one additional public rejection on CPU.

The [reproduction test](../../crates/transcribe-inference/src/qwen_qualification_tests.rs) writes all rows before checking the qualification gate and intentionally fails when the candidate regresses or misses a known failure. A completed comparison is not a passing candidate. Local logs, the frozen fixture identity, original diagnostic source and full comparison are retained under `.cache/benchmarks/results/native-learning-followup-2026-09-19/`. At this initial stage production prompts, guards, model ownership and learning rules were unchanged. This result rejects that particular workaround; the subsequent bounded recovery below is a separate candidate.

The initial core/data/desktop/inference library checks passed **172 tests** with **25 opt-in tests omitted**. Strict Clippy for desktop and inference tests passed. Installed-model experiments remain separate from that fast suite, including the deliberately failing prompt-candidate gate. At that stage the only shipped Rust-source edits were `cfg(test)` module declarations; SHA-256 additions are development dependencies on the existing workspace crate.

### Bounded formatting recovery

Review of the [official pinned Qwen template](https://huggingface.co/Qwen/Qwen3.5-0.8B/blob/f35bff78516d5b109092c941393bb6b124723ab1/chat_template.jinja) and installed llama-cpp-2 0.1.156 confirmed the existing nonthinking boundary and token acceptance. Moving the examples into actual user/assistant turns is valid template usage, but as the primary prompt it produced **10/58** development rejections versus **9/58** originally, including seven new rejections. That replacement was also rejected. Valid template support is not evidence of improved editing.

The integrated recovery keeps the original first pass and every already-accepted output. For built-in Qwen with the default Minimal instruction, it can restore a whitelist of source English contractions expanded by the model. The complete word sequences must align; other edits, omitted repetition and possessive-name guesses cannot pass. Only whitespace may separate an expansion's words, and the unchanged complete lexical/number/negation guard validates the restored output. This preserves source wording rather than accepting the model's grammatical paraphrase.

If that fails, the existing writer may try the conversation-example prompt **once** for an output-limit, unusable-output or fidelity rejection. This applies only to an unpersonalized request with at most **240 model tokens**, bounding the retry to **512 output tokens** under the existing generation formula. It uses the same resident weights and a fresh context. Cancellation, native/context failures, imported writers, custom instructions and Balanced mode do not trigger this retry. Personalized requests can restore source contractions, but do not use the alternate prompt. A second rejection keeps raw output and the warning. Metrics retain both passes, the original failure class, recovery outcome and whether contractions were restored; transcript text is not added to diagnostics.

Before viewing candidate qualification outputs, [58 disjoint text cases](../../benchmarks/cleanup/native-learning-holdout.json) were frozen at SHA-256 `74f5898e514f2a39a87369e4c6200a09bf731654997fc46735d756ad383d37e3`: the first two remaining earlier Cohere CPU hypotheses per Svarah cohort, excluding all development IDs, plus 20 authored controls. This is disjoint cleanup text evidence from a previously used ASR corpus, not an untouched acoustic or natural-user study.

| CPU text comparison | Cases | Original failures | Recovery failures |
| --- | ---: | ---: | ---: |
| Development | 58 | 9 | 2 |
| Disjoint qualification | 58 | 7 | 2 |

All three original failures are recovered. Every previously accepted output is identical; remaining failures are retained. The original three recovered outputs equal their raw transcripts, so those cases establish reliable completion without a warning, **not a word-error reduction or added punctuation benefit**. Other authored controls gain punctuation. The development and qualification screens used respectively eight and seven retries. These baseline-first text comparisons do not establish a latency speedup; retry cost must remain in native timings.

This is frozen-weight application work through the existing generator, with no new decoder/kernel, checkpoint, training or model owner. Source contraction restoration and the fallback are narrow engineering mechanisms, not semantic personalization or recursive policy improvement. Logs and complete rows remain under `.cache/benchmarks/results/native-learning-chat-examples-2026-09-19/`; [commands](native-qualification.md#reproduce-the-diagnostic-runs) compare the actual production path with the preserved original path. The earlier failed prompt candidates remain reproducible and excluded as primary prompts.

### Integrated native recovery verification

The final production entry point reproduces the **9 → 2** development and **7 → 2** disjoint text results. The core/data/desktop/inference library suite passes **176 tests**, with **27 opt-in tests omitted**; strict desktop/inference test Clippy and formatting pass. Installed-model runs are separate from that count.

The same 38 public recordings were then replayed through desktop `process_capture` in both learning and frozen-memory arms. All **76 requests pass with zero warnings/failures**, resolving the earlier six warning-bearing requests. Diagnostics retain **four accepted second passes** and **two source-contraction restorations**. The original first-pass failures are visible in the recovery metrics. Audit word edits still tie at **46**; 15/19 audits need correction in both arms, no memory activates, and paired improvements/harms remain zero. This is a cleanup reliability fix, not evidence of new personal-learning or recognition accuracy gains.

| Production backend completion, 37 matched resident pairs | Frozen memory | Automatic learning |
| --- | ---: | ---: |
| Mean | 613.72 ms | 621.37 ms |
| p95, nearest rank | 1,200.37 ms | 1,217.16 ms |
| Mean ASR stage | 430.33 ms | 432.90 ms |
| Mean cleanup stage, including retries | 178.40 ms | 178.58 ms |

Learning overhead is **1.25% mean / 1.40% p95**, within the predeclared 10% exploratory margin for this run. The first request with model loading is retained separately at **10,604.24 ms**; filesystem-cache state was uncontrolled. Timing still covers backend processing entry through publication, excluding microphone stop, IPC, rendering and insertion. The much lower absolute times than the earlier run are **not a measured speedup from the fix**: power and competing host activity differ, and no paired old/new performance experiment was performed.

Actual providers are Cohere CPU and Qwen Metal, eight graph/worker threads, on the same Mac17,8 / 64 GiB / 18 logical CPUs / macOS 27.0 (26A428), using the Rust test profile. The preflight reported AC power, while the during-run sampler reported battery power at 84% → 83%; this transition was uncontrolled. Four ten-second thermal samples during part of the 58.03-second process reported no thermal/performance warnings. Peak RSS was **4.94 GiB**. This is neither sustained thermal qualification nor a release-build/UI latency benchmark.

The separate four-request native identifier round trip also passes after integration: activation, later application after Runtime recreation, reversal, Forget and stale-evidence isolation. It uses authored edits on repeated public audio and cannot supply independent human support. The frozen native test executable is recorded at SHA-256 `b91ea1dfc78cc78e9b7d16c7902728a2311426664b6fcd3d0140184a7ee853ea`, with source/fixture hashes in `native-source-identity.json` beside complete native reports, stderr resource totals and `verified-native-summary.json`.

**Current decision:** the scoped recovery is implemented and passes these automated gates. #58 remains open for the authored unflagged-intent harm, natural longitudinal feedback/correction effort, and actual microphone/UI/process-restart qualification. The [Phase B shadow proposal](https://github.com/rahulsharmavishwakarma/transcribe/issues/59#issuecomment-5740928523) is awaiting architecture approval; no semantic-analysis storage, idle analysis API, live policy controller or training has been added.

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

The user requires automatic background learning as core product behavior, so the desktop no longer has an opt-in gate. That requirement does not change the measured efficacy limits or qualify broader admission thresholds. A future qualification needs multiple sessions per consenting user, independently labelled corrections/retry relations, grouped chronological splits, locked release gates, protected-meaning failures, correction time/burden and representative unflagged samples. Keep synthetic, public-corpus and real-user results separate. The later shadow mechanism checks below exercise Phase B execution and isolation, without establishing semantic style benefit or acoustic adaptation.

## Semantic shadow qualification

The [Phase B implementation contract](semantic-shadow.md) separates production spelling learning from evaluation-only semantic proposals. The installed-model runner compares supplied labels, retains invalid responses and tests cancellation during 20 alternating-order foreground cleanup pairs per run. Fixtures are authored text; no natural-user benefit, acoustic accent adaptation or complete microphone/UI latency is inferred.

The September 19 screen used the pinned Qwen3.5-0.8B Q4_0 artifact (revision `8fea620810c4afa23dd6443f999a48574c1611a3`, SHA-256 `57d1997790d1744fba5b40a7317df71ea5e2acee28c47e78f0cce39c0703f8cf`), llama-cpp-2 0.1.156/native `e79e4bf660e19f2ad851e06c6913f7a8c5852621`, 8 threads, Mac17,8 with 64 GiB RAM/18 logical CPUs, macOS 27.0 (26A428). CPU and actual Metal providers were recorded separately. The host was shared; power changed from AC/battery discharging to battery. Peak memory and sustained thermal behavior were not qualified.

Initial prompt version 1 failed schema validation on all 24 development cases because its example encouraged unquoted enum values. Version 2 produced 18 valid responses and 9 correct relation labels. A balanced five-class-example variant produced 21 valid but only 6 correct labels and was rejected before opening the holdout. The retained version 2 was then tested on 22 separate authored cases. These labels were written before scoring, without independent human review; they cannot establish natural-user efficacy.

| Final holdout measurement | Metal | CPU |
| --- | ---: | ---: |
| Structurally valid responses | 18/22 | 19/22 |
| Correct relation labels, all cases | 7/22 | 7/22 |
| Invalid responses retained as failures | 4 | 3 |
| Mean semantic analysis time | 315.57 ms | 640.13 ms |
| p95 semantic analysis time | 372.59 ms | 712.61 ms |
| Mean foreground cleanup, no shadow | 73.18 ms | 239.82 ms |
| Mean foreground cleanup, interrupted shadow | 85.98 ms | 244.33 ms |
| Mean paired added foreground delay | 12.80 ms | 4.52 ms |
| p95 paired added foreground delay | 20.90 ms | 9.03 ms |

Each provider used 20 alternating-order latency pairs. All 40 final pairs returned identical foreground text without failure; all interrupted jobs returned cancellation without a proposal. No timeouts occurred in these short cases. Cold foreground load plus cleanup was 555.38 ms on Metal and 841.65 ms on CPU, separate from resident timings. A CPU run overlapping the native app test is retained as `cpu-final.json`; the table uses the subsequent serial `cpu-final-serial.json`, not a mixture of runs. These timings exclude ASR, microphone capture, Tauri/UI, insertion and desktop proposal persistence. They do not establish a universal responsiveness budget.

The classifier missed every changed-intent, unrelated and ambiguous/unknown holdout label. Repair precision was 35.29% Metal / 33.33% CPU; continuation recall was 25% on both. Each provider made two output-form errors among its valid responses and missed the sole relevant-memory selection; zero false-positive memory IDs is uninformative with that zero recall. Full per-class precision/recall, confusion matrices, errors and abstention/failure counts remain in the JSON artifacts.

**Decision: no automatic semantic activation.** The mechanism works, but this installed writer does not pass semantic quality qualification. A dedicated installed-writer structured-output test also fails on a retained development example; it remains an explicitly ignored, unmet gate rather than being weakened to pass. Do not use these guesses as policy feedback, training labels or evidence of recursive improvement.

The native desktop integration test exercised a real resident writer, ordinary draft edits, proposal persistence with audio collection off and Forget while analysis was running. It passed, retained the edited text and left no sessions or resurrected evidence. This is the native Rust application path with an isolated temporary store; it is not a microphone/UI/process-restart end-to-end test.

Artifacts are local under `.cache/benchmarks/results/semantic-shadow-2026-09-19/`: development variants, final provider reports, a separately retained contended CPU run, native app/failed-schema logs, regression/Clippy logs, environment metadata and source hashes. The inference benchmark binary SHA-256 is `5179534c7532de812d640840059808057138ea454a03f39d6c94864a984b0142` (base `7a596c1` plus unstaged implementation). Subsequent saved/unsaved Undo association hardening changes application/data deletion only, not the measured prompt/decoder. Development fixture SHA-256: `0e25ffa6c35f7a9acd6a2b8ebabde10b99d4872649eb5f4ce76d890ea1220be6`; holdout: `0b14bad8347a9cb2e7310ece4999856276bcde5b7cc4dac55a38c527b19b7e78`. No classifier tuning was performed on the holdout.

## Policy shadow and TextEdit component checks — 19 September 2026

The approved application shadow implementations are recorded in [policy evaluation](policy-experiment.md#persistent-application-shadow-evaluation) and [external observation](external-observer.md). The ignored local artifact `.cache/benchmarks/results/personalization-shadow-2026-09-19/debug-summary.json` records the source hashes and exact scope, following the [benchmark evidence policy](../../benchmarks/asr/README.md). Measurements used the local implementation identified by those source hashes; they do not qualify a release.

The final checks passed **196 Rust unit/integration tests** (24 existing opt-in tests excluded), **28 offline policy replay tests**, **17 configuration tests**, and **46 browser regression checks**, plus strict Clippy and Svelte checks. The new coverage includes pre-edit snapshots, stale reservations, Undo/Forget, corruption/expiry, scope/partition ambiguity, Unicode boundaries, cancellation and correlation links from coalesced/unscored events. Native matcher/transition tests run pure Foundation logic without opening an editor. The optional control was also checked using keyboard Space, Save/reload and a 720-pixel layout without horizontal overflow.

The authored 128-pair debug-build replay on Mac17,8 / arm64 / 64 GiB / macOS 27.0 measured:

| Stage | Mean | p95 |
| --- | ---: | ---: |
| Baseline draft learning/store operations | 3.65 ms | 6.17 ms |
| Draft learning with pre-edit snapshot and shadow reservation | 16.23 ms | 27.80 ms |
| Paired added foreground work | 12.58 ms | 21.80 ms |
| Separate background score/persistence work | 11.97 ms | 21.32 ms |

These are sequential component timings with alternating baseline/shadow order on a shared, uncontrolled host. They include repeated SQLite opens and retention checks. They exclude the actual worker queue, ASR, microphone, UI and editor; do not report them as average dictation latency or a production latency budget. Both stores produced the same active personalized output, and shadow storage retained eight decisions. No candidate changed the live policy.

Real plain-text TextEdit tests used newly authored temporary documents. AX-driven insertion acknowledged and produced the intended emoji-containing correction with **five text reads, largest 68 UTF-16 units**. An outside-span replacement produced no candidate (four reads, largest 81 units); explicit cancellation produced no candidate. Two actual Command-V runs returned Sent, but identity-change notifications ended observation (one acknowledged before stopping). Thus **full paste-to-correction qualification did not pass**. Visible GUI tests were stopped and the temporary documents closed; further checks were headless. Natural author attribution, document reuse, IME, other editors, microphone-to-editor operation and sustained resource behavior remain unqualified.

The tests found and fixed an empty-boundary matcher error, creation of a store for a learning-disabled result, and TextEdit's missing optional Enabled attribute handling. Synthetic V key-up now releases its Command flag. A proposed broad saved-result cleanup was rejected before execution by automatic approval review; the implementation instead invalidates related session/result evidence and tests preservation of unrelated records. No active vocabulary, policy or reviewed labels are derived from external shadow candidates. #60 remains gated by representative feedback; #61 remains gated by native attribution/compatibility qualification.

### Combined application regression and transaction isolation

Consolidating the native cleanup recovery, the three shadow paths and optional
automatic audio leveling exposed an intermittent ordinary draft-save failure:
a background SQLite writer could commit after the foreground transaction read
its snapshot, causing its later write to fail with `database is locked`.
Mutations now reserve the writer before reading; read-only snapshots remain
separate, and shadow-state pruning is atomic. The [storage contract](storage-and-privacy.md#undo-conflicts-and-deletion)
explains the isolation and background lock-wait boundary.

A deterministic regression failed before the fix and passes afterward. The
previously failing desktop test also passes 20 consecutive runs. The integrated
checks pass 226 Rust library/binary tests plus 28 policy replay tests (30 existing
opt-in tests excluded), 102 Node tests, five automatic-level reporting tests,
the headless browser regression suite, strict all-target Clippy, formatting and
Svelte checks. No desktop application was opened during this integration pass.
A shared native build-cache collision was resolved by rebuilding in an isolated
target directory; it is retained separately from the application defect.

The final combined implementation on local `main` also passes the complete
`make check`: locked Python manifests/exports, formatting and linting,
Svelte/TypeScript, 102 Node tests, all 13 Python suites, headless browser
regressions, native completion fixtures, all-target Rust checks/tests,
doctests, strict Clippy and rustdoc. The final log is
`main-make-check.log` in the local artifact directory below. No microphone or
visible editor was opened for these deterministic checks.

A fresh 128-pair authored policy component run after the transaction fix measured
baseline mean/p95 **2.31/3.51 ms**, shadow foreground **10.11/16.61 ms**, paired
added foreground **7.80/13.19 ms**, and separate background **7.27/12.48 ms**.
Active output remained identical and eight decisions were retained. This is a
Rust development build on the same uncontrolled shared host; lower times than
the earlier run do not establish a speedup. The definitions and exclusions above
still apply, including no worker queue, ASR, microphone, UI or editor latency.

Source hashes, complete measurements, initial failures and final logs are local
under `.cache/benchmarks/results/personalization-shadow-2026-09-19/integration-followup/`.
These mechanical checks do not resolve semantic quality, natural-user benefit,
TextEdit keyboard observation, external admission or live-policy promotion gates.
