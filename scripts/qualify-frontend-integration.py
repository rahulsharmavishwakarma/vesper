"""Sequential application-path frontend qualification; preserve failed jobs.

Run preparation and the release build first. The native model contracts run
separately. Output contains local transcripts; publish with an explicit redactor.
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import time
from pathlib import Path

ROOT = Path(".cache/benchmarks/results/frontend-integration-2026-09-18")


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def processes(own_group=None):
    external = []
    rss = 0
    for line in subprocess.check_output(
        ["ps", "-ww", "-axo", "pgid,pid,pcpu,rss,comm"], text=True
    ).splitlines()[1:]:
        fields = line.strip().split(None, 4)
        if len(fields) != 5:
            continue
        group, _pid, cpu, memory, command = fields
        name = Path(command).name
        if int(group) == own_group:
            rss += int(memory) * 1024
        elif (
            float(cpu) >= 50
            or name
            in {
                "cargo",
                "rustc",
                "clang",
                "clang++",
                "ld",
                "vesper-eval",
                "policy_learning_replay",
            }
            and float(cpu) > 10
        ):
            external.append({"process": name, "cpuPercent": float(cpu)})
    return external, rss


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", choices=["quality", "streaming", "soak"])
    args = parser.parse_args()
    runs = ROOT / "runs"
    runs.mkdir(exist_ok=True)
    binary = ROOT / "vesper-eval"
    if not binary.exists():
        shutil.copy2("target/release/vesper-eval", binary)
    elif digest(binary) != digest(Path("target/release/vesper-eval")):
        raise SystemExit("Frozen binary differs from current build; do not mix revisions")
    runtime = json.loads(Path(".tooling/sherpa-runtime/current.json").read_text())
    environment_path = ROOT / "environment.json"
    if not environment_path.exists():
        env = json.loads(
            Path(
                ".cache/benchmarks/results/whisper-adapter-2026-09-18/environment.json"
            ).read_text()
        )
        env.update(
            label="frontend-integration-2026-09-18",
            powerMode="See per-process powerAtStart; no sustained power or temperature telemetry",
            runtimeRevision=f"sherpa-onnx1.13.8 {runtime['revision']}; build {runtime['buildId']}; NeMo-Speech.cpp0.1.0; llama-cpp-2 0.1.156",
            precision="Pinned Silero/DPDF ONNX; Cohere/Whisper INT8; Nemotron Q8_0; writer Q4_0",
            notes="Production manager; frontend1thread, Sherpa ASR<=8, NeMo4; no affinity; record competition and RSS; microphone/edge/word-aligned emission/power watts unmeasured; no endpoint-based Stop",
        )
        write(environment_path, env)
        paths = [
            "Cargo.lock",
            "models/artifacts.json",
            "scripts/qualify-frontend-integration.py",
            "scripts/prepare-frontend-integration.py",
            "patches/sherpa-onnx/decode-report.patch",
        ]
        for folder in [
            "crates/vesper-audio/src",
            "crates/vesper-core/src",
            "crates/vesper-inference/src",
            "crates/vesper-eval/src",
            "crates/vesper-models/src",
        ]:
            paths += [str(p) for p in Path(folder).rglob("*.rs")]
        write(ROOT / "sources.json", {p: digest(Path(p)) for p in sorted(paths)})
    models = {
        "cohere": "CohereLabs/cohere-transcribe-03-2026",
        "small": "openai/whisper-small",
        "turbo": "openai/whisper-large-v3-turbo",
        "nemotron": "nvidia/nemotron-3.5-asr-streaming-0.6b",
    }
    profiles = {
        "raw": [],
        "vad": ["--vad"],
        "dpdfnet4": ["--vad", "--denoiser", "dpdfnet4"],
        "dpdfnet8": ["--vad", "--denoiser", "dpdfnet8"],
    }
    jobs = []
    if args.phase == "quality":
        for i, model in enumerate(["cohere", "small", "turbo"]):
            order = list(profiles)
            order = order[i:] + order[:i]
            for profile in order:
                jobs.append((model, profile, "quality", False))
    elif args.phase == "streaming":
        for profile in profiles:
            jobs.append(("nemotron", profile, "streaming", True))
    else:
        jobs.append(("nemotron", "dpdfnet4", "soak", True))
    overrides = {
        "OMP_NUM_THREADS": "1",
        "OPENBLAS_NUM_THREADS": "1",
        "VECLIB_MAXIMUM_THREADS": "1",
        "MKL_NUM_THREADS": "1",
        "VESPER_EXPERIMENTAL_PREFIX_CACHE": "0",
    }
    env = {**os.environ, **overrides}
    write(ROOT / "runtime-env.json", overrides)
    ledger_path = ROOT / f"processes-{args.phase}.json"
    ledger = json.loads(ledger_path.read_text()) if ledger_path.exists() else []
    completed = {r["name"] for r in ledger}
    for model, profile, manifest_name, streaming in jobs:
        name = f"{args.phase}-{model}-{profile}"
        if name in completed:
            continue
        manifest = ROOT / f"{manifest_name}.json"
        out = runs / f"{name}.json"
        log = runs / f"{name}.log"
        model_root = (
            ".cache/benchmarks/model-roots/nemotron-cpu" if streaming else "models/downloads"
        )
        command = [
            str(binary),
            "run",
            str(manifest),
            str(ROOT / "corpus"),
            model_root,
            str(environment_path),
            str(out),
            "--model",
            models[model],
            "--language",
            "en",
            *profiles[profile],
        ]
        if streaming:
            command += ["--streaming", "--realtime", "--chunk-ms", "20"]
        else:
            command += ["--cleanup"]
        # Bound waiting; retain contention instead of quietly discarding quality runs.
        wait = time.monotonic()
        while processes()[0] and time.monotonic() - wait < 30:
            time.sleep(5)
        started = time.monotonic()
        external = []
        rss = []
        print("START", name, flush=True)
        power = subprocess.check_output(["pmset", "-g", "batt"], text=True).splitlines()[0]
        thermal_start = subprocess.run(
            ["pmset", "-g", "therm"], capture_output=True, text=True, check=False
        ).stdout.strip()
        with log.open("wb") as output:
            proc = subprocess.Popen(
                ["/usr/bin/time", "-l", *command],
                stdout=output,
                stderr=output,
                env=env,
                start_new_session=True,
            )
            next_progress = 60
            while proc.poll() is None:
                active, memory = processes(proc.pid)
                elapsed = time.monotonic() - started
                if active:
                    external.append({"seconds": elapsed, "observed": active})
                if int(elapsed) % 5 == 0:
                    rss.append({"seconds": elapsed, "bytes": memory})
                if elapsed >= next_progress:
                    print("RUNNING", name, round(elapsed), "seconds", flush=True)
                    next_progress += 60
                time.sleep(1)
        text = log.read_text(errors="replace")
        peak = re.search(r"(\d+)\s+maximum resident set size", text)
        try:
            report = json.loads(out.read_text())
        except (OSError, ValueError):
            report = {"runs": []}
        expected = {r["id"] for r in json.loads(manifest.read_text())["samples"]}
        present = {r["sampleId"] for r in report["runs"]}
        entry = {
            "name": name,
            "model": model,
            "profile": profile,
            "returnCode": proc.returncode,
            "wallSeconds": time.monotonic() - started,
            "executableSha256": digest(binary),
            "manifestSha256": digest(manifest),
            "expected": len(expected),
            "present": len(present),
            "missingIds": sorted(expected - present),
            "peakRssBytes": int(peak[1]) if peak else None,
            "rssSamples": rss,
            "powerAtStart": power,
            "thermalAtStart": thermal_start,
            "thermalAtEnd": subprocess.run(
                ["pmset", "-g", "therm"], capture_output=True, text=True, check=False
            ).stdout.strip(),
            "competingWork": external,
            "latencyUsable": not external,
            "nativeExceptionLog": bool(
                re.search(
                    r"Caught exception|Ort::Exception|Exception caught|num_samples.*negative",
                    text,
                )
            ),
            "command": command,
        }
        ledger.append(entry)
        write(ledger_path, ledger)
        print(
            "DONE",
            name,
            "return",
            proc.returncode,
            "rows",
            len(present),
            "/",
            len(expected),
            "contended",
            bool(external),
            flush=True,
        )


if __name__ == "__main__":
    main()
