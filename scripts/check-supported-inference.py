#!/usr/bin/env python3
"""Compare frozen application binaries after shared request-path fixes.

Uses installed models and the pinned public hard-regression corpus. No downloads,
compilation, microphone access or concurrent timed models. Keep all failures.
"""

import argparse
import hashlib
import json
import os
import re
import statistics
import subprocess
import time
from pathlib import Path

MODELS = [
    ("cohere", "CohereLabs/cohere-transcribe-03-2026"),
    ("qwen-asr", "Qwen/Qwen3-ASR-0.6B"),
    ("parakeet", "nvidia/parakeet-tdt-0.6b-v3"),
    ("nemotron", "nvidia/nemotron-3.5-asr-streaming-0.6b"),
    ("lfm", "LiquidAI/LFM2.5-Audio-1.5B"),
]


def read(path):
    return json.loads(path.read_text())


def write(path, data):
    path.write_text(json.dumps(data, indent=2) + "\n")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("corpus", type=Path)
    parser.add_argument("--models", nargs="+", choices=[name for name, _ in MODELS])
    parser.add_argument("--offline-only", action="store_true")
    args = parser.parse_args()
    root, corpus = args.root, args.corpus
    runs = root / "runs"
    runs.mkdir(exist_ok=False)
    manifest = read(corpus / "asr-clean.json")
    manifest["samples"] = [manifest["samples"][i] for i in [0, 1, 5, 6, 8, 9, 24, 28]]
    write(root / "offline.json", manifest)
    environment = read(root / "environment.json")
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
    selected = set(args.models or [name for name, _ in MODELS])
    streaming = "nemotron" in selected and not args.offline_only
    for index, (name, model) in enumerate(MODELS):
        if name not in selected:
            continue
        order = ["baseline", "candidate"] if index % 2 == 0 else ["candidate", "baseline"]
        for mode in order:
            jobs.append((f"{name}-{mode}", mode, model, root / "offline.json", ["--cleanup"]))
    for mode in ["baseline", "candidate"] if streaming else []:
        jobs.append(
            (
                f"stream-{mode}",
                mode,
                MODELS[3][1],
                corpus / "continuous.json",
                ["--streaming"],
            )
        )
    if streaming:
        jobs.append(
            (
                "paced-candidate",
                "candidate",
                MODELS[3][1],
                corpus / "continuous.json",
                ["--streaming", "--realtime"],
            )
        )
    ledger = []
    for name, mode, model, input_manifest, extra in jobs:
        binary = root / "binaries" / f"{mode}-eval"
        command = [
            str(binary),
            "run",
            str(input_manifest),
            str(corpus / "corpus"),
            ".cache/benchmarks/model-roots/nemotron-cpu"
            if model == MODELS[3][1]
            else "models/downloads",
            str(root / "environment.json"),
            str(runs / f"{name}.json"),
            "--model",
            model,
            "--language",
            "en",
            *extra,
        ]
        power = subprocess.check_output(["pmset", "-g", "batt"], text=True)
        print(f"START {name}", flush=True)
        started = time.monotonic()
        with (runs / f"{name}.log").open("w") as log:
            process = subprocess.run(
                ["/usr/bin/time", "-l", *command],
                env=env,
                stdout=log,
                stderr=log,
                check=False,
            )
        rss = re.search(
            r"(\d+)\s+maximum resident set size",
            (runs / f"{name}.log").read_text(errors="replace"),
        )
        ledger.append(
            {
                "name": name,
                "returnCode": process.returncode,
                "wallSeconds": time.monotonic() - started,
                "peakRssBytes": int(rss[1]) if rss else None,
                "powerAtStart": power.splitlines()[0],
                "executableSha256": digest(binary),
                "command": command,
            }
        )
        write(root / "processes.json", ledger)
        print(f"DONE {name}: {process.returncode}", flush=True)
    reports = {p.stem: read(p) for p in runs.glob("*.json")}
    summary = {
        "schemaVersion": 1,
        "environment": environment,
        "runnerSha256": digest(Path(__file__)),
        "processes": ledger,
        "comparisons": [],
        "metrics": {},
    }
    expected = {s["id"] for s in manifest["samples"]}
    fields = [
        "rawHypothesis",
        "cleanedHypothesis",
        "personalizedHypothesis",
        "rawScore",
        "cleanedScore",
        "personalizedScore",
        "error",
    ]
    for name in [m[0] for m in MODELS if m[0] in selected] + (["stream"] if streaming else []):
        ids = (
            expected
            if name != "stream"
            else {s["id"] for s in read(corpus / "continuous.json")["samples"]}
        )
        variants = {
            mode: {r["sampleId"]: r for r in reports.get(f"{name}-{mode}", {}).get("runs", [])}
            for mode in ["baseline", "candidate"]
        }
        for sample_id in sorted(ids):
            a, b = (
                variants["baseline"].get(sample_id),
                variants["candidate"].get(sample_id),
            )
            summary["comparisons"].append(
                {
                    "lane": name,
                    "id": sample_id,
                    "differentFields": [k for k in fields if a.get(k) != b.get(k)]
                    if a is not None and b is not None
                    else ["missing"],
                }
            )
    for name, report in reports.items():
        rows = report["runs"]
        expected_count = 3 if name.startswith(("stream-", "paced-")) else 8
        summary["metrics"][name] = {
            "expected": expected_count,
            "present": len(rows),
            "failures": sum(bool(r.get("error")) for r in rows),
            "runtimeFailures": sum(
                bool(r.get("error"))
                and not (
                    r["error"].startswith("the polished transcript failed the fidelity check (")
                    and r.get("rawHypothesis") is not None
                    and r.get("cleanedHypothesis") is None
                )
                for r in rows
            ),
            "medianAsrMs": statistics.median(r["asrMs"] for r in rows) if rows else None,
            "medianWriterMs": statistics.median(
                r["cleanupMs"] for r in rows if r.get("cleanupMs") is not None
            )
            if any(r.get("cleanupMs") is not None for r in rows)
            else None,
        }
    paced = {r["sampleId"]: r for r in reports.get("paced-candidate", {}).get("runs", [])}
    accelerated = {r["sampleId"]: r for r in reports.get("stream-candidate", {}).get("runs", [])}
    summary["pacedComparisons"] = [
        {
            "id": sample_id,
            "sameFinalText": sample_id in paced
            and sample_id in accelerated
            and paced[sample_id].get("error") is None
            and accelerated[sample_id].get("error") is None
            and paced[sample_id].get("rawHypothesis") is not None
            and paced[sample_id].get("rawHypothesis")
            == accelerated[sample_id].get("rawHypothesis"),
        }
        for sample_id in (
            {s["id"] for s in read(corpus / "continuous.json")["samples"]} if streaming else []
        )
    ]
    write(root / "summary.json", summary)
    if (
        len(reports) != len(jobs)
        or any(p["returnCode"] for p in ledger)
        or any(r["differentFields"] for r in summary["comparisons"])
        or any(not r["sameFinalText"] for r in summary["pacedComparisons"])
        or any(m["runtimeFailures"] for m in summary["metrics"].values())
        or any(m["present"] != m["expected"] for m in summary["metrics"].values())
    ):
        raise SystemExit("regression or missing result; all evidence preserved")


if __name__ == "__main__":
    main()
