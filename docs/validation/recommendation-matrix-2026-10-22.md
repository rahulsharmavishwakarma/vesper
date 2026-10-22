# Per-language speech-model matrix for setup recommendations — 22 October 2026

Question ([#296](https://github.com/rahulsharmavishwakarma/vesper/issues/296)): through the application's own path on one Mac, which catalog speech model is most accurate in each of setup's main languages, and which ones pass the ADR-0010 screening gates? Setup now ranks speech models from these results ([language support and recommendations](../language-support-and-recommendations.md#what-the-application-recommends-today)).

**Result.** Parakeet v3 was the most accurate model in English, French, German and Spanish, and Nemotron was the only model that passed in Hindi.

- **English:** Granite Speech 5.0 TurboCTC was within half a point of Parakeet and faster, so the tie rule picks Granite.
- **Cohere, the default model,** failed the reliability gate in French (25 of 100 recordings discarded) and German (14 of 100). Every failure was the same decoder stop reason, tracked in [#323](https://github.com/rahulsharmavishwakarma/vesper/issues/323).
- **Qwen3-ASR** failed the accuracy gate in Hindi.

## Method

- **Application revision:**
  - `main` at `cf0ec68` (v0.1.1-rc.5) with this branch's runner.
  - `vesper-eval` built with `--release --features mlx`; binary SHA-256 prefix `82ed46db3f24a2de`.
  - Every model file was installed and hash-checked with the application installer (`crates/vesper-models/examples/install.rs`).
- **Engine:**
  - The environment backend is `auto` and the speech engine is Automatic, exactly as the app runs them.
  - The engine that actually ran is recorded per result as `engine`: English Parakeet ran on the Neural Engine (`neural_engine_tdt`), and other Parakeet lanes ran on the unified GGUF on Metal (`nemo_tdt`).
- **Audio:**
  - 100 held-out FLEURS test recordings per language, at the pinned revision `70bb2e84`, selected by `scripts/prepare-fleurs-benchmark.mjs` (the first unique sentences of 4–20 s).
  - Manifest SHA-256 prefixes: en-US `8dd6cabe50f0` (the same set as earlier English runs), fr-FR `7a2ce71e92ca`, de-DE `5081efa1780f`, es-419 `14c7e6fc5670`, hi-IN `e82e51a88f58`.
  - FLEURS publishes Spanish only as Latin American Spanish (`es_419`).
- **Lanes:**
  - `scripts/run-recommendation-matrix.mjs` runs one process per lane: every model on every declared language with `--language` set, plus a separate Automatic-detection lane where the vendor declares detection. That is 55 lanes, 43 minutes in total.
  - Mode: completed recordings at accelerated pace, offline.
- **Machine:**
  - Apple M5 Pro, 18 logical CPUs, 64 GiB, macOS 27.0.1, AC power.
  - Each lane records its power source. Five early lanes that started on battery were rerun on AC; their battery runs are not used.
- **Scoring:**
  - `vesper-eval`'s normalized scorer.
  - **WER** is corpus-level (word edits divided by reference words) over the recordings that finished. Failed recordings count in the failure rate, never as perfect.
  - **ms** is the median time to transcribe one recording.
  - **Peak memory** is the process's peak footprint from `/usr/bin/time -l`. The Neural Engine's memory is not charged to the process, so the English Parakeet figure understates its use by about the encoder's compressed size (roughly 0.6 GB; [quantization research](../research-quantization.md#neural-engine-options)).
- **Gates:** median per-recording WER ≤ 20%, failure rate ≤ 2%, median real-time factor ≤ 1.0 (the checked-in screening request values).
- **Evidence path:**
  1. `scripts/export-recommendation-evidence.mjs` writes the aggregates into `models/recommendations.json` (`local.results` for explicit runs, `local.automatic` for detection runs).
  2. `scripts/summarize-asr-benchmarks.mjs` accepted all 55 lanes as comparable envelope groups.

  No audio or transcripts are committed.

## Results: language set explicitly

WER and CER in percent; memory in GB.

| Language                | Model                       |   WER |   CER | Median ms | Peak memory | Failed | Gates               |
| ----------------------- | --------------------------- | ----: | ----: | --------: | ----------: | -----: | ------------------- |
| English                 | Parakeet v3 (Neural Engine) |  4.95 |  2.57 |        39 |       0.56† |      0 | pass                |
| English                 | **Granite Speech 5.0**      |  5.08 |  2.91 |        23 |        1.71 |      0 | pass, **pick**      |
| English                 | Whisper Large v3 Turbo      |  5.63 |  2.83 |     1,371 |        2.66 |      0 | pass                |
| English                 | Phonon-2                    |  5.86 |  3.25 |        46 |        1.26 |      0 | pass                |
| English                 | Whisper Small               |  5.96 |  3.21 |       286 |        1.75 |      0 | pass                |
| English                 | Parakeet Redux              |  6.28 |  3.58 |        43 |        1.33 |      0 | pass                |
| English                 | Qwen3-ASR                   |  6.55 |  4.04 |       178 |        1.77 |      0 | pass                |
| English                 | Cohere Transcribe           |  6.88 |  3.93 |       475 |        3.47 |      2 | pass (2%)           |
| English                 | LFM2.5 Audio                |  7.60 |  4.67 |       168 |        0.54 |      1 | pass                |
| English                 | Moonshine Base              |  9.99 |  5.40 |        97 |        0.54 |      0 | pass                |
| English                 | Nemotron 3.5                | 10.49 |  6.01 |        87 |        1.00 |      0 | pass                |
| French                  | **Parakeet v3**             |  4.86 |  1.73 |        64 |        0.97 |      0 | pass, **pick**      |
| French                  | Granite Speech 5.0          |  7.54 |  3.35 |        22 |        1.70 |      0 | pass                |
| French                  | Whisper Large v3 Turbo      |  7.54 |  2.70 |     1,520 |        2.66 |      0 | pass                |
| French                  | Parakeet Redux              |  8.79 |  3.36 |        46 |        1.29 |      0 | pass                |
| French                  | Nemotron 3.5                | 11.69 |  5.54 |       106 |        1.00 |      0 | pass                |
| French                  | Qwen3-ASR                   | 13.05 |  6.38 |       773 |        1.98 |      0 | pass                |
| French                  | Whisper Small               | 15.23 |  6.25 |     1,033 |        1.59 |      0 | pass                |
| French                  | Cohere Transcribe           | 5.85* | 2.81* |       643 |        3.46 | **25** | **fail**            |
| German                  | **Parakeet v3**             |  4.99 |  1.48 |        72 |        0.95 |      0 | pass, **pick**      |
| German                  | Granite Speech 5.0          |  5.86 |  3.12 |        27 |        1.70 |      0 | pass                |
| German                  | Parakeet Redux              |  6.32 |  2.12 |        49 |        1.32 |      0 | pass                |
| German                  | Nemotron 3.5                | 11.18 |  6.06 |       106 |        1.00 |      0 | pass                |
| German                  | Qwen3-ASR                   | 14.38 |  6.12 |       798 |        2.06 |      0 | pass                |
| German                  | Cohere Transcribe           | 7.37* | 3.41* |       765 |        3.45 | **14** | **fail**            |
| Spanish (Latin America) | **Parakeet v3**             |  2.90 |  0.98 |        69 |        0.97 |      0 | pass, **pick**      |
| Spanish (Latin America) | Parakeet Redux              |  3.60 |  1.28 |        53 |        1.34 |      0 | pass                |
| Spanish (Latin America) | Granite Speech 5.0          |  3.76 |  1.53 |        29 |        1.72 |      0 | pass                |
| Spanish (Latin America) | Cohere Transcribe           |  5.09 |  2.81 |       775 |        3.57 |      1 | pass                |
| Spanish (Latin America) | Nemotron 3.5                |  5.68 |  2.82 |       104 |        1.01 |      0 | pass                |
| Spanish (Latin America) | Qwen3-ASR                   |  9.09 |  3.94 |       781 |        1.98 |      0 | pass                |
| Hindi                   | **Nemotron 3.5**            | 16.59 | 10.49 |       104 |        1.00 |      0 | pass, **pick**      |
| Hindi                   | Qwen3-ASR                   | 24.91 | 18.37 |     1,477 |        1.98 |      0 | **fail** (accuracy) |

\* Over the recordings that finished only; the discarded recordings are not scored.
† Excludes Neural Engine memory (see Method).

### Results: automatic language detection

Kept separate and not used for ranking. Parakeet's English detection lane runs on the unified GGUF, because the Neural Engine engine is English-only and needs an explicit language.

| Language                | Model                  |   WER | Median ms | Failed | Gates           |
| ----------------------- | ---------------------- | ----: | --------: | -----: | --------------- |
| English                 | Parakeet v3            |  4.86 |        58 |      0 | pass            |
| English                 | Whisper Large v3 Turbo |  5.63 |     1,437 |      0 | pass            |
| English                 | Whisper Small          |  6.18 |       756 |      0 | pass            |
| English                 | Parakeet Redux         |  6.28 |        43 |      0 | pass            |
| English                 | Qwen3-ASR              |  8.15 |       558 |      0 | pass            |
| English                 | Nemotron 3.5           | 10.81 |        90 |      0 | pass            |
| French                  | Parakeet v3            |  4.86 |        65 |      0 | pass            |
| French                  | Whisper Large v3 Turbo |  7.54 |     1,513 |      0 | pass            |
| French                  | Parakeet Redux         |  8.79 |        45 |      0 | pass            |
| French                  | Nemotron 3.5           | 11.80 |       105 |      0 | pass            |
| French                  | Qwen3-ASR              | 12.82 |       820 |      0 | pass            |
| French                  | Whisper Small          | 15.23 |     1,078 |      0 | pass            |
| German                  | Parakeet v3            |  4.99 |        69 |      0 | pass            |
| German                  | Parakeet Redux         |  6.32 |        49 |      0 | pass            |
| German                  | Nemotron 3.5           | 10.95 |       106 |      0 | pass            |
| German                  | Qwen3-ASR              | 14.34 |       864 |      0 | pass            |
| Spanish (Latin America) | Parakeet v3            |  2.90 |        68 |      0 | pass            |
| Spanish (Latin America) | Parakeet Redux         |  3.60 |        50 |      0 | pass            |
| Spanish (Latin America) | Nemotron 3.5           |  5.91 |       113 |      0 | pass            |
| Spanish (Latin America) | Qwen3-ASR              | 10.18 |       900 |      0 | pass            |
| Hindi                   | Nemotron 3.5           | 17.69 |       106 |      0 | pass            |
| Hindi                   | Qwen3-ASR              | 25.05 |     1,635 |      1 | fail (accuracy) |

## Findings

- **English tie.**
  - Granite (5.08%) and Parakeet on the Neural Engine (4.95%) differ by 0.13 points. With about 2,100 words that is well within noise.
  - The existing rule (within half a point, faster wins) picks Granite at 23 ms per recording, against 39 ms.
  - Granite needs an Apple silicon GPU. On other Macs the next passing model is chosen.
- **Cohere** discards 14–25% of short French and German recordings with `duration_token_limit`. This is a product bug, not a benchmark artifact ([#323](https://github.com/rahulsharmavishwakarma/vesper/issues/323)). Its English result passes at exactly the 2% limit.
- **Nemotron** is the only model that passes in Hindi, but at 16.6% WER it is far from the European languages.
- **Comparison with published figures:**
  - Nemotron's English error rate here (10.5%) is much higher than Parakeet's, in completed-recording mode.
  - The published leaderboard used before this run ranked Cohere first for German and French. Measured through Vesper, Parakeet leads in both.

## Limitations

- One machine (Apple M5 Pro, 64 GiB). No 8 or 16 GB Mac was available, so speed and memory on smaller Macs are not measured. Setup's memory filter still uses these peaks.
- Read speech only (FLEURS). Noise, accents, long recordings, live dictation and the cleanup stage are outside this matrix.
- 100 recordings (about 2,100–2,550 words) per language give roughly ±0.5-point uncertainty, which is why half-point ties go to speed.
- Neural Engine memory is not in the peak figures.
- Spanish is Latin American Spanish; Castilian Spanish is not measured.
