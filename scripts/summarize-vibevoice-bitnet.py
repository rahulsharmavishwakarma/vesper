#!/usr/bin/env python3
"""Publish transcript-free BitNet measurements with canonical Rust ASR scoring."""

import argparse
import hashlib
import importlib.util
import json
import math
import re
import statistics
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def distribution(values):
    if not values:
        return None
    ordered = sorted(values)
    return {
        "count": len(values),
        "median": statistics.median(values),
        "p95NearestRank": ordered[math.ceil(len(ordered) * 0.95) - 1],
        "min": ordered[0],
        "max": ordered[-1],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location(
        "vibevoice_scoring", Path(__file__).with_name("summarize-vibevoice.py")
    )
    scoring = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(scoring)
    scorer = args.root / "binaries/score-json"
    result = {"schemaVersion": 1, "runs": [], "aggregates": {}}
    for path in sorted((args.root / "runs").glob("*.json")):
        report = json.loads(path.read_text())
        plan = {x["id"]: x for x in report["plan"]["cases"]}
        rows = []
        for row in report["cases"]:
            sample = plan[row["id"]]
            native = row.get("native", {})
            raw = native.get("rawText", "")
            speech = raw
            parse_error = None
            speaker_text = None
            if sample.get("promptFormat") == "json":
                try:
                    segments = json.loads(raw)
                    if not isinstance(segments, list) or not all(
                        isinstance(x, dict) and isinstance(x.get("Content"), str) for x in segments
                    ):
                        raise ValueError("expected segment array with Content strings")
                    speech = " ".join(x["Content"] for x in segments)
                    if all(isinstance(x.get("Speaker"), int) for x in segments):
                        speaker_text = " ".join(
                            f"Speaker {x['Speaker']}: {x['Content']}" for x in segments
                        )
                except (ValueError, TypeError) as error:
                    parse_error = str(error)
                    speech = ""
            published = {k: v for k, v in row.items() if k != "native"}
            published["native"] = {k: v for k, v in native.items() if k != "rawText"}
            published.update(
                lane=sample["lane"],
                audioSeconds=sample["durationSeconds"],
                audioSha256=sample["sha256"],
                sourceAudioSha256=sample["sourceAudioSha256"],
                resampledPcmSha256=sample["resampledPcmSha256"],
                rawTextSha256=hashlib.sha256(raw.encode()).hexdigest(),
                outputParseError=parse_error,
            )
            for key in (
                "sourceId",
                "sourceStartSeconds",
                "sourceEndSeconds",
                "annotationArchiveSha256",
                "preparedSamples",
            ):
                if key in sample:
                    published[key] = sample[key]
            if sample.get("hotwords"):
                published["hotwords"] = sample["hotwords"]
            if not sample["reference"] and sample["lane"] != "noise":
                score = {"unavailableReason": "No aligned reference for stitched speech fixture"}
            else:
                score = scoring.score_pairs(
                    scorer,
                    [
                        {
                            "reference": sample["reference"],
                            "hypothesis": speech,
                            "durationSeconds": sample["durationSeconds"],
                        }
                    ],
                )[0]
                if "speech" in score:
                    del score["speech"]["normalizedReference"]
                    del score["speech"]["normalizedHypothesis"]
            published["score"] = score
            if speaker_text is not None and "referenceSpeakers" in sample:
                published["cpwer"] = scoring.cpwer(scorer, sample, speaker_text)
            if sample["lane"] == "noise":
                markers = re.findall(r"\[(?:Noise|Music|Silence|Laughter)\]", speech)
                lexical = re.sub(r"\[(?:Noise|Music|Silence|Laughter)\]", "", speech)
                published["eventAnnotations"] = markers
                published["lexicalNoiseScore"] = scoring.score_pairs(
                    scorer,
                    [
                        {
                            "reference": "",
                            "hypothesis": lexical,
                            "durationSeconds": sample["durationSeconds"],
                        }
                    ],
                )[0]["nonSpeech"]
            rows.append(published)
            lane = sample["lane"]
            if lane == "meeting":
                lane += ":" + sample.get("promptFormat", "text")
            key = path.stem + ":" + lane
            group = result["aggregates"].setdefault(
                key,
                {
                    "plannedCases": 0,
                    "measuredCases": 0,
                    "failures": 0,
                    "blocked": 0,
                    "unscoredCases": 0,
                    "wordEdits": 0,
                    "referenceWords": 0,
                    "falseTranscripts": 0,
                    "lexicalFalseTranscripts": 0,
                    "requestMs": [],
                    "requestRtf": [],
                    "processWallMs": [],
                },
            )
            group["plannedCases"] += 1
            group["measuredCases"] += bool(native)
            group["failures"] += row["status"] == "failed" or parse_error is not None
            group["blocked"] += row["status"] == "blocked"
            if "speech" in score:
                edits, words = scoring.counts(score)
                group["wordEdits"] += edits
                group["referenceWords"] += words
            elif "nonSpeech" in score:
                group["falseTranscripts"] += score["nonSpeech"]["falseTranscript"]
                group["lexicalFalseTranscripts"] += published["lexicalNoiseScore"][
                    "falseTranscript"
                ]
            else:
                group["unscoredCases"] += 1
            if "requestMs" in native:
                group["requestMs"].append(native["requestMs"])
                group["requestRtf"].append(native["requestMs"] / (1000 * native["audioSeconds"]))
            if "process" in row:
                group["processWallMs"].append(row["process"]["wallMs"])
        published_run = {k: v for k, v in report.items() if k not in ("cases", "plan")}
        published_run.update(
            name=path.stem,
            cases=rows,
            sources=report["plan"]["sources"],
            expectedCases=len(plan),
            missingCases=sorted(set(plan) - {x["id"] for x in rows}),
            rawReportSha256=digest(path),
        )
        result["runs"].append(published_run)
    for group in result["aggregates"].values():
        group["wer"] = (
            group["wordEdits"] / group["referenceWords"] if group["referenceWords"] else None
        )
        for name in ("requestMs", "requestRtf", "processWallMs"):
            group[name] = distribution(group[name])
    result["host"] = json.loads((args.root / "host-preflight.json").read_text())
    result["nativeProvenance"] = json.loads((args.root / "native-provenance.json").read_text())
    cohere_path = args.root / "cohere.json"
    if cohere_path.exists():
        cohere = json.loads(cohere_path.read_text())
        baseline = []
        for row in cohere["runs"]:
            score = {
                k: v
                for k, v in row["rawScore"].items()
                if k not in ("normalizedReference", "normalizedHypothesis")
            }
            baseline.append(
                {
                    k: row[k]
                    for k in (
                        "sampleId",
                        "sourceId",
                        "audioSeconds",
                        "residentRun",
                        "asrMs",
                        "setupMs",
                        "cleanupMs",
                        "realTimeFactor",
                        "execution",
                        "error",
                    )
                }
                | {
                    "score": score,
                    "outputSha256": hashlib.sha256(row["rawHypothesis"].encode()).hexdigest(),
                }
            )
        result["cohereComparison"] = {
            "rawReportSha256": digest(cohere_path),
            "environment": cohere["environment"],
            "model": cohere["modelVersions"]["asr"],
            "cases": baseline,
            "process": json.loads((args.root / "cohere-process.json").read_text()),
            "runtimeProvenance": json.loads(
                (args.root / "cohere-native-provenance.json").read_text()
            ),
            "evaluatorSha256": digest(args.root / "binaries/vesper-eval"),
            "residentAsrMs": distribution([x["asrMs"] for x in baseline if x["residentRun"]]),
        }
    for filename, key in (
        ("stock-parity.json", "stockInstrumentationParity"),
        ("position-fix-provenance.json", "positionFixProvenance"),
    ):
        path = args.root / filename
        if path.exists():
            result[key] = json.loads(path.read_text())
    result["scorerSha256"] = digest(scorer)
    result["publisherSha256"] = digest(Path(__file__))
    result["measurementNotes"] = [
        "sampledProcessRssBytes is the running sampled process high-water value, not instantaneous or isolated per-case RSS",
        "peakRssBytes comes from macOS /usr/bin/time -l after process exit",
        "firstGeneratedTokenMs may identify an assistant header, not a spoken word",
        "quality scores include failed cases; reference-free stitched speech remains unscored",
    ]
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result["aggregates"], indent=2))


if __name__ == "__main__":
    main()
