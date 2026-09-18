#!/usr/bin/env python3
"""Publish transcript-free Whisper probe or adapter metrics; retain failures."""

import argparse
import hashlib
import json
import math
from pathlib import Path


def read(path):
    return json.loads(path.read_text())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def distribution(values):
    values = sorted(v for v in values if v is not None)
    if not values:
        return {"n": 0}
    return {
        "n": len(values),
        "p50": values[math.ceil(len(values) * 0.5) - 1],
        "p95": values[math.ceil(len(values) * 0.95) - 1],
        "min": values[0],
        "max": values[-1],
    }


def rates(rows, field="rawScore"):
    scores = [r[field] for r in rows if r.get(field)]
    result = {"scored": len(scores), "unscored": len(rows) - len(scores)}
    for metric in ["wordErrorRate", "graphemeErrorRate"]:
        edits = sum(s[metric]["edits"] for s in scores)
        units = sum(s[metric]["referenceUnits"] for s in scores)
        result[metric] = {
            "edits": edits,
            "referenceUnits": units,
            "rate": edits / units if units else None,
        }
    return result


def failure_stage(row):
    if row.get("error"):
        return "cleanup" if row.get("rawHypothesis") is not None else "asr"
    return "cleanup" if row.get("cleanupError") else None


def exclude_unreferenced_soak_scores(row):
    # The historical frontend soak stitches actual speech but has an empty
    # reference/noise scenario solely to satisfy its old manifest schema.
    # Preserve its execution, text hashes and original report hash, but never
    # publish speech words as noise hallucinations or invent a WER reference.
    if row.get("sampleId") == "continuous-180s" and row.get("replayKind") == "synthetic_soak":
        row["qualityMeasurementExcluded"] = (
            "Unreferenced stitched-speech soak; manifest noise label is invalid for quality scoring"
        )
        for field in [
            "rawNonSpeechScore",
            "cleanedNonSpeechScore",
            "personalizedNonSpeechScore",
        ]:
            row[field] = None


def sanitize(value):
    if isinstance(value, dict):
        return {
            k: sanitize(v)
            for k, v in value.items()
            if k
            not in {
                "rawHypothesis",
                "cleanedHypothesis",
                "personalizedHypothesis",
                "normalizedReference",
                "normalizedHypothesis",
                "reference",
                "writerEvidence",
                "source",
                "command",
                "libDir",
            }
        }
    if isinstance(value, list):
        return [sanitize(v) for v in value]
    if isinstance(value, str):
        return value.replace(str(Path.cwd()), "<workspace>")
    return value


