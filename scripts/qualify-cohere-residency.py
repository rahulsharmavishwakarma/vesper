#!/usr/bin/env python3
"""Compare frozen per-job/resident Cohere binaries, preserving all failed cases."""

import argparse
import hashlib
import json
import math
import os
import re
import subprocess
import time
from pathlib import Path


def read(path):
    return json.loads(path.read_text())


def write(path, data):
    path.write_text(json.dumps(data, indent=2) + "\n")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def distribution(values):
    values = sorted(values)
    if not values:
        return {"n": 0}
    return {
        "n": len(values),
        "p50": values[math.ceil(0.50 * len(values)) - 1],
        "p95": values[math.ceil(0.95 * len(values)) - 1],
        "min": values[0],
        "max": values[-1],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("corpus", type=Path)
    parser.add_argument("french", type=Path)
    args = parser.parse_args()
    root, corpus, french = args.root, args.corpus, args.french
    (root / "runs").mkdir(exist_ok=False)
    clean = read(corpus / "asr-clean.json")
    assert len(clean["samples"]) == 29
    clean["samples"].append({**clean["samples"][0], "id": "resident-final-reset"})
    manifests = {}
    for i in range(2):
        name = f"english-{i}"
        manifests[name] = {**clean, "samples": clean["samples"][i * 15 : (i + 1) * 15]}
    diagnostic = read(corpus / "cohere-diagnostic.json")
    noise = read(corpus / "asr-noise.json")
    manifests["diagnostic"] = {
        "schemaVersion": 1,
        "sources": diagnostic["sources"],
        "samples": diagnostic["samples"]
        + noise["samples"][:3]
        + [{**clean["samples"][0], "id": "after-diagnostic-reset"}],
    }
    manifests["french"] = read(french / "fr_fr-manifest.json")
    for name, manifest in manifests.items():
        write(root / f"{name}.json", manifest)
    env = {
        k: v for k, v in os.environ.items() if not k.startswith(("TRANSCRIBE_", "GGML_", "LLAMA_"))
    }
    overrides = {
        "OMP_NUM_THREADS": "1",
        "OPENBLAS_NUM_THREADS": "1",
        "VECLIB_MAXIMUM_THREADS": "1",
        "MKL_NUM_THREADS": "1",
        "TRANSCRIBE_EXPERIMENTAL_PREFIX_CACHE": "0",
    }
    env.update(overrides)
    write(root / "runtime-env.json", overrides)
    jobs = []
    for index, lane in enumerate(manifests):
        for mode in ["baseline", "candidate"] if index % 2 == 0 else ["candidate", "baseline"]:
            jobs.append((lane, mode))
    ledger = []
    for lane, mode in jobs:
        name = f"{lane}-{mode}"
        binary = root / "binaries" / f"{mode}-eval"
        output = root / "runs" / f"{name}.json"
        command = [
            str(binary),
            "run",
            str(root / f"{lane}.json"),
            str(french if lane == "french" else corpus / "corpus"),
            "models/downloads",
            str(root / "environment.json"),
            str(output),
            "--model",
            "CohereLabs/cohere-transcribe-03-2026",
            "--language",
            "fr" if lane == "french" else "en",
        ]
        if lane != "french":
            command.append("--cleanup")
        power = subprocess.check_output(["pmset", "-g", "batt"], text=True).splitlines()[0]
        started = time.monotonic()
        print("START", name, flush=True)
        log = root / "runs" / f"{name}.log"
        with log.open("wb") as f:
            result = subprocess.run(
                ["/usr/bin/time", "-l", *command],
                env=env,
                stdout=f,
                stderr=f,
                check=False,
            )
        rss = re.search(r"(\d+)\s+maximum resident set size", log.read_text(errors="replace"))
        ledger.append(
            {
                "name": name,
                "returnCode": result.returncode,
                "wallSeconds": time.monotonic() - started,
                "peakRssBytes": int(rss[1]) if rss else None,
                "powerAtStart": power,
                "executableSha256": digest(binary),
                "command": command,
            }
        )
        write(root / "processes.json", ledger)
        print("DONE", name, result.returncode, flush=True)
    reports = {p.stem: read(p) for p in (root / "runs").glob("*.json")}
    summary = {
        "runnerSha256": digest(Path(__file__)),
        "processes": ledger,
        "comparisons": [],
        "metrics": {},
    }
    fields = [
        "rawHypothesis",
        "cleanedHypothesis",
        "personalizedHypothesis",
        "rawScore",
        "cleanedScore",
        "personalizedScore",
        "error",
    ]
    for lane, manifest in manifests.items():
        variants = {
            mode: {r["sampleId"]: r for r in reports.get(f"{lane}-{mode}", {}).get("runs", [])}
            for mode in ["baseline", "candidate"]
        }
        for sample in manifest["samples"]:
            a, b = (variants[mode].get(sample["id"]) for mode in ["baseline", "candidate"])
            summary["comparisons"].append(
                {
                    "lane": lane,
                    "id": sample["id"],
                    "differentFields": [k for k in fields if a.get(k) != b.get(k)]
                    if a is not None and b is not None
                    else ["missing"],
                }
            )
    for mode in ["baseline", "candidate"]:
        for lane in ["english", "diagnostic", "french"]:
            rows = [
                r
                for name, report in reports.items()
                if name.startswith(lane) and name.endswith(mode)
                for r in report["runs"]
            ]
            summary["metrics"][f"{lane}-{mode}"] = {
                "present": len(rows),
                "errors": sum(bool(r.get("error")) for r in rows),
                "asrMs": distribution([r["asrMs"] for r in rows]),
                "asrAndWriterMs": distribution(
                    [r["asrMs"] + (r.get("cleanupMs") or 0) for r in rows]
                ),
            }
    write(root / "summary.json", summary)
    # Matching pre-existing quality/native failures are data, not successful ASR.
    # They remain in counts; changed/missing outcomes or process failures fail parity.
    if (
        len(reports) != len(jobs)
        or any(p["returnCode"] for p in ledger)
        or any(r["differentFields"] for r in summary["comparisons"])
    ):
        raise SystemExit("regression or missing result; evidence preserved")


if __name__ == "__main__":
    main()
