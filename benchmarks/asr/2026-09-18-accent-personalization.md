# Real-audio accent and dictionary evaluation — 18 September 2026

> Historical evidence: names and commands below describe the recorded revision.
> For the current checkout, use the corresponding `vesper-*` package/executable
> and `VESPER_*` override names; legacy environment aliases remain supported.
> See [current naming and compatibility](../../docs/development.md#product-name-and-compatibility). No measurement has been requalified by renaming.

**Both local recognizers ran all 304 selected Svarah recordings without execution failure.** On identical audio, Cohere INT8 scored 9.54% normalized WER and Nemotron Q8 scored 20.78%. The frozen corpus-reference dictionary slightly worsened both results by applying one incorrect pluralization. These are fresh native audio measurements, separate from the [text/feedback stress simulation](2026-09-18-policy-learning-stress.md).

The local debug report at `.cache/benchmarks/results/2026-09-18-accent-personalization/debug-summary.json` retains each cohort, score denominators, failures, sample/audio hashes, model/runtime artifacts, actual execution settings and timing distributions without exporting transcript rows. [Issue #34](https://github.com/rahulsharmavishwakarma/transcribe/issues/34#issuecomment-5719577820) records the authorized testing scope. No model/default, runtime ownership, live user profile or training behavior changed.

## Corpus and grouping

The three existing Svarah Parquet shards were verified against their pinned hashes at dataset revision `ebbf7777fe771490696a3f7b007097606fa8c924`. Selection used a fixed SHA-256 ordering, 4–20-second recordings and one clip per source-recording filename group. All earlier personalization calibration/evaluation recording groups and normalized prompts were excluded. Selection and audio hashes were frozen before inference; no model score was used to choose recordings.

There are **16 recordings in each of 19 primary-language groups**, totaling 304 clips and **39.02 minutes**. The model prompt is English for every group. Cohorts are interleaved in the run order to avoid always running one language background under later process conditions. A separate 32-recording FLEURS US-English control repeats an established cached holdout with fresh inference. It is not a matched-content causal comparison of US and Indian accents.

The [official repository](https://github.com/AI4Bharat/Svarah#tutorial) defines `primary_language` as the speaker's language background. The [Svarah paper](https://www.isca-archive.org/interspeech_2023/javed23_interspeech.pdf) uses native-language groups for its accent analysis. We follow that grouping convention: these are **accent proxies**, not acoustic accent classifications. The cached schema has no verified speaker ID, so filename groups do not establish a speaker-disjoint or longitudinal user split. The paper is from 2023 and is cited for benchmark provenance, not as recent-two-year adaptation research.

Svarah and FLEURS are attributed under CC BY 4.0. Cached WAV bytes are copied unchanged; the application performs its normal decoding/resampling. This subset does not cover global English accents, noisy-room robustness or microphone devices. Neither a primary-language label nor successful transcription demonstrates that the application detects or learns an accent.

## Accuracy by language background

WER is pooled word edits divided by reference words using the existing `transcribe-eval` normalization. It is not the `word_edits` helper used in the policy simulation. There are 4,667 reference words and no failed/unscored Svarah samples for either recognizer.

| Speaker primary language      |   Clips | Reference words |           Nemotron WER |            Cohere WER |
| ----------------------------- | ------: | --------------: | ---------------------: | --------------------: |
| Assamese                      |      16 |             246 |                 27.24% |                11.38% |
| Bengali                       |      16 |             249 |                 27.31% |                 9.64% |
| Bodo                          |      16 |             275 |                 30.91% |                12.36% |
| Dogri                         |      16 |             298 |                 15.10% |                 8.05% |
| Gujarati                      |      16 |             222 |                 16.22% |                 7.21% |
| Hindi                         |      16 |             241 |                 14.11% |                 9.96% |
| Kannada                       |      16 |             191 |                 19.37% |                 7.85% |
| Kashmiri                      |      16 |             268 |                 23.88% |                 8.58% |
| Konkani                       |      16 |             245 |                 25.31% |                15.92% |
| Maithili                      |      16 |             220 |                 22.73% |                 7.73% |
| Malayalam                     |      16 |             267 |                 14.61% |                 7.12% |
| Marathi                       |      16 |             252 |                 21.83% |                 7.94% |
| Nepali                        |      16 |             187 |                 27.81% |                10.16% |
| Odia                          |      16 |             272 |                 13.24% |                 2.57% |
| Punjabi                       |      16 |             275 |                 15.27% |                 8.36% |
| Sindhi                        |      16 |             256 |                 16.41% |                11.72% |
| Tamil                         |      16 |             255 |                 17.25% |                 5.49% |
| Telugu                        |      16 |             230 |                 17.83% |                11.30% |
| Urdu                          |      16 |             218 |                 32.57% |                19.72% |
| **All selected Svarah clips** | **304** |       **4,667** | **970 edits / 20.78%** | **445 edits / 9.54%** |

Sixteen clips per background are a small descriptive screen. The utterances, vocabulary and number formats differ by group; this table does not isolate accent as the cause of error differences or rank population-level accent difficulty. Exact per-group grapheme error rates and timing distributions are in the JSON.

Text/reference spot checks of large errors found both numeric-format mismatches and missing digits, plus omissions/additions in conversational text. For example, spelling out a correct number can incur several word edits against a numeric reference, while a missing digit is a real content error. The report retains strict WER and a post-hoc digit-containing/reference-without-digits diagnostic; it does not normalize away those cases or claim a verified semantic number score.

Sixty-nine references contain digits: their strict WER is 55.14% for Nemotron and 23.35% for Cohere. The remaining 235 recordings score 11.75% and 5.90%. This localizes errors without explaining all of them or changing the primary score.

## Dictionary behavior and known failure

The paired dictionary contains 51 entries frozen from the earlier separate corpus calibration. It represents **simulated explicit review**, not automatic learned accent behavior. Native phrase boosting and cleanup are disabled, so raw and dictionary scores share the same ASR output. The same profile is used with both recognizers and passes calibration/holdout overlap checks.

On one Hindi-background English recording, the dictionary changed correct singular `cost` to `costs`. Both models already transcribed the reference correctly. Nemotron's score therefore changed from 970 to 971 edits (20.78% → 20.81%); Cohere changed from 445 to 446 (9.54% → 9.56%). Neither model received a beneficial dictionary change on this set. The frozen profile and unfavorable case were preserved; no entry was edited using these held-out answers. This is a failure of the corpus-review memory's generalization, not evidence that acoustic accent adaptation worked.

The FLEURS US-English control retains the previously observed Cohere `duration_token_limit` failure on `en_us-test-15199695878621076767`. The incomplete transcript was rejected and remains in the attempted denominator. That known decoder qualification issue is separate from these all-successful Svarah runs; no decoder budget was changed to conceal it.

Nemotron scored all 32 US-control clips at 121/738 word edits (16.40%); dictionary output was 123/738 (16.67%). Cohere scored 31/32 at 61/714 (8.54%), with dictionary output 63/714 (8.82%). Cohere's failed clip and its 24 unscored reference words remain explicit; these control rows do not have identical complete-case coverage. The dictionary's US initialism regression is the same one recorded in the earlier screen.

## Does automatic learning help these accent recordings?

A separate actual-store replay combines the earlier 192 calibration outputs/reference edits with the **304 freshly decoded Nemotron Svarah outputs**. References for these 304 recordings remain scoring-only; they never become feedback. This is a 496-episode simulated ordinary-edit experiment, not 496 additional acoustic executions. The prior calibration and new held-out ASR envelopes remain identified separately by their report hashes.

| Replay mode           | Stored vocabulary | Held-out core word edits | Improved / worsened recordings |
| --------------------- | ----------------: | -----------------------: | -----------------------------: |
| Off                   |                 0 |                      977 |                          0 / 0 |
| Manual oracle         |                50 |                      978 |                          0 / 1 |
| Automatic edits       |                 0 |                      977 |                          0 / 0 |
| Automatic usage/retry |                 0 |                      977 |                          0 / 0 |

Both automatic modes abstained from activating entries: 14 candidates remained in the edit-only mode and 18 in usage/retry mode. No replay/store failures occurred. Every primary-language group had unchanged automatic output. These core word-edit counts use a different tokenizer from native normalized WER and must not be mixed with the 970 native word errors above.

This set shows **no automatic accent-related gain**. Cross-speaker corpus references provide little repeated personal vocabulary evidence, and the current learner changes guarded text memory rather than acoustic weights. A future longitudinal test needs repeated real corrections and later independent uses from consenting, reliably identified speakers. The observed manual pluralization error must remain a negative case when evaluating broader automatic rules; simply admitting more aliases would not establish improvement.

## Normal-speed prerecorded streaming

Nemotron received 20 ms packets at their wall-clock deadlines for **38 recordings, two per language-background group**, totaling 311.50 seconds of speech. All completed without error. These are prerecorded audio replays through the application's existing streaming adapter, not microphone or desktop end-to-end tests.

| ASR metric, 38 recordings       |     Mean |   Median |      p95 |  Maximum |
| ------------------------------- | -------: | -------: | -------: | -------: |
| First nonempty partial          | 1,306 ms | 1,002 ms | 2,920 ms | 3,235 ms |
| Last audio packet to final text |  60.6 ms |  59.2 ms |  87.8 ms |  90.3 ms |

First-partial latency includes applicable cold setup. Stop-to-final begins after the last paced audio packet and excludes subsequent dictionary work, writing, UI and insertion. The approximately 1.01 paced real-time factor includes intentionally waiting for audio delivery and is not a decoder-throughput result. Two examples per cohort do not establish a reliable per-accent latency percentile.

On these same 38 recordings, completed-file Nemotron scored 128/602 word edits (21.26%); paced streaming scored 120/602 (19.93%). Thirty raw strings differed, including formatting; this small delivery-path comparison is not a general streaming quality claim. Dictionary output did not change any streaming result. The report retains 13,229 callbacks and cumulative partial word-edit distance 720. Callbacks can repeat unchanged text, and cumulative distance includes newly added words; neither count is a direct word-emission or revision-only stability metric. Endpoint/word-emission timestamps, queue drops and sustained thermal behavior were not instrumented by this existing harness.

## Completed-file latency and environment

Apple M5 Pro (Mac17,8), 18 logical CPUs, 64 GiB, macOS 27.0 (26A428), Rust 1.98.1 release build, explicit CPU. The evaluator executable was frozen before timing; its SHA-256 is `586b4d14819b36eed2bc710ef593b9464f654e35aefafc1c52bd67403343818e`. Fifty-five application source/build files are hashed separately because Git HEAD alone does not identify the unstaged implementation.

Actual execution evidence reports **four native graph threads for Nemotron and eight for Cohere**. This supersedes the older generic `environment.threads = 8` worker-cap annotation and its note about unavailable NeMo thread information. Per-process supply snapshots report battery operation during timing, superseding the preparation-time AC observation. Power mode was zero; there was no recorded thermal warning at the start, but no continuous thermal/power trace. No timed models ran concurrently; unrelated host activity and filesystem caches were uncontrolled.

| Svarah completed-file component   | Nemotron |   Cohere |
| --------------------------------- | -------: | -------: |
| Mean ASR time, all 304            |   363 ms |   428 ms |
| Median ASR time                   |   310 ms |   366 ms |
| ASR p95                           |   691 ms |   794 ms |
| Mean ASR time, 303 later requests |   361 ms |   421 ms |
| Mean dictionary application       | 0.049 ms | 0.055 ms |

File ASR time covers the existing manager request, including applicable load/verification and decoding, and excludes file setup, subsequent dictionary application, UI and insertion. The first request in each process is cold; later requests use the adapter's resident weights and fresh recording state. These numbers are not microphone Stop-to-final latency. The JSON records model/runtime revisions, actual initialized providers, native artifact hashes and process peak RSS from `time -l`.

Svarah process peak RSS was 1.06 GiB for Nemotron, 4.24 GiB for Cohere and 0.97 GiB for Nemotron streaming. These are whole-process maxima, not artifact sizes or VRAM. Model artifacts remain pinned: Nemotron Q8 revision `ea30d66debe3740a08b573244286791d423d6b3e`, Cohere INT8 export `156a470cf08eefe706a0004f3c52d9ee567ca7a0`, NeMo-Speech.cpp CPU runtime `4f9676226f667d14608487df744f375db87127f8`, and sherpa-onnx upstream `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`. Full artifact hashes and native build identity are in the JSON.

## Reproduction

The Python helper uses PyArrow solely to decode the existing Parquet files. It does not introduce Python inference, a dataframe framework or a production dependency. The local test environment used Python 3.12.13 and PyArrow 25.0.1.

```sh
uv venv --cache-dir .cache/uv --python 3.12 .cache/accent-eval-venv
uv pip install --python .cache/accent-eval-venv/bin/python --cache-dir .cache/uv pyarrow==25.0.1
. ./scripts/dev-env.sh
cargo build --release --locked -p transcribe-eval
.cache/accent-eval-venv/bin/python scripts/run-accent-benchmark.py \
  .cache/benchmarks/accent-new --freeze
.cache/accent-eval-venv/bin/python scripts/prepare-accent-benchmark.py \
  --output .cache/benchmarks/accent-new/corpus
```

The preparer expects the pinned Svarah shards and the prior calibration/holdout/FLEURS files described in [the original corpus protocol](personalization.md). It refuses to overwrite a corpus directory and fails on shard mismatches, missing cohorts, duplicate selected audio or inconsistent durations. The selection audit pins all input manifests and copied WAVs. `--freeze` saves the just-built evaluator and its source hashes; the runner refuses to overwrite a results directory. Its dated hardware envelope must be updated for a different host or runtime. Future runs use the corrected four/eight native-thread annotations; the historical reports, measured runner and their hashes remain unchanged.

```sh
.cache/accent-eval-venv/bin/python scripts/run-accent-benchmark.py .cache/benchmarks/accent-new
.cache/accent-eval-venv/bin/python scripts/summarize-accent-benchmark.py \
  .cache/benchmarks/accent-new /tmp/accent-results.json
.cache/accent-eval-venv/bin/python scripts/test-accent-benchmark.py
```

Full local reports, logs, the frozen evaluator, selection audit and original run ledger are retained under `.cache/benchmarks/accent-2026-09-18/`. The runner invokes only installed models and records every planned process, including smoke tests and failures; no remote inference API, private recording, weight training or live personalization database is involved.

To reproduce the ordinary-edit replay, use the existing converter with the earlier calibration and the newly produced Svarah report:

```sh
node scripts/prepare-automatic-learning-replay.mjs \
  .cache/benchmarks/personalization-2026-09-16/calibration.json \
  .cache/benchmarks/personalization-2026-09-16/results/calibration-nemotron.json \
  .cache/benchmarks/accent-new/corpus/svarah.json \
  .cache/benchmarks/accent-new/results/nemotron-svarah.json \
  .cache/benchmarks/accent-new/automatic-episodes.json
cargo run --release --locked -p transcribe-data --example automatic_learning_replay -- \
  .cache/benchmarks/accent-new/automatic-episodes.json \
  .cache/benchmarks/accent-new/automatic-results.json
```

Rerun the summarizer after this optional replay to include its per-cohort results. The converter verifies upstream IDs, hashes and successful ASR coverage; it refuses failed/missing inputs rather than dropping recordings.

## Verification and remaining scope

There were **712 native sample executions**: 608 primary Svarah, 64 US controls, 38 streaming replays and two smoke checks. All planned reports and sample IDs are present. There is one native failure, confined to the established Cohere US control; the overall runner therefore correctly exits nonzero while preserving all seven reports. Six new Python selection/summary tests pass, covering reserved-source/prompt exclusion, correlated fragments, order and quality-field invariance, duration bounds, failure retention and pooled score denominators. Twenty existing Rust evaluator tests pass. Ruff lint/format checks cover the four new scripts.

The native source/binary snapshots, selected audio, source shards and measured runner are independently hashed; the generic thread/power annotations were corrected for future runner invocations while historical reports remain intact. The detailed conclusions and limits are linked from [personalization evaluation](../../docs/personalization/evaluation.md). Real microphone quality, other global English accents, verified speaker-level longitudinal improvement, semantic number fidelity and the known Cohere cutoff remain outside this completed subset test.
