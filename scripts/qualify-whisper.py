#!/usr/bin/env python3
"""Run an isolated Whisper native screen and the production Cohere baseline.

Requires preverified artifact JSONs and frozen binaries; no downloads or builds.
Use the dated protocol in docs/validation/whisper-2026-09-18.md.
"""

import argparse
import hashlib
import json
import os
import re
import subprocess
import time
from pathlib import Path


def read(path):
    return json.loads(path.read_text())


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def competing_work(own_group=None):
    watched = {
        "rustc",
        "clang",
        "clang++",
        "cc1",
        "cc1plus",
        "ld",
        "cargo",
        "vesper-eval",
        "whisper-probe",
        "whisper_probe",
        "audio_frontend",
        "asr_bench_server",
        "asr_bench_server_prefill",
        "asr_server",
        "llama-mtmd-cli",
    }
    found = []
    for line in subprocess.check_output(
        ["ps", "-ww", "-axo", "pgid,pcpu,comm"], text=True
    ).splitlines()[1:]:
        fields = line.strip().split(None, 2)
        if len(fields) != 3:
            continue
        group, cpu, command = fields
        name = Path(command).name
        if int(group) != own_group and ((name in watched and float(cpu) > 10) or float(cpu) >= 80):
            found.append({"process": name, "cpuPercent": float(cpu)})
    return found


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("artifacts", type=Path)
    parser.add_argument("corpus", type=Path)
    parser.add_argument("languages", type=Path)
    parser.add_argument("--english-only", action="store_true")
    parser.add_argument(
        "--application",
        action="store_true",
        help="Run Small/Turbo through the production adapter and common evaluator",
    )
    args = parser.parse_args()
    root = args.output
    runs = root / "runs"
    runs.mkdir(exist_ok=False)
    clean = read(args.corpus / "asr-clean.json")
    clean["samples"].append({**clean["samples"][0], "id": "whisper-final-reset"})
    assert len(clean["samples"]) == 30
    jobs = []
    for block in range(2):
        lane = f"english-{block}"
        manifest = root / f"{lane}.json"
        write(
            manifest,
            {**clean, "samples": clean["samples"][block * 15 : (block + 1) * 15]},
        )
        models = (
            ["cohere", "small", "turbo"]
            if args.application
            else ["cohere", "tiny", "small", "turbo"]
        )
        for model in models if block == 0 else reversed(models):
            jobs.append((lane, model, manifest, args.corpus / "corpus", "en", True))
    diagnostic = read(args.corpus / "cohere-diagnostic.json")
    diagnostic["samples"] += read(args.corpus / "asr-noise.json")["samples"][:3]
    diagnostic["samples"].append({**clean["samples"][0], "id": "after-diagnostic-reset"})
    write(root / "diagnostic.json", diagnostic)
    for model in (
        ["cohere", "small", "turbo"] if args.application else ["cohere", "tiny", "small", "turbo"]
    ):
        jobs.append(
            (
                "diagnostic",
                model,
                root / "diagnostic.json",
                args.corpus / "corpus",
                "en",
                True,
            )
        )
    for lang, tag in [("fr", "fr_fr"), ("hi", "hi_in")]:
        manifest = root / f"{lang}.json"
        write(manifest, read(args.languages / f"{tag}-manifest.json"))
        for mode in [lang, "auto"]:
            for model in ["small", "turbo"] if args.application else ["tiny", "small", "turbo"]:
                jobs.append((f"{lang}-{mode}", model, manifest, args.languages, mode, False))
    # A continuous input is an explicit compatibility test, not a streaming run.
    long_manifest = root / "duration-boundary.json"
    write(long_manifest, read(args.corpus / "continuous.json"))
    for model in ["cohere", "small", "turbo"] if args.application else ["tiny", "small", "turbo"]:
        jobs.append(
            (
                "duration-boundary",
                model,
                long_manifest,
                args.corpus / "corpus",
                "en",
                False,
            )
        )
    if args.english_only:
        jobs = [job for job in jobs if job[0].startswith("english-")]
    write(
        root / "plan.json",
        [
            {
                "name": f"{lane}-{model}",
                "sampleIds": [s["id"] for s in read(manifest)["samples"]],
            }
            for lane, model, manifest, *_ in jobs
        ],
    )
    runner_sha256 = digest(Path(__file__))
    write(root / "runner-start.json", {"sha256": runner_sha256, "jobs": len(jobs)})
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
    write(root / "runtime-env.json", overrides)
    ledger = []
    for lane, model, manifest, corpus, language, cleanup in jobs:
        name = f"{lane}-{model}"
        output = runs / f"{name}.json"
        if model == "cohere" or args.application:
            binary = root / "binaries" / "vesper-eval"
            command = [
                str(binary),
                "run",
                str(manifest),
                str(corpus),
                "models/downloads",
                str(root / "environment.json"),
                str(output),
                "--model",
                {
                    "cohere": "CohereLabs/cohere-transcribe-03-2026",
                    "small": "openai/whisper-small",
                    "turbo": "openai/whisper-large-v3-turbo",
                }[model],
                "--language",
                language,
            ]
            if cleanup:
                command.append("--cleanup")
        else:
            binary = root / "binaries" / "whisper-probe"
            command = [
                str(binary),
                str(manifest),
                str(corpus),
                str(args.artifacts / f"{model}.json"),
                str(root / "environment.json"),
                str(output),
                language,
                "1000",
                "models/downloads",
                str(cleanup).lower(),
            ]
        wait_started = time.monotonic()
        while competing_work():
            if time.monotonic() - wait_started >= 120:
                write(
                    root / "incomplete.json",
                    {
                        "reason": "competing build/inference for 120 seconds",
                        "beforeJob": name,
                        "completedJobs": len(ledger),
                    },
                )
                raise SystemExit("No quiet benchmark slot; partial evidence retained")
            print("WAIT competing build/inference before", name, flush=True)
            time.sleep(5)
        print("START", name, flush=True)
        power = subprocess.check_output(["pmset", "-g", "batt"], text=True).splitlines()[0]
        started = time.monotonic()
        log = runs / f"{name}.log"
        interference = []
        with log.open("wb") as file:
            process = subprocess.Popen(
                ["/usr/bin/time", "-l", *command],
                env=env,
                stdout=file,
                stderr=file,
                start_new_session=True,
            )
            while process.poll() is None:
                found = competing_work(process.pid)
                if found:
                    interference.append({"seconds": time.monotonic() - started, "observed": found})
                time.sleep(1)
        log_text = log.read_text(errors="replace")
        rss = re.search(r"(\d+)\s+maximum resident set size", log_text)
        try:
            report = read(output)
        except (OSError, ValueError):
            report = {"runs": []}
        expected = {s["id"] for s in read(manifest)["samples"]}
        present = {r["sampleId"] for r in report["runs"]}
        ledger.append(
            {
                "name": name,
                "returnCode": process.returncode,
                "wallSeconds": time.monotonic() - started,
                "peakRssBytes": int(rss[1]) if rss else None,
                "powerAtStart": power,
                "competingWork": interference,
                "executableSha256": digest(binary),
                "expected": len(expected),
                "present": len(present),
                "missingIds": sorted(expected - present),
                "nativeExceptionLog": bool(
                    re.search(
                        r"Caught exception|Ort::Exception|Exception caught|Whisper decode failed",
                        log_text,
                    )
                ),
                "command": command,
            }
        )
        write(root / "processes.json", ledger)
        print(
            "DONE",
            name,
            process.returncode,
            len(present),
            "/",
            len(expected),
            flush=True,
        )
    write(root / "runner.json", {"sha256": runner_sha256, "jobs": len(jobs)})


if __name__ == "__main__":
    main()
