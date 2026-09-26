# Scoped policy refinement — 18 September 2026

> Historical evidence: names and commands below describe the recorded revision.
> For the current checkout, use the corresponding `vesper-*` package/executable
> and `VESPER_*` override names; legacy environment aliases remain supported.
> See [current naming and compatibility](../../docs/development.md#product-name-and-compatibility). No measurement has been requalified by renaming.


**The refinement improves this fresh simulation beyond both our original adaptive controller and the strong fixed policy.** Across 7,300 held-out episodes, word edits fall from **2,598 original adaptive → 1,372 scoped adaptive**. The previously identified fixed context policy scores **1,468**. This is a promising offline result, with remaining per-case regressions and no gain on the cached public-audio controls. It does not qualify a live rollout or establish recursive discovery of better learning algorithms.

The local debug report at `.cache/benchmarks/results/2026-09-18-scoped-policy-comparison/debug-summary.json` contains all history/source/binary hashes, fixed controls, scenario aggregates, paired episode outcomes, scope counts, isolation checks and repeated timings. The [canonical contract](../../docs/personalization/policy-experiment.md#scoped-policy-alternative) defines the implementation. [Issue #34](https://github.com/rahulsharmavishwakarma/transcribe/issues/34#issuecomment-5733990275) records the requested refinement and predeclared comparison.

## What changed

The earlier [global historical-veto experiment](2026-09-18-failure-bank-comparison.md) blocked useful adaptation along with old mistakes. The new alternative starts from the previously identified `(supportFloor=1, contextOverlap=1)` policy and maintains separate bounded controllers for observable project, language and lexical teaching cues. Unknown or ambiguous routes use the fixed fallback. Fresh feedback can revise or roll back one scope without globally changing other contexts.

Both the policy family and improvement gates remain fixed. The same four-independent-case/two-improvement/zero-case-regression checks apply separately to development and subsequent validation. Source/repair components are formed before scope grouping. Cross-scope or unresolved components cannot supply scoped votes. Model identity, reference answers, corrected targets, scenario labels and round IDs never define a routing key. Scope state is temporary, capped at 32 controllers with oldest-created eviction; there is no production schema, model, inference-owner or scheduler change.

This is our engineering hypothesis. [SE-GoS](https://arxiv.org/html/2609.08228v1) motivates separating retrieval adaptation from stored content and documents repeated-round regression; [MASkills](https://arxiv.org/html/2609.02094v1) motivates attribution and held-out validation. Neither paper demonstrates this lexical router or validates its deterministic thresholds. No author implementation or model API is used.

## Frozen comparison and attribution

The suite contains **96 fresh histories**, seeds **20000–20011** across eight existing scenario families, plus the reused capacity control: **97 histories and 23,843 episodes per variant**. Hidden audits plus final tests contain 7,300 episodes, including 2,692 final-only examples. Only the capacity manifest overlaps the previous suite by hash. Every manifest was frozen before scoring and still reproduces byte for byte; no targets, thresholds or routing rules were retuned to the new held-out results.

Four adaptive variants run identical inputs through the real SQLite learner, each with an independent starting-policy baseline and all six independent fixed-policy controls:

| Variant | Purpose |
| --- | --- |
| Original adaptive | Preserve the previous `(1,0)` starting point and global controller |
| Global failure bank | Retest the failed historical-veto approach |
| Strong-start global | Use `(1,1)` with the same global controller, isolating initialization |
| Scoped adaptive | Use `(1,1)` fallback with context-specific controllers, isolating scope from initialization |

The first run caught a provenance problem: a shared core configuration source changed while the unchanged executable was running. Its outputs and measurements were retained. Rust/Cargo inputs and the fixture were then copied into an immutable local snapshot, rebuilt with `--locked`, and the complete four-variant suite and timing comparison rerun. Final source hashes refer to that snapshot, not a subsequently edited working tree. No unrelated configuration changes were reverted. The recorded Git HEAD alone does not identify the evaluated untracked source.

## Quality

Scores are total word-edit distances, **not normalized WER**. Lower is better.

| Partition | Episodes | Existing eligibility baseline | Original adaptive | Global bank | Strong-start global | Scoped |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Complete replay | 23,843 | 19,031 | 18,142 | 18,078 | 16,477 | **16,407** |
| Hidden audits plus final | 7,300 | 3,171 | 2,598 | 2,636 | 1,412 | **1,372** |
| Final only | 2,692 | 1,939 | 1,741 | 1,733 | 696 | **652** |

The scoped held-out total is **47.2% lower than original adaptive**, **6.5% lower than fixed context**, and **2.8% lower than strong-start global**. Most improvement over the original controller comes from the stronger initialization; the additional 40-edit reduction over the same-start global controller measures the contribution of scoping on this suite. Do not attribute the entire 47.2% to learning a better routing policy.

| Fixed policy | Held-out edits | Outputs worse than raw |
| --- | ---: | ---: |
| Support 1, no extra context | 3,171 | 1,110 |
| Support 1, context required | **1,468** | 198 |
| Support 2, no extra context | 3,163 | 509 |
| Support 2, context required | 2,834 | 209 |
| Support 3, no extra context | 3,274 | 102 |
| Support 3, context required | 3,274 | 102 |
| Strong-start global adaptive | 1,412 | 135 |
| Scoped adaptive | 1,372 | 132 |

The fewer-error result is not an all-metrics win: fixed support 3 has fewer outputs worse than raw while missing more useful corrections. “Worse than raw” is based on edit distance, not an independent judgment of meaning preservation.

| Paired held-out comparison: scoped versus… | Improved episodes | Regressed episodes | Tied episodes |
| --- | ---: | ---: | ---: |
| Original adaptive | 757 | **11** | 6,532 |
| Global failure bank | 774 | **10** | 6,516 |
| Strong-start global | 26 | **6** | 7,268 |

At the history level, scoped improves 80 and ties 17 versus original adaptive; no history has a worse aggregate, although 11 individual episodes regress. Against strong-start global, nine histories improve, two regress and 86 tie. Against fixed context, 11 histories improve and 86 tie. Aggregate improvement is not a future non-regression guarantee.

| Held-out scenario | Original adaptive | Global bank | Strong-start global | Scoped |
| --- | ---: | ---: | ---: | ---: |
| Stable | 255 | 255 | 0 | 0 |
| Context shift | 39 | 39 | 0 | 0 |
| Support shift | 264 | 310 | 206 | **166** |
| Misleading feedback | 437 | 429 | 199 | 199 |
| Sparse feedback | 408 | 408 | 371 | 371 |
| Correlated retries | 391 | 391 | 101 | 101 |
| Scope/language change | 290 | 290 | 21 | 21 |
| No feedback | 514 | 514 | 514 | 514 |
| Capacity protected queries | 0 | 0 | 0 | 0 |

All incremental benefit over the same-start global controller occurs in support shifts. The eight families reuse simple artificial lexical topics; these results do not demonstrate semantic understanding or robustness to paraphrases. The no-feedback condition gains nothing and creates no memories. The supplied unchanged-output labels in other conditions represent simulated explicit review, not approval inferred from silence or Copy.

Scoped adaptation makes 24 promotions, four rollbacks and seven rejected proposals. It routes 3,194 of 23,843 episodes to an unambiguous key; an eligible key may still use the fallback if it has no learned override. The maximum retained scope count is three, with no evictions in the main suite. The 32-scope limit, eviction fallback and ambiguous/correlated routing are exercised by targeted tests, not by this distribution's normal load.

## Public and accent controls

The cached controls use 192 earlier calibration hypotheses with references simulating corrections. They do not rerun acoustic inference or capture genuine user correction sequences.

| Held-out control | Recordings | Word edits for each of the four variants | Learned-memory/policy outcome |
| --- | ---: | ---: | --- |
| Earlier FLEURS/Svarah control | 48 | 181 | No active automatic memory or effective policy changes |
| Expanded Svarah screen | 304 | 977 | No gain |

The Svarah screen covers the 19 language-background groups documented in the [native accent report](2026-09-18-accent-personalization.md). They are accent proxies, not predictions from an accent detector. The lack of usable repeated personalized vocabulary means these controls do not establish benefit for real users. Existing model/runtime and dataset revisions remain in that report; this experiment changes no weights or ASR backend.

## Timing and verification

The final release executable is built from frozen sources on Apple M5 Pro, 64 GiB, macOS 27.0 (26A428), Rust/Cargo 1.98.1. Execution is local and sequential with fresh temporary SQLite stores; no model is loaded. Timing uses one history from each scenario, three repetitions and rotated order across all four variants: **24 matched blocks, 96 executions**. Every repeat must exactly reproduce output rows, decisions, bank/scope state and fixed controls.

Final timing values are retained in the local debug report with count-weighted component means and every block's replay/process duration. Application timing includes scope selection, profile filtering and guarded text application, but excludes SQLite snapshot reads. Feedback timing covers the transaction and teaching metadata. Adaptive-plus-baseline replay timing includes setup, snapshot reads and both trajectories; total executable timing additionally includes six fixed controls and serialization/process overhead. These are not ASR, native UI or stop-to-final latencies. No peak RAM, power-mode, battery or thermal qualification is claimed.

| Mean measured duration | Original adaptive | Global bank | Strong-start global | Scoped |
| --- | ---: | ---: | ---: | ---: |
| Apply memory, per event | 0.0254 ms | 0.0252 ms | 0.0225 ms | 0.0356 ms |
| Record feedback, per event | 0.2786 ms | 0.2911 ms | 0.2968 ms | 0.2767 ms |
| Adaptive plus baseline, per history | 119.52 ms | 124.09 ms | 121.63 ms | 118.36 ms |
| Entire executable, per history | 463.15 ms | 467.18 ms | 465.15 ms | 461.51 ms |

Scoped application adds about **0.013 ms per event** versus strong-start global. Whole-history timing does not establish a speedup: scoped minus strong-start global ranges from −19.56 to +15.63 ms across matched blocks, and the earlier preserved run had the opposite mean direction. The final run's mean difference is −3.27 ms. Treat this as observed shared-host variability; qualification still needs native app latency and resource measurements.

Verification includes 28 Rust evaluator tests, 13 Python harness/report tests, strict example/test Clippy, formatting, all primary histories, 10,058 protected-text/scope checks per variant, and 32 hidden-label/model-metadata isolation reruns covering 7,620 episodes. Deliberate scoring failures in poisoned-reference runs remain visible and do not affect policies, scopes or output. Previous-executable compatibility is checked on one fresh history per scenario for both original and failure-bank modes: 16 comparisons. All **388 history/variant results** in the frozen-source rerun exactly match the initial run's outputs, decisions, scope/bank state, quality counts and fixed controls. The report records zero validation errors. No failed or unfavorable primary episode is dropped.

## Reproduce and next qualification

Use [the existing stress harness](../../scripts/stress-policy-learning.py) with `--seed-start 20000` and a new output directory per variant. It accepts `--variant current`, `failure_bank`, `strong_global` or `scoped` for both execution and isolation. The [report tool](../../scripts/compare-scoped-policy.py) verifies shared controls, retains adverse outcomes, checks frozen-source hashes and runs optional rotated timing blocks.

Freeze the workspace Rust/Cargo inputs, referenced fixtures, and hashes before building. The local snapshot and complete first/final runs are retained under `.cache/scoped-policy-benchmark/`; `frozen-source-manifest.json` identifies every snapshot file and `frozen-replay` is the measured binary. Save the prior executable as `previous-replay` for the compatibility check. A working-tree source change must not be silently attached to an already-built binary.

```sh
. ./scripts/dev-env.sh
CARGO_TARGET_DIR="$PWD/target" cargo build --release --locked \
  --manifest-path .cache/scoped-policy-benchmark/frozen-source/Cargo.toml \
  -p transcribe-data --example policy_learning_replay
python3 scripts/test-stress-policy-learning.py
python3 scripts/test-compare-policy-failure-bank.py
python3 scripts/test-compare-scoped-policy.py
python3 scripts/stress-policy-learning.py prepare /tmp/scoped/manifests --seed-start 20000
python3 scripts/stress-policy-learning.py run /tmp/scoped/manifests \
  --binary .cache/scoped-policy-benchmark/frozen-replay --variant scoped \
  --output /tmp/scoped/scoped
python3 scripts/stress-policy-learning.py verify-isolation /tmp/scoped/manifests \
  --binary .cache/scoped-policy-benchmark/frozen-replay --variant scoped \
  --output /tmp/scoped/scoped
```

Repeat execution/isolation for the other three variants. Prepare the same `public.json` and `accent.json` controls and evaluate each variant, using the prior [failure-bank report's converter instructions](2026-09-18-failure-bank-comparison.md#reproduce). The report directory also needs the frozen source snapshot/manifest, prior executable and recorded `environment.json`.

```sh
python3 scripts/compare-scoped-policy.py .cache/scoped-policy-benchmark \
  --binary .cache/scoped-policy-benchmark/frozen-replay --timing-repeats 3 \
  --output /tmp/scoped-policy-comparison.json
```

Keep this as an offline candidate. The next qualification needs real longitudinal edits/retries, paraphrased and ambiguous contexts, changed preferences, broader context diversity, and measured native resource interference. Production persistence, user-visible provenance/undo and activation still require a concrete integration design. This result justifies further evaluation; it does not silently replace the app's live behavior.
