#!/usr/bin/env python3
"""Failure-preserving paired regression analysis; exported results omit transcripts."""

import argparse
import hashlib
import json
import math
import re
from collections import defaultdict
from pathlib import Path

import numpy as np
import soundfile as sf


def digest(data):
    return hashlib.sha256(data).hexdigest()


def quantiles(values):
    values = sorted(values)
    if not values:
        return None
    return {
        "count": len(values),
        "p50": values[math.ceil(len(values) * 0.5) - 1],
        "p95": values[math.ceil(len(values) * 0.95) - 1],
        "max": values[-1],
    }


def word_totals(rows, key="rawScore", metric="wordErrorRate"):
    scores = [r[key][metric] for r in rows if r.get(key)]
    edits = sum(s["edits"] for s in scores)
    words = sum(s["referenceUnits"] for s in scores)
    return {
        "scoredCases": len(scores),
        "edits": edits,
        "referenceUnits": words,
        "rate": edits / words if words else None,
    }


def retained_result_totals(rows):
    """Include empty ASR and rejected rewrites instead of selecting writer successes."""
    return word_totals([{"rawScore": r.get("cleanedScore") or r.get("rawScore")} for r in rows])


def paired_bootstrap(blocks, iterations=10000):
    """Rows are source-utterance blocks: (baseline edits, candidate edits, words)."""
    data = np.asarray(blocks, dtype=np.float64)
    if not len(data) or np.any(data[:, 2] <= 0):
        return None
    rng = np.random.default_rng(20260916)
    indices = rng.integers(0, len(data), size=(iterations, len(data)))
    totals = data[indices].sum(axis=1)
    deltas = (totals[:, 1] - totals[:, 0]) / totals[:, 2]
    return {
        "deltaWer": float((data[:, 1].sum() - data[:, 0].sum()) / data[:, 2].sum()),
        "percentile95": [float(v) for v in np.quantile(deltas, [0.025, 0.975])],
        "iterations": iterations,
        "seed": 20260916,
        "referenceSentenceBlocks": len(data),
        "definition": "candidate-minus-raw micro-WER; reference-sentence blocks, not speaker bootstrap",
    }


def fingerprint(row, model):
    return (
        row.get("frontend", {}).get("segments")
        if model.startswith("silero")
        else row.get("outputPcmSha256")
    )


def interval_overlap(a, b):
    return max(0.0, min(a[1], b[1]) - max(a[0], b[0]))


def si_sdr(reference, estimate):
    """Zero-mean SI-SDR in dB; undefined or perfect cases remain explicit nulls."""
    reference = np.asarray(reference, dtype=np.float64)
    estimate = np.asarray(estimate, dtype=np.float64)
    if reference.shape != estimate.shape or not reference.size:
        raise ValueError("SI-SDR requires equal nonempty signals")
    reference = reference - reference.mean()
    estimate = estimate - estimate.mean()
    energy = float(reference @ reference)
    if energy <= 1e-20 or not np.isfinite(reference).all() or not np.isfinite(estimate).all():
        return None
    target = reference * float(estimate @ reference) / energy
    error = estimate - target
    numerator, denominator = float(target @ target), float(error @ error)
    if numerator <= 1e-20 or denominator <= 1e-20:
        return None
    return 10 * math.log10(numerator / denominator)


def signal_difference(x, y):
    result = {"samples": len(x), "otherSamples": len(y), "sameLength": len(x) == len(y)}
    if len(x) != len(y) or not len(x):
        return {**result, "error": "waveform length mismatch or empty output"}
    if not np.isfinite(x).all() or not np.isfinite(y).all():
        return {**result, "error": "nonfinite output"}
    return {
        **result,
        "maxAbsoluteDifference": float(np.max(abs(x - y))),
        "relativeL2Difference": float(np.linalg.norm(x - y) / max(np.linalg.norm(x), 1e-12)),
    }


