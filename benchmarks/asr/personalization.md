# Native corpus test for local correction memory

This development-only test uses the application's Rust/native inference APIs and the same `PersonalizationProfile::personalize` function as desktop finalization. It does not modify user settings, stored recordings, models or personal memory. The protocol and findings are tracked in [#34](https://github.com/rahulsharmavishwakarma/transcribe/issues/34).

## Corpus and split

`scripts/prepare-personalization-benchmark.py` prepares a fixed selection without reading model predictions:

| Pack | Calibration | Holdout | Source |
| --- | --- | --- | --- |
| US English | 64 development clips | 32 test clips | [FLEURS](https://huggingface.co/datasets/google/fleurs), CC-BY-4.0 |
| Hindi | 64 development clips | 32 test clips | FLEURS, CC-BY-4.0 |
| Indian English | 64 reserved clips | 48 separate clips | [Svarah](https://huggingface.co/datasets/ai4bharat/Svarah), CC-BY-4.0 |
| Hindi-English code switching | None | Existing 16-clip screen | [MUCS / OpenSLR104](https://www.openslr.org/104), CC-BY-SA-4.0 |

The preparer hashes source archives/shards and audio, selects 4–20-second natural recordings in fixed hash order, and deduplicates normalized reference text. Previously prepared FLEURS test prompts are excluded. FLEURS calibration uses its official development split; test references never create memory.

Svarah is a test-only corpus. Its calibration partition is explicitly reserved for this new memory experiment, so results are not the official full-test score. Selection cycles through the cached `primary_language` strata. All fragments sharing the filename stem before `_chunk_` stay on one side; only one fragment per group is selected. The Parquet shards lack reliable speaker IDs and read/spontaneous labels. Thus the split is not claimed speaker-disjoint, and `primary_language` is not a detected accent. `mixed_speech` preserves this uncertainty in the manifest. Duplicate/overlapping audio beyond these exact hashes and group proxies is not exhaustively identified. Public corpus membership also does not prove exclusion from upstream model training.

Preparation needs Python with the already available PyArrow Parquet decoder; native inference does not use Python. No dataframe, embedding model or training framework is added to the application.

FLEURS pin: `70bb2e84b976b7e960aa89f1c648e09c59f894dd`. Fetch the two development metadata files and archives from `data/{en_us,hi_in}/dev.tsv` and `data/{en_us,hi_in}/audio/dev.tar.gz` at that revision, saving them under `downloads/{locale}-dev.tsv` and `downloads/{locale}-dev.tar.gz` inside the experiment directory. Existing test archives and pinned Svarah shards follow the [ASR setup](README.md). Archive extraction is limited to selected WAV members read as bytes; no archive-supplied paths are extracted.

```sh
python3 scripts/prepare-personalization-benchmark.py \
  --cache .cache/benchmarks \
  --output .cache/benchmarks/personalization-2026-09-16
```

The script emits manifests, public audio and `split-audit.json`. Record the actual machine/backend/thread/precision/power envelope in `environment-nemotron.json` and `environment-cohere.json`, following `environment.example.json`. Use the explicit CPU runtime directory for CPU tests, not an Auto GPU fallback. Preserve source-code and binary hashes with every experiment; Git HEAD alone does not describe this unstaged worktree.

## Frozen profile and paired scoring

Build the release evaluator and run calibration without memory. `build-memory` requires the calibration report's manifest hash to match and refuses personalized calibration reports. It derives bounded suggestions from raw predictions and the corpus's supplied human references, skips failed samples, and uses the production correction guard. The corpus references simulate explicit confirmation for this experiment; generated predictions never serve as targets. These are not real user review IDs or a measure of user confirmation behavior.

The resulting `EvaluationMemory` file contains the portable profile, calibration report/manifest hashes, and all calibration audio hashes and normalized references. Before inference, `--profile` rejects holdout audio or prompts also present in calibration. Keep the file frozen after inspecting calibration. Never tune aliases, thresholds or profiles on holdout outputs.

```sh
. ./scripts/dev-env.sh
cargo build --release --locked -p transcribe-eval
sh scripts/run-personalization-benchmark.sh
```

The runner first uses resident Nemotron with fresh per-recording state. It then tests the same memory with Cohere, whose adapter loads per recording. Normal memory runs leave native speech boosting disabled and cleanup off: raw and personalized text form a paired comparison on the identical ASR result, isolating the application correction stage without another model call. This is not a test of acoustic adaptation or native word boosting. Retain unchanged cases and regressions.

A smaller Cohere/Qwen subset runs cleanup both without and with memory. Compare those conditions by sample ID and identical reference; comparing raw ASR against cleaned output alone would confound memory with cleanup. These corpora supply verbatim references, not user-approved style/intent labels, so WER cannot establish presentation preference or inferred-intent correctness.

The general model-recommendation summarizer rejects personalized runs. They belong in the separate paired report and cannot silently qualify or promote a base checkpoint.

## Real-time meaning and latency

The Nemotron subset supplies 20 ms packets at normal wall-clock speed using the existing stream API. It records first nonempty partial, partial changes, ASR stop-to-final and memory duration. This is **real-time prerecorded corpus replay**, not microphone capture, noise/room testing, UI rendering or external insertion latency.

First-partial timing starts before stream/model setup, so the first cold sample includes initialization. ASR stop-to-final starts after the last paced audio delivery and ends at native finalization; it excludes subsequent cleanup, memory, UI and copying. Completed-file ASR time includes the model work after audio is available and is not microphone stop latency. The memory timer measures the deterministic final correction function only.

`/usr/bin/time -l` records process elapsed time and peak RSS on macOS. Reports retain the actual backend and pinned artifacts, and separate cold/resident runs. Queue submission failures remain failed samples; zero failures does not independently prove zero internal drops. Endpoint/word-emission timestamps and continuous power/thermal telemetry are not measured by this runner. Do not infer them from RTF or model size.

## Interpretation

Report WER/CER with denominators, failures, changed recordings, improved/worsened recordings, applied counts and all changed examples. Keep language, locale, configured prompt, mode, profile, model and hardware envelopes separate. The summarizer uses the existing Unicode-aware ASR scoring contract, which splits punctuation: changing `UN` to `U.N` can increase WER despite preserving its meaning. Orthographic agreement is not evidence of better acoustic recognition.

These tests establish a bounded multi-speaker corpus result. They do not demonstrate longitudinal learning for a particular person, reliable accent detection, intent accuracy, a calibrated review queue or reduced editing time. Those need later sessions from the same consenting user and suitable reviewed labels. No new model defaults or automatic promotion follow from this screen.
