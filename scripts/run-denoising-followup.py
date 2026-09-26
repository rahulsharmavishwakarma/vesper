"""Sequential frozen-binary follow-up; preserve failed processes and all rows."""

import argparse
import hashlib
import importlib.util
import json
import os
import re
import subprocess
import time
from pathlib import Path

ROOT = Path(".cache/benchmarks/results/denoising-followup-2026-09-19")
spec = importlib.util.spec_from_file_location(
    "frontend_runner", Path(__file__).with_name("qualify-frontend-integration.py")
)
common = importlib.util.module_from_spec(spec)
spec.loader.exec_module(common)
PROFILES = ["raw", "dpdfnet4-full", "dpdfnet4-mix50", "dpdfnet8-full", "dpdfnet8-mix50"]
DIAGNOSTICS = [
    f"{m}-{g}" for m in ["dpdfnet4", "dpdfnet8"] for g in ["pregain-restore", "postgain"]
]
MODELS = {
    "cohere": "CohereLabs/cohere-transcribe-03-2026",
    "small": "openai/whisper-small",
    "turbo": "openai/whisper-large-v3-turbo",
}


def read(path):
    return json.loads(path.read_text())


def thermal():
    return subprocess.run(
        ["pmset", "-g", "therm"], capture_output=True, text=True, check=False
    ).stdout.strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", choices=["contracts", "prepare", "quality", "streaming"])
    phase = parser.parse_args().phase
    harness_hashes = {
        p: hashlib.sha256(Path(p).read_bytes()).hexdigest()
        for p in ["scripts/run-denoising-followup.py", "scripts/qualify-frontend-integration.py"]
    }
    harness_path = ROOT / f"harness-hashes-{phase}.json"
    if harness_path.exists() and read(harness_path) != harness_hashes:
        raise RuntimeError("Harness changed since the first process; preserve the frozen protocol")
    common.write(harness_path, harness_hashes)
    runs = ROOT / "runs"
    runs.mkdir(exist_ok=True)
    environment = ROOT / "environment.json"
    if not environment.exists():
        runtime = read(Path(".tooling/sherpa-runtime/current.json"))
        common.write(
            environment,
            {
                "label": "denoising-followup-2026-09-19",
                "hardware": "Apple M5 Pro; 18 logical CPUs; 68719476736 bytes RAM",
                "operatingSystem": subprocess.check_output(
                    ["sw_vers", "-productVersion"], text=True
                ).strip(),
                "powerMode": "Per-process powerAtStart; watts and continuous thermals unmeasured",
                "backend": "cpu",
                "precision": "Pinned DPDF/Silero ONNX; Cohere/Whisper INT8; Nemotron Q8_0; Qwen writer Q4_0",
                "threads": 8,
                "applicationRevision": (ROOT / "source-revision.txt").read_text().strip()
                + "+frozen-source-snapshot",
                "runtimeRevision": f"Sherpa1.13.8 {runtime['revision']}; build {runtime['buildId']}; NeMo-Speech.cpp0.1.0; llama-cpp-2 0.1.156",
                "driver": "CPU; no accelerator driver",
                "notes": "Frontend1thread; ASR8/Nemo4; fixed half-original blend; quality preprocessed separately; stream bounded observer and nominal-end Stop timestamp; no microphone/edge qualification",
            },
        )
    overrides = {
        "OMP_NUM_THREADS": "1",
        "OPENBLAS_NUM_THREADS": "1",
        "VECLIB_MAXIMUM_THREADS": "1",
        "MKL_NUM_THREADS": "1",
        "VESPER_EXPERIMENTAL_PREFIX_CACHE": "0",
    }
    common.write(ROOT / "runtime-env.json", overrides)
    env = {**os.environ, **overrides}
    exe = ROOT / "denoising_followup"
    eval_exe = ROOT / "vesper-eval"
    cpu_models = ".cache/benchmarks/model-roots/nemotron-cpu"
    jobs = []
    if phase in ["prepare", "contracts"]:
        jobs.append((phase, None, None, [str(exe), phase, str(ROOT), "models/downloads"], None))
    elif phase == "streaming":
        for profile in PROFILES:
            name = "streaming-" + profile
            jobs.append(
                (
                    name,
                    "nemotron",
                    profile,
                    [
                        str(exe),
                        "stream",
                        str(ROOT),
                        cpu_models,
                        profile,
                        str(runs / (name + ".json")),
                    ],
                    ROOT / "streaming.json",
                )
            )
    else:
        for index, (model, model_id) in enumerate(MODELS.items()):
            order = PROFILES[index:] + PROFILES[:index] + DIAGNOSTICS[index:] + DIAGNOSTICS[:index]
            for profile in order:
                name = f"quality-{model}-{profile}"
                manifest = ROOT / "manifests" / (profile + ".json")
                command = [
                    str(eval_exe),
                    "run",
                    str(manifest),
                    str(ROOT / "corpus"),
                    "models/downloads",
                    str(environment),
                    str(runs / (name + ".json")),
                    "--model",
                    model_id,
                    "--language",
                    "en",
                    "--cleanup",
                ]
                jobs.append((name, model, profile, command, manifest))
    pinned_paths = {exe, eval_exe, ROOT / "quality.json", ROOT / "streaming.json"}
    pinned_paths.update(manifest for _, _, _, _, manifest in jobs if manifest)
    run_jobs(ROOT, phase, jobs, env, pinned_paths)