def input_levels(root):
    result = []
    manifest = json.loads((root / "frontend.json").read_text())
    for sample in manifest["samples"]:
        if (
            sample["corpusLane"] != "natural"
            or sample["language"] != "en-US"
            or sample["variant"] not in {"clean", "quiet30"}
        ):
            continue
        path = root / "corpus" / sample["audioPath"]
        if digest(path.read_bytes()) != sample["sha256"]:
            raise ValueError("input-level waveform integrity failure")
        audio, rate = sf.read(path, dtype="float64")
        if rate != 16000 or audio.ndim != 1 or not np.isfinite(audio).all():
            raise ValueError("invalid input-level waveform")
        rms = float(np.sqrt(np.mean(audio**2)))
        result.append(
            {
                "id": sample["id"],
                "variant": sample["variant"],
                "rmsDbFs": 20 * math.log10(rms) if rms else None,
                "peakAbs": float(np.max(abs(audio))),
            }
        )
    return result


def ami_score(manifest, rows):
    by_id = {r["id"]: r for r in rows}
    result = []
    for sample in manifest["samples"]:
        annotation = sample["annotation"]
        row = by_id.get(sample["id"], {"error": "missing"})
        # SDK spans use seconds; annotation time references the full recording.
        predicted = [(s["startSeconds"], s["endSeconds"]) for s in row.get("spans", [])]
        offset = annotation["source_start_seconds"]
        truth = [
            (
                max(0.0, w["start"] - offset),
                min(sample["durationSeconds"], w["end"] - offset),
            )
            for w in annotation["reference_filler_events"]
        ]
        hits = sum(any(interval_overlap(t, p) > 0 for p in predicted) for t in truth)
        anchor = annotation["anchor"]
        anchor_interval = (anchor["start"] - offset, anchor["end"] - offset)
        result.append(
            {
                "id": sample["id"],
                "label": annotation["label"],
                "error": row.get("error"),
                "otherSpeakers": annotation["other_speakers_in_window"],
                "referenceFillerEvents": len(truth),
                "eventsWithAnyOverlap": hits,
                "predictedSpanSeconds": sum(b - a for a, b in predicted),
                "referenceFillerSeconds": sum(b - a for a, b in truth),
                "referenceOverlapSeconds": sum(
                    interval_overlap(t, p) for t in truth for p in predicted
                ),
                "anyPredictedSpan": bool(predicted),
                "predictedSpanTouchesAnchor": any(
                    interval_overlap(anchor_interval, p) > 0 for p in predicted
                ),
                "inferenceMs": row.get("inferenceMs"),
                "spans": row.get("spans", []),
            }
        )
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    root = args.root
    reports = {}
    summaries = []
    for path in sorted((root / "runs").glob("*.json")):
        report = json.loads(path.read_text())
        reports[path.stem] = report
        rows = report["runs"]
        samples = {s["id"]: s for s in report["manifest"]["samples"]}
        log = path.with_suffix(".log").read_text()
        rss = re.search(r"(\d+)\s+maximum resident set size", log)
        missing = sorted(set(samples) - {r["id"] for r in rows})
        timings = [v for r in rows for v in r.get("frontend", {}).get("chunkCallMs", [])]
        audio_seconds = sum(r.get("audioSeconds", 0) for r in rows if not r.get("error"))
        compute_ms = sum(
            r.get("frontend", {}).get("computeMs", 0) for r in rows if not r.get("error")
        )
        summary = {
            "name": path.stem,
            "rawReportSha256": digest(path.read_bytes()),
            "sourceSha256": report.get("sourceSha256"),
            "executableSha256": report.get("executableSha256"),
            "manifestSha256": report.get("manifestSha256"),
            "runtime": report.get("runtime", "sherpa-onnx 1.13.8"),
            "frontendRequestedThreads": report.get("threads", 1),
            "frontendGolden": report.get("frontendGolden"),
            "expectedCases": len(samples),
            "completedRows": len(rows),
            "missingCases": missing,
            "failures": [{"id": r["id"], "error": r["error"]} for r in rows if r.get("error")],
            "cleanupFailures": [r["id"] for r in rows if r.get("cleanupError")],
            "frontendComputeMs": compute_ms,
            "successfulAudioSeconds": audio_seconds,
            "frontendRtf": compute_ms / (audio_seconds * 1000) if audio_seconds else None,
            "peakProcessRssBytes": int(rss[1]) if rss else None,
            "chunkCallMs": quantiles(timings),
            "asrMs": quantiles([r["asrMs"] for r in rows if "asrMs" in r]),
            "cleanupMs": quantiles([r["cleanupMs"] for r in rows if "cleanupMs" in r]),
            "inferenceMs": quantiles([r["inferenceMs"] for r in rows if "inferenceMs" in r]),
            "loadMs": report.get("loadMs", report.get("artifactVerificationAndLoadMs")),
            "preprocessMs": quantiles([r["preprocessMs"] for r in rows if "preprocessMs" in r]),
            "maximumDeliveryLatenessMs": max(
                (r.get("frontend", {}).get("maximumDeliveryLatenessMs", 0) for r in rows),
                default=0,
            ),
        }
        summaries.append(summary)
    comparisons = []
    noise_breakdowns = []
    asr_rows = []
    for condition in ["clean", "quiet30", "natural10", "natural0", "noise"]:
        baseline = reports.get(f"asr-{condition}-raw")
        if not baseline:
            continue
        metadata = {s["id"]: s for s in baseline["manifest"]["samples"]}
        for model in ["raw", "dpdfnet4", "dpdfnet8"]:
            report = reports.get(f"asr-{condition}-{model}")
            if not report:
                continue
            for lane in ["natural", "synthetic", "noise"]:
                rows = [r for r in report["runs"] if metadata[r["id"]]["corpusLane"] == lane]
                if rows:
                    asr_rows.append(
                        {
                            "model": model,
                            "condition": condition,
                            "lane": lane,
                            "attempted": sum(s["corpusLane"] == lane for s in metadata.values()),
                            "completed": len(rows),
                            "errors": sum(bool(r.get("error")) for r in rows),
                            "rawWer": word_totals(rows),
                            "rawCer": word_totals(rows, metric="graphemeErrorRate"),
                            "acceptedCleanedWer": word_totals(rows, "cleanedScore"),
                            "cleanedOrRetainedRawWer": retained_result_totals(rows),
                            "emptyRawTranscripts": sum(
                                "raw" in r and not r["raw"].strip() for r in rows
                            ),
                            "cleanupFailures": sum(bool(r.get("cleanupError")) for r in rows),
                            "falseTranscripts": sum(
                                bool(r.get("nonSpeechScore", {}).get("falseTranscript"))
                                for r in rows
                            ),
                            "noiseInsertedWords": sum(
                                r.get("nonSpeechScore", {}).get("insertedWords", 0) for r in rows
                            ),
                        }
                    )
            if condition in {"natural0", "natural10"}:
                for noise_id in sorted(
                    {s.get("noiseId") for s in metadata.values() if s.get("noiseId")}
                ):
                    rows = [
                        r
                        for r in report["runs"]
                        if metadata[r["id"]]["corpusLane"] == "natural"
                        and metadata[r["id"]].get("noiseId") == noise_id
                    ]
                    noise_breakdowns.append(
                        {
                            "model": model,
                            "condition": condition,
                            "noiseId": noise_id,
                            "completed": len(rows),
                            "rawWer": word_totals(rows),
                            "failures": sum(bool(r.get("error")) for r in rows),
                        }
                    )
            if model == "raw":
                continue
            raw = {r["id"]: r for r in baseline["runs"]}
            candidate = {r["id"]: r for r in report["runs"]}
            for lane in ["natural", "synthetic"]:
                expected = [s for s in metadata.values() if s["corpusLane"] == lane]
                pairs = []
                failed = []
                for sample in expected:
                    a = raw.get(sample["id"], {})
                    b = candidate.get(sample["id"], {})
                    if (
                        a.get("inputPcmSha256")
                        and b.get("inputPcmSha256")
                        and a["inputPcmSha256"] != b["inputPcmSha256"]
                    ):
                        raise ValueError("unpaired ASR input")
                    if not a.get("rawScore") or not b.get("rawScore"):
                        failed.append(sample["id"])
                        continue
                    sa, sb = (
                        a["rawScore"]["wordErrorRate"],
                        b["rawScore"]["wordErrorRate"],
                    )
                    if sa["referenceUnits"] != sb["referenceUnits"]:
                        raise ValueError("unpaired ASR references")
                    pairs.append(
                        {
                            "id": sample["id"],
                            "baseId": sample["baseId"],
                            "rawEdits": sa["edits"],
                            "candidateEdits": sb["edits"],
                            "referenceWords": sa["referenceUnits"],
                            "sentenceBlock": digest(a["rawScore"]["normalizedReference"].encode()),
                        }
                    )
                if expected:
                    blocks = defaultdict(lambda: [0, 0, 0])
                    for p in pairs:
                        blocks[p["sentenceBlock"]] = [
                            a + b
                            for a, b in zip(
                                blocks[p["sentenceBlock"]],
                                [
                                    p["rawEdits"],
                                    p["candidateEdits"],
                                    p["referenceWords"],
                                ],
                            )
                        ]
                    comparisons.append(
                        {
                            "model": model,
                            "condition": condition,
                            "lane": lane,
                            "expectedPairs": len(expected),
                            "failedOrMissingPairs": failed,
                            "improved": sum(p["candidateEdits"] < p["rawEdits"] for p in pairs),
                            "regressed": sum(p["candidateEdits"] > p["rawEdits"] for p in pairs),
                            "equal": sum(p["candidateEdits"] == p["rawEdits"] for p in pairs),
                            "bootstrap": paired_bootstrap(list(blocks.values())),
                            "pairs": pairs,
                        }
                    )
    vad = []
    ear = []
    for name, target in [("frontend-silero-v6.2", vad), ("ear-ear", ear)]:
        report = reports.get(name)
        if report:
            samples = {s["id"]: s for s in report["manifest"]["samples"]}
            for row in report["runs"]:
                s = samples[row["id"]]
                target.append(
                    {
                        "id": row["id"],
                        "language": s["language"],
                        "variant": s["variant"],
                        "lane": s["corpusLane"],
                        "error": row.get("error"),
                        "speechExpected": bool(s["reference"]),
                        "hasVadSegment": bool(row.get("frontend", {}).get("segments")),
                        "segmentSeconds": sum(
                            x["samples"] for x in row.get("frontend", {}).get("segments", [])
                        )
                        / 16000,
                        "correctLanguage": row.get("correct"),
                        "languagePrediction": row.get("languagePrediction"),
                        "topLanguageProbability": row.get("probabilities", {}).get(
                            row.get("languagePrediction")
                        ),
                        "isReliable": row.get("isReliable"),
                    }
                )
    reset_checks = []
    for name, report in reports.items():
        model = report.get("frontend", report.get("model", ""))
        for row in report["runs"]:
            if row["id"].endswith("--reset"):
                original_id = row["id"][:-7]
                original = next((r for r in report["runs"] if r["id"] == original_id), None)
                reference_report = name
                if original is None and name.startswith("boundaries-"):
                    reference_report = (
                        f"frontend-{model}"
                        if report.get("delivery") == "offline"
                        else f"continuous-{model}-chunks"
                    )
                    original = next(
                        (
                            r
                            for r in reports.get(reference_report, {}).get("runs", [])
                            if r["id"] == original_id
                        ),
                        None,
                    )
                if original:
                    if model.startswith("uhm"):
                        equal = row.get("probabilitiesSha256") == original.get(
                            "probabilitiesSha256"
                        )
                    elif model == "ear":
                        equal = row.get("probabilities") == original.get("probabilities")
                    else:
                        equal = fingerprint(row, model) == fingerprint(original, model)
                    check = {
                        "report": name,
                        "referenceReport": reference_report,
                        "sameProcess": name == reference_report,
                        "samePacketSchedule": report.get("packetSizes")
                        == reports[reference_report].get("packetSizes"),
                        "inputEqual": row.get("inputPcmSha256") == original.get("inputPcmSha256")
                        if row.get("inputPcmSha256")
                        else None,
                        "equal": equal and not row.get("error") and not original.get("error"),
                    }
                    if report.get("asr") not in (None, "none"):
                        check.update(
                            rawTextEqual=row.get("raw") == original.get("raw"),
                            cleanedTextEqual=row.get("cleaned") == original.get("cleaned"),
                            cleanupFailureEqual=row.get("cleanupError")
                            == original.get("cleanupError"),
                        )
                    reset_checks.append(check)
    alignment = []
    for model in ["silero-v6.2", "dpdfnet4", "dpdfnet8"]:
        base = reports.get(f"boundaries-{model}-offline")
        if not base:
            continue
        for mode in ["chunks", "variable"]:
            other = reports.get(f"boundaries-{model}-{mode}")
            if not other:
                continue
            by_id = {r["id"]: r for r in other["runs"]}
            for a in base["runs"]:
                b = by_id.get(a["id"], {})
                record = {
                    "model": model,
                    "delivery": mode,
                    "id": a["id"],
                    "error": a.get("error") or b.get("error") or ("missing" if not b else None),
                }
                if not record["error"]:
                    if model.startswith("silero"):
                        record["segmentsEqual"] = fingerprint(a, model) == fingerprint(b, model)
                    else:
                        x = np.fromfile(
                            root / "runs/pcm" / f"{a['outputPcmSha256']}.f32",
                            dtype="<f4",
                        ).astype(np.float64)
                        y = np.fromfile(
                            root / "runs/pcm" / f"{b['outputPcmSha256']}.f32",
                            dtype="<f4",
                        ).astype(np.float64)
                        record.update(signal_difference(x, y))
                alignment.append(record)
    ami = {
        model: ami_score(report["manifest"], report["runs"])
        for model in ["uhm-raw", "uhm-normalized"]
        if (report := reports.get(f"ami-{model}"))
    }
    uhm = []
    for name, report in reports.items():
        if not report.get("model", "").startswith("uhm-"):
            continue
        samples = {s["id"]: s for s in report["manifest"]["samples"]}
        for row in report["runs"]:
            sample = samples[row["id"]]
            uhm.append(
                {
                    "report": name,
                    "model": report["model"],
                    "id": row["id"],
                    "error": row.get("error"),
                    "lane": sample.get("corpusLane", "synthetic-control"),
                    "language": sample["language"],
                    "variant": sample.get("variant", "control"),
                    "audioSeconds": row.get("audioSeconds"),
                    "spans": row.get("spans", []),
                    "predictedSpanSeconds": sum(
                        s["endSeconds"] - s["startSeconds"] for s in row.get("spans", [])
                    ),
                }
            )
    vad_gate = []
    vad_report = reports.get("frontend-silero-v6.2")
    if vad_report:
        decisions = {r["id"]: r for r in vad_report["runs"]}
        for condition in ["clean", "quiet30", "natural10", "natural0", "noise"]:
            baseline = reports.get(f"asr-{condition}-raw")
            if not baseline:
                continue
            metadata = {s["id"]: s for s in baseline["manifest"]["samples"]}
            for row in baseline["runs"]:
                sample = metadata[row["id"]]
                if sample["corpusLane"] == "reset":
                    continue
                decision = decisions.get(row["id"], {"error": "missing VAD result"})
                failure = row.get("error") or decision.get("error")
                if failure:
                    vad_gate.append({"id": row["id"], "error": failure})
                    continue
                if row["inputPcmSha256"] != decision["inputPcmSha256"]:
                    raise ValueError("unpaired VAD/ASR counterfactual input")
                keep = bool(decision["frontend"]["segments"])
                score = row.get("rawScore", {}).get("wordErrorRate", {})
                vad_gate.append(
                    {
                        "id": row["id"],
                        "condition": condition,
                        "lane": sample["corpusLane"],
                        "wouldDropClip": not keep,
                        "rawTranscriptNonempty": bool(row.get("raw", "").strip()),
                        "referenceWords": score.get("referenceUnits"),
                        "originalEdits": score.get("edits"),
                        "hypotheticalEdits": score.get("edits")
                        if keep
                        else score.get("referenceUnits"),
                        "rawNoiseFalseTranscript": row.get("nonSpeechScore", {}).get(
                            "falseTranscript"
                        ),
                        "hypotheticalNoiseFalseTranscript": bool(
                            row.get("nonSpeechScore", {}).get("falseTranscript")
                        )
                        and keep,
                    }
                )
    signal_rows = []
    repeated_frontend_checks = []
    for model in ["dpdfnet4", "dpdfnet8"]:
        report = reports.get(f"frontend-{model}")
        if not report:
            continue
        metadata = {s["id"]: s for s in report["manifest"]["samples"]}
        for row in report["runs"]:
            sample = metadata[row["id"]]
            variant = sample["variant"]
            if sample["corpusLane"] == "reset" or row.get("error"):
                continue
            baseline_report = reports.get(f"asr-{variant}-{model}")
            if baseline_report:
                previous = next((r for r in baseline_report["runs"] if r["id"] == row["id"]), None)
                if previous and not previous.get("error"):
                    repeated_frontend_checks.append(
                        {
                            "model": model,
                            "id": row["id"],
                            "inputEqual": previous["inputPcmSha256"] == row["inputPcmSha256"],
                            "outputEqual": previous["outputPcmSha256"] == row["outputPcmSha256"],
                        }
                    )
            if variant not in {"natural10", "natural0", "naturalm5"}:
                continue
            reference, rate = sf.read(
                root / "corpus" / f"{sample['baseId']}--matchedClean.wav",
                dtype="float64",
            )
            if rate != 16000:
                raise ValueError("SI-SDR sample rate mismatch")
            original = np.fromfile(root / "runs/pcm" / f"{row['inputPcmSha256']}.f32", dtype="<f4")
            enhanced = np.fromfile(root / "runs/pcm" / f"{row['outputPcmSha256']}.f32", dtype="<f4")
            before, after = si_sdr(reference, original), si_sdr(reference, enhanced)
            signal_rows.append(
                {
                    "model": model,
                    "id": row["id"],
                    "variant": variant,
                    "lane": sample["corpusLane"],
                    "language": sample["language"],
                    "inputSiSdrDb": before,
                    "outputSiSdrDb": after,
                    "improvementDb": after - before
                    if before is not None and after is not None
                    else None,
                }
            )
    process_ledgers = {p.stem: json.loads(p.read_text()) for p in root.glob("processes-*.json")}
    expected_jobs = {"asr": 15, "frontend": 3, "contracts": 12, "desert": 7, "paced": 2}
    completion = {
        phase: {
            "expectedJobs": count,
            "recordedJobs": len(process_ledgers.get("processes-" + phase, [])),
        }
        for phase, count in expected_jobs.items()
    }
    missing_reports = [
        job["name"]
        for jobs in process_ledgers.values()
        for job in jobs
        if job["name"] not in reports
    ]
    asr_example = next(
        (r for p in reports.values() for r in p["runs"] if r.get("modelVersions")), {}
    )
    diagnostic = None
    probe_path = root / "cohere-stage-probe.json"
    if probe_path.exists():
        probe = json.loads(probe_path.read_text())
        timing_keys = [
            "verificationMs",
            "modelLoadMs",
            "acceptWaveformMs",
            "decodeMs",
            "modelReleaseMs",
        ]
        rss = re.search(
            r"(\d+)\s+maximum resident set size",
            probe_path.with_suffix(".log").read_text(),
        )
        diagnostic = {
            "rawReportSha256": digest(probe_path.read_bytes()),
            "sourceSha256": probe["sourceSha256"],
            "executableSha256": probe["executableSha256"],
            "manifestSha256": digest((root / "cohere-diagnostic.json").read_bytes()),
            "requestedNativeThreads": probe["requestedNativeThreads"],
            "expectedCases": len(probe["manifest"]["samples"]),
            "completedCases": len(probe["runs"]),
            "peakProcessRssBytes": int(rss[1]) if rss else None,
            "timings": {
                key: quantiles([r[key] for r in probe["runs"] if key in r]) for key in timing_keys
            },
            "cases": [
                {
                    key: row.get(key)
                    for key in [
                        "id",
                        "error",
                        "inputPcmSha256",
                        "audioSeconds",
                        "completion",
                        "completionAccepted",
                        *timing_keys,
                    ]
                }
                | {
                    "rawWer": row.get("rawScore", {}).get("wordErrorRate"),
                    "emptyTranscript": not row.get("raw", "").strip(),
                }
                for row in probe["runs"]
            ],
            "qualificationExcluded": True,
            "limitations": [
                *probe["limitations"],
                "Selected after observing a quiet-input failure; diagnostic cases do not enlarge the qualification corpus",
            ],
        }
    packet_diagnostic = None
    packet_path = root / "silero-variable-diagnostic-result.json"
    if packet_path.exists():
        packet = json.loads(packet_path.read_text())
        packet_diagnostic = {
            "rawReportSha256": digest(packet_path.read_bytes()),
            "sourceSha256": packet["sourceSha256"],
            "executableSha256": packet["executableSha256"],
            "manifestSha256": packet["manifestSha256"],
            "packetSizes": packet["packetSizes"],
            "qualificationExcluded": True,
            "reason": "Follow-up after fixed-versus-variable packet segment mismatch; compare fresh and reset variable-schedule states",
            "cases": [
                {
                    "id": r["id"],
                    "error": r.get("error"),
                    "inputPcmSha256": r.get("inputPcmSha256"),
                    "segments": r.get("frontend", {}).get("segments"),
                }
                for r in packet["runs"]
            ],
        }
    result = {
        "schemaVersion": 1,
        "protocol": "audio-hard-v1",
        "environment": json.loads((root / "environment.json").read_text()),
        "quantileDefinition": "nearest-rank; bootstrap percentile uses numpy linear quantiles",
        "sourceSha256": digest(Path(__file__).read_bytes()),
        "modelVersionCatalog": asr_example.get("modelVersions"),
        "observedAsrExecutionExample": asr_example.get("execution"),
        "frontendArtifactManifestSha256": digest(
            Path("benchmarks/asr/audio-frontend-artifacts.json").read_bytes()
        ),
        "experiments": summaries,
        "asrAggregates": asr_rows,
        "writerScoringDefinition": "acceptedCleanedWer selects only accepted nonempty-ASR rewrites; cleanedOrRetainedRawWer includes accepted cleaned scores and otherwise retains raw scores, including empty ASR and fidelity rejections; raw WER remains the primary frontend comparison",
        "cohereStageDiagnostic": diagnostic,
        "packetDiagnostic": packet_diagnostic,
        "denoiserStartupDiagnostic": json.loads(
            (root / "denoiser-startup-diagnostic-result.json").read_text()
        )
        if (root / "denoiser-startup-diagnostic-result.json").exists()
        else None,
        "integrityDiagnostic": json.loads((root / "integrity-probe.json").read_text())
        if (root / "integrity-probe.json").exists()
        else None,
        "inputLevels": input_levels(root),
        "pairedAsrComparisons": comparisons,
        "noiseTypeAsr": noise_breakdowns,
        "vadRows": vad,
        "earRows": ear,
        "resetChecks": reset_checks,
        "boundaryComparisons": alignment,
        "amiUhm": ami,
        "uhmRows": uhm,
        "vadGateCounterfactual": vad_gate,
        "vadGateCounterfactualDefinition": "Replace whole-clip ASR with empty text when VAD returns no segment; paired output analysis only, no actual gating or segment trimming",
        "signalRows": signal_rows,
        "repeatedFrontendChecks": repeated_frontend_checks,
        "signalMetricDefinition": "zero-mean SI-SDR, no time realignment; null for zero/perfect energy; not ASR meaning quality",
        "processes": process_ledgers,
        "phaseCompletion": completion,
        "missingReports": missing_reports,
        "limits": [
            "One M5Pro CPU, no end-to-end liveASR or thermals",
            "One AMI meeting, approximate lexical spans",
            "Reference-sentence bootstrap is not speaker bootstrap; intervals are unadjusted descriptive intervals",
            "Natural-source speech with artificial corruption; no whisper/singing/code-switch qualification",
        ],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
    print(f"Summarized {len(summaries)} reports")


if __name__ == "__main__":
    main()
