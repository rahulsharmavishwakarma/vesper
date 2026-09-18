"""Content-free, failure-preserving analysis of the frozen denoising experiment."""

import importlib.util
import json
from collections import defaultdict
from pathlib import Path

import numpy as np

spec = importlib.util.spec_from_file_location(
    "frontend_summary", Path(__file__).with_name("summarize-frontend-integration.py")
)
common = importlib.util.module_from_spec(spec)
spec.loader.exec_module(common)
ROOT = Path(".cache/benchmarks/results/denoising-followup-2026-09-19")
OUTPUT = ROOT / "debug-summary.json"
PROFILES = ["raw", "dpdfnet4-full", "dpdfnet4-mix50", "dpdfnet8-full", "dpdfnet8-mix50"]
DIAGNOSTICS = [
    f"{m}-{g}" for m in ["dpdfnet4", "dpdfnet8"] for g in ["pregain-restore", "postgain"]
]
read, distribution = common.read, common.distribution


def sanitized_row(row, process, source):
    streaming = process["phase"] == "streaming"
    natural = source.get("replayKind") == "natural"
    result = {
        "job": process["name"],
        "model": process["model"],
        "profile": process["profile"],
        "phase": process["phase"],
        "sampleId": row["sampleId"],
        "condition": source.get("condition"),
        "evaluationRole": source.get("evaluationRole"),
        "replayKind": source.get("replayKind"),
        "nonSpeechReference": source.get("reference") == "",
        "baseId": source.get("baseId"),
        "audioSeconds": row["audioSeconds"],
        "resident": row.get("residentRun"),
        "asrFailed": row["rawHypothesis"] is None,
        "writerFailed": not streaming
        and row["rawHypothesis"] is not None
        and row["error"] is not None,
        "failureKind": common.failure_kind(row),
        "stageEvidence": common.stage_evidence(row["execution"]),
        "rawSha256": common.hash_text(row["rawHypothesis"]),
        "cleanedSha256": common.hash_text(row.get("cleanedHypothesis")),
        "latencyUsable": process["latencyUsable"],
        "rawNonSpeech": row.get("rawNonSpeechScore"),
    }
    for stage in ["raw", "cleaned"]:
        score = row.get(stage + "Score")
        result[stage + "Score"] = (
            {k: score[k] for k in ["wordErrorRate", "graphemeErrorRate"]}
            if natural and score
            else None
        )
    if streaming:
        result.update(
            {
                k: row[k]
                for k in [
                    "firstPartialMs",
                    "partialUpdates",
                    "changedUpdates",
                    "observerRetainedSnapshots",
                    "timings",
                ]
            }
        )
    else:
        result.update({k: row[k] for k in ["asrMs", "cleanupMs", "realTimeFactor"]})
    return result


def aggregate(cases):
    result = {
        "attempted": len(cases),
        "asrFailures": sum(r["asrFailed"] for r in cases),
        "writerFailures": sum(r["writerFailed"] for r in cases),
    }
    for stage in ["raw", "cleaned", "delivered"]:
        scores = [
            (r["cleanedScore"] or r["rawScore"]) if stage == "delivered" else r[stage + "Score"]
            for r in cases
        ]
        scores = [r for r in scores if r]
        result[stage + "Scored"] = len(scores)
        for metric, label in [("wordErrorRate", "Wer"), ("graphemeErrorRate", "Cer")]:
            units = sum(r[metric]["referenceUnits"] for r in scores)
            edits = sum(r[metric]["edits"] for r in scores)
            result[stage + label] = edits / units if units else None
            result[stage + label + "ReferenceUnits"] = units
            result[stage + label + "Edits"] = edits
    noise = [r["rawNonSpeech"] for r in cases if r["rawNonSpeech"] is not None]
    result.update(
        noiseAttempted=sum(r["nonSpeechReference"] for r in cases),
        noiseAsrFailures=sum(r["nonSpeechReference"] and r["asrFailed"] for r in cases),
        noiseCases=len(noise),
        noiseFalseTranscripts=sum(r["falseTranscript"] for r in noise),
        noiseInsertedWords=sum(r["insertedWords"] for r in noise),
    )
    for stage in ["asrMs", "cleanupMs"]:
        result["resident" + stage[0].upper() + stage[1:]] = distribution(
            [
                r[stage]
                for r in cases
                if r["resident"] and not r["asrFailed"] and r[stage] is not None
            ]
        )
    return result