def summarize(root):
    ledger = read(root / "processes.json")
    publication = {
        "schemaVersion": 1,
        "environment": read(root / "environment.json"),
        "processes": sanitize(ledger),
        "reports": {},
        "english": {},
        "sourceHashes": read(root / "source.json"),
        "artifactPins": read(Path("benchmarks/asr/whisper-artifacts.json")),
        "publicationScriptSha256": digest(Path(__file__)),
    }
    publication["nativeProvenance"] = read(root / "native-provenance.json")
    publication["postFreezeSourceChanges"] = read(root / "post-freeze-changes.json")
    publication["corpora"] = {
        path.name: {"manifestSha256": digest(path), **sanitize(read(path))}
        for name in [
            "english-0",
            "english-1",
            "diagnostic",
            "fr",
            "hi",
            "duration-boundary",
            "meeting",
            "english-auto",
        ]
        if (path := root / f"{name}.json").exists()
    }
    if (root / "plan.json").exists():
        plan = read(root / "plan.json")
        completed = {p["name"] for p in ledger}
        publication["plannedCases"] = sum(len(p["sampleIds"]) for p in plan)
        publication["notRun"] = [p for p in plan if p["name"] not in completed]
    if (root / "interference.json").exists():
        publication["attemptStatus"] = read(root / "interference.json")
    reports = {}
    report_processes = list(ledger)
    for extra in ["supplemental-processes.json", "automatic-processes.json"]:
        if (root / extra).exists():
            extra_ledger = read(root / extra)
            publication[extra.removesuffix(".json")] = sanitize(extra_ledger)
            report_processes.extend(
                p for p in extra_ledger if not p["name"].startswith("lifecycle-")
            )
    publication["lifecycle"] = {}
    for path in sorted((root / "runs").glob("lifecycle-*.log")):
        records = []
        for line in path.read_text(errors="replace").splitlines():
            if '"case":"resident-asr-' in line:
                records.append(json.loads(line[line.index("{") :]))
        publication["lifecycle"][path.stem] = {
            "logSha256": digest(path),
            "records": records,
            "completed": any(r.get("passed") and r.get("mixedCycles") == 100 for r in records),
        }
    for process in report_processes:
        name = process["name"]
        path = root / "runs" / f"{name}.json"
        try:
            report = read(path)
        except (OSError, ValueError):
            publication["reports"][name] = {"error": "missing or invalid report"}
            continue
        for i, row in enumerate(report["runs"]):
            row["ordinalWithinProcess"] = i + 1
            row["failureStage"] = failure_stage(row)
            exclude_unreferenced_soak_scores(row)
            for field in ["rawHypothesis", "cleanedHypothesis"]:
                if row.get(field) is not None:
                    row[field + "Sha256"] = hashlib.sha256(row[field].encode()).hexdigest()
        reports[name] = report
        publication["reports"][name] = {
            "reportSha256": digest(path),
            "logSha256": digest(path.with_suffix(".log")),
            **sanitize(report),
        }
    models = (
        [
            model
            for model in ["cohere", "tiny", "small", "turbo"]
            if any(p["name"] == f"english-0-{model}" for p in plan)
        ]
        if (root / "plan.json").exists()
        else ["cohere", "tiny", "small", "turbo"]
    )
    for model in models:
        relevant = [reports.get(f"english-{i}-{model}", {"runs": []}) for i in range(2)]
        rows = [r for report in relevant for r in report["runs"]]
        # Production evidence reports actual requests on this resident instance.
        # Probe reports predate that contract and use their explicit process order.
        warm = [
            r
            for r in rows
            if r.get("execution", {})
            .get("asr", {})
            .get("metrics", {})
            .get("requestOrdinal", r["ordinalWithinProcess"])
            > 1
            and failure_stage(r) != "asr"
        ]
        natural = [
            r
            for r in rows
            if r["sampleId"].startswith("en_us-") and r["sampleId"].endswith("--clean")
        ]
        processes = [
            p
            for p in ledger
            if p["name"].startswith("english-") and p["name"].endswith("-" + model)
        ]
        publication["english"][model] = {
            "latencyUsable": len(processes) == 2
            and len(rows) == 30
            and all(p["returnCode"] == 0 for p in processes)
            and not any(p.get("competingWork") for p in processes)
            and not any(failure_stage(r) == "asr" for r in rows),
            "expected": 30,
            "present": len(rows),
            "natural": rates(natural),
            "allSpeech": rates(rows),
            "cleaned": rates(rows, "cleanedScore"),
            "errorRows": sum(bool(r.get("error")) for r in rows),
            "asrErrorRows": sum(failure_stage(r) == "asr" for r in rows),
            "cleanupErrorRows": sum(failure_stage(r) == "cleanup" for r in rows),
            "residentAsrMs": distribution([r.get("asrMs") for r in warm]),
            "residentAsrAndWriterMs": distribution(
                [
                    r["asrMs"] + r["cleanupMs"]
                    for r in warm
                    if r.get("asrMs") is not None
                    and r.get("cleanupMs") is not None
                    and failure_stage(r) is None
                ]
            ),
            "peakRssBytes": max((p.get("peakRssBytes") or 0 for p in processes), default=0),
            "verificationMs": [r.get("artifactVerificationMs") for r in relevant],
            "modelLoadMs": [r.get("modelLoadMs") for r in relevant],
            "coldAsrMs": [r.get("asrMs") for r in rows if r["ordinalWithinProcess"] == 1],
            "residentLoads": [
                {
                    "verificationMs": report["runs"][0]["execution"]["asr"].get("verificationUs", 0)
                    / 1000,
                    "modelLoadMs": report["runs"][0]["execution"]["asr"].get("modelLoadUs", 0)
                    / 1000,
                }
                for report in relevant
                if report["runs"] and report["runs"][0].get("execution", {}).get("asr")
            ],
        }
    # Native provenance and environment can also contain workspace paths.
    return sanitize(publication)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--repeat-root", type=Path)
    parser.add_argument("--excluded-root", type=Path)
    args = parser.parse_args()
    publication = summarize(args.root)
    if args.excluded_root:
        publication["excludedAttempt"] = {
            "status": read(args.excluded_root / "interference.json"),
            "completedProcessLedger": sanitize(read(args.excluded_root / "processes.json")),
            "reports": [
                {
                    "name": path.name,
                    "sha256": digest(path),
                    "caseRecords": len(read(path).get("runs", [])),
                }
                for path in sorted((args.excluded_root / "runs").glob("*.json"))
            ],
        }
    if args.repeat_root:
        repetition = summarize(args.repeat_root)
        publication["englishRepeat"] = repetition
        differences = []
        for name, report in repetition["reports"].items():
            prior = {r["sampleId"]: r for r in publication["reports"].get(name, {}).get("runs", [])}
            current = {r["sampleId"]: r for r in report.get("runs", [])}
            for sample_id in sorted(prior.keys() | current.keys()):
                a, b = prior.get(sample_id), current.get(sample_id)
                fields = [
                    "rawHypothesisSha256",
                    "cleanedHypothesisSha256",
                    "rawScore",
                    "cleanedScore",
                    "error",
                    "cleanupError",
                ]
                changed = [k for k in fields if a.get(k) != b.get(k)] if a and b else ["missing"]
                if changed:
                    differences.append({"report": name, "sampleId": sample_id, "fields": changed})
        publication["englishRepeatDifferences"] = differences
    text = json.dumps(publication, indent=2) + "\n"
    if "/Users/" in text:
        raise ValueError("Unexpected personal machine path in publication")
    args.output.write_text(text)
    print(
        json.dumps(
            {
                "initial": publication["english"],
                "repeat": publication.get("englishRepeat", {}).get("english"),
                "differences": publication.get("englishRepeatDifferences"),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
