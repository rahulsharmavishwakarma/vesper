#!/usr/bin/env python3
"""Summarize paired native accent runs without exporting transcript rows."""

import argparse
import hashlib
import json
import math
import re
from collections import defaultdict
from pathlib import Path


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def distribution(values):
    values = sorted(v for v in values if v is not None)
    if not values:
        return None
    return {
        "count": len(values),
        "mean": sum(values) / len(values),
        **{f"p{p}": values[math.ceil(len(values) * p / 100) - 1] for p in (50, 95, 99)},
        "max": values[-1],
    }


def error_rate(rows, field, metric):
    scores = [r[field][metric] for r in rows if r.get(field)]
    edits = sum(s["edits"] for s in scores)
    units = sum(s["referenceUnits"] for s in scores)
    return {
        "scored": len(scores),
        "edits": edits,
        "referenceUnits": units,
        "percent": 100 * edits / units if units else None,
    }


def aggregate(rows):
    paired = [r for r in rows if r.get("rawScore") and r.get("personalizedScore")]
    later = [r for r in rows if r["residentRun"]]
    changes = [r for r in paired if r["rawHypothesis"] != r["personalizedHypothesis"]]
    return {
        "attempted": len(rows),
        "failures": sum(r["error"] is not None for r in rows),
        "audioSeconds": sum(r["audioSeconds"] for r in rows),
        "rawWer": error_rate(rows, "rawScore", "wordErrorRate"),
        "memoryWer": error_rate(rows, "personalizedScore", "wordErrorRate"),
        "rawCer": error_rate(rows, "rawScore", "graphemeErrorRate"),
        "memoryCer": error_rate(rows, "personalizedScore", "graphemeErrorRate"),
        "changed": len(changes),
        "improved": sum(
            r["personalizedScore"]["wordErrorRate"]["edits"]
            < r["rawScore"]["wordErrorRate"]["edits"]
            for r in paired
        ),
        "regressed": sum(
            r["personalizedScore"]["wordErrorRate"]["edits"]
            > r["rawScore"]["wordErrorRate"]["edits"]
            for r in paired
        ),
        "timingAllAttempts": {
            "asrMs": distribution([r["asrMs"] for r in rows]),
            "realTimeFactor": distribution([r["realTimeFactor"] for r in rows]),
            "firstPartialMs": distribution([r["firstPartialMs"] for r in rows]),
            "stopToFinalMs": distribution([r["stopToFinalMs"] for r in rows]),
            "personalizationMs": distribution([r["personalizationMs"] for r in rows]),
        },
        "residentAsrMsAllAttempts": distribution([r["asrMs"] for r in later]),
        "partialUpdates": sum(r["partialUpdates"] for r in rows),
        "partialEditDistance": sum(r["partialEditDistance"] for r in rows),
        "changedCases": [
            {
                "id": r["sampleId"],
                "rawWordEdits": r["rawScore"]["wordErrorRate"]["edits"],
                "memoryWordEdits": r["personalizedScore"]["wordErrorRate"]["edits"],
            }
            for r in changes
        ],
        "failureCases": [{"id": r["sampleId"], "error": r["error"]} for r in rows if r["error"]],
    }


