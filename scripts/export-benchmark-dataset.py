#!/usr/bin/env python3
"""Export Vesper's local `vesper-eval` reports as a text-only dataset.

Every row is one transcription from a dated benchmark run: the clip's ID and source,
the language and condition, the speech model and engine that ran, the raw transcript,
the writing model's cleaned text when cleanup ran, scores and timings. No audio is
exported; clips are referenced by public dataset IDs.

Rules (see docs/benchmark-dataset.md):

- Only rows from allowlisted public or Vesper-generated sources. Anything else,
  including any recording of the owner, is skipped.
- Reference text only where its licence allows redistribution under CC BY 4.0.
  Share-alike or unlicensed sources keep their IDs and scores but no reference.
- Personalized outputs are skipped: their learned vocabulary has no recorded origin.
- No machine names, free-text notes, file paths or user names. The export fails if
  any string looks like one.

Usage:

    uv run --locked python scripts/export-benchmark-dataset.py \
        --benchmarks .cache/benchmarks --out OUTPUT_DIR
"""

import argparse
import hashlib
import json
import re
import sys
from collections import Counter
from pathlib import Path

# sourceId -> (dataset, licence, reference text may be published)
SOURCES = {
    "fleurs-en_us-test": ("google/fleurs", "CC-BY-4.0", True),
    "fleurs-en_us-dev": ("google/fleurs", "CC-BY-4.0", True),
    "fleurs-hi_in-test": ("google/fleurs", "CC-BY-4.0", True),
    "fleurs-hi_in-dev": ("google/fleurs", "CC-BY-4.0", True),
    "fleurs-de_de-test": ("google/fleurs", "CC-BY-4.0", True),
    "fleurs-es_419-test": ("google/fleurs", "CC-BY-4.0", True),
    "fleurs-fr_fr-test": ("google/fleurs", "CC-BY-4.0", True),
    "google-fleurs-test": ("google/fleurs", "CC-BY-4.0", True),
    "ami-es2004a": ("AMI Meeting Corpus", "CC-BY-4.0", True),
    "ms-snsd-noise-test": ("microsoft/MS-SNSD noise", "MIT", True),
    "audio-frontend-synthetic-noise": ("Vesper generated noise", "CC-BY-4.0", True),
    # Vesper's own scripts; the audio was synthesized with macOS system voices and is
    # not published, but the scripted reference text is Vesper's.
    "synthetic-dictation-controls": ("Vesper synthetic dictation controls", "CC-BY-4.0", True),
    "svarah": ("ai4bharat/Svarah", "gated; licence not stated", False),
    "openslr-104-mucs-hi-en-test": ("OpenSLR 104 (MUCS 2021)", "CC-BY-SA-4.0", False),
}
SKIP_DIRS = {"model", "models", "binaries", "NeMo-Speech.cpp", "hash-probe-build", "download-parts"}
# File paths must never appear anywhere. Names and machine labels are checked in metadata
# only: public transcripts can legitimately mention a person called Rahul.
PATHS = re.compile(r"(/Users/|/home/|/private/|/tmp/|\\\\|[A-Za-z]:\\)")
NAMES = re.compile(r"(rahul|sharma|vishwakarma|macbook)", re.I)
CONTENT = {"reference_normalized", "hypothesis_raw", "hypothesis_raw_normalized", "cleaned_text"}
DATE = re.compile(r"20\d\d-\d\d-\d\d")
ENVIRONMENT_FIELDS = (
    "hardware",
    "operatingSystem",
    "powerMode",
    "backend",
    "precision",
    "threads",
    "applicationRevision",
    "runtimeRevision",
    "driver",
)
MODEL_FIELDS = (
    "modelId",
    "revision",
    "runtime",
    "runtimeVersion",
    "runtimeRevision",
    "engine",
    "provider",
    "selectedProvider",
    "requestedBackend",
    "quantization",
)


def clean(value):
    """Keep only short scalar strings that carry no machine-specific detail."""
    if value is None or isinstance(value, (bool, int, float)):
        return value
    text = str(value)
    return None if PATHS.search(text) else text


def score(entry):
    if not isinstance(entry, dict):
        return None
    out = {}
    for kind in ("wordErrorRate", "graphemeErrorRate"):
        value = entry.get(kind)
        if isinstance(value, dict):
            out[kind] = {k: value.get(k) for k in ("edits", "referenceUnits", "rate")}
    return out or None


