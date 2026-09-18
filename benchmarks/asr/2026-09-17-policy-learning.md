# Bounded recursive-policy trial — 17 September 2026

**The mechanism works, but the trial does not qualify it for live deployment.** The controller made two eligibility-policy changes and a later rollback. Intermediate held-out audits improved versus the starting policy, but the complete constructed run and final test tied that baseline. The adaptive controller also tied the best fixed policies on held-out word edits while making more harmful changes. The separate public replay produced no policy update or quality gain.

The [implementation contract](../../docs/personalization/policy-experiment.md) explains the finite policy family, feedback boundaries and rollback. [Issue #34](https://github.com/rahulsharmavishwakarma/transcribe/issues/34#issuecomment-5717283286) records authorization for this isolated prototype. The [result JSON](results/2026-09-17-policy-learning.json) contains exact comparisons, distributions, failures, source/binary/manifest hashes and public artifact provenance. No live application policy, model, storage schema or default was changed.

## Locked protocol

The controller starts with the existing automatic-memory eligibility policy. It can add a minimum support floor of 1–3 independent corrections and require 0–1 shared teaching-context terms. One coordinate changes per proposal. A proposal must reduce development errors and then pass fresh validation with at least four independent feedback components, two improved cases, zero per-case regressions and no increase in outputs worse than raw input. A saved parent policy can pass the same fresh-feedback gate to trigger rollback.

Each round uses learn → develop → validate → hidden audit, followed by a final hidden test. Candidate selection is fixed before validation arrives. The controller only receives externally supplied feedback targets and pre-event snapshots; hidden references and expected-decision metadata are absent from its API. Audit/test feedback is forbidden. Six fixed policies run as separate trajectories afterward and cannot influence the controller.

The constructed generator, manifest and public conversion were frozen before the first behavior run. Policy family and gates were not retuned after seeing hidden results. A subsequent transitive-correlation bug fix did not change these manifests or the unique-source replay decisions. Gates are engineering checks, not statistical proof or production thresholds.

## Constructed sequence

The authored fixture contains **66 episodes**: 46 supplied feedback targets, 12 hidden intermediate-audit episodes and eight final-test episodes. Its explicit labels on unchanged outputs simulate stronger feedback than passive usage supplies.

| Round | Decision | Resulting policy |
| --- | --- | --- |
| Context | Promote | Support floor 1; require a shared teaching-context term |
| Independent support | Promote | Support floor 2; keep the context requirement |
| Distribution change | Roll back | Restore support floor 1 with the context requirement |

Each accepted decision improved two of four fresh validation cases without a validation regression. Those cases did not represent every later condition: the rollback re-exposed two harmful final cases. They remain in the aggregate and were not used to redesign the fixture or controller.

| Evaluation slice | Episodes | Starting fixed policy word edits | Adaptive word edits |
| --- | ---: | ---: | ---: |
| Entire chronological sequence | 66 | 68 | 68 |
| Intermediate hidden audits | 12 | 8 | 0 |
| Final hidden test | 8 | 4 | 4 |
| All hidden episodes | 20 | 12 | 4 |

Across all hidden episodes, four outputs improved and none worsened relative to the starting fixed-policy trajectory. Nevertheless, two adaptive outputs were worse than raw input. On the complete sequence, four earlier improvements were offset by four regressions, so the hidden-audit gain is not a demonstrated reduction in overall correction burden.

| Fixed comparison policy | Hidden word edits | Outputs worse than raw input |
| --- | ---: | ---: |
| Support 1, no extra context | 12 | 6 |
| Support 1, context required | 8 | 4 |
| Support 2, with or without context | 4 | 0 |
| Support 3, with or without context | 4 | 0 |
| Adaptive controller | 4 | 2 |

The best fixed alternatives tied the adaptive error total and avoided its harmful substitutions. Their best status is a post-hoc comparison, not an independently selected production default. This trial establishes finite policy updating and rejection/rollback mechanics, not superiority over fixed policies or open-ended recursive algorithm improvement.

## Public negative/coverage control

The second manifest reuses the prior **192 calibration + 48 held-out** Nemotron Q8 hypotheses from FLEURS English/Hindi and Svarah Indian English. Each 64-recording calibration block becomes 48 learning, eight development and eight validation episodes. Final references are never feedback. Source/audio/episode identities cannot cross partitions.

The replay used all 240 episodes without execution failure. It activated **zero automatic memories**, made **zero policy changes**, and retained **181 → 181 word edits** on the 48 held-out recordings. All six fixed policies also scored 181. The narrow underlying spelling learner's coverage remains a limiting factor; selecting among stricter eligibility policies cannot recover a spelling it never learns.

The cached corpus contains independent reference text, not natural user corrections or retry histories. Chronology and feedback are simulated. No new ASR inference was performed. The [earlier corpus protocols](2026-09-16-automatic-learning.md#reproduce) and JSON retain [FLEURS](https://huggingface.co/datasets/google/fleurs)/FLoRes and [Svarah](https://huggingface.co/datasets/ai4bharat/Svarah) attribution, source revisions and native model/runtime artifacts.

Scores use `transcribe_core::word_edits`: lowercase whitespace-token Levenshtein distance with edge punctuation trimmed. They are not `transcribe-eval` normalized WER. “Harmful” here means more word edits than raw input, not a separately human-scored semantic-harm measure. Language subtotals, attempted/scored denominators and failures are preserved in the JSON.

## Local component timings

Apple M5 Pro, 64 GiB RAM, **macOS 27.0 (26A428)**, Rust 1.98.1, release build with the recorded lockfile. This differs from the earlier macOS 26.6.2 envelope. The sequential CPU replay loads no model and uses temporary stores with warm open SQLite connections.

| Constructed component | Count | Mean | p50 | p95 |
| --- | ---: | ---: | ---: | ---: |
| Filter profile and apply memory | 66 | 0.0196 ms | 0.0193 ms | 0.0270 ms |
| Feedback transaction and metadata | 46 | 0.2461 ms | 0.2378 ms | 0.3127 ms |
| Propose from bounded neighbors | 3 | 0.3654 ms | 0.3153 ms | 0.4811 ms |
| Validate/rollback policy decision | 3 | 0.2951 ms | 0.2411 ms | 0.4501 ms |

The JSON also records p99 and all public timings. With three decisions, tail percentiles are merely descriptive extrema. Public proposal mean was 0.4826 ms, but it had no eligible policy change; its near-empty validation decision is not a useful load benchmark.

Application timing excludes SQLite snapshot reads. Feedback includes the existing learner transaction and transient teaching-context update, but not opening the connection. Proposal/gate timing covers in-memory comparison. Setup, fixed-policy comparison runs, report generation, IPC, ASR, cleanup and insertion are excluded. No peak-memory, thermal, battery, scheduling-interference or end-to-end latency result is claimed.

## Verification and reproduced defect

**29 core tests, 27 data tests and 11 experiment tests passed**; one existing optional public-data test was left ignored. Strict Clippy passed for all `transcribe-data` targets, including the example, and Rust formatting passed. Both locked manifests were reproduced by the preparer, and an invalid public split was rejected. No frontend or native inference behavior changed, so this trial did not rerun or claim those tests.

A new test reproduced an independence-counting error: A shared an audio source with B, while B shared a repair chain with C; the initial evaluator counted A and C as independent. Connected-component grouping now counts the chain once. The experimental eligibility floor also includes retained duplicate bridge events. The failing test log is preserved alongside the passing final tests. Hidden-label and expected-metadata mutation tests verify that reporting targets cannot change policy decisions.

Other tests cover fresh-validation rejection, insufficient feedback, injected learning failure, score overflow, forbidden policy jumps, duplicate support, undo/no-reactivation, source leakage, time order, quotes and language/project isolation. Failures and unscorable cases block policy updates. Transitive grouping is experimental here; production behavior was not silently changed.

## Reproduction

```sh
. ./scripts/dev-env.sh
cargo test --locked -p transcribe-core -p transcribe-data --all-targets
cargo run --release --locked -p transcribe-data --example policy_learning_replay -- \
  benchmarks/asr/fixtures/policy-learning-episodes.json /tmp/policy-constructed-results.json
python3 scripts/prepare-policy-learning-replay.py public /tmp/policy-public.json \
  --public-input .cache/automatic-learning-replay/public.json
target/release/examples/policy_learning_replay \
  /tmp/policy-public.json /tmp/policy-public-results.json
```

The constructed manifest is reproducible with `python3 scripts/prepare-policy-learning-replay.py constructed /tmp/policy-constructed.json`. Full results and logs are in `.cache/policy-learning-replay/`; the portable summary contains hashes and aggregate evidence without copying transcript rows.

Future work should test validation coverage, retention of earlier failure cases and feedback availability on new development data with a fresh locked comparison. It must demonstrate an advantage over strong fixed policies and lower real editing burden before proposing live integration. This trial provides no reason to enable automatic policy updates in the app.
