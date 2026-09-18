"""Publish content-free frontend integration evidence; no transcripts or paths."""

import hashlib
import json
from collections import defaultdict
from pathlib import Path

import numpy as np

ROOT = Path(".cache/benchmarks/results/frontend-integration-2026-09-18")
OUTPUT = Path(".cache/benchmarks/results/audio-frontend-integration-2026-09-18/debug-summary.json")


def read(path):
    return json.loads(path.read_text())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def distribution(values):
    if not values:
        return None
    return {
        k: float(np.percentile(values, p)) for k, p in [("p50", 50), ("p95", 95), ("p99", 99)]
    } | {"max": float(max(values)), "count": len(values)}


def hash_text(text):
    return hashlib.sha256(text.encode()).hexdigest() if text is not None else None


def failure_kind(row):
    error = row.get("error")
    if error is None:
        return None
    if "duration_token_limit" in error:
        return "asr_duration_token_limit"
    if "model_token_limit" in error:
        return "asr_model_token_limit"
    if "fidelity check" in error:
        return "writer_fidelity_rejection"
    if "writing model reached its output limit" in error:
        return "writer_output_limit"
    return "asr_error" if row["rawHypothesis"] is None else "writer_error"


def stage_evidence(execution):
    asr = execution.get("asr", {})
    writer = execution.get("rewriter", {})
    return {
        "asr": {
            k: asr[k]
            for k in (
                "initializedProvider",
                "nativeGraphThreads",
                "residentLoadOrdinal",
                "verificationUs",
                "modelLoadUs",
                "metrics",
                "streamingPolicy",
            )
            if k in asr
        },
        "writer": {"passes": writer.get("metrics", {}).get("passes", [])},
    }


