#!/usr/bin/env python3
"""Sequential existing-native-ASR runs on the prepared public accent screen."""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import time
from pathlib import Path


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def freeze(root):
    root.mkdir(parents=True, exist_ok=True)
    (root / "bin").mkdir(exist_ok=False)
    shutil.copy2("target/release/transcribe-eval", root / "bin/transcribe-eval")
    paths = [Path("Cargo.lock"), Path("Cargo.toml")]
    for crate in (
        "transcribe-eval",
        "transcribe-core",
        "transcribe-inference",
        "transcribe-models",
        "transcribe-audio",
    ):
        directory = Path("crates", crate)
        paths.extend(directory.rglob("*.rs"))
        paths.append(directory / "Cargo.toml")
    write(root / "source-hashes.json", {str(p): digest(p) for p in sorted(set(paths))})
    print("Frozen evaluator:", digest(root / "bin/transcribe-eval"))


def run(root):
    corpus = root / "corpus"
    audit = json.loads((corpus / "selection-audit.json").read_text())
    for name, checksum in audit["manifestSha256"].items():
        if digest(corpus / name) != checksum:
            raise ValueError(f"Prepared manifest changed: {name}")
    results, logs = root / "results", root / "logs"
    results.mkdir(exist_ok=False)
    logs.mkdir(exist_ok=True)
    binary = root / "bin/transcribe-eval"
    memory = root / "memory.json"
    shutil.copyfile(".cache/benchmarks/personalization-2026-09-16/memory.json", memory)
    source_hashes = json.loads((root / "source-hashes.json").read_text())
    source_stamp = hashlib.sha256(json.dumps(source_hashes, sort_keys=True).encode()).hexdigest()
    base = {
        "hardware": "Apple M5 Pro (Mac17,8); 18 logical CPUs; 64 GiB RAM",
        "operatingSystem": "macOS 27.0 (26A428)",
        "powerMode": "Power mode 0 on the measured host; supply snapshot per process is authoritative; no continuous thermal trace",
        "backend": "cpu",
        "threads": 8,
        "applicationRevision": "592e0084fe5cf486b4d56c575423e9a4f219bfb8+unstaged-source-sha256:"
        + source_stamp,
        "runtimeRevision": "Exact artifacts/build identity in native report; sherpa-onnx1.13.8 upstream11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf; NeMo-Speech.cpp0.1.0 4f9676226f667d14608487df744f375db87127f8",
        "driver": "CPU; no accelerator driver",
        "notes": "Frozen native application evaluator; English prompt for all Svarah groups. Existing 51-entry corpus-review memory, no native bias, no cleanup. New temporary benchmark outputs; no user profile. Expected native graph threads: Cohere8, pinned NeMo CPU4; verify actual per-request execution evidence. First request cold, later resident according to actual execution evidence. ASR file time is not stop-to-final. Real-time mode uses paced20ms prerecorded packets. No timed-model concurrency; host activity uncontrolled.",
    }
    env = {
        k: v
        for k, v in os.environ.items()
        if not k.startswith(("TRANSCRIBE_", "UKTI_", "STT_", "GGML_", "LLAMA_"))
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
    models = {
        "nemotron": (
            "nvidia/nemotron-3.5-asr-streaming-0.6b",
            Path(".cache/benchmarks/model-roots/nemotron-cpu"),
            "Q8_0",
        ),
        "cohere": (
            "CohereLabs/cohere-transcribe-03-2026",
            Path("models/downloads"),
            "ONNX INT8",
        ),
    }
    for name, (_, _, precision) in models.items():
        write(
            root / f"environment-{name}.json",
            {
                **base,
                "label": "accent-2026-09-18-" + name,
                "precision": precision,
                "threads": 4 if name == "nemotron" else 8,
            },
        )
    jobs = [
        ("nemotron", "smoke", False),
        ("cohere", "smoke", False),
        ("nemotron", "svarah", False),
        ("cohere", "svarah", False),
        ("cohere", "us-control", False),
        ("nemotron", "us-control", False),
        ("nemotron", "svarah-realtime", True),
    ]
    ledger = {
        "binarySha256": digest(binary),
        "sourceSha256": source_hashes,
        "memorySha256": digest(memory),
        "selectionAuditSha256": digest(corpus / "selection-audit.json"),
        "runnerSha256": digest(Path(__file__)),
        "jobs": [],
    }
    write(root / "run-ledger.json", ledger)
    failed = False
    for model, pack, realtime in jobs:
        name = f"{model}-{pack}"
        model_id, model_root, _ = models[model]
        args = [
            str(binary),
            "run",
            str(corpus / f"{pack}.json"),
            str(corpus),
            str(model_root),
            str(root / f"environment-{model}.json"),
            str(results / f"{name}.json"),
            "--model",
            model_id,
            "--language",
            "en",
            "--profile",
            str(memory),
        ]
        if realtime:
            args += ["--streaming", "--realtime", "--chunk-ms", "20"]
        print(f"Running {name}", flush=True)
        supply = subprocess.run(
            ["pmset", "-g", "batt"], capture_output=True, text=True, check=False
        ).stdout
        started = time.perf_counter()
        with (logs / f"{name}.log").open("w") as stream:
            try:
                result = subprocess.run(
                    ["/usr/bin/time", "-l", *args],
                    env=env,
                    stdout=stream,
                    stderr=subprocess.STDOUT,
                    timeout=3600,
                    check=False,
                )
                exit_code = result.returncode
            except subprocess.TimeoutExpired:
                exit_code = 124
        entry = {
            "name": name,
            "manifestSha256": digest(corpus / f"{pack}.json"),
            "supplyAtStart": supply.strip(),
            "exitCode": exit_code,
            "wallSeconds": time.perf_counter() - started,
        }
        output = results / f"{name}.json"
        if output.exists():
            report = json.loads(output.read_text())
            expected = len(json.loads((corpus / f"{pack}.json").read_text())["samples"])
            entry.update(
                reportSha256=digest(output),
                attempted=len(report["runs"]),
                expected=expected,
                sampleFailures=sum(r["error"] is not None for r in report["runs"]),
            )
            failed |= expected != entry["attempted"] or entry["sampleFailures"] > 0
        else:
            entry["missingReport"] = True
        failed |= exit_code != 0
        ledger["jobs"].append(entry)
        write(root / "run-ledger.json", ledger)
        print(json.dumps(entry), flush=True)
    return failed


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument(
        "--freeze",
        action="store_true",
        help="Copy a just-built evaluator and hash its application sources before preparing/running a new experiment",
    )
    args = parser.parse_args()
    if args.freeze:
        freeze(args.root)
    else:
        raise SystemExit(run(args.root))
