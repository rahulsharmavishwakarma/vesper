#!/usr/bin/env python3
"""Summarize completed development reports, retaining failed/missing cases.

Does not copy audio or transcript text. WER is split by language and replay kind;
reset repeats remain diagnostics rather than independent accuracy observations.
"""

import argparse
import hashlib
import json
import math
import re
from pathlib import Path


def quantiles(values):
    values = sorted(values)
    if not values:
        return None
    return {
        name: values[max(0, math.ceil(q * len(values)) - 1)]
        for name, q in [("p50", 0.5), ("p95", 0.95), ("max", 1)]
    }


def memory(path):
    text = path.read_text(errors="replace") if path.exists() else ""
    result = re.search(r"(\d+)\s+maximum resident set size", text)
    return int(result[1]) if result else None


def summarize(path):
    report = json.loads(path.read_text())
    rows = report["runs"]
    expected = report["manifest"]["samples"]
    missing = sorted({s["id"] for s in expected} - {r["id"] for r in rows})
    good = [r for r in rows if not r.get("error")]
    unique = [r for r in rows if not r["id"].endswith("-reset-repeat")]
    result = {
        "report": path.name,
        "reportSha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "sourceSha256": report.get("sourceSha256"),
        "executableSha256": report.get("executableSha256"),
        "model": report.get("frontend", report.get("model")),
        "asr": report.get("asr", "none"),
        "delivery": report.get("delivery", "single-window"),
        "condition": report.get("condition", "clean"),
        "manifestSha256": report["manifestSha256"],
        "expectedCases": len(expected),
        "recordedCases": len(rows),
        "missingCases": missing,
        "failures": [{"id": r["id"], "error": r["error"]} for r in rows if r.get("error")],
        "cleanupFailures": [
            {"id": r["id"], "error": r["cleanupError"]} for r in rows if r.get("cleanupError")
        ],
        "peakProcessRssBytes": memory(path.with_suffix(".log")),
        "setupMs": report.get("artifactVerificationAndLoadMs", report.get("loadMs")),
        "setupDefinition": "native: artifact verification+create; Python: runtime import+create; not comparable pure load timers",
        "frontendGolden": report.get("frontendGolden"),
    }
    model = result["model"]
    result["releaseEligibility"] = (
        "recent-checkpoint"
        if model in {"silero-v6.2", "dpdfnet4", "dpdfnet8", "uhm-raw", "uhm-normalized"}
        else "recent-export-of-older-checkpoint"
        if model == "ear"
        else "historical-baseline"
    )
    frontend = [r for r in good if "frontend" in r]
    if frontend:
        seconds = sum(r["audioSeconds"] for r in frontend)
        compute = [r["frontend"]["computeMs"] for r in frontend]
        calls = [v for r in frontend for v in r["frontend"].get("chunkCallMs", [])]
        result["frontendTiming"] = {
            "successfulInputSeconds": seconds,
            "computeMs": sum(compute),
            "computeRtf": sum(compute) / seconds / 1000,
            "perRecordingComputeMs": quantiles(compute),
            "chunkCallMs": quantiles(calls),
            "flushMs": quantiles(
                [r["frontend"]["flushMs"] for r in frontend if "flushMs" in r["frontend"]]
            ),
            "maximumDeliveryLatenessMs": max(
                r["frontend"].get("maximumDeliveryLatenessMs", 0) for r in frontend
            ),
        }
    result["stageTimingsMs"] = {
        key: quantiles([r[key] for r in rows if key in r])
        for key in ["asrMs", "cleanupMs", "preprocessMs", "inferenceMs"]
    }
    lanes = {}
    for row in unique:
        for score_key in ["rawScore", "cleanedScore"]:
            if score_key not in row:
                continue
            key = "/".join([row["language"], row["replayKind"], score_key])
            lane = lanes.setdefault(
                key,
                {
                    "cases": 0,
                    "wordEdits": 0,
                    "referenceWords": 0,
                    "graphemeEdits": 0,
                    "referenceGraphemes": 0,
                },
            )
            lane["cases"] += 1
            lane["wordEdits"] += row[score_key]["wordErrorRate"]["edits"]
            lane["referenceWords"] += row[score_key]["wordErrorRate"]["referenceUnits"]
            lane["graphemeEdits"] += row[score_key]["graphemeErrorRate"]["edits"]
            lane["referenceGraphemes"] += row[score_key]["graphemeErrorRate"]["referenceUnits"]
    for lane in lanes.values():
        lane["wer"] = lane["wordEdits"] / lane["referenceWords"]
        lane["cer"] = lane["graphemeEdits"] / lane["referenceGraphemes"]
    result["accuracyLanes"] = lanes
    noise = [r for r in unique if "nonSpeechScore" in r]
    result["nonSpeech"] = {
        "scoredCases": len(noise),
        "falseTranscripts": sum(r["nonSpeechScore"]["falseTranscript"] for r in noise),
        "insertedWords": sum(r["nonSpeechScore"]["insertedWords"] for r in noise),
        "seconds": sum(r["audioSeconds"] for r in noise),
    }
    result["cases"] = [
        {
            key: value
            for key, value in r.items()
            if key
            in {
                "id",
                "error",
                "language",
                "replayKind",
                "audioSeconds",
                "rawScore",
                "cleanedScore",
                "nonSpeechScore",
                "cleanedNonSpeechScore",
                "languagePrediction",
                "isReliable",
                "correct",
                "spans",
                "preprocessMs",
                "inferenceMs",
                "validFrames",
                "uncoveredTailSeconds",
                "inputPcmSha256",
                "outputPcmSha256",
                "inputSamples",
                "outputSamples",
            }
        }
        for r in rows
    ]
    # Score objects include normalized transcript strings; publish counts only.
    for row in result["cases"]:
        for key in ["rawScore", "cleanedScore"]:
            if key in row:
                row[key] = {
                    k: v for k, v in row[key].items() if k in {"wordErrorRate", "graphemeErrorRate"}
                }
    for row, source in zip(result["cases"], rows):
        front = source.get("frontend", {})
        if "segments" in front:
            row["segments"] = front["segments"]
        if "events" in front:
            row["topEvents"] = front["events"][:5]
            row["speechProbability"] = next(
                e["probability"] for e in front["events"] if e["id"] == 0
            )
        if "probabilities" in source:
            row["topLanguages"] = sorted(source["probabilities"].items(), key=lambda p: -p[1])[:5]
    first, last = rows[0], rows[-1]
    if last["id"].endswith("-reset-repeat") and not first.get("error") and not last.get("error"):

        def fingerprint(row):
            front = row.get("frontend", {})
            if "events" in front:
                return front["events"]
            if result["model"].startswith("silero"):
                return front["segments"]
            return row.get(
                "probabilities",
                row.get("probabilitiesSha256", row.get("outputPcmSha256")),
            )

        result["resetExactOutputEquality"] = fingerprint(first) == fingerprint(last)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    paths = sorted(
        set(args.root.glob("*-v3.json"))
        | set(args.root.glob("*-v4.json"))
        | set(args.root.glob("*-clean-v2.json"))
    )
    summaries = [summarize(path) for path in paths if "runs" in json.loads(path.read_text())]
    legacy = [
        summarize(path)
        for path in sorted(args.root.glob("*clean-frontend.json"))
        if json.loads(path.read_text())["frontend"] in {"gtcrn", "dpdfnet4", "dpdfnet8"}
    ]
    legacy.extend(
        summarize(args.root / name) for name in ["uhm-raw-clean.json", "uhm-normalized-clean.json"]
    )
    baseline = json.loads((args.root / "raw-offline-clean-cohere-v3.json").read_text())
    result = {
        "date": "2026-09-16",
        "schemaVersion": 1,
        "environment": json.loads((args.root / "environment.json").read_text()),
        "asrConfiguration": baseline["runs"][0]["modelVersions"],
        "scope": "Local screening only; timings within envelope, no pooling incompatible languages/corpora; failed and reset cases retained",
        "modelReleaseWindow": ["2025-09-16", "2026-09-16"],
        "releaseEvidence": "audio-frontend-recency.json",
        "experiments": summaries,
        "supersededContractOrFixtureFailures": legacy,
    }
    args.output.write_text(json.dumps(result, indent=2, allow_nan=False) + "\n")
    print(f"Wrote {len(summaries)} experiments and {len(legacy)} preserved first-pass reports")


if __name__ == "__main__":
    main()