def paired(rows):
    groups = defaultdict(dict)
    for r in rows:
        if (
            r["phase"] == "quality"
            and r["replayKind"] == "natural"
            and r["condition"] in ["clean", "quiet30", "noise0"]
        ):
            groups[(r["model"], r["condition"], r["evaluationRole"], r["profile"])][
                r["sampleId"]
            ] = r
    comparisons = [("raw", p) for p in PROFILES[1:] + DIAGNOSTICS]
    comparisons += [
        (f"{m}-full", f"{m}-{p}")
        for m in ["dpdfnet4", "dpdfnet8"]
        for p in ["mix50", "pregain-restore", "postgain"]
    ]
    rng, output = np.random.default_rng(91926), []
    for model, condition, role in sorted({k[:3] for k in groups}):
        for baseline, candidate in comparisons:
            a = groups.get((model, condition, role, baseline), {})
            b = groups.get((model, condition, role, candidate), {})
            if not a or not b:
                continue
            pairs = [
                (a[k], b[k])
                for k in sorted(a.keys() & b.keys())
                if a[k]["rawScore"] and b[k]["rawScore"]
            ]
            item = {
                "model": model,
                "condition": condition,
                "evaluationRole": role,
                "baseline": baseline,
                "candidate": candidate,
                "pairedScored": len(pairs),
                "unpairedOrFailed": len(a.keys() | b.keys()) - len(pairs),
                "baselineAsrFailures": sum(r["asrFailed"] for r in a.values()),
                "candidateAsrFailures": sum(r["asrFailed"] for r in b.values()),
            }
            if pairs:
                counts = np.array(
                    [
                        [
                            x["rawScore"]["wordErrorRate"]["edits"],
                            y["rawScore"]["wordErrorRate"]["edits"],
                            x["rawScore"]["wordErrorRate"]["referenceUnits"],
                        ]
                        for x, y in pairs
                    ],
                    dtype=float,
                )
                for x, y in pairs:
                    assert (
                        x["rawScore"]["wordErrorRate"]["referenceUnits"]
                        == y["rawScore"]["wordErrorRate"]["referenceUnits"]
                    )
                samples = counts[rng.integers(0, len(pairs), size=(5000, len(pairs)))].sum(axis=1)
                differences = 100 * (samples[:, 1] - samples[:, 0]) / samples[:, 2]
                item.update(
                    wordErrorDifferencePoints=float(
                        100 * (counts[:, 1].sum() - counts[:, 0].sum()) / counts[:, 2].sum()
                    ),
                    descriptive95PercentIntervalPoints=[
                        float(x) for x in np.percentile(differences, [2.5, 97.5])
                    ],
                    identicalTranscriptHashes=sum(
                        x["rawSha256"] == y["rawSha256"] for x, y in pairs
                    ),
                )
            output.append(item)
    return output


