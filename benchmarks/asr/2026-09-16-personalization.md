# Local correction memory: native corpus screen, 16 September 2026

The frozen application memory ran across two installed recognizers on public speech. It produced a small Hindi spelling gain, an English initialism WER regression, and no changes on Indian English or Hinglish. This establishes that the model-independent correction path executes on real audio; it does **not** establish useful continuous improvement for a particular user.

The user authorized collecting public datasets and running native/real-time tests. This is development evaluation under [#34](https://github.com/rahulsharmavishwakarma/transcribe/issues/34) and [#33](https://github.com/rahulsharmavishwakarma/transcribe/issues/33). No cloud inference, user recording, model training, default change, decoder modification or inference-lifetime change was involved.

The [protocol](personalization.md) documents preparation and replay. The local debug report at `.cache/benchmarks/results/2026-09-16-personalization/debug-summary.json` preserves scores, denominators, failures, all changed examples, sample identities/audio hashes, source licenses, artifact hashes, environment, paired cleanup outcomes and per-recording streaming measurements. Full native reports and logs remain in the ignored `.cache/benchmarks/personalization-2026-09-16/` directory.

## Data and calibration

There were **320 unique recordings, 49.24 minutes of audio and 444 native sample executions** including calibration and repeated conditions. All phases completed; one execution returned an explicit decode failure.

| Public source                                                                        | Calibration |  Evaluation | Qualification limit                                                    |
| ------------------------------------------------------------------------------------ | ----------: | ----------: | ---------------------------------------------------------------------- |
| [Google FLEURS](https://huggingface.co/datasets/google/fleurs), US English           |      64 dev |     32 test | Short read speech; prior prepared test prompts excluded                |
| FLEURS, Hindi                                                                        |      64 dev |     32 test | Short read speech                                                      |
| [AI4Bharat Svarah](https://huggingface.co/datasets/ai4bharat/Svarah), Indian English | 64 reserved | 48 separate | Test-only corpus; recording-group proxy, not verified speaker-disjoint |
| [MUCS / OpenSLR104](https://www.openslr.org/104/), Hindi-English                     |           0 |          16 | Existing technical-tutorial screen, configured Auto                    |

FLEURS and Svarah are CC-BY-4.0; MUCS is CC-BY-SA-4.0. Exact source revisions, archive/shard hashes and attributions are in the JSON. The Svarah sample cycles through cached primary-language metadata; that field is not an accent detector, and the cached shards do not reliably identify speakers or distinguish read from spontaneous clips. These are selected subsets, not official full-test scores. Upstream model-training overlap is unknown.

Selection preceded predictions. Calibration and evaluation have no overlapping selected audio hashes, normalized references or recorded source-group proxies. Corpus human references simulate explicit confirmation in a separate evaluation profile; they do not write into the user's memory database. The profile was frozen before holdout inference, with **51 aliases in 51 entries**, zero style examples and native phrase boosting disabled. Hinglish had no matching Auto-language calibration entries, which limits that condition to a baseline/portability screen.

Calibration exposed unsafe suggestions such as `I → Ama` and `है → हैं`. Before freezing memory and evaluating holdout, the production guard was tightened for common pronouns/auxiliary verbs, and unchanged prefix/suffix matching became case-insensitive. Candidate count fell from 60 to 51. Existing reviewed entries remain pausable under the stricter suggestion policy; changed sourced aliases still require matching reviews. These are conservative heuristic guards, not a general semantic safety guarantee. The initial candidate profile remains in the local cache. No alias was tuned against holdout results.

## Paired completed-audio accuracy

Raw and personalized text below share the identical native ASR output; cleanup and recognition boosting are off. WER is word edits divided by reference words. Lower is better.

| Recognizer  | Corpus         | Scored / attempted |          Raw WER |       Memory WER | Changed recordings          |
| ----------- | -------------- | -----------------: | ---------------: | ---------------: | --------------------------- |
| Nemotron Q8 | US English     |            32 / 32 | 121/738 = 16.40% | 123/738 = 16.67% | 1 worse under normalization |
| Nemotron Q8 | Hindi          |            32 / 32 | 133/770 = 17.27% | 130/770 = 16.88% | 2 improved                  |
| Nemotron Q8 | Indian English |            48 / 48 | 128/692 = 18.50% | 128/692 = 18.50% | 0                           |
| Nemotron Q8 | Hindi-English  |            16 / 16 | 114/229 = 49.78% | 114/229 = 49.78% | 0                           |
| Cohere int8 | US English     |            31 / 32 |   61/714 = 8.54% |   63/714 = 8.82% | 1 worse under normalization |
| Cohere int8 | Indian English |            48 / 48 |   68/692 = 9.83% |   68/692 = 9.83% | 0                           |

The Hindi changes were three occurrences of `ज्यादा → ज़्यादा` across two recordings. Grapheme error rate moved from 11.81% to 11.63%. This is orthographic agreement, not improved acoustic recognition.

The same English recording changed `UN → U.N` with both recognizers. The scorer splits punctuation into word boundaries, increasing that recording's word errors from one to three; grapheme error rate was unchanged. This is an initialism formatting/scoring regression, not evidence of a changed intended entity. The canonical span also lacks the source's final period because correction extraction trims edge punctuation. Preserve the observed regression; evaluate any normalization or extraction change on a newly reserved set.

Cohere failed on `en_us-test-15199695878621076767` with `duration_token_limit`; the adapter discarded the incomplete transcription. Its 24 reference words are absent from the 714-word scored denominator, while the failure remains in coverage. Therefore the US model rows are not identical complete-case comparisons. No decoder budget was adjusted to make this sample pass.

Strict Svarah WER also counts correctly spoken-out digit strings against numeric references as multiple errors. Other recordings actually omit digits. A future number-value metric must distinguish these cases; the current report does not silently normalize away either. On the identical 48-clip Indian English set, Cohere has the lower measured raw WER, while neither model applies this memory. This narrow CPU result does not promote a model or qualify another language/backend.

## Cleanup comparison

Eight US English and eight Indian English recordings each ran through Cohere plus Qwen with memory disabled and enabled: 32 executions. Pairing checks identical sample IDs, manifest hashes and raw output.

| Subset            | Memory off WER |  Memory on WER | Different final text | Failures |
| ----------------- | -------------: | -------------: | -------------------: | -------: |
| US English, 8     | 13/215 = 6.05% | 13/215 = 6.05% |                  0/8 |        0 |
| Indian English, 8 |  4/102 = 3.92% |  4/102 = 3.92% |                  0/8 |        0 |

All cleanup outcomes were accepted, with no relevant vocabulary or examples retrieved. This tests the no-match path under real native cleanup. It provides no positive evidence for remembered style, supplied intent or contextual vocabulary retrieval. Verbatim corpus references cannot label desired output form.

## Real-time prerecorded replay

Nemotron received normal-speed 20 ms packets for four recordings per locale. The first sample in each process was cold; subsequent samples used resident weights with fresh per-recording state. No replay returned an error or changed text through memory.

| Locale, n=4 each | First nonempty partial p50 | ASR stop-to-final p50 | ASR stop-to-final p95 | WER raw = memory |
| ---------------- | -------------------------: | --------------------: | --------------------: | ---------------: |
| US English       |                   1,969 ms |                 81 ms |                103 ms |  22/108 = 20.37% |
| Hindi            |                   1,954 ms |                 68 ms |                 89 ms |     9/91 = 9.89% |
| Indian English   |                   1,016 ms |                 79 ms |                103 ms |   15/45 = 33.33% |

Across these 12 recordings, first partial ranged from 1,008 to 2,566 ms and ASR stop-to-final from 46 to 104 ms. Each locale's p95 is its maximum with only four observations, not an estimate of production tail latency. Per-sample cold/resident flags and partial edit/update counts are preserved in the JSON; callbacks can repeat an unchanged partial and should not be interpreted as new word emissions.

First-partial timing includes setup/model initialization when cold. Stop-to-final starts after the last paced audio delivery and excludes later memory, cleanup, UI rendering and insertion. This is **prerecorded real-time replay, not a microphone or desktop end-to-end test**. Endpoint and word-emission timestamps, internal queue-drop counts, cancellation/reset under real audio and continuous power/thermal behavior were not measured in this experiment.

The deterministic memory stage had p95 below 0.11 ms in each nonempty-profile condition. Completed-file ASR p50 was 295–473 ms for Nemotron's separate locale groups and 6,943–7,095 ms for Cohere's. Those file timings include each adapter's existing ownership behavior: resident Nemotron versus Cohere loading per recording. They are not equivalent to microphone stop latency or the cost of memory.

Measured process peak RSS was about 1.07 GiB for the 112-clip Nemotron holdout, 4.39–4.47 GiB for Cohere's standalone corpus runs, and 5.33–5.35 GiB for Cohere/Qwen cleanup runs. These are process maxima, not model artifact sizes or VRAM. Calibration completed inference, but its macOS `time -l` system query was denied afterward; calibration peak RSS is unavailable.

## Reproducibility and evidence boundary

Local hardware: Apple M5 Pro, 18 logical CPUs, 64 GiB RAM; macOS 26.6.2 (25G83), AC power mode 0. All inference explicitly selected CPU; no Auto/GPU fallback was used. Start-of-run checks showed no thermal/performance warning; there is no continuous thermal trace. The application worker cap is eight threads for applicable adapters. The NeMo ABI does not expose a thread setting, so its actual graph thread count remains unknown; `environment.threads = 8` must not be read as a verified NeMo internal count.

| Component                              | Exact revision                             |
| -------------------------------------- | ------------------------------------------ |
| Nemotron 3.5 ASR 0.6B Q8               | `ea30d66debe3740a08b573244286791d423d6b3e` |
| NeMo-Speech.cpp 0.1.0 CPU runtime      | `4f9676226f667d14608487df744f375db87127f8` |
| Cohere ONNX int8 export                | `156a470cf08eefe706a0004f3c52d9ee567ca7a0` |
| sherpa-onnx 1.13.8 native source       | `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf` |
| Qwen3.5-0.8B Q4_0 GGUF                 | `8fea620810c4afa23dd6443f999a48574c1611a3` |
| llama-cpp-2 0.1.156 / native llama.cpp | `e79e4bf660e19f2ad851e06c6913f7a8c5852621` |

The JSON contains every selected model/runtime artifact SHA-256, report/manifest hashes, and measured source-file hashes. Git HEAD was `592e0084fe5cf486b4d56c575423e9a4f219bfb8` with unstaged work; HEAD alone does not reproduce these results. Holdout source stamp: `b27f508e2304bf20ce37a2d73a0d51f3339f6d8593eb1565c3515ab2cbbaec60`. Evaluator binary SHA-256: `963cfa5532912dec1d56fe93128f94f15d16e34d6c1bc74e253ea1c54e37eaea`. Frozen memory SHA-256: `c77345ed6acea87bfa92e6e797068fcb124937ad0629be0c0cf117b8e13f124a`. Later regression-test/documentation edits are not represented by the measured source stamp. Calibration used the earlier source stamp retained in its report and performed no memory substitutions.

This report contains **local measurements**. Dataset cards and runtime pins establish **official source/capability facts**. Relevant **paper results** remain separate in the [research review](../../docs/research-personal-dictation.md). Reduced personal editing effort, reliable accent adaptation and intent usefulness remain **unverified hypotheses**. Runtime-internal decoder work and training/checkpoint changes were not implemented.

## What this changes next

The next useful quality set needs repeated names and technical terms across separate later sessions from the same consenting speaker, plus unrelated/homophone negatives. Measure entity errors, false substitutions and editing effort with the same frozen profile across models. These corpus results do not justify increasing automatic learning or claiming that more stored aliases mean better dictation.

Native microphone/device validation, reviewed intent/style labels and longitudinal personal benefit remain outstanding under #34. The Cohere cutoff remains recorded under #33. Training stays deferred under #5. The general model-recommendation summarizer now rejects personalized reports, keeping these application-memory measurements out of base-model promotion.

## Verification

After all timing runs finished, `make fmt && make check` passed: 176 Rust tests, 68 Node tests, native completion-report fixtures, strict Clippy, frontend build, version checks and Svelte/TypeScript with zero warnings/errors. Fourteen optional model/device Rust tests remain ignored by that default command; the 444 corpus executions above are a separate explicit native test lane. Added regression coverage checks unsafe pronoun/grammar suggestions, management of legacy reviewed memory, calibration/holdout overlap rejection, failure/regression-preserving summaries and exclusion of personalized results from base-model recommendations. Changes remain unstaged and uncommitted.