def summarize(root, output):
    corpus = root / "corpus"
    audit = json.loads((corpus / "selection-audit.json").read_text())
    cohort = {r["id"]: r["cohort"] for r in audit["selected"]}
    ledger = json.loads((root / "run-ledger.json").read_text())
    expected_jobs = {
        f"{model}-{pack}"
        for model, pack in (
            ("nemotron", "smoke"),
            ("cohere", "smoke"),
            ("nemotron", "svarah"),
            ("cohere", "svarah"),
            ("cohere", "us-control"),
            ("nemotron", "us-control"),
            ("nemotron", "svarah-realtime"),
        )
    }
    if {j["name"] for j in ledger["jobs"]} != expected_jobs or len(ledger["jobs"]) != len(
        expected_jobs
    ):
        raise ValueError("Run ledger is incomplete or contains duplicate/unexpected jobs")
    reports, issues = {}, []
    for job in ledger["jobs"]:
        path = root / "results" / (job["name"] + ".json")
        if job["exitCode"] or not path.exists():
            issues.append(
                {
                    "job": job["name"],
                    "exitCode": job["exitCode"],
                    "missingReport": not path.exists(),
                }
            )
        if not path.exists():
            continue
        if digest(path) != job["reportSha256"]:
            raise ValueError("Native result changed since execution")
        report = json.loads(path.read_text())
        if report["manifestSha256"] != job["manifestSha256"]:
            raise ValueError("Native manifest hash mismatch")
        if report["personalizationSha256"] != ledger["memorySha256"]:
            raise ValueError("Frozen memory differs between native conditions")
        runs = report["runs"]
        if len({r["sampleId"] for r in runs}) != len(runs):
            raise ValueError("Duplicate native result IDs")
        pack = job["name"].split("-", 1)[1]
        samples = json.loads((corpus / f"{pack}.json").read_text())["samples"]
        expected_ids = {s["id"] for s in samples}
        if {r["sampleId"] for r in runs} != expected_ids:
            raise ValueError("Native result IDs do not match every selected recording")
        digit_ids = {s["id"] for s in samples if any(c.isdigit() for c in s["reference"])}
        groups = defaultdict(list)
        for row in runs:
            groups[cohort.get(row["sampleId"], "FLEURS en-US control")].append(row)
        actual_threads = sorted(
            {
                r["execution"].get("asr", {}).get("nativeGraphThreads")
                for r in runs
                if r["execution"].get("asr", {}).get("nativeGraphThreads") is not None
            }
        )
        providers = sorted(
            {r["execution"].get("asr", {}).get("initializedProvider", "unreported") for r in runs}
        )
        log = (root / "logs" / (job["name"] + ".log")).read_text()
        rss = re.search(r"(\d+)\s+maximum resident set size", log)
        reports[job["name"]] = {
            "resultSha256": digest(path),
            "manifestSha256": report["manifestSha256"],
            "memorySha256": report["personalizationSha256"],
            "environment": report["environment"],
            "actualNativeGraphThreads": actual_threads,
            "initializedProviders": providers,
            "modelVersions": report["modelVersions"],
            "artifactProvenance": report["artifactProvenance"],
            "peakRssBytes": int(rss[1]) if rss else None,
            "supplyAtStart": job["supplyAtStart"].splitlines()[0],
            "wallSeconds": job["wallSeconds"],
            "totals": aggregate(runs),
            "cohorts": {k: aggregate(v) for k, v in sorted(groups.items())},
            "referenceDigitDiagnostic": {
                "containsDigits": aggregate([r for r in runs if r["sampleId"] in digit_ids]),
                "noDigits": aggregate([r for r in runs if r["sampleId"] not in digit_ids]),
                "interpretation": "Post-hoc error localization only. Strict WER is unchanged; spelling out a correct number and omitting digits require different semantic judgments.",
            },
            "caseMetrics": [
                {
                    k: r[k]
                    for k in (
                        "sampleId",
                        "audioSeconds",
                        "residentRun",
                        "asrMs",
                        "firstPartialMs",
                        "stopToFinalMs",
                        "partialUpdates",
                        "partialEditDistance",
                        "error",
                    )
                }
                | {
                    "cohort": cohort.get(r["sampleId"], "FLEURS en-US control"),
                    "wordEdits": r["rawScore"]["wordErrorRate"]["edits"] if r["rawScore"] else None,
                    "referenceWords": r["rawScore"]["wordErrorRate"]["referenceUnits"]
                    if r["rawScore"]
                    else None,
                }
                for r in runs
            ],
        }
    summary = {
        "schemaVersion": 1,
        "date": "2026-09-18",
        "evidenceClass": "fresh local native audio inference; frozen corpus-reference spelling memory; primary-language groups are accent proxies",
        "selection": audit,
        "binarySha256": ledger["binarySha256"],
        "sourceSha256": ledger["sourceSha256"],
        "measuredRunnerSha256": ledger["runnerSha256"],
        "toolingSha256": {
            str(p): digest(p)
            for p in map(
                Path,
                [
                    "scripts/prepare-accent-benchmark.py",
                    "scripts/run-accent-benchmark.py",
                    "scripts/summarize-accent-benchmark.py",
                    "scripts/test-accent-benchmark.py",
                    "scripts/prepare-personalization-benchmark.py",
                ],
            )
        },
        "processFailures": issues,
        "reports": reports,
        "sources": [
            {
                "title": "Official Svarah metadata",
                "url": "https://github.com/AI4Bharat/Svarah#tutorial",
            },
            {
                "title": "Svarah paper, 2023 benchmark provenance",
                "url": "https://www.isca-archive.org/interspeech_2023/javed23_interspeech.pdf",
            },
            {
                "title": "Svarah dataset, CC BY 4.0",
                "url": "https://huggingface.co/datasets/ai4bharat/Svarah",
            },
        ],
        "limitations": [
            "16 recordings per primary-language group; no verified speaker-disjoint or longitudinal split",
            "English recognition by language background; no acoustic accent detector or trained accent adaptation",
            "No generalization to other devices/backends/noise/long speech or global English accents",
            "WER uses existing transcribe-eval normalization; not the word_edits metric in the policy replay",
            "First-partial includes cold setup, stop-to-final follows last paced packet; excludes UI/writing/insertion",
            "Word emission, endpoint, queue drops, continuous thermal/power telemetry not captured",
            "Historical generic environment.threads=8 denotes app worker cap; measured native evidence is authoritative (Nemotron4, Cohere8). Supply snapshots supersede the preparation-time AC note.",
        ],
    }
    stream = json.loads((root / "results/nemotron-svarah-realtime.json").read_text())["runs"]
    offline = {
        r["sampleId"]: r
        for r in json.loads((root / "results/nemotron-svarah.json").read_text())["runs"]
    }
    summary["pairedStreamingVsOffline"] = {
        "streaming": aggregate(stream),
        "sameRecordingsOffline": aggregate([offline[r["sampleId"]] for r in stream]),
        "changedRawText": sum(
            r["rawHypothesis"] != offline[r["sampleId"]]["rawHypothesis"] for r in stream
        ),
    }
    fixture_path, result_path = (
        root / "automatic-episodes.json",
        root / "automatic-results.json",
    )
    if fixture_path.exists() or result_path.exists():
        fixture, result = (
            json.loads(fixture_path.read_text()),
            json.loads(result_path.read_text()),
        )
        if result["fixtureSha256"] != digest(fixture_path):
            raise ValueError("Automatic replay fixture does not match its result")
        n = fixture["source"]["learningEpisodes"]
        heldout_ids = {e["id"] for e in fixture["episodes"][n:]}
        if heldout_ids != set(cohort) or any(
            e["edited"] is not None for e in fixture["episodes"][n:]
        ):
            raise ValueError(
                "Automatic replay changed the accent holdout or supplied hidden feedback"
            )

        def automatic_counts(rows):
            scored = [
                r for r in rows if r["rawErrors"] is not None and r["memoryErrors"] is not None
            ]
            return {
                "attempted": len(rows),
                "scored": len(scored),
                "failureRows": sum(bool(r["failures"]) for r in rows),
                "rawWordEdits": sum(r["rawErrors"] for r in scored),
                "memoryWordEdits": sum(r["memoryErrors"] for r in scored),
                "improved": sum(r["memoryErrors"] < r["rawErrors"] for r in scored),
                "regressed": sum(r["memoryErrors"] > r["rawErrors"] for r in scored),
            }

        modes = []
        for mode in result["modes"]:
            if len(mode["rows"]) != len(fixture["episodes"]) or len(
                {r["id"] for r in mode["rows"]}
            ) != len(mode["rows"]):
                raise ValueError("Incomplete or duplicate automatic replay results")
            rows = [r for r in mode["rows"] if r["id"] in heldout_ids]
            modes.append(
                {
                    **{k: v for k, v in mode.items() if k != "rows"},
                    "heldout": automatic_counts(rows),
                    "cohorts": {
                        label: automatic_counts([r for r in rows if cohort[r["id"]] == label])
                        for label in sorted(set(cohort.values()))
                    },
                }
            )
        replay_sources = [
            *Path("crates/transcribe-core/src").rglob("*.rs"),
            *Path("crates/transcribe-data/src").rglob("*.rs"),
            Path("crates/transcribe-data/examples/automatic_learning_replay.rs"),
            Path("scripts/prepare-automatic-learning-replay.mjs"),
            Path("Cargo.lock"),
        ]
        summary["automaticReplay"] = {
            "calibrationEpisodes": n,
            "heldoutEpisodes": len(heldout_ids),
            "fixtureSha256": digest(fixture_path),
            "resultSha256": digest(result_path),
            "binarySha256": digest(Path("target/release/examples/automatic_learning_replay")),
            "sourceSha256": {str(p): digest(p) for p in sorted(replay_sources)},
            "upstreamInputs": fixture["source"]["inputs"],
            "modes": modes,
            "limitation": "Earlier cached calibration ASR plus this fresh native holdout; corpus references simulate ordinary edits. No personal longitudinal feedback. Word-edit counts use the core helper, not native normalized WER.",
        }
    output.write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n")
    for name, report in reports.items():
        t = report["totals"]
        print(
            name,
            "attempted",
            t["attempted"],
            "failures",
            t["failures"],
            "WER",
            t["rawWer"]["percent"],
            "memory",
            t["memoryWer"]["percent"],
        )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    summarize(args.root, args.output)
