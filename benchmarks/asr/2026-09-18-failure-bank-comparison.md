# Observed-failure replay comparison — 18 September 2026

> Historical evidence: names and commands below describe the recorded revision.
> For the current checkout, use the corresponding `vesper-*` package/executable
> and `VESPER_*` override names; legacy environment aliases remain supported.
> See [current naming and compatibility](../../docs/development.md#product-name-and-compatibility). No measurement has been requalified by renaming.

**The proposed failure-bank variant is not an improvement over our current controller on the primary held-out score.** It increases held-out word edits from **2,599 to 2,625 (+1.0%)**, despite slightly reducing errors over the complete replay and final partition. Both adaptive controllers remain substantially worse than the previously identified fixed context policy, which scores **1,464** on this fresh-seed comparison. Keep this variant isolated from live dictation.

This evaluates our application of the feedback idea discussed in the [Z.ai research assessment](../../docs/research-rsi-2026-09.md#zai-inspiration-diagnose-replay-and-verify). It is not a reproduction of Z.ai's GLM agent, infrastructure or throughput claims. All computation uses the local Rust learner and temporary SQLite stores; no model call, training or production profile change occurs. The [canonical experiment contract](../../docs/personalization/policy-experiment.md#observed-failure-replay-variant) defines the mechanism. [Issue #34](https://github.com/rahulsharmavishwakarma/transcribe/issues/34#issuecomment-5733700652) records the requested scope.

The local debug report at `.cache/benchmarks/results/2026-09-18-failure-bank-comparison/debug-summary.json` retains every manifest hash, source/binary hashes, environment, per-history comparisons, fixed controls, public-corpus checks, isolation tests and all paired timing measurements. Full episode outputs remain in the ignored local benchmark directory.

## Design and controls

The current controller proposes one-coordinate changes within six eligibility policies, tests fresh feedback and can roll back. The variant adds up to 64 previously observed development/validation failures on frozen pre-event snapshots. History may veto promotion or rollback but cannot supply fresh improvement votes. Failed comparisons remain failures; audit and final-test references never reach policy selection. The production learner and policy family are unchanged.

The eight established simulation families use **fresh seeds 10000–10011**, locked before scoring: stable usage, context shifts, support shifts, misleading feedback, sparse feedback, correlated retries, project/language changes and silence. They cover 96 histories plus the original capacity control: **97 histories, 23,809 episodes**, with **7,300 hidden audit/final episodes** and **2,692 final-only episodes**. Only the capacity manifest overlaps the previous suite by hash. The episode count coincidentally matches that earlier suite. Fresh seeds do not make an authored generator an independent real-user study.

Every history runs the adaptive trajectory, an independent starting-policy trajectory and all six independent fixed-policy trajectories. The previous release executable, rebuilt current mode and failure-bank mode each completed all 97 histories. **The original and rebuilt current modes matched every output row, original decision field, quality aggregate and fixed comparison.** Fixed comparators also matched across the two new modes. No failed or unfavorable history was removed.

The `(support=1, context=1)` fixed comparator was already identified by the earlier benchmark, before this fresh suite was scored. It is a useful stronger control, but its success on this generator does not qualify it as a production default. No thresholds or policies were retuned after inspecting these new results.

## Quality comparison

These are total word-edit distances, **not normalized WER**. Fewer is better. An output described as worse than raw has greater edit distance to the reference; that is not an independent semantic judgment.

| Partition                                      | Episodes | Raw input | Starting policy | Current adaptive | Failure bank |
| ---------------------------------------------- | -------: | --------: | --------------: | ---------------: | -----------: |
| Complete replay, including feedback collection |   23,809 |    19,554 |          19,010 |           18,110 |       18,036 |
| Hidden audits plus final                       |    7,300 |     3,311 |           3,183 |        **2,599** |        2,625 |
| Final only                                     |    2,692 |     1,895 |           1,947 |            1,745 |    **1,739** |

Compared directly with current adaptive, the variant improves **22** held-out episodes, regresses **35**, and ties **7,243**. At the history level it improves two, regresses seven and ties 88. All differences are in the support-shift family. The final-only result improves 14 episodes and regresses 11; the favorable six-edit final difference must not hide the unfavorable combined held-out result.

| Held-out scenario          | Episodes | Current adaptive | Failure bank |
| -------------------------- | -------: | ---------------: | -----------: |
| Stable                     |      912 |              244 |          244 |
| Context shift              |      912 |               38 |           38 |
| Support shift              |      912 |          **292** |          318 |
| Misleading feedback        |      912 |              382 |          382 |
| Sparse feedback            |      912 |              390 |          390 |
| Correlated retries         |      912 |              398 |          398 |
| Scope/language             |      912 |              285 |          285 |
| No feedback                |      912 |              570 |          570 |
| Capacity protected queries |        4 |                0 |            0 |

| Policy across the entire suite          | Held-out word edits | Held-out outputs worse than raw |
| --------------------------------------- | ------------------: | ------------------------------: |
| Fixed support 1, no context requirement |               3,183 |                           1,132 |
| Fixed support 1, context required       |           **1,464** |                             192 |
| Fixed support 2, no context requirement |               3,181 |                             508 |
| Fixed support 2, context required       |               2,812 |                             189 |
| Fixed support 3, no context requirement |               3,308 |                             114 |
| Fixed support 3, context required       |               3,308 |                             114 |
| Current adaptive                        |               2,599 |                             816 |
| Failure-bank adaptive                   |               2,625 |                             794 |

The variant makes 26 promotions, no rollbacks and eight rejections, compared with 32 promotions, ten rollbacks and 16 rejections for current adaptive. Retaining old errors prevents the known regression in the development fixture, but also prevents useful adaptation when support requirements change. Fewer rollbacks are therefore not a success metric by themselves. It produces 22 fewer held-out outputs worse than raw while increasing overall error, demonstrating the tradeoff between cautious application and missed useful corrections.

The suite admits 701 historical cases in total, with at most 24 retained in any history. It does not reach the 64-case bank limit; whole-component eviction is covered by targeted tests. The separate capacity control still exercises the real learner's 128-entry vocabulary, 384-candidate and 1,024-event bounds.

## Public and accent controls

These checks use **previously measured native ASR hypotheses**, with reference text simulating correction feedback on 192 calibration recordings. They do not rerun audio or represent naturally occurring longitudinal user feedback.

| Cached held-out corpus         | Recordings | Current adaptive | Failure bank | Outcome                        |
| ------------------------------ | ---------: | ---------------: | -----------: | ------------------------------ |
| Original FLEURS/Svarah control |         48 |        181 edits |    181 edits | No improvement                 |
| Svarah expanded accent screen  |        304 |        977 edits |    977 edits | Every emitted output identical |

The Svarah set covers the same 19 language-background groups documented in the [native accent report](2026-09-18-accent-personalization.md). Those are accent proxies, not detected accents or verified separate speakers. No automatic memory became active and neither controller changed policy. The bank retains 40 calibration failures, but remembering those failures creates no useful correction in these controls. This is a negative applicability result; it is not evidence that adding more history will improve acoustic recognition. Existing model/runtime, corpus revisions and native failure accounting remain in the linked accent report.

## Latency and validation

Apple M5 Pro, Mac17,8, 64 GiB, macOS 27.0 (26A428), Rust/Cargo 1.98.1; release build with locked dependencies. No model is loaded. Each run uses new temporary stores and sequential execution on a shared host.

To reduce fixed run-order effects, one history from each of the eight families was rerun **three times per variant**, alternating order by family/repeat: **24 pairs, 48 executions**. Every repeat reproduced its original quality, decisions and bank state. Component means below are weighted by invocation count across those repeated runs.

| Component                                                  | Current mean | Failure-bank mean |
| ---------------------------------------------------------- | -----------: | ----------------: |
| Filter/apply memory                                        |    0.0241 ms |         0.0241 ms |
| Feedback transaction and metadata                          |    0.2673 ms |         0.2718 ms |
| Propose policy                                             |    1.0325 ms |         1.0932 ms |
| Validate/rollback                                          |    0.3548 ms |         0.3737 ms |
| Update failure history, once per round                     |            — |         0.5649 ms |
| Adaptive plus baseline replay, per complete history        |    111.84 ms |         116.88 ms |
| Whole executable including six fixed controls, per history |    434.52 ms |         442.79 ms |

The paired adaptive-plus-baseline duration increases by a mean **5.04 ms per complete history** (median 3.45 ms; range 0.03–34.17 ms), about 4.5% of the current mean. This includes setup, SQLite snapshots, both trajectories and reporting rows, and is not a per-dictation delay. Individual application timings exclude SQLite snapshot reads; feedback timing excludes connection setup. The extra history-update mean is per experimental round, not per word or utterance. These are descriptive shared-host timings, without a statistical latency guarantee.

The local debug report also records the main 97-history timing aggregates and the largest per-history p99 values. Those tails are not pooled p99 estimates. No microphone, ASR, cleanup, UI, insertion, battery, peak-RAM or sustained thermal measurement was performed in this comparison. Background production latency is not established by this replay.

Validation completed:

- **23 Rust evaluator tests**, including failure admission, historical rollback veto, fresh-evidence requirements, whole-component eviction, and hidden-label/model-metadata isolation; strict example/test Clippy and Rust formatting passed.
- **10 Python harness/report tests**, including deterministic manifests, input tampering, failed exits/timeouts/missing artifacts, forwarding the variant flag, explicit public-corpus sizing, absent held-out feedback, weighted timing and incomplete-run reporting.
- **10,043 protected-text/scope checks per variant**, zero primary-suite failed/unscored rows, all six fixed controls retained.
- **16 poisoned-reference isolation reruns**, eight per mode, covering 3,768 episodes. Decisions, bank state and output text remained unchanged; all deliberate unscorable references remained visible as reporting failures.
- Repeated timing runs and both public controls completed without evaluation failures. The report generator initially encountered sandbox-restricted hardware queries after timing had finished; metadata collection was separated from aggregation, the saved timings were reused, and the final report has no validation errors.

## Reproduce

Preserve a copy of the existing version-2 executable before rebuilding version 3 if checking original-binary equivalence. The recorded original hash is in the local debug report; do not substitute an unrelated binary. Use new directories for each stress run. Record hardware/OS/compiler metadata in `environment.json` under the benchmark directory; missing hardware is reported as unknown rather than guessed.

```sh
. ./scripts/dev-env.sh
cargo test --locked -p transcribe-data --example policy_learning_replay
cargo clippy --locked -p transcribe-data --example policy_learning_replay --tests -- -D warnings
cargo build --release --locked -p transcribe-data --example policy_learning_replay
python3 scripts/test-stress-policy-learning.py
python3 scripts/test-compare-policy-failure-bank.py
python3 scripts/stress-policy-learning.py prepare /tmp/failure-bank/manifests --seed-start 10000
python3 scripts/stress-policy-learning.py run /tmp/failure-bank/manifests \
  --binary /tmp/failure-bank/original-replay --output /tmp/failure-bank/original
python3 scripts/stress-policy-learning.py run /tmp/failure-bank/manifests \
  --output /tmp/failure-bank/current
python3 scripts/stress-policy-learning.py run /tmp/failure-bank/manifests \
  --failure-bank --output /tmp/failure-bank/bank
python3 scripts/stress-policy-learning.py verify-isolation /tmp/failure-bank/manifests \
  --output /tmp/failure-bank/current
python3 scripts/stress-policy-learning.py verify-isolation /tmp/failure-bank/manifests \
  --failure-bank --output /tmp/failure-bank/bank
```

The public controls use the [existing converter](../../scripts/prepare-policy-learning-replay.py), defaulting to 192 calibration plus 48 held-out episodes; `--heldout-count 304` explicitly accepts the expanded accent manifest. Freeze each converted input hash before invoking the replay executable with and without `--failure-bank`. The comparison script expects `public-{current,bank}.json`, `accent-{current,bank}.json`, `accent.json` and `accent-lock.json` in the benchmark directory. The latter records `manifestSha256`. Source public manifests and reproduction steps are documented in the [initial policy report](2026-09-17-policy-learning.md) and [accent report](2026-09-18-accent-personalization.md).

```sh
python3 scripts/compare-policy-failure-bank.py /tmp/failure-bank \
  --timing-repeats 3 --output /tmp/failure-bank-comparison.json
```

Full local artifacts are retained under `.cache/failure-bank-benchmark/`. The working-tree source is untracked, so the recorded Git HEAD alone does not identify this experiment; use the source and binary hashes. No files were staged or committed.

## Decision

Do not promote this variant. Preserving previous errors is a useful constraint, but a global historical veto is too rigid for this support-shift simulation. A future hypothesis could scope negative evidence to the relevant context or explicitly model changed preferences; it would need a separately frozen comparison against the same strong fixed controls. This report does not tune that next hypothesis using these held-out answers or activate it in the app.