def run_jobs(root, phase, jobs, env, pinned_paths):
    """Run fixed native jobs sequentially; share integrity and failure accounting."""
    runs = root / "runs"
    runs.mkdir(exist_ok=True)
    ledger_path = root / f"processes-{phase}.json"
    current_pins = {str(p): common.digest(p) for p in sorted(pinned_paths)}
    pins_path = root / f"execution-pins-{phase}.json"
    if pins_path.exists() and read(pins_path) != current_pins:
        raise RuntimeError("Executable or input differs from this phase's frozen inventory")
    common.write(pins_path, current_pins)
    ledger = read(ledger_path) if ledger_path.exists() else []
    completed = {p["name"] for p in ledger}
    for name, model, profile, command, manifest in jobs:
        if name in completed:
            continue
        before = {p: common.digest(Path(p)) for p in current_pins}
        if before != current_pins:
            raise RuntimeError("Executable or input changed before process launch")
        expected = {r["id"] for r in read(manifest)["samples"]} if manifest else set()
        if (runs / (name + ".log")).exists():
            raise RuntimeError(f"Unrecorded attempt exists: {name}; preserve it before retrying")
        wait = time.monotonic()
        while common.processes()[0] and time.monotonic() - wait < 30:
            time.sleep(5)
        power = subprocess.check_output(["pmset", "-g", "batt"], text=True).splitlines()[0]
        thermal_start = thermal()
        started = time.monotonic()
        external, rss = [], []
        print("START", name, flush=True)
        with (runs / (name + ".log")).open("wb") as log:
            process = subprocess.Popen(
                ["/usr/bin/time", "-l", *command],
                stdout=log,
                stderr=log,
                env=env,
                start_new_session=True,
            )
            progress = 60
            while process.poll() is None:
                active, memory = common.processes(process.pid)
                elapsed = time.monotonic() - started
                if active:
                    external.append({"seconds": elapsed, "observed": active})
                if int(elapsed) % 5 == 0:
                    rss.append({"seconds": elapsed, "bytes": memory})
                if elapsed >= progress:
                    print("RUNNING", name, round(elapsed), "seconds", flush=True)
                    progress += 60
                time.sleep(1)
        log = (runs / (name + ".log")).read_text(errors="replace")
        peak = re.search(r"(\d+)\s+maximum resident set size", log)
        result = runs / (name + ".json")
        parse_error = False
        try:
            present = {r["sampleId"] for r in read(result)["runs"]} if result.exists() else set()
        except (OSError, ValueError, KeyError, TypeError):
            present, parse_error = set(), True
        after = {p: common.digest(Path(p)) if Path(p).exists() else None for p in current_pins}
        entry = {
            "name": name,
            "model": model,
            "profile": profile,
            "phase": phase,
            "returnCode": process.returncode,
            "wallSeconds": time.monotonic() - started,
            "executableSha256": after[command[0]],
            "executableSha256Before": before[command[0]],
            "manifestSha256": after[str(manifest)] if manifest else None,
            "manifestSha256Before": before[str(manifest)] if manifest else None,
            "expected": len(expected),
            "present": len(present),
            "missingIds": sorted(expected - present),
            "resultParseError": parse_error,
            "peakRssBytes": int(peak[1]) if peak else None,
            "rssSamples": rss,
            "powerAtStart": power,
            "thermalAtStart": thermal_start,
            "thermalAtEnd": thermal(),
            "competingWork": external,
            "latencyUsable": not external,
            "nativeExceptionLog": bool(
                re.search(
                    r"Caught exception|Ort::Exception|Exception caught|num_samples.*negative", log
                )
            ),
        }
        entry["changedInputHashes"] = {
            p: {"before": before[p], "after": after[p]} for p in before if before[p] != after[p]
        }
        ledger.append(entry)
        common.write(ledger_path, ledger)
        print(
            "DONE",
            name,
            "return",
            process.returncode,
            "rows",
            len(present),
            "/",
            len(expected),
            "contended",
            bool(external),
            flush=True,
        )
        if phase in ["prepare", "contracts"] and process.returncode:
            raise SystemExit("Required preprocessing/contract process failed; evidence retained")
        if entry["changedInputHashes"]:
            raise SystemExit("Executable or input changed during execution; attempt retained")


if __name__ == "__main__":
    main()