def main():
    sources = {
        phase: {r["id"]: r for r in read(ROOT / f"{phase}.json")["samples"]}
        for phase in ["quality", "streaming"]
    }
    rows, processes, provenance = [], [], {}
    for phase in ["contracts", "prepare", "quality", "streaming"]:
        ledger = ROOT / f"processes-{phase}.json"
        for process in read(ledger) if ledger.exists() else []:
            processes.append({k: v for k, v in process.items() if k != "command"})
            result_path = ROOT / "runs" / (process["name"] + ".json")
            if phase not in sources or not result_path.exists():
                continue
            try:
                report = read(result_path)
            except (OSError, ValueError):
                continue  # The process ledger preserves parse failure and missing expected IDs.
            provenance[process["name"]] = {
                k: report[k]
                for k in ["modelVersions", "artifactProvenance", "frontendModelVersions"]
                if k in report
            }
            for ordinal, row in enumerate(report["runs"], start=1):
                rows.append(
                    sanitized_row(row, process, sources[phase][row["sampleId"]])
                    | {"processRecordingOrdinal": ordinal}
                )
    grouped = defaultdict(list)
    for row in rows:
        if row["phase"] == "quality":
            grouped[
                tuple(row[k] for k in ["model", "profile", "condition", "evaluationRole"])
            ].append(row)
    aggregates = [
        dict(zip(["model", "profile", "condition", "evaluationRole"], key, strict=True))
        | aggregate(cases)
        for key, cases in sorted(grouped.items())
    ]
    preprocessing = []
    for row in read(ROOT / "preprocessing.json")["rows"]:
        preprocessing.append(
            {k: v for k, v in row.items() if k != "error"} | {"failed": "error" in row}
        )
    frontends = [r["metrics"]["frontend"] for r in preprocessing if "metrics" in r]
    frontends += [
        r["gainDiagnostic"]["metrics"]["frontend"] for r in preprocessing if "gainDiagnostic" in r
    ]
    frontends += [
        r["timings"]["frontend"]
        for r in rows
        if r["phase"] == "streaming" and r["timings"].get("frontend")
    ]
    expected_jobs = (
        {f"quality-{m}-{p}" for m in ["cohere", "small", "turbo"] for p in PROFILES + DIAGNOSTICS}
        | {f"streaming-{p}" for p in PROFILES}
        | {"contracts", "prepare"}
    )
    checks = {
        "missingJobs": sorted(expected_jobs - {p["name"] for p in processes}),
        "missingRows": sum(len(p["missingIds"]) for p in processes),
        "failedProcesses": sum(p["returnCode"] != 0 for p in processes),
        "nativeExceptionLogs": sum(p["nativeExceptionLog"] for p in processes),
        "preprocessingFailures": sum(r["failed"] for r in preprocessing),
        "asrFailures": sum(r["asrFailed"] for r in rows),
        "writerFailures": sum(r["writerFailed"] for r in rows),
        "nonFinalized": sum(not r["finalized"] for r in frontends),
        "lengthMismatch": sum(r["inputSamples16k"] != r["outputSamples16k"] for r in frontends),
        "dispatchHistogramMismatch": sum(
            sum(r["dispatchHistogram"]) != r["dispatches"] for r in frontends
        ),
        "invalidVadIntervals": sum(
            not 0 <= s["startSample"] < s["endSample"] <= r["inputSamples16k"]
            for r in frontends
            for s in r["speechIntervals"]
        ),
    }
    checks["matrixComplete"] = (
        not (checks["missingJobs"] or checks["missingRows"]) and len(rows) == 2153
    )
    streaming_groups = [
        [r for r in rows if r["phase"] == "streaming" and r["profile"] == profile]
        for profile in PROFILES
    ]
    checks["streamingResetPairs"] = sum(len(group) == 7 for group in streaming_groups)
    checks["streamingResetPassed"] = sum(
        len(group) == 7
        and not group[0]["asrFailed"]
        and not group[-1]["asrFailed"]
        and group[0]["rawSha256"] == group[-1]["rawSha256"]
        for group in streaming_groups
    )
    checks["omittedVadIntervals"] = sum(r["omittedIntervals"] for r in frontends)
    prior_path = Path(
        ".cache/benchmarks/results/audio-frontend-integration-2026-09-18/debug-summary.json"
    )
    prior_parity = None
    if prior_path.exists():
        mapping = {"raw": "raw", "dpdfnet4": "dpdfnet4-full", "dpdfnet8": "dpdfnet8-full"}
        previous = {
            (r["model"], mapping[r["profile"]], r["sampleId"]): r
            for r in read(prior_path)["runs"]
            if r["phase"] == "quality" and r["profile"] in mapping
        }
        pairs = [
            (previous[(r["model"], r["profile"], r["sampleId"])], r)
            for r in rows
            if r["phase"] == "quality" and (r["model"], r["profile"], r["sampleId"]) in previous
        ]
        prior_parity = {"compared": len(pairs)} | {
            key + "Identical": sum(a[key] == b[key] for a, b in pairs)
            for key in ["rawSha256", "cleanedSha256", "failureKind"]
        }
    output = {
        "schemaVersion": 1,
        "environment": read(ROOT / "environment.json"),
        "runtimeEnvironment": read(ROOT / "runtime-env.json"),
        "sourceHashes": read(ROOT / "source-hashes.json"),
        "analysisSourceHashes": {
            p: common.digest(Path(p))
            for p in [
                "scripts/summarize-denoising-followup.py",
                "scripts/summarize-frontend-integration.py",
                "scripts/test_denoising_followup_report.py",
                "scripts/verify-denoising-followup.py",
            ]
        },
        "harnessHashes": read(ROOT / "harness-hashes.json"),
        "phaseHarnessHashes": {p.stem: read(p) for p in sorted(ROOT.glob("harness-hashes-*.json"))},
        "phaseExecutionPins": {p.stem: read(p) for p in sorted(ROOT.glob("execution-pins-*.json"))},
        "protocol": read(ROOT / "protocol.json"),
        "nativeContracts": read(ROOT / "contracts.json"),
        "archiveVerification": read(ROOT / "archive-verification.json")
        if (ROOT / "archive-verification.json").exists()
        else None,
        "verification": read(ROOT / "verification.json")
        if (ROOT / "verification.json").exists()
        else None,
        "validationChecks": read(ROOT / "validation-checks.json")
        if (ROOT / "validation-checks.json").exists()
        else None,
        "meaningReview": read(ROOT / "meaning-review.json")
        if (ROOT / "meaning-review.json").exists()
        else None,
        "frontendArtifacts": [
            a
            for a in read(
                ROOT / "source-snapshot/models/artifacts.json"
                if (ROOT / "source-snapshot/models/artifacts.json").exists()
                else Path("models/artifacts.json")
            )
            if a["role"] == "audio_frontend"
        ],
        "modelProvenance": provenance,
        "processes": processes,
        "preprocessing": preprocessing,
        "runs": rows,
        "aggregates": aggregates,
        "paired": paired(rows),
        "checks": checks,
        "priorObservedParity": prior_parity,
        "caveats": [
            "Accuracy aggregates exclude failed ASR and all synthetic controls; attempted/scored/failure counts are explicit",
            "Descriptive intervals are unadjusted, 5000 paired recording resamples, seed91926; not speaker-disjoint inference",
            "Completed ASR and writer timing excludes separate cached preprocessing; no end-to-end acceleration claim",
            "Streaming probe enhances on the feeder before ASR enqueue; shipped frontend is inside the worker, so scheduling/latency are not production-placement qualification",
            "Streaming Stop starts at nominal arrival of last audio packet and includes delivery backlog; feedFinishedToFinalMs is separate",
            "First partial includes setup; changed updates are not word-level stability; observer retains one text snapshot",
            "Latency usable only within recorded power/contention envelope; no physical microphone, target edge, watts or thermal-trace qualification",
            "Known gain reversal is diagnostic, not deployable AGC; fresh file IDs do not establish unseen speakers or training data",
        ],
    }
    encoded = json.dumps(output, indent=2) + "\n"
    for forbidden in [
        "normalizedReference",
        "normalizedHypothesis",
        "rawHypothesis",
        "cleanedHypothesis",
        "personalizedHypothesis",
        "/Users/",
    ]:
        assert forbidden not in encoded, forbidden
    OUTPUT.write_text(encoded)
    print(f"Summarized {len(rows)} rows and {len(processes)} processes; {checks}")


if __name__ == "__main__":
    main()
