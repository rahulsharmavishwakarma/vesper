#!/usr/bin/env python3
"""Publish transcript-free VibeVoice evidence using the canonical Rust scorer."""

import argparse
import hashlib
import itertools
import json
import re
import subprocess
from pathlib import Path


def score_pairs(scorer, pairs):
    result = subprocess.run(
        [str(scorer)],
        input=json.dumps(pairs),
        text=True,
        capture_output=True,
        check=True,
    )
    return json.loads(result.stdout)


def counts(score):
    if "speech" in score:
        value = score["speech"]["wordErrorRate"]
        return value["edits"], value["referenceUnits"]
    return score["nonSpeech"]["insertedWords"], 0


def cpwer(scorer, sample, raw):
    refs = list(sample["referenceSpeakers"].values())
    speakers = {}
    current = "unlabeled"
    # Concatenate each speaker's words before trying label permutations.
    for part in re.split(r"(Speaker\s+\d+\s*:)", raw):
        if re.fullmatch(r"Speaker\s+\d+\s*:", part):
            current = re.search(r"\d+", part).group()
        elif part.strip():
            speakers.setdefault(current, []).append(part)
    hyps = [" ".join(parts) for parts in speakers.values()]
    size = max(len(refs), len(hyps))
    if size > 6:
        return {"error": "too many predicted speakers for bounded exact permutation"}
    refs += [""] * (size - len(refs))
    hyps += [""] * (size - len(hyps))
    matrix = score_pairs(
        scorer,
        [
            {
                "reference": reference,
                "hypothesis": hypothesis,
                "durationSeconds": sample["durationSeconds"],
            }
            for reference in refs
            for hypothesis in hyps
        ],
    )
    best = min(
        sum(counts(matrix[i * size + j])[0] for i, j in enumerate(order))
        for order in itertools.permutations(range(size))
    )
    words = sum(counts(matrix[i * size])[1] for i in range(size))
    return {
        "edits": best,
        "referenceWords": words,
        "rate": best / words,
        "referenceSpeakers": len(sample["referenceSpeakers"]),
        "predictedSpeakers": len(speakers),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    scorer = args.root / "binaries" / "score-json"
    result = {
        "schemaVersion": 1,
        "runs": [],
        "aggregates": {},
        "limitations": [
            "reference Python runtime, not product integration",
            "small public regression corpus, not full test sets",
        ],
    }
    for path in sorted((args.root / "runs").glob("*.json")):
        if path.name.endswith("-process.json"):
            continue
        report = json.loads(path.read_text())
        plan = {item["id"]: item for item in report["plan"]["cases"]}
        process_path = path.with_name(path.stem + "-process.json")
        process = json.loads(process_path.read_text()) if process_path.exists() else None
        rows = report["cases"]
        scores = score_pairs(
            scorer,
            [
                {
                    "reference": plan[row["id"]]["reference"],
                    "hypothesis": row["speech"],
                    "durationSeconds": plan[row["id"]]["durationSeconds"],
                }
                for row in rows
            ],
        )
        published = []
        for row, score in zip(rows, scores, strict=True):
            sample = plan[row["id"]]
            if not sample["reference"] and sample["lane"] != "noise":
                score = {
                    "unavailableReason": "Synthetic soak has no reference transcript; not non-speech"
                }
            item = {
                key: value for key, value in row.items() if key not in ("raw", "speech", "chunks")
            }
            item["chunks"] = [
                {key: value for key, value in chunk.items() if key != "text"}
                for chunk in row["chunks"]
            ]
            if "speech" in score:
                del score["speech"]["normalizedReference"]
                del score["speech"]["normalizedHypothesis"]
            item.update(score=score, lane=sample["lane"], audioSha256=sample["sha256"])
            annotations = re.findall(r"\[(?:Noise|Music|Silence|Laughter)\]", row["speech"])
            item["nonSpeechAnnotations"] = annotations
            if not sample["reference"] and sample["lane"] == "noise":
                lexical = re.sub(r"\[(?:Noise|Music|Silence|Laughter)\]", "", row["speech"])
                item["noiseSpeechOnlyScore"] = score_pairs(
                    scorer,
                    [
                        {
                            "reference": "",
                            "hypothesis": lexical,
                            "durationSeconds": sample["durationSeconds"],
                        }
                    ],
                )[0]["nonSpeech"]
            for key in (
                "sourceId",
                "sourceAudioSha256",
                "annotationArchiveSha256",
                "sourceStartSeconds",
                "sourceEndSeconds",
                "hotwords",
            ):
                if key in sample:
                    item[key] = sample[key]
            if "referenceSpeakers" in sample:
                item["cpwer"] = cpwer(scorer, sample, row["raw"])
            published.append(item)
            key = path.stem + ":" + sample["lane"]
            if "unavailableReason" in score:
                key += ":unscored"
            group = result["aggregates"].setdefault(
                key,
                {
                    "cases": 0,
                    "failures": 0,
                    "wordEdits": 0,
                    "referenceWords": 0,
                    "falseTranscripts": 0,
                    "insertedNoiseWords": 0,
                    "speechOnlyFalseTranscripts": 0,
                    "noiseAnnotations": 0,
                    "unscoredCases": 0,
                },
            )
            group["cases"] += 1
            group["failures"] += row["error"] is not None
            if "speech" in score:
                edits, words = counts(score)
                group["wordEdits"] += edits
                group["referenceWords"] += words
            elif "nonSpeech" in score:
                group["falseTranscripts"] += score["nonSpeech"]["falseTranscript"]
                group["insertedNoiseWords"] += score["nonSpeech"]["insertedWords"]
                group["speechOnlyFalseTranscripts"] += item["noiseSpeechOnlyScore"][
                    "falseTranscript"
                ]
                group["noiseAnnotations"] += len(annotations)
            else:
                group["unscoredCases"] += 1
        run = {key: value for key, value in report.items() if key not in ("plan", "cases")}
        run.update(
            name=path.stem,
            cases=published,
            process=process,
            rawReportSha256=hashlib.sha256(path.read_bytes()).hexdigest(),
            expectedCases=len(plan),
            missingCases=sorted(set(plan) - {r["id"] for r in rows}),
        )
        run["sources"] = report["plan"]["sources"]
        if process and "elapsedSeconds" in process:
            recording_ms = sum(
                row.get("elapsedMs", 0) + row.get("preparationMs", 0) for row in rows
            )
            setup_ms = report.get("verificationMs", 0) + report.get("loadMs", 0)
            run["wallAccounting"] = {
                "recordingAndPreparationMs": recording_ms,
                "verificationAndLoadMs": setup_ms,
                "unattributedCoordinatorWallMs": (
                    process["elapsedSeconds"] * 1000 - recording_ms - setup_ms
                ),
                "definition": (
                    "Coordinator wall time minus reported recording, audio preparation, "
                    "artifact verification and model-load stages. Remaining time is "
                    "unattributed; includes uninstrumented work and coordinator overhead."
                ),
            }
        result["runs"].append(run)
    for group in result["aggregates"].values():
        group["wer"] = (
            group["wordEdits"] / group["referenceWords"] if group["referenceWords"] else None
        )
    result["environment"] = json.loads((args.root / "host-preflight.json").read_text())
    result["nativeProvenance"] = json.loads((args.root / "native-provenance.json").read_text())
    preflight = args.root / "streaming-preflight.json"
    if preflight.exists():
        result["streamingPreflight"] = json.loads(preflight.read_text())
    result["inventory"] = json.loads(Path("benchmarks/asr/vibevoice-inventory.json").read_text())
    result["scorerSha256"] = hashlib.sha256(scorer.read_bytes()).hexdigest()
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result["aggregates"], indent=2))


if __name__ == "__main__":
    main()