def main():
    quality = {r["id"]: r for r in read(ROOT / "quality.json")["samples"]}
    processes = []
    rows = []
    model_provenance = {}
    for phase in ["streaming", "quality", "soak"]:
        ledger = ROOT / f"processes-{phase}.json"
        if not ledger.exists():
            continue
        for process in read(ledger):
            # Keep sanitized basename-only interference evidence and exact hashes.
            entry = {k: v for k, v in process.items() if k != "command"}
            entry["phase"] = phase
            processes.append(entry)
            path = ROOT / "runs" / (process["name"] + ".json")
            if not path.exists():
                continue
            report = read(path)
            model_provenance[process["name"]] = {
                k: report[k] for k in ("modelVersions", "artifactProvenance")
            }
            for r in report["runs"]:
                source = quality.get(r["sampleId"], {})
                front = r["execution"].get("audioFrontend", {})
                metrics = front.get("metrics")
                item = {
                    "job": process["name"],
                    "model": process["model"],
                    "profile": process["profile"],
                    "phase": phase,
                    "sampleId": r["sampleId"],
                    "condition": source.get("condition"),
                    "evaluationRole": source.get("evaluationRole"),
                    "replayKind": r["replayKind"],
                    "resident": r["residentRun"],
                    "audioSeconds": r["audioSeconds"],
                    "asrFailed": r["rawHypothesis"] is None,
                    "writerFailed": r["rawHypothesis"] is not None and r["error"] is not None,
                    "errorPresent": r["error"] is not None,
                    "failureKind": failure_kind(r),
                    "stageEvidence": stage_evidence(r["execution"]),
                    "frontendFallback": front.get("originalAudioFallback", False),
                    "rawSha256": hash_text(r["rawHypothesis"]),
                    "cleanedSha256": hash_text(r["cleanedHypothesis"]),
                    "asrMs": r["asrMs"],
                    "cleanupMs": r["cleanupMs"],
                    "firstPartialMs": r["firstPartialMs"],
                    "stopToFinalMs": r["stopToFinalMs"],
                    "partialUpdates": r["partialUpdates"],
                    "partialEditDistance": r["partialEditDistance"],
                    "latencyUsable": process["latencyUsable"],
                    "mode": r["mode"],
                    "pace": r["pace"],
                    "chunkMs": r["chunkMs"],
                    "realTimeFactor": r["realTimeFactor"],
                    "frontend": metrics,
                    "rawNonSpeech": r.get("rawNonSpeechScore"),
                    "cleanedNonSpeech": r.get("cleanedNonSpeechScore"),
                }
                for stage in ["raw", "cleaned"]:
                    score = r.get(stage + "Score")
                    item[stage + "Score"] = (
                        {
                            k: v
                            for k, v in score.items()
                            if k in ("wordErrorRate", "graphemeErrorRate")
                        }
                        if score and r["replayKind"] != "synthetic_soak"
                        else None
                    )
                rows.append(item)
    groups = defaultdict(list)
    for r in rows:
        if r["phase"] == "quality":
            groups[(r["model"], r["profile"], r["condition"], r["evaluationRole"])].append(r)
    aggregates = []
    for (model, profile, condition, role), cases in sorted(groups.items()):
        scores = [r["rawScore"]["wordErrorRate"] for r in cases if r["rawScore"]]
        words = sum(r["referenceUnits"] for r in scores)
        edits = sum(r["edits"] for r in scores)
        cleaned = [r["cleanedScore"]["wordErrorRate"] for r in cases if r["cleanedScore"]]
        cleaned_words = sum(r["referenceUnits"] for r in cleaned)
        delivered = [
            (r["cleanedScore"] or r["rawScore"])["wordErrorRate"]
            for r in cases
            if r["cleanedScore"] or r["rawScore"]
        ]
        delivered_words = sum(r["referenceUnits"] for r in delivered)
        noise = [r["rawNonSpeech"] for r in cases if r["rawNonSpeech"] is not None]
        aggregates.append(
            {
                "model": model,
                "profile": profile,
                "condition": condition,
                "evaluationRole": role,
                "attempted": len(cases),
                "asrFailures": sum(r["asrFailed"] for r in cases),
                "writerFailures": sum(r["writerFailed"] for r in cases),
                "frontendFallbacks": sum(r["frontendFallback"] for r in cases),
                "scoredSpeech": len(scores),
                "referenceWords": words,
                "wordEdits": edits,
                "wer": edits / words if words else None,
                "cleanedScoredSpeech": len(cleaned),
                "cleanedWer": sum(r["edits"] for r in cleaned) / cleaned_words
                if cleaned_words
                else None,
                "deliveredScoredSpeech": len(delivered),
                "deliveredWer": sum(r["edits"] for r in delivered) / delivered_words
                if delivered_words
                else None,
                "noiseCases": len(noise),
                "noiseFalseTranscripts": sum(r["falseTranscript"] for r in noise),
                "noiseInsertedWords": sum(r["insertedWords"] for r in noise),
                "vadObservedCases": sum(r["frontend"] is not None for r in cases),
                "vadSpeechDetected": sum(
                    bool(r["frontend"] and r["frontend"]["speechIntervals"]) for r in cases
                ),
                "residentAsrMs": distribution(
                    [r["asrMs"] for r in cases if r["resident"] and not r["asrFailed"]]
                ),
                "uncontendedResidentAsrMs": distribution(
                    [
                        r["asrMs"]
                        for r in cases
                        if r["resident"] and not r["asrFailed"] and r["latencyUsable"]
                    ]
                ),
            }
        )
    paired = []
    rng = np.random.default_rng(91819)
    for model in ["cohere", "small", "turbo"]:
        for profile in ["vad", "dpdfnet4", "dpdfnet8"]:
            for condition in ["clean", "quiet30", "noise0"]:
                a = {
                    r["sampleId"]: r
                    for r in rows
                    if r["phase"] == "quality"
                    and r["model"] == model
                    and r["profile"] == "raw"
                    and r["condition"] == condition
                }
                b = {
                    r["sampleId"]: r
                    for r in rows
                    if r["phase"] == "quality"
                    and r["model"] == model
                    and r["profile"] == profile
                    and r["condition"] == condition
                }
                pairs = [
                    (a[k], b[k])
                    for k in sorted(a.keys() & b.keys())
                    if a[k]["rawScore"] and b[k]["rawScore"]
                ]
                if not pairs:
                    continue
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
                indices = rng.integers(0, len(pairs), size=(5000, len(pairs)))
                resampled = counts[indices].sum(axis=1)
                differences = (resampled[:, 1] - resampled[:, 0]) / resampled[:, 2] * 100
                paired.append(
                    {
                        "model": model,
                        "profile": profile,
                        "condition": condition,
                        "pairedScored": len(pairs),
                        "unpairedOrFailed": len(set(a) | set(b)) - len(pairs),
                        "wordErrorDifferencePoints": float(
                            (counts[:, 1].sum() - counts[:, 0].sum()) / counts[:, 2].sum() * 100
                        ),
                        "descriptive95PercentIntervalPoints": [
                            float(x) for x in np.percentile(differences, [2.5, 97.5])
                        ],
                        "identicalTranscriptHashes": sum(
                            x["rawSha256"] == y["rawSha256"] for x, y in pairs
                        ),
                    }
                )
    checks = {
        "nonFinalized": sum(
            r["frontend"] is not None and not r["frontend"]["finalized"] for r in rows
        ),
        "lengthMismatch": sum(
            r["frontend"] is not None
            and r["frontend"]["inputSamples16k"] != r["frontend"]["outputSamples16k"]
            for r in rows
        ),
        "frontendFallbacks": sum(r["frontendFallback"] for r in rows),
        "asrFailures": sum(r["asrFailed"] for r in rows),
        "writerFailures": sum(r["writerFailed"] for r in rows),
        "invalidVadIntervals": sum(
            not (0 <= span["startSample"] < span["endSample"] <= r["frontend"]["inputSamples16k"])
            for r in rows
            if r["frontend"] is not None
            for span in r["frontend"]["speechIntervals"]
        ),
        "dispatchHistogramMismatch": sum(
            sum(r["frontend"]["dispatchHistogram"]) != r["frontend"]["dispatches"]
            for r in rows
            if r["frontend"] is not None
        ),
    }
    expected_jobs = (
        {
            f"quality-{model}-{profile}"
            for model in ("cohere", "small", "turbo")
            for profile in ("raw", "vad", "dpdfnet4", "dpdfnet8")
        }
        | {f"streaming-nemotron-{profile}" for profile in ("raw", "vad", "dpdfnet4", "dpdfnet8")}
        | {"soak-nemotron-dpdfnet4"}
    )
    checks["missingJobs"] = sorted(expected_jobs - {p["name"] for p in processes})
    checks["missingRows"] = sum(len(p.get("missingIds", [])) for p in processes)
    checks["failedProcesses"] = sum(p.get("returnCode", 0) != 0 for p in processes)
    checks["nativeExceptionLogs"] = sum(p.get("nativeExceptionLog", False) for p in processes)
    checks["matrixComplete"] = (
        not (checks["missingJobs"] or checks["missingRows"]) and len(rows) == 1063
    )
    output = {
        "schemaVersion": 1,
        "environment": read(ROOT / "environment.json"),
        "runtimeEnvironment": read(ROOT / "runtime-env.json"),
        "sourceHashes": read(ROOT / "sources.json"),
        "postBenchmarkVerification": read(ROOT / "post-benchmark-verification.json")
        if (ROOT / "post-benchmark-verification.json").exists()
        else None,
        "protocol": read(ROOT / "protocol.json"),
        "artifacts": [
            a for a in read(Path("models/artifacts.json")) if a["role"] == "audio_frontend"
        ],
        "preflight": read(ROOT / "processes-preflight-streaming.json")
        if (ROOT / "processes-preflight-streaming.json").exists()
        else [],
        "modelProvenance": model_provenance,
        "processes": processes,
        "runs": rows,
        "aggregates": aggregates,
        "paired": paired,
        "checks": checks,
        "caveats": [
            "Latency flagged per process; no comparison across contention envelopes",
            "Per-process powerAtStart is authoritative; the original shared environment template retained a stale AC label",
            "Bootstrap intervals are descriptive, unadjusted, recording-paired, not speaker-disjoint",
            "Synthetic soak excluded from all WER/CER",
            "Physical microphone queues, word-aligned emission latency, endpoint detection, watts, GPU and edge devices not measured",
            "Peak RSS includes ASR/writer/application observers; it is not frontend-only memory",
            "Streaming observer retains partial snapshots; RSS growth is not a frontend-leak measurement",
            "Frozen streaming asrMs/RTF include post-finish partial-edit analysis and observer destruction; they do not isolate inference or feeder lateness",
            "Partial edit distance includes newly appended words; it is not word-stabilization latency",
        ],
    }
    for p in output["preflight"]:
        p.pop("command", None)
    encoded = json.dumps(output, indent=2) + "\n"
    for forbidden in [
        "normalizedReference",
        "normalizedHypothesis",
        "rawHypothesis",
        "cleanedHypothesis",
        "/Users/",
    ]:
        assert forbidden not in encoded, forbidden
    OUTPUT.write_text(encoded)
    print(
        f"Published {len(rows)} content-free rows and {len(processes)} process records; checks {checks}"
    )


if __name__ == "__main__":
    main()