def reports(root: Path):
    stack = [root]
    while stack:
        directory = stack.pop()
        for path in sorted(directory.iterdir()):
            if path.is_dir():
                if path.name not in SKIP_DIRS and not path.is_symlink():
                    stack.append(path)
            elif path.suffix == ".json" and path.stat().st_size < 200_000_000:
                try:
                    data = json.loads(path.read_text())
                except (ValueError, UnicodeDecodeError):
                    continue
                runs = data.get("runs") if isinstance(data, dict) else None
                if (
                    isinstance(runs, list)
                    and runs
                    and isinstance(runs[0], dict)
                    and "rawHypothesis" in runs[0]
                ):
                    yield path, data


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--benchmarks", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.out.exists():
        sys.exit(f"{args.out} already exists")
    root = args.benchmarks.resolve()

    rows, report_rows = [], []
    skipped = Counter()
    for path, data in reports(root):
        relative = path.relative_to(root).as_posix()
        report_id = hashlib.sha256(relative.encode()).hexdigest()[:12]
        date = (DATE.findall(relative) or [str(data.get("generatedAt", ""))[:10]])[0]
        environment = data.get("environment") or {}
        versions = data.get("modelVersions") or {}
        asr = versions.get("asr") or {}
        rewriter = versions.get("rewriter") or {}
        report = {
            "report_id": report_id,
            "run_date": date,
            "benchmark": clean(
                relative.split("/")[1]
                if relative.startswith("results/")
                else relative.split("/")[0]
            ),
            "environment": {k: clean(environment.get(k)) for k in ENVIRONMENT_FIELDS},
            "asr": {k: clean(asr.get(k)) for k in MODEL_FIELDS},
            "rewriter": {k: clean(rewriter.get(k)) for k in MODEL_FIELDS} if rewriter else None,
        }
        used = 0
        for run in data["runs"]:
            source = run.get("sourceId")
            if source not in SOURCES:
                skipped[f"source:{source}"] += 1
                continue
            dataset, licence, publish_reference = SOURCES[source]
            raw = run.get("rawScore") if isinstance(run.get("rawScore"), dict) else {}
            reference = raw.get("normalizedReference") if publish_reference else None
            row = {
                "report_id": report_id,
                "run_date": report["run_date"],
                "benchmark": report["benchmark"],
                "sample_id": clean(run.get("sampleId")),
                "source_id": source,
                "source_dataset": dataset,
                "source_licence": licence,
                "language": clean(run.get("language")),
                "configured_language": clean(run.get("configuredLanguage")),
                "scenario": clean(run.get("scenario")),
                "replay_kind": clean(run.get("replayKind")),
                "mode": clean(run.get("mode")),
                "audio_seconds": run.get("audioSeconds"),
                "asr_model": report["asr"]["modelId"],
                "asr_runtime": report["asr"]["runtime"],
                "asr_runtime_revision": report["asr"]["runtimeRevision"],
                "backend": report["environment"]["backend"],
                "precision": report["environment"]["precision"],
                "application_revision": report["environment"]["applicationRevision"],
                "rewriter_model": (report["rewriter"] or {}).get("modelId")
                if run.get("cleanedHypothesis")
                else None,
                "reference_normalized": reference,
                "hypothesis_raw": clean(run.get("rawHypothesis")),
                "hypothesis_raw_normalized": raw.get("normalizedHypothesis")
                if publish_reference
                else None,
                "cleaned_text": clean(run.get("cleanedHypothesis")),
                "raw_score": score(raw),
                "cleaned_score": score(run.get("cleanedScore")),
                "asr_ms": run.get("asrMs"),
                "cleanup_ms": run.get("cleanupMs"),
                "real_time_factor": run.get("realTimeFactor"),
                "first_partial_ms": run.get("firstPartialMs"),
                "stop_to_final_ms": run.get("stopToFinalMs"),
                "error": bool(run.get("error")),
            }
            if not row["asr_model"]:
                skipped["no recorded speech model"] += 1
                continue
            if run.get("rawHypothesis") and row["hypothesis_raw"] is None:
                skipped["hypothesis looked private"] += 1
                continue
            rows.append(row)
            used += 1
        if used:
            report_rows.append(report)

    # Final guard: nothing machine-specific may leave this machine.
    def leaks(value, where):
        if isinstance(value, dict):
            for key, item in value.items():
                yield from leaks(item, f"{where}.{key}")
        elif isinstance(value, str) and (
            PATHS.search(value) or (where.rsplit(".", 1)[-1] not in CONTENT and NAMES.search(value))
        ):
            yield where, value

    for row in rows + report_rows:
        for where, value in leaks(row, row.get("report_id", "?")):
            sys.exit(f"private detail in export at {where}: {value[:120]!r}")

    data_dir = args.out / "data"
    data_dir.mkdir(parents=True)
    with (data_dir / "transcripts.jsonl").open("w") as handle:
        for row in rows:
            handle.write(json.dumps(row, ensure_ascii=False) + "\n")
    with (data_dir / "reports.jsonl").open("w") as handle:
        for report in report_rows:
            handle.write(json.dumps(report, ensure_ascii=False) + "\n")
    summary = {
        "rows": len(rows),
        "reports": len(report_rows),
        "sources": Counter(r["source_id"] for r in rows).most_common(),
        "models": Counter(r["asr_model"] for r in rows).most_common(),
        "languages": Counter(r["language"] for r in rows).most_common(),
        "with_reference": sum(r["reference_normalized"] is not None for r in rows),
        "with_cleaned_text": sum(r["cleaned_text"] is not None for r in rows),
        "skipped": skipped.most_common(),
    }
    (args.out / "summary.json").write_text(json.dumps(summary, indent=1) + "\n")
    print(json.dumps(summary, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
