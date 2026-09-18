#!/usr/bin/env python3
"""Run frozen audio regression jobs sequentially; retain process failures."""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import time
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("phase", choices=["asr", "frontend", "contracts", "desert", "paced"])
    args = parser.parse_args()
    root = args.root
    results = root / "runs"
    results.mkdir(exist_ok=True)
    binary = root / "audio_frontend"
    if not binary.exists():
        shutil.copy2("target/release/examples/audio_frontend", binary)
    environment = root / "environment.json"
    if not environment.exists():
        env = json.loads(
            Path(
                ".cache/benchmarks/results/audio-frontends-2026-09-16/environment.json"
            ).read_text()
        )
        env["label"] = "audio-hard-regression-2026-09-16"
        env["notes"] = (
            "Sequential models; frontend1thread; existing ASR/writer<=8; no controlled other system load or thermal telemetry. Exact source/executable/artifact/manifest hashes in reports."
        )
        environment.write_text(json.dumps(env, indent=2) + "\n")
    jobs = []

    def native(name, model, manifest, asr="none", delivery="offline", packets="320"):
        jobs.append(
            (
                name,
                [
                    str(binary),
                    "benchmarks/asr/audio-frontend-artifacts.json",
                    model,
                    str(root / f"{manifest}.json"),
                    str(root / "corpus"),
                    "models/downloads",
                    str(environment),
                    str(results / f"{name}.json"),
                    asr,
                    delivery,
                    "clean",
                    packets,
                ],
            )
        )

    if args.phase == "asr":
        # Rotate model order by condition to avoid assigning every cold block to raw.
        full = json.loads((root / "asr.json").read_text())
        for index, variant in enumerate(["clean", "quiet30", "natural10", "natural0", "noise"]):
            name = "asr-" + variant
            rows = [s for s in full["samples"] if s["variant"] == variant]
            (root / f"{name}.json").write_text(
                json.dumps({**full, "samples": rows}, indent=2) + "\n"
            )
            models = ["raw", "dpdfnet4", "dpdfnet8"]
            models = models[index % 3 :] + models[: index % 3]
            for model in models:
                native(f"asr-{variant}-{model}", model, name, "cohere")
    elif args.phase == "frontend":
        for model in ["silero-v6.2", "dpdfnet4", "dpdfnet8"]:
            native(f"frontend-{model}", model, "frontend")
    elif args.phase == "contracts":
        for model in ["silero-v6.2", "dpdfnet4", "dpdfnet8"]:
            for delivery, packets in [
                ("offline", "320"),
                ("chunks", "320"),
                ("chunks", "1,63,159,511,320,1600,17"),
            ]:
                label = "variable" if "," in packets else delivery
                native(
                    f"boundaries-{model}-{label}",
                    model,
                    "boundaries",
                    delivery=delivery,
                    packets=packets,
                )
            native(f"continuous-{model}-chunks", model, "continuous", delivery="chunks")
    elif args.phase == "paced":
        for model in ["silero-v6.2", "dpdfnet8"]:
            native(f"continuous-{model}-realtime", model, "continuous", delivery="realtime")
    else:
        for model, manifest in [
            ("ear", "ear"),
            ("uhm-raw", "uhm"),
            ("uhm-normalized", "uhm"),
            ("uhm-raw", "filler-controls"),
            ("uhm-normalized", "filler-controls"),
            ("uhm-raw", "ami"),
            ("uhm-normalized", "ami"),
        ]:
            if not (root / f"{manifest}.json").exists():
                raise FileNotFoundError(f"required frozen manifest missing: {manifest}")
            name = f"{manifest}-{model}"
            command = [
                ".cache/audio-eval-venv/bin/python",
                "scripts/benchmark-desert-ant.py",
                model,
                "--artifacts",
                "benchmarks/asr/audio-frontend-artifacts.json",
                "--model-root",
                ".cache/benchmarks/models/audio-frontends",
                "--manifest",
                str(root / f"{manifest}.json"),
                "--corpus",
                str(root / "corpus"),
                "--environment",
                str(environment),
                "--output",
                str(results / f"{name}.json"),
            ]
            if model == "ear":
                command += ["--golden", str(root / "ear-golden.json")]
            jobs.append((name, command))
    ledger = root / f"processes-{args.phase}.json"
    if ledger.exists():
        raise FileExistsError(
            "refusing to overwrite prior process results; use another result root"
        )
    completed = []
    env = {
        **os.environ,
        "OMP_NUM_THREADS": "1",
        "OPENBLAS_NUM_THREADS": "1",
        "VECLIB_MAXIMUM_THREADS": "1",
        "MKL_NUM_THREADS": "1",
    }
    for name, command in jobs:
        if (results / f"{name}.json").exists():
            raise FileExistsError(f"refusing to overwrite completed or partial report: {name}")
        print(f"START {name}", flush=True)
        started = time.time()
        with (results / f"{name}.log").open("w") as log:
            process = subprocess.run(
                ["/usr/bin/time", "-l", *command],
                stdout=log,
                stderr=log,
                env=env,
                check=False,
            )
        row = {
            "name": name,
            "returnCode": process.returncode,
            "startedUnixSeconds": started,
            "wallSeconds": time.time() - started,
            "command": command,
            "runnerSha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        }
        completed.append(row)
        ledger.write_text(json.dumps(completed, indent=2) + "\n")
        print(
            f"DONE {name}: exit={process.returncode} seconds={row['wallSeconds']:.1f}",
            flush=True,
        )


if __name__ == "__main__":
    main()
