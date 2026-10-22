# Benchmark output dataset

Vesper publishes the transcripts its benchmark runs produce as a text-only dataset:
[`rahulsharmavishwakarma/vesper-data-asr-outputs`](https://huggingface.co/datasets/rahulsharmavishwakarma/vesper-data-asr-outputs)
([#324](https://github.com/rahulsharmavishwakarma/vesper/issues/324)). The current export, from 23 October 2026, holds
16,602 transcriptions from 475 local `vesper-eval` reports (September–October 2026, including the 22 October
per-language recommendation matrix). It covers 15 speech models across English, Indian English, Hindi, German, French
and Spanish, and 4,996 rows also carry the writing model's cleaned text. The
first upload is private, so the owner can review it before publishing.

## What is published

One row per transcription:
- **Clip:** its public dataset ID and source.
- **Conditions:** language and condition.
- **What ran:** speech model, runtime, backend and precision.
- **Text:** raw transcript, normalized reference where allowed, and cleaned text.
- **Scores and timing:** word and grapheme error rates, plus timing.

Run-level metadata is published separately, limited to hardware, OS, backend, precision and revisions. The dataset
card on Hugging Face lists the fields, sources, model licences and caveats.

## Audio

The dataset ships no source-dataset audio. Rows reference FLEURS, AMI, MS-SNSD, Svarah and MUCS clips by their own
IDs. [Getting the audio for each row](benchmark-dataset-referencing.md) explains the mapping, the pinned source
revisions, the processing behind condition suffixes such as `--quiet30`, and how to join with FLEURS. The dataset
carries the same guide as `REFERENCING.md`.

The only audio included is Vesper's four generated noise clips (`audio/synthetic-noise/`, 5 s each, deterministic,
redistributable). The scripted dictation controls stay text-only: their audio was spoken by macOS system voices.
Republishable versions would need an open-licence voice; that's planned with the regenerated dataset.

Raw `vesper-eval` reports are not published. Per-row results and run metadata are in the dataset, and the conclusions
are in `docs/validation/`. What remains in the reports is process logs, hashes, machine labels and paths.

## Publication rules

`scripts/export-benchmark-dataset.py` applies these rules. It only reads the benchmark cache.

- **No audio.** Public corpora such as FLEURS are referenced by their IDs, never re-uploaded.
- **No owner or user recordings.** Only rows from allowlisted sources are exported:
  - FLEURS, AMI, MS-SNSD noise;
  - Vesper's synthetic noise and scripted dictation controls;
  - Svarah and MUCS, without reference text.

  The 23 October 2026 audit (#324) traced every audio folder in `.cache/benchmarks` to its declared source and found no
  recording of the owner. Rows without a declared source are dropped.
- **Reference text only under compatible licences.** FLEURS, AMI and Vesper's own text are CC BY 4.0. Svarah (gated, no
  stated licence) and MUCS (CC BY-SA 4.0) keep IDs, transcripts and scores, but not their reference text.
- **Synthetic audio stays private.** The dictation controls were spoken by macOS system voices, so that audio isn't
  distributed. Their reference text is Vesper's.
- **No personalized outputs**, because their learned vocabulary has no recorded origin.
- **No machine detail.** The export drops machine labels and free-text notes, and fails if a file path, or a name in
  metadata, appears anywhere.

## Rebuilding

```sh
uv run --locked python scripts/export-benchmark-dataset.py \
    --benchmarks .cache/benchmarks --out OUTPUT_DIR
```

The script writes:
- `data/transcripts.jsonl`
- `data/reports.jsonl`
- `summary.json`, with counts and skipped rows by reason

Review the summary and the dataset card before uploading. New benchmark runs are not added automatically; a
purpose-built, balanced dataset generated from public corpora with pinned models is planned separately.

## Limitations

- **Historical, not balanced.** The rows mix experiments, application revisions, quantizations and configurations,
  including repeats, and about two thirds are English FLEURS read speech.
- **Timings.** They come from Apple silicon Macs and are comparable only within one report.
