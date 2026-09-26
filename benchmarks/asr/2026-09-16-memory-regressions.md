# Expanded local-memory testing and fixes — 16 September 2026

> Historical evidence: names and commands below describe the recorded revision.
> For the current checkout, use the corresponding `vesper-*` package/executable
> and `VESPER_*` override names; legacy environment aliases remain supported.
> See [current naming and compatibility](../../docs/development.md#product-name-and-compatibility). No measurement has been requalified by renaming.


This follow-up reuses cached public recordings and adds controlled simulations for the learning features. **778 deterministic cases now pass, 72 fresh native ASR executions completed without failures, and eight native cleanup calls selected and isolated the expected memories.** Six real-corpus storage workflows, browser error recovery, and native cancellation/profile reset also passed. The [earlier 444-execution screen](2026-09-16-personalization.md) is preserved.

Work and outstanding qualification remain in [#34](https://github.com/rahulsharmavishwakarma/transcribe/issues/34); model/runtime limitations remain under [#33](https://github.com/rahulsharmavishwakarma/transcribe/issues/33). The local debug report at `.cache/benchmarks/results/2026-09-16-memory-regressions/debug-summary.json` records before/after failures, examples, native scores, hashes, artifacts, execution metadata, resource measurements and validation. No private audio, user database, cloud inference, training or model-default change was involved.

## Reproduced failures and fixes

| Finding | Reproduction | Focused fix |
| --- | --- | --- |
| Quoted text could be corrected | An internal apostrophe in `'don't change UN'` closed the quote early; an escaped double quote could also expose an alias. | Distinguish word-internal apostrophes and escaped delimiters from quote boundaries. Test both protected text and an eligible alias after the closing quote. |
| Negation could become a replacement | Curly-apostrophe `don’t → do` and additional negative contractions such as `won't → will` produced suggestions. | Normalize apostrophes for protection and reject negative `n't` forms, including application of unsafe manually stored aliases. |
| Some suggested memories were unusable or formatting-only | Extraction offered multiword spans crossing punctuation, while application correctly abstained; `UN → U.N.` was mined as a recognition confusion. | Share the boundary check between extraction/application; suppress initialism punctuation-only suggestions. Explicitly entered formatting preferences remain explicit. |
| Dataset parsing misread literal quotes | FLEURS en-US has 647 valid seven-field TSV rows; normal CSV quote handling produced two malformed six-field rows. | Treat quotes literally in both Python preparers. Auditing prior selected calibration/holdout references found **zero affected references**. |

The same final 778 golden case definitions failed 155 times with the pre-fix executable and zero times afterward: 49 straight-quote contraction cases, 49 curly-quote contraction cases, 49 escaped-quote cases, two cross-punctuation suggestions and six negation/initialism suggestions. These are repeated probes of the bugs above, not 155 independent product defects. Four expectations in the initial draft incorrectly demanded cross-punctuation substitutions; the final oracle correctly expects abstention. Both the draft and corrected baseline remain in the ignored local artifacts.

Fixtures cover positive aliases, unrelated text, wrong/right project, wrong context/language, pause/disable, conflicting spellings, quotes/escapes, negation, and intent opt-in/agreement/conflict/duplicate-session abstention. Their text wrappers and intent labels are **authored simulations**, not original corpus annotations or a measured accent classifier.

## Fresh native audio

Selected **48 previously unused FLEURS recordings, 496.48 seconds (8.27 minutes)** from the cached, hash-verified public archives: 24 US English and 24 Hindi. Selection happened before new predictions. Each locale contains eight prompts mentioning a calibration vocabulary term plus 16 selected in fixed hash order. Prior selected normalized prompts were excluded. This deliberately tests recurring vocabulary; it is not a representative random sample or directly comparable to the earlier screen's accuracy.

The corrected extractor rebuilt and froze **45 entries** from the original separate calibration predictions/references. The six removed entries were one initialism and five source/target spans crossing punctuation. Native phrase boosting and model cleanup were off. Raw and personalized scores use the identical ASR output.

| Recognizer / locale | Scored / attempted | Raw WER | Memory WER | Changed / worsened |
| --- | ---: | ---: | ---: | ---: |
| Nemotron Q8 / US English | 24/24 | 85/597 = 14.24% | 84/597 = 14.07% | 1 / 0 |
| Nemotron Q8 / Hindi | 24/24 | 81/554 = 14.62% | 76/554 = 13.72% | 5 / 0 |
| Cohere int8 / US English | 24/24 | 69/597 = 11.56% | 68/597 = 11.39% | 1 / 0 |

Both recognizers produced the same English name confusion, `Martelli's → Martelly's`, which the same application memory corrected. The Hindi changes were five `ज्यादा → ज़्यादा` spelling substitutions. These results demonstrate a narrow recurring-name/spelling benefit without changing weights; they do not establish personal longitudinal learning or acoustic adaptation.

Completed-file ASR mean was 538 ms for Nemotron English, 496 ms for Nemotron Hindi and 7,542 ms for Cohere English. Memory alone averaged 0.045, 0.075 and 0.054 ms respectively. Cohere retains its existing per-recording loading behavior; these timings are not microphone stop latency. Process peak RSS and elapsed time are recorded separately in the JSON. No new normal-speed latency benchmark replaces the prior 12-clip replay measurement.

## Native cleanup and storage simulations

Two actual public calibration predictions each ran through four controlled Qwen conditions: no memory, matching memory, wrong project and paused memory. Their repeated examples and technical output-form labels are explicitly simulated; this is **not held-out cleanup/intent accuracy**.

All eight calls completed. Matching conditions retrieved the expected vocabulary entry and both examples and selected the technical output form. Wrong-project, paused and empty conditions retrieved none and returned to verbatim. Default Minimal cleanup preserved the original wording in all eight; the separate deterministic memory stage then applied the expected alias only in the two matching conditions. This verifies routing, retrieval and application isolation, not an improvement in writing style.

Six isolated `DatasetStore` workflows used actual cached audio, predictions and human corpus references, two each from US English, Indian English and Hindi. They checked: no learning before explicit review; separate confirmation and pronunciation flag; preserved raw/reviewed/personalized text; undo; exclusion of source recordings from improvement replay; optional output-form examples; reopening the store; pause/removal; and stale-save rejection. Temporal held-out replay, duplicate exclusion, bounded review selection and regression retention also remain covered by the required existing unit tests.

The browser fixture exercised confirmation, pronunciation flag, undo, pause/save, review-queue navigation, visible source counts, and recovery from an injected save failure. The error was visible, unsaved state remained available, and Discard restored the saved profile. A screenshot was inspected. The fixture mocks native commands and is separate from real Rust storage/inference evidence; it does not validate native IPC, microphone permissions or cross-application insertion.

## Verification and reproduction

`make fmt && make check` passed: **178 Rust tests, 68 Node tests**, native completion-report fixtures, strict Clippy, frontend build, and zero Svelte/TypeScript errors or warnings. Three additional explicitly invoked tests passed: the six public-data workflows; Nemotron resident streaming/profile switch/cancel followed by fresh completion; and Qwen fresh-versus-cached output equivalence, cancellation and profile reset. The default command leaves 15 optional tests ignored; these three were run separately. The Nemotron fixture now explicitly requests CPU.

To reproduce the portable text regressions without downloading data:

```sh
python3 scripts/prepare-memory-regressions.py \
  benchmarks/asr/fixtures/memory-corpus-seed.json /tmp/memory-cases.json
. ./scripts/dev-env.sh
cargo run --locked -p transcribe-core --example memory_replay -- \
  /tmp/memory-cases.json /tmp/memory-results.json
```

The [public-corpus seed](fixtures/memory-corpus-seed.json) reproduces all 778 case definitions exactly and retains attribution. It intentionally includes historical unusable suggestions for rejection tests; it is not a user profile. The [original native protocol](personalization.md) prepares calibration and cached archives. New audio selection is reproducible with:

```sh
python3 scripts/prepare-memory-holdout.py \
  .cache/benchmarks/personalization-2026-09-16 \
  .cache/benchmarks/personalization-regressions-2026-09-16/fresh
```

The existing `transcribe-eval build-memory` and `run --profile` commands then evaluate `fresh/holdout.json` with Nemotron and `fresh/en_us.json` with Cohere. The `memory_cleanup` example consumes the [portable native fixtures](fixtures/memory-cleanup-cases.json) and validates vocabulary/example/intent evidence:

```sh
cargo run --release --locked -p transcribe-inference --example memory_cleanup -- \
  models/downloads benchmarks/asr/fixtures/memory-cleanup-cases.json \
  /tmp/memory-cleanup-results.json
```

Full inputs, outputs, environment files, source/binary hashes and logs are retained under `.cache/benchmarks/personalization-regressions-2026-09-16/`.

Hardware and unchanged pins match the [earlier CPU envelope](2026-09-16-personalization.md#reproducibility-and-evidence-boundary): Apple M5 Pro, 64 GiB, macOS 26.6.2, explicit CPU, AC power mode 0. The JSON includes exact native model/runtime artifacts and reverified Qwen SHA-256. Measured source stamp: `0b93e43264bb85963118424fd0e525a1c5ac3047973e9d71fa4fef8eb1e8eb83`. Optional test CPU selection and extra cleanup assertions were added after the timed release binaries; their inference behavior did not change. NeMo's actual internal thread count and continuous thermal behavior remain unmeasured.

All reported outcomes are **local measurements or labelled simulations**. [FLEURS](https://huggingface.co/datasets/google/fleurs) and the seed's [Svarah](https://huggingface.co/datasets/ai4bharat/Svarah) attribution are official dataset facts (CC-BY-4.0); paper claims remain in the separate research review. Runtime-internal decoder work and training/checkpoint changes were not implemented. The earlier Cohere `duration_token_limit` failure is still recorded under #33 and is not claimed fixed by these memory changes. Live microphone/noise/device testing, real user editing effort and reviewed intent/style accuracy remain outstanding. Changes are unstaged and uncommitted.
