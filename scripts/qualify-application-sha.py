#!/usr/bin/env python3
"""Paired Apple CPU qualification of frozen application-linked SHA builds.

Run from the repository root, after freezing software/hardware audio_frontend
and cohere_stage_probe binaries under OUTPUT/binaries. Never builds during runs.
"""

import argparse
import hashlib
import json
import math
import os
import re
import subprocess
import time
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def read(path):
    return json.loads(path.read_text())


def text_hash(value):
    return None if value is None else hashlib.sha256(value.encode()).hexdigest()


def distribution(values):
    values = sorted(values)
    if not values:
        return {"n": 0}
    return {
        "n": len(values),
        "p50": values[math.ceil(0.5 * len(values)) - 1],
        "p95": values[math.ceil(0.95 * len(values)) - 1],
        "min": values[0],
        "max": values[-1],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("corpus_root", type=Path)
    args = parser.parse_args()
    root, corpus = args.output, args.corpus_root
    if (root / "processes.json").exists() or (root / "runs").exists():
        raise FileExistsError("preserve previous/partial runs; choose a fresh output")
    binaries = root / "binaries"
    for mode in ("software", "hardware"):
        for example in ("audio_frontend", "cohere_stage_probe"):
            if not (binaries / f"{mode}-{example}").is_file():
                raise FileNotFoundError(f"freeze {mode}-{example} first")
    (root / "runs").mkdir()
    manifest = read(corpus / "asr-clean.json")
    if len(manifest["samples"]) != 29:
        raise ValueError("expected frozen 24 English + four controls + reset")
    # Add one explicit reset, then split in two equal blocks for S/H, H/S order.
    samples = manifest["samples"] + [{**manifest["samples"][0], "id": "sha-final-reset"}]
    for block in range(2):
        write(
            root / f"block-{block}.json",
            {**manifest, "samples": samples[block * 15 : (block + 1) * 15]},
        )
    environment = read(corpus / "environment.json")
    environment.update(
        label="application-sha-2026-09-17",
        precision="Cohere INT8 ONNX; Qwen writer default GGUF (pins per row)",
        threads=8,
        powerMode="Battery; powermode=0; per-process supply captured in ledger; no thermal telemetry",
        notes="Sequential CPU jobs; ASR/writer <=8 threads; two process-cold writers per mode; per-job Cohere; filesystem cache not flushed; no compiler during inference; no thermal telemetry.",
    )
    write(root / "environment.json", environment)
    # Use default runtime behavior, independent of shell experiment overrides.
    env = {
        k: v
        for k, v in os.environ.items()
        if not k.startswith(("VESPER_", "TRANSCRIBE_", "GGML_", "LLAMA_"))
    }
    overrides = {
        "OMP_NUM_THREADS": "1",
        "OPENBLAS_NUM_THREADS": "1",
        "VECLIB_MAXIMUM_THREADS": "1",
        "MKL_NUM_THREADS": "1",
        "VESPER_EXPERIMENTAL_PREFIX_CACHE": "0",
    }
    env.update(overrides)
    write(root / "experiment-env.json", overrides)
    jobs = []
    for block, modes in enumerate((("software", "hardware"), ("hardware", "software"))):
        for mode in modes:
            name = f"pipeline-{block}-{mode}"
            jobs.append(
                (
                    name,
                    mode,
                    "pipeline",
                    block,
                    [
                        str(binaries / f"{mode}-audio_frontend"),
                        "benchmarks/asr/audio-frontend-artifacts.json",
                        "raw",
                        str(root / f"block-{block}.json"),
                        str(corpus / "corpus"),
                        "models/downloads",
                        str(root / "environment.json"),
                        str(root / "runs" / f"{name}.json"),
                        "cohere",
                        "offline",
                        "clean",
                    ],
                )
            )
    for mode in ("hardware", "software"):
        name = f"stages-{mode}"
        jobs.append(
            (
                name,
                mode,
                "stages",
                None,
                [
                    str(binaries / f"{mode}-cohere_stage_probe"),
                    str(corpus / "cohere-diagnostic.json"),
                    str(corpus / "corpus"),
                    "models/downloads",
                    str(root / "environment.json"),
                    str(root / "runs" / f"{name}.json"),
                ],
            )
        )
    processes = []
    for name, mode, kind, block, command in jobs:
        print(f"START {name}", flush=True)
        power = subprocess.check_output(["pmset", "-g", "batt"], text=True)
        started = time.monotonic()
        log_path = root / "runs" / f"{name}.log"
        with log_path.open("w") as log:
            process = subprocess.run(
                ["/usr/bin/time", "-l", *command],
                stdout=log,
                stderr=log,
                env=env,
                check=False,
            )
        rss = re.search(r"(\d+)\s+maximum resident set size", log_path.read_text())
        processes.append(
            {
                "name": name,
                "mode": mode,
                "kind": kind,
                "block": block,
                "returnCode": process.returncode,
                "powerAtStart": power,
                "wallSeconds": time.monotonic() - started,
                "peakRssBytes": int(rss[1]) if rss else None,
                "executableSha256": digest(Path(command[0])),
                "command": command,
            }
        )
        write(root / "processes.json", processes)
        print(f"DONE {name}: {process.returncode}", flush=True)
    summary = {
        "schemaVersion": 1,
        "environment": environment,
        "runnerSha256": digest(Path(__file__)),
        "processes": processes,
        "cases": [],
        "stages": {},
        "metrics": {},
        "limitations": [
            "Actual ASR+writer stages, not microphone/GUI/request-to-visible timing",
            "30 paired completed-file attempts per mode; same fixed two-block order",
            "Quantiles use nearest rank; variable-duration corpus, not repeated fixed-size service requests",
            "Fresh Cohere each request; two process-cold and 28 resident-writer requests per mode",
            "Filesystem cache not flushed; no causal thermal or cross-device claim",
        ],
    }
    by_mode = {"software": {}, "hardware": {}}
    for process in processes:
        report_path = root / "runs" / f"{process['name']}.json"
        if not report_path.exists():
            continue
        report = read(report_path)
        if process["kind"] == "stages":
            summary["stages"][process["mode"]] = [
                {
                    **{k: v for k, v in r.items() if k not in ("raw", "error")},
                    "rawSha256": text_hash(r.get("raw")),
                    "failed": bool(r.get("error")),
                }
                for r in report["runs"]
            ]
            continue
        for index, row in enumerate(report["runs"]):
            if row["id"] in by_mode[process["mode"]]:
                raise ValueError("duplicate result ID")
            by_mode[process["mode"]][row["id"]] = row
            summary["cases"].append(
                {
                    **{
                        k: v
                        for k, v in row.items()
                        if k not in ("reference", "raw", "cleaned", "error", "cleanupError")
                    },
                    "mode": process["mode"],
                    "block": process["block"],
                    "writerState": "process-cold" if index == 0 else "resident-weights",
                    "rawSha256": text_hash(row.get("raw")),
                    "cleanedSha256": text_hash(row.get("cleaned")),
                    "failed": bool(row.get("error")),
                    "cleanupRejected": bool(row.get("cleanupError")),
                }
            )
    for mode in by_mode:
        rows = [r for r in summary["cases"] if r["mode"] == mode]
        summary["metrics"][mode] = {
            "expected": 30,
            "present": len(rows),
            "failures": sum(r["failed"] for r in rows),
            "cleanupRejections": sum(r["cleanupRejected"] for r in rows),
            "asrMs": distribution([r["asrMs"] for r in rows if "asrMs" in r]),
            "inferenceStagesMs": distribution(
                [r["asrMs"] + r.get("cleanupMs", 0) for r in rows if "asrMs" in r]
            ),
            "residentWriterMs": distribution(
                [
                    r["cleanupMs"]
                    for r in rows
                    if r["writerState"] == "resident-weights" and "cleanupMs" in r
                ]
            ),
            "coldWriterMs": [
                r.get("cleanupMs") for r in rows if r["writerState"] == "process-cold"
            ],
        }
    keys = (
        "inputPcmSha256",
        "outputPcmSha256",
        "raw",
        "cleaned",
        "rawScore",
        "cleanedScore",
        "nonSpeechScore",
        "error",
        "cleanupError",
    )
    summary["comparisons"] = []
    for sample in samples:
        left = by_mode["software"].get(sample["id"])
        right = by_mode["hardware"].get(sample["id"])
        summary["comparisons"].append(
            {
                "id": sample["id"],
                "presentBoth": left is not None and right is not None,
                "differentFields": [k for k in keys if left.get(k) != right.get(k)]
                if left is not None and right is not None
                else ["missing"],
            }
        )
    write(root / "summary.json", summary)
    stage_keys = (
        "inputPcmSha256",
        "rawSha256",
        "rawScore",
        "completion",
        "completionAccepted",
    )
    stage_maps = {mode: {r["id"]: r for r in summary["stages"].get(mode, [])} for mode in by_mode}
    summary["stageComparisons"] = []
    for sample in read(corpus / "cohere-diagnostic.json")["samples"]:
        left = stage_maps["software"].get(sample["id"])
        right = stage_maps["hardware"].get(sample["id"])
        summary["stageComparisons"].append(
            {
                "id": sample["id"],
                "differentFields": [k for k in stage_keys if left.get(k) != right.get(k)]
                if left is not None and right is not None
                else ["missing"],
            }
        )
    write(root / "summary.json", summary)
    failed = (
        any(p["returnCode"] for p in processes)
        or any(r["differentFields"] for r in summary["comparisons"])
        or any(r["differentFields"] for r in summary["stageComparisons"])
        or any(r["failed"] for r in summary["cases"])
        or any(
            len(summary["stages"].get(mode, [])) != 5
            or any(r["failed"] for r in summary["stages"].get(mode, []))
            for mode in by_mode
        )
    )
    if failed:
        raise SystemExit("qualification has failures; evidence preserved")


if __name__ == "__main__":
    main()
