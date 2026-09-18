# Local policy-learning stress evaluation — 18 September 2026

**The expanded simulation found two evaluator defects, now fixed, and confirms that adaptive policy selection is not ready for live use.** All 97 frozen histories completed: 23,809 episodes, including 7,300 held-out audit/final episodes. Adaptation reduced held-out word edits by 18.7% versus the starting policy, but a single fixed context policy performed substantially better. No production policy, model weights, model lifetime or storage schema changed.

The [canonical experiment contract](../../docs/personalization/policy-experiment.md) describes the implementation. [Issue #34](https://github.com/rahulsharmavishwakarma/transcribe/issues/34#issuecomment-5719345518) records the expanded testing scope. The local debug report at `.cache/benchmarks/results/2026-09-18-policy-learning-stress/debug-summary.json` contains per-history counts, all manifest hashes, source/binary hashes, environment, timings, controls and validation results without transcript rows. The [initial 66-episode trial](2026-09-17-policy-learning.md) is preserved as historical evidence.

## What was simulated

Eight scenario families each use 12 deterministic seeds. Each history has four learning/development/validation/audit rounds and 28 final examples. Vocabulary, context, correction availability, model metadata and event spacing vary by seed. A separate 1,084-episode capacity history brings the total to 97 histories. Each runs an adaptive trajectory, an independent starting-policy baseline and six independent fixed-policy trajectories through the real SQLite learner and guarded text matcher.

| Scenario | Episodes | Main pressure |
| --- | ---: | --- |
| Stable teaching | 2,848 | Consistent teaching, with adverse contexts in the final test |
| Context shifts | 2,843 | Alternating valid technical usage and unrelated contexts |
| Support shifts | 2,845 | One-edit technical terms, two-edit names and changed usefulness |
| Misleading feedback | 2,842 | 25% probabilistic corruption of supplied teaching/evaluation labels |
| Sparse feedback | 2,840 | About 80% of supplied feedback removed |
| Correlated retries | 2,838 | Twelve feedback events linked into two transitive components |
| Scope/language changes | 2,840 | English, Hindi/automatic-language tags and unrelated projects |
| No feedback | 2,829 | Hidden desired outputs exist, but nothing can teach the learner |
| Capacity | 1,084 | 420 distinct terms and 660 non-spelling edits, then four protected queries |

All 97 manifests were hashed before the first stress run. Every manifest still reproduces byte for byte after the evaluator fixes and added runner assertions. No seed was dropped and no target, candidate policy or quality gate was tuned to the final scores. The original lock records the earlier combined generator/runner file hash; the final file adds validation functionality while reproducing the same input bytes. This suite is **development stress evidence**, not an untouched deployment qualification set.

These are authored text and explicit correction/review targets. An unchanged output with a target represents externally supplied review feedback; silence is not treated as acceptance. The histories are not actual users, audio recordings, accent samples or realistic estimates of how often users provide feedback. Model IDs only test memory portability. Timestamps enforce order but do not advance SQLite's real retention clock; the separate storage unit test directly exercises 30-day pruning.

## Quality, including unfavorable results

Scores are total `transcribe_core::word_edits`, not normalized WER. “Worse than raw” means a larger word-edit distance to the hidden reference, not an independent human judgment of semantic harm.

| Partition | Episodes | Raw | Starting policy | Adaptive |
| --- | ---: | ---: | ---: | ---: |
| All, including feedback collection | 23,809 | 19,605 | 19,022 | 18,143 |
| Hidden audits plus final test | 7,300 | 3,341 | 3,171 | 2,577 |
| Final test only | 2,692 | 1,859 | 1,904 | 1,696 |

The adaptive trajectory improved 362 hidden episodes and regressed 21 versus the starting policy. It produced 788 hidden outputs worse than raw input, versus 1,114 for the starting policy. Across the complete run, there were 598 improved and 80 regressed episodes. No execution failures or unscored episodes were excluded from these denominators.

| Hidden result by scenario | Starting policy | Adaptive | Best fixed per history, summed |
| --- | ---: | ---: | ---: |
| Stable teaching | 226 | 226 | 0 |
| Context shifts | 465 | 39 | 0 |
| Support shifts | 447 | 279 | 226 |
| Misleading feedback | 417 | 417 | 190 |
| Sparse feedback | 420 | 420 | 371 |
| Correlated retries | 384 | 384 | 127 |
| Scope/language changes | 250 | 250 | 17 |
| No feedback | 562 | 562 | 562 |
| Capacity protected queries | 0 | 0 | 0 |

The last column is a post-hoc oracle choosing a fixed policy separately for each history; its total is 1,493 word edits and 116 outputs worse than raw. It is not an available online strategy. A fairer constrained comparison uses **one fixed policy across all 97 histories**:

| Fixed support floor, context requirement | Hidden word edits | Outputs worse than raw |
| --- | ---: | ---: |
| 1, none — starting policy | 3,171 | 1,114 |
| 1, required | **1,549** | 201 |
| 2, none | 3,201 | 493 |
| 2, required | 2,858 | 179 |
| 3, none | 3,334 | 119 |
| 3, required | 3,334 | 119 |
| Adaptive | 2,577 | 788 |

Even the best single fixed policy beats the adaptive controller here. That winner is selected after scoring and is not a qualified production default. Synthetic context separation favors lexical filtering; this result cannot establish its benefit on natural dictation.

The controller made 33 promotions, 12 rollbacks and 11 rejections; it retained the policy without a proposal in 329 rounds. Gains were concentrated in context/support shifts. Misleading feedback caused six fresh-validation rejections but did not reliably correct bad memories. Sparse feedback did not supply enough useful evidence for policy improvement. Correlated-only feedback and silence produced no policy updates. The no-feedback trajectories learned zero memories. Quality limitations remain visible rather than being treated as passing tests merely because the process ran.

## Reproduced defects and correctness checks

1. **Later correlated harm was invisible.** First-only component scoring missed a later regression/overflow and could approve different outcomes after reordering the same feedback. Two new tests failed before the fix. Version 2 evaluates every member, gives the component one worst-case aggregate and one independent improvement vote, and lets any regression or failure veto a change. Duplicate volume cannot add evidence weight.
2. **Disabled evidence eviction caused unnecessary errors.** A test with a disabled entry and evicted non-active candidate failed on provenance lookup. Disabled entries now bypass evaluation; enabled entries still fail if their required provenance is missing. This does not reactivate suspended memory.

The final stress rerun retained the first run's quality counts, decisions and fixed-policy comparisons exactly. It passed **10,044 protected-text and scope checks** against both adaptive and baseline outputs. The capacity run reached and respected 128 vocabulary entries, 384 retained candidates and 1,024 events; the remaining candidates and contexts were bounded. Capacity queries only test protected-text behavior, not broad quality at saturation.

Eight additional isolation reruns cover 1,887 episodes. Every hidden reference was replaced with a valid-length but unscorable 513-word target, model IDs changed, and source metadata supplied false expected decisions. Outputs, active policies and decisions stayed identical. All 1,887 deliberate hidden-scoring failures remained reported as unscored; they neither contaminated feedback nor disappeared from reporting. These deliberate failures are separate from the primary 23,809-episode quality suite.

**76 Rust tests passed:** 30 core, 27 data and 19 experiment tests; one existing optional public-data test remained ignored. The experiment includes 512 seeded source/repair graphs checked against an independent graph-closure oracle, threshold boundaries, all six policy guard combinations, independent stores/forgetting, injected failures at every duplicate position, and manifest limits. **Five Python harness tests passed**, covering reproducibility, no hidden feedback, tamper rejection and retention of failed exits, timeouts and missing result artifacts. Strict data-target Clippy and formatting of the changed Rust example passed.

The original 66-episode and cached 240-episode public manifests were rerun with the final binary. The original promotion/promotion/rollback sequence and 12 → 4 hidden word-edit result remain. Public held-out quality remains 181 → 181, with no policy updates. That public control reuses prior FLEURS/Svarah hypotheses; it is not a new acoustic run. Native inference and frontend paths were unchanged by this task and were not retested here.

## Local latency

Apple M5 Pro (Mac17,8), 64 GiB, macOS 27.0 (26A428), Rust/Cargo 1.98.1, release build with locked dependencies. Histories run sequentially on a shared host, with new temporary stores and open SQLite connections. No model is loaded. The following means are weighted by invocation count across the 96 ordinary histories; capacity is reported separately.

| Component | Invocations | Mean | Largest per-history p99 |
| --- | ---: | ---: | ---: |
| Filter/apply memory | 22,725 | 0.0290 ms | 0.2231 ms |
| Feedback transaction and metadata | 11,980 | 0.3192 ms | 3.7270 ms |
| Propose from bounded neighbors | 384 | 1.2160 ms | 4.7513 ms |
| Validate/rollback decision | 384 | 0.2826 ms | 3.9607 ms |

The tail column is the largest p99 of any single history, **not a pooled p99**. A history has only four proposal/gate timings, so those tail values are descriptive maxima. The capacity history averaged 0.2818 ms per application and 1.3498 ms per feedback transaction, with p99 values 0.4268 ms and 2.4351 ms respectively. Its empty proposal/gate stages are not meaningful load measurements.

Application timings exclude SQLite snapshot reads; feedback timings exclude connection setup. Proposal/gate timing covers in-memory comparisons. Setup, seven comparator trajectories, report writing, ASR, cleanup, UI scheduling and insertion are excluded. The 97 full executable runs took approximately 67.1 seconds in total, including those replay/setup/report costs. No end-to-end Stop-to-final, peak RAM, power mode, battery or thermal qualification is claimed.

## Reproduce

```sh
. ./scripts/dev-env.sh
cargo test --locked -p transcribe-core -p transcribe-data --all-targets
cargo clippy --locked -p transcribe-data --all-targets -- -D warnings
python3 scripts/test-stress-policy-learning.py
cargo build --release --locked -p transcribe-data --example policy_learning_replay
python3 scripts/stress-policy-learning.py prepare /tmp/policy-stress-manifests
python3 scripts/stress-policy-learning.py run /tmp/policy-stress-manifests \
  --output /tmp/policy-stress-results
python3 scripts/stress-policy-learning.py verify-isolation /tmp/policy-stress-manifests \
  --output /tmp/policy-stress-results
```

Use new manifest/results directories. The preparation step creates a lock before scoring, and the runner refuses changed inputs. Failed processes, timeouts and missing reports remain in the attempted-history aggregate. Full local outputs and pre-fix failure logs are retained in `.cache/policy-learning-stress/`; the local debug report records their relevant hashes and aggregate evidence.

The next qualification step needs representative independently labelled feedback, coverage of earlier adverse cases, and a fresh locked comparison against strong fixed policies. This simulation does not establish unrestricted recursive improvement or justify enabling the offline controller in live dictation.
