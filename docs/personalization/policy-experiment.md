# Bounded policy-improvement experiment

This is an **offline prototype of the second learning loop**, authorized by the user's request to try and test policy improvement. It is implemented in the [Rust replay example](../../crates/transcribe-data/examples/policy_learning_replay.rs), not connected to live dictation. [Issue #34](https://github.com/rahulsharmavishwakarma/transcribe/issues/34#issuecomment-5717283286) records the experiment scope. The [initial measurement report](../../benchmarks/asr/2026-09-17-policy-learning.md) and [expanded stress report](../../benchmarks/asr/2026-09-18-policy-learning-stress.md) preserve results and limitations.

The prototype can change which learned memories are eligible for use, test a proposed change on subsequent feedback and return to the preceding policy. Its candidate family, scoring rules and controller are human-written and fixed. This is **bounded policy selection**, not open-ended discovery of better learning algorithms or a demonstration of unrestricted recursive self-improvement.

On 19 September the user approved integrating the **fixed teaching-context guard** into the desktop separately from this controller. Its persistent metadata and raw-hypothesis selection rules are specified in [dictionary learning](automatic-dictionary.md#context-required-for-post-recognition-use). The app additionally requires an unquoted alias/canonical occurrence and bounded cues; it is not a verbatim deployment of the replay policy. The earlier experimental scores remain historical offline evidence. Live scoped promotion/rollback is still unimplemented and requires representative feedback qualification.

## What changes and what stays fixed

The real `DatasetStore` still extracts guarded spellings, stores ordinary-edit provenance and performs its existing admission/suspension checks. The experiment applies an additional eligibility filter to a copied profile before calling the real deterministic matcher. It does not relax number/date/negation/quote guards, change acoustic decoding, rewrite code, or train weights.

| Experimental parameter | Allowed values | Meaning |
| --- | --- | --- |
| `supportFloor` | 1, 2, 3 | Additional minimum independent correction support before an automatically learned entry can be used. Existing stricter admission rules still apply. |
| `contextOverlap` | 0, 1 | Require no extra lexical context or at least one shared informative term from an eligible teaching edit. |

The starting policy is `(1, 0)`, which represents the existing eligibility behavior on this corpus. A proposal changes one coordinate by one step. At most three neighbors are examined per round; there are six policies total. Every fixed policy is also run as a separate comparison after the adaptive trajectory. Hidden scores and the best fixed comparator never influence the controller.

Teaching context consists of bounded lexical terms derived from observable edits, excluding the alias, intended spelling, project words and a small fixed English function-word list. This is not semantic context understanding. These terms exist only in temporary experimental memory; they are not a new production profile schema. Manual entries are not changed by the policy filter. Model identity is provenance and is not a memory key.

## A complete round

1. **Learn:** feed supplied correction/review targets to the existing learner after producing that episode's output.
2. **Develop:** capture the incumbent's state before each episode, then collect externally supplied feedback. Compare immediate candidate outputs on those immutable states. Choose at most one neighbor that passes the development gate.
3. **Validate:** lock the proposed policy before seeing a fresh feedback partition. Compare it with the incumbent. Promote only if the gate passes again. The previous policy can instead be restored if fresh feedback demonstrates a qualifying regression of the active policy.
4. **Audit:** score later outputs with hidden desired text. Audit and final-test episodes have no feedback field and cannot affect the learner, policy proposal, validation or rollback.

The policy gate requires at least **four independent feedback components**, at least **two independently improved components**, strictly fewer aggregate word edits, zero per-case regressions and no increase in harmful components. Any evaluation or learning failure in the compared feedback blocks promotion. Ties retain the incumbent. These are small deterministic experiment gates, not statistically validated production thresholds.

An audio-source match or a repair-chain match links feedback, including transitive connections through duplicate events. **Experiment version 2 evaluates every member of a connected component.** Each component contributes the maximum incumbent and candidate word-error counts, at most one harmful-output flag per policy, and one improvement vote only when a member improves and none regresses. Any member's regression or evaluation/feedback failure vetoes the gate. Repeating a favorable case cannot increase its component's weight. Earlier first-only scoring could hide later adverse feedback and depended on case order; regression tests reproduced this before the fix.

The experimental support floor also uses retained edit-event links so a rejected duplicate cannot bridge two apparently independent supports. Disabled entries bypass eligibility checks, including provenance lookup; an enabled automatic entry with missing provenance still fails. This handles non-active evidence eviction without reactivating disabled memory. These changes are in the offline evaluator, not the live learner. Dependencies that have aged out of retained event history cannot be reconstructed; longer-term provenance needs qualification before integration.

The selected comparison is **one-step counterfactual replay on incumbent states**. It does not prove that a candidate would produce the same future feedback or memory trajectory if deployed. The adaptive trajectory and six fixed comparators separately process their own generated outputs and the same supplied targets. Simulated targets make this reproducible but cannot establish real interaction effort or causal user benefit.

## Observed-failure replay variant

Experiment version 3 adds the optional **benchmark argument** `--failure-bank`; it is not a desktop preference or a live learner. Omitting it retains the current controller's decisions. The variant evaluates the [Z.ai-inspired feedback proposal](../research-rsi-2026-09.md#zai-inspiration-diagnose-replay-and-verify) using the same real SQLite learner, six policies and fresh-evidence gates. It does not reproduce Z.ai's infrastructure agent, kernels or serving benchmarks.

After a round's decision, the bank admits development/validation cases where the pre-event incumbent output has a positive word-edit distance from the supplied feedback. Each case retains its immutable pre-event snapshot, input and supplied target. Failed/unscorable feedback is counted and excluded from the bank; it still vetoes decisions in its original fresh partition. Learn, audit and final-test labels never enter this bank. Supplied targets remain simulated feedback, which can be misleading; admission does not certify user intent.

The bank retains at most **64 cases**, evicting the oldest whole source/repair-connected component when over capacity. A component larger than the entire bank can be evicted completely. It is temporary experiment memory, with no production database changes, TTL or preference-expiry mechanism. Snapshot contents follow existing store bounds; this count limit is not a measured byte/RAM budget.

Historical comparisons may **veto**, but never provide missing fresh improvement votes. A proposal must still pass development and a subsequent validation partition independently. Historical acceptance requires no per-case regression, no evaluation failure and no increase in aggregate errors or harmful components. The same history check applies to rollback. Current-round failures become historical evidence only after the decision, preventing validation feedback from entering proposal selection. Cases are evaluated on their frozen earlier snapshots, not the current memory state.

This can prevent recurrence, but stale or incorrect feedback can also block useful adaptation. It cannot prove future non-regression, and eviction limits what it remembers. The [comparison report](../../benchmarks/asr/2026-09-18-failure-bank-comparison.md) retains the current-controller, variant, fixed-policy and public-corpus results rather than treating fewer rollbacks as inherently better.

## Scoped policy alternative

Experiment version 4 adds two further **offline** arguments. `--strong-global` initializes the existing global controller at `(1,1)`, isolating the effect of the better starting policy. `--scoped` uses that same fixed fallback and separate controllers for an unambiguous observable `(project, language, lexical teaching cue)` key. Routing reads the current raw hypothesis and pre-event teaching metadata only. Model IDs, feedback targets, hidden references, scenario names and round IDs are not routing inputs. One eligible automatic vocabulary match and one shared teaching cue are required; missing/multiple cues, multiple matching entries and unmatched scope use the fallback.

The scoped variant retains the same fresh development/validation gates, applies updates and rollback only to that key, and removes the earlier global historical veto. Source/repair components are formed before grouping; a component spanning different or unknown keys is excluded from scoped evidence. It cannot become multiple independent votes. At most 32 attempted scope controllers are retained; oldest-created controllers are evicted whole and their scopes return to the fixed fallback. Decisions and current scoped policies are reported separately. `finalPolicy` is the fallback in this mode; actual policies are recorded per output row. This is lexical routing, not semantic intent or acoustic accent detection.

These mechanisms remain human-written and fixed. They can adapt application policy choices, but do not invent improved learning algorithms. Changing the initialization or adding scopes is not evidence of benefit by itself; compare with fixed `(1,1)`, the same-initialization global controller, the original controller and the failed failure-bank mode on identical fresh data. The [scoped-policy comparison](../../benchmarks/asr/2026-09-18-scoped-policy-comparison.md) owns that evaluation and its limitations.

## Inputs, isolation and bounds

The manifest has ordered rounds with `learn`, `develop`, `validate`, `audit`, then a final test. Each episode supplies an input containing result/source/episode IDs, time, project, language, model provenance and text; an optional `edit` target supplies feedback; `reference` is reporting-only. A target on an unchanged output means an **explicit independently supplied outcome label**, not inferred acceptance from silence or Copy. The constructed and public fixtures both simulate this stronger feedback availability.

The parser rejects nonchronological events, duplicate result IDs, audio/repair-chain reuse across partitions and any audit/test feedback. Limits are 16 rounds, 2,048 total episodes, 64 development/validation episodes per round, 8,192 bytes per text and bounded identifiers. Existing core word-comparison limits remain; unscorable comparisons are failures, not zero errors.

Each learner owns a new temporary SQLite directory. The executable accepts an input manifest, output report path and the optional experiment flag; it has no live-profile argument. It performs no network request, model call, model download, microphone recording, external insertion or production policy promotion. Temporary snapshot reads, fixed-policy comparisons and setup are excluded from the individually labelled component timings. Version 3 separately records the adaptive-plus-baseline replay duration and total duration including all six fixed controls; these include setup/snapshot work but exclude JSON file serialization. There is no UI scheduling or battery/thermal qualification.

This is category-1 application experimentation using frozen model outputs. There are no category-2 decoder/kernel changes or category-3 checkpoint changes. The current production policies, schemas and model lifetimes remain unchanged by this experiment.

## Reproduce and verify

```sh
. ./scripts/dev-env.sh
cargo test --locked -p transcribe-data --example policy_learning_replay
cargo run --release --locked -p transcribe-data --example policy_learning_replay -- \
  benchmarks/asr/fixtures/policy-learning-episodes.json /tmp/policy-learning-results.json
```

The [fixture preparer](../../scripts/prepare-policy-learning-replay.py) reproduces the checked-in constructed manifest and converts the prior public feedback manifest. The measurement report gives the public command and artifact hashes. Generated outputs belong in the ignored cache or a temporary directory.

Nineteen experiment tests cover two successive changes and rollback, candidate rejection on fresh regressions, hidden-label/metadata invariance, source/chain deduplication including transitive bridges, lack of feedback, injected failure, score overflow, invalid/non-neighbor proposals, actual learner support/undo, partition leakage and quote/language/project guards. The expanded checks include 512 seeded correlation graphs compared with an independent graph-reachability oracle, manifest budget boundaries, repeated improvement votes, contradictory duplicate order, failures at every duplicate position, separate user stores, forgetting, and disabled evidence eviction. These test implementation behavior. They do not qualify real users, semantic judgments or live scheduler interference.

Version 3 adds four tests for failure admission, whole-component bank eviction, historical rollback veto without fresh-vote inflation, and bank/output invariance to poisoned hidden references and changed model metadata: **23 evaluator tests total**. The comparison also runs ten Python harness/report tests. `--seed-start` freezes new histories without changing the generator's scenario rules; `--failure-bank` selects the experimental variant for both stress execution and isolation checks. The [comparison report](../../benchmarks/asr/2026-09-18-failure-bank-comparison.md#reproduce) gives the complete commands and timing definitions.

Version 4 adds five scoped tests: context-local updates and later rollback; unknown/multiple-cue, quote, project/language and multiple-entry fallback; correlated/cross-scope evidence rejection; 32-controller eviction; and hidden-label/model-metadata invariance. The suite now has **28 Rust evaluator tests** and **13 Python harness/report tests**. `--variant` in the stress harness selects `current`, `failure_bank`, `strong_global` or `scoped`; the same variant must be supplied for isolation verification.

The [stress harness](../../scripts/stress-policy-learning.py) freezes 97 histories before scoring: eight scenarios with 12 seeds each plus a capacity history. They contain 23,809 text/feedback episodes. Scenarios cover stable teaching, context/support shifts, wrong supplied feedback, sparse feedback, correlated retries, language/project switching and silence without feedback. Each history runs the adaptive learner, its starting-policy baseline and all six fixed comparators in separate temporary stores. Held-out quality, failures, protected-text checks, exact capacity counters and component timings are retained. The capacity history crosses the 128 vocabulary, 384 candidate and 1,024 event limits; the unit storage tests separately exercise 30-day pruning with an explicit clock value. Manifest timestamps establish order only and do not advance the SQLite learner's real clock.

```sh
uv run --locked python scripts/test-stress-policy-learning.py
uv run --locked python scripts/stress-policy-learning.py prepare /tmp/policy-stress-manifests
uv run --locked python scripts/stress-policy-learning.py run /tmp/policy-stress-manifests \
  --output /tmp/policy-stress-results
uv run --locked python scripts/stress-policy-learning.py verify-isolation /tmp/policy-stress-manifests \
  --output /tmp/policy-stress-results
```

Preparation and results require new directories to preserve existing evidence. The default executable is `target/release/examples/policy_learning_replay`; build it with the release command above. Five Python harness tests verify deterministic generation, absent audit feedback, manifest tamper rejection and retention of nonzero exits, timeouts and missing output artifacts. Isolation checks rerun one seed per scenario after changing model metadata, expected-decision metadata and every hidden reference to an unscorable target. Decisions and emitted text must remain identical while hidden-scoring failures remain visible. No observed target, policy family or gate is tuned using final scores. This is development stress testing with authored feedback, not a pristine deployment qualification set.

## Interpretation and next decision

The [September RSI review](../research-rsi-2026-09.md) evaluates Dream-RSI and related work against this experiment. The first extension, a bounded bank of already-observed failure cases alongside fresh validation, is now an isolated benchmark variant described above; hidden audit labels remain unavailable. Its implementation does not activate a live controller.

The constructed run improved versus the starting policy but **tied the best fixed policies on total held-out word edits and produced more harmful changes than those fixed alternatives**. The public replay made no policy updates and showed no gain. Validation approved a rollback that later re-exposed adverse cases absent from that validation partition. The held-out results were kept out of the controller; the corpus and policy family were not retuned to remove those failures.

Across all 66 constructed episodes, adaptive and baseline error totals also tied; the final eight test episodes showed no gain. The improvement came from intermediate hidden audits. This distinction matters when assessing the cost of learning and the user's total correction burden.

The expanded simulation also improves versus the starting policy in context/support shifts but remains worse overall than the best fixed alternatives. Wrong feedback and sparse feedback do not produce reliable adaptation. A passing implementation test is not evidence that every user's experience improves.

The subsequent failure-bank comparison is also negative for deployment: hidden errors rise **2,599 → 2,625**, while the strong fixed control scores **1,464**. The bank blocks some recurrence and some useful adaptation; all 304 cached Svarah outputs remain unchanged. The measured history-update mean is about 0.565 ms per experimental round, but no live background latency was measured. Retain the variant as an experiment, not a default improvement.

The scoped alternative produces a **positive simulation result**, with 1,372 hidden edits versus 1,468 fixed context and 1,412 same-start global adaptive on its fresh suite. Its extra benefit over the same-start control is confined to support shifts; six individual held-out examples still regress. Cached Svarah outcomes remain unchanged. Better initialization and context-specific adaptation must be reported separately, and a passing simulation must not be labelled broad recursive improvement.

Before proposing live integration, qualify representative longitudinal feedback, paraphrased/ambiguous contexts, preference drift, real editing burden and native resource interference. Include representative unchanged outputs with independent labels, not implicit approval from silence. Persistent shadow evaluation is described below. Atomic live promotion/rollback and native resource qualification remain necessary; neither is activated by these trials.

## Persistent application shadow evaluation

The September 19 [approved design](https://github.com/rahulsharmavishwakarma/transcribe/issues/60#issuecomment-5742682689) is implemented separately from live memory. Ordinary non-noop in-app edits capture an immutable pre-learning snapshot. One lazy application worker holds one running job and one replaceable pending job; foreground work cancels its token. The six-policy predictor, independent-component scorer and gates are shared with this replay executable in `transcribe_data::policy`.

The worker freezes six word-edit scores against the explicit edited wording, not a model-generated label. It persists compact scores and source/repair identities instead of whole profiles or transcripts. Such wording is an observable correction target, not an independently reviewed verbatim audio label. Raw text equal to the target is still valid negative feedback when personalization changed it incorrectly.

Development/validation assignment is frozen by admission sequence before selecting a proposal. Connected components are computed across all retained identities before scope/partition routing; a late transitive bridge across scopes or partitions removes that entire component's votes. Both partitions must independently have four components, two improvements, no regression/failure, a strict error reduction and no added harmful component. Unknown scope retains the fixed fallback. Six-policy scores remain tied to the pre-event profile; new learning cannot rewrite old counterfactuals.

Storage retains 256 scored evidence cases and up to 1,024 correlation-only identity links from the existing event store (including cancelled/coalesced edits and raw observations, never success votes), at most 32 routed scopes and eight versioned decisions per scope, with 30-day expiry on access. Decisions include the prior decision version, incumbent/candidate, independent gate scores, supporting result identities and failures. The incumbent remains the strong fixed `(1,1)` comparator for every shadow evaluation: repeated evaluation does not promote a candidate or manufacture fresh labels. Historical decisions describe earlier evidence; only the latest decision reflects current retained bridges. This implementation does not recursively advance a shadow incumbent or deploy the offline controller's promotions/rollback.

One exact persisted job reservation and the existing learning epoch guard completion. New jobs supersede old tokens. Undo invalidates dependent evidence/decisions; saved-result Undo uses session identity. Forget deletes evidence, decisions and reservations. Unknown/corrupt schemas fall back to fixed evaluation. Pending jobs never replay on startup. Cancellation is cooperative between bounded stages; SQLite busy waiting is limited to 25 ms for completion, not a hard bound on filesystem latency.

No production memory application reads these decisions. Live promotion and persisted rollback still require a separate proposal and representative independently labelled longitudinal feedback from #58. Failed semantic proposals from #59 cannot fill this gap. The approved addition provides evidence and diagnostics without claiming a new accuracy benefit.

Reproduce isolated authored feedback overhead with `cargo run -p transcribe-data --example policy_shadow_replay --locked --offline`. It alternates 128 baseline/shadow corrections in separate temporary stores and reports foreground capture/reservation and background scoring/persistence separately. It excludes ASR, worker queue, microphone, UI and editor latency. Existing policy replay tests remain the regression oracle for grouping, gate failures, scope routing and rollback; the new storage tests cover pre-edit immutability, stale tokens, corruption, bounded retention and deletion.

Current component measurements and remaining qualification gates are in the [September shadow validation](evaluation.md#policy-shadow-and-textedit-component-checks--19-september-2026). The desktop local diagnostic command `get_learning_shadow_diagnostics` exposes bounded policy decisions and external candidate counts; it has no promotion action.

The 25 September serial rerun at `8a2c27b` preserves active outputs across all
128 authored baseline/shadow pairs. Added foreground store latency is 7.07 ms
mean / 11.94 ms p95; background scoring/persistence is 6.66 / 11.55 ms. The
debug example ran after compilation, with binary/source identity and process
observations retained alongside the [inference qualification](../inference-performance.md).
These remain component measurements, excluding ASR, queue contention, UI and
real correction effort. They do not qualify live promotion or substitute for
independent natural feedback.
