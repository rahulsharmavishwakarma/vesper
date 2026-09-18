#!/usr/bin/env python3
"""Isolated, sequential full-file SHA-256 backend comparison; no app changes."""

import argparse
import hashlib
import json
import shutil
import subprocess
import time
from pathlib import Path


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("--model-root", type=Path, default=Path("models/downloads"))
    args = parser.parse_args()
    output = args.root / "integrity-probe.json"
    if output.exists():
        raise FileExistsError("refusing to overwrite prior integrity results")
    probe = Path("benchmarks/asr/hash-probe")
    build = Path(".cache/benchmarks/hash-probe-build")
    binaries = args.root / "hash-binaries"
    binaries.mkdir(exist_ok=True)
    report = {
        "schemaVersion": 1,
        "environment": json.loads((args.root / "environment.json").read_text()),
        "sourceSha256": sha(probe / "src/main.rs"),
        "cargoLockSha256": sha(probe / "Cargo.lock"),
        "cargoManifestSha256": sha(probe / "Cargo.toml"),
        "runnerSha256": sha(Path(__file__)),
        "rustc": subprocess.run(
            ["rustc", "--version"], capture_output=True, text=True, check=True
        ).stdout.strip(),
        "executableSha256": {},
        "runs": [],
        "contractChecks": [],
        "limitations": [
            "File reading plus SHA-256 only; no ASR or UI path",
            "Three sequential repeated passes; OS cache not flushed",
            "Same sha2 0.10.9; isolated feature graph does not alter production build",
            "Includes read/update/finalize; excludes process startup and file open in per-file milliseconds",
        ],
    }
    for mode in ["software", "hardware"]:
        command = [
            "cargo",
            "build",
            "--release",
            "--locked",
            "--offline",
            "--manifest-path",
            str(probe / "Cargo.toml"),
            "--target-dir",
            str(build),
        ]
        if mode == "hardware":
            command += ["--features", "hardware"]
        subprocess.run(command, check=True)
        shutil.copy2(build / "release/audio-integrity-probe", binaries / mode)
        report["executableSha256"][mode] = sha(binaries / mode)
    artifacts = json.loads((args.root / "runs/asr-clean-raw.json").read_text())["runs"][0][
        "modelVersions"
    ]["asr"]["artifacts"]
    for repeat in range(3):
        for mode in ["software", "hardware"] if repeat % 2 == 0 else ["hardware", "software"]:
            for artifact in artifacts:
                command = [
                    str(binaries / mode),
                    str(args.model_root / artifact["relativePath"]),
                    str(artifact["sizeBytes"]),
                    artifact["sha256"],
                ]
                started = time.perf_counter()
                process = subprocess.run(command, capture_output=True, text=True, check=False)
                row = {
                    "repeat": repeat,
                    "mode": mode,
                    "artifact": artifact["relativePath"],
                    "returnCode": process.returncode,
                    "processWallMs": (time.perf_counter() - started) * 1000,
                    "error": process.stderr.strip() or None,
                }
                if process.returncode == 0:
                    row.update(json.loads(process.stdout))
                report["runs"].append(row)
                output.write_text(json.dumps(report, indent=2) + "\n")
            print(f"Completed integrity pass {repeat + 1}: {mode}", flush=True)
    # SHA padding/block/reader boundaries plus same-size mutation and size mismatch.
    fixture = binaries / "fixture.bin"
    for size in [0, 1, 55, 56, 63, 64, 65, 127, 128, 129, 131071, 131072, 131073]:
        data = bytes((i * 37 + 11) % 256 for i in range(size))
        fixture.write_bytes(data)
        for mode in ["software", "hardware"]:
            expected = hashlib.sha256(data).hexdigest()
            process = subprocess.run(
                [str(binaries / mode), str(fixture), str(size), expected],
                capture_output=True,
                text=True,
                check=False,
            )
            report["contractChecks"].append(
                {
                    "mode": mode,
                    "bytes": size,
                    "kind": "known-digest",
                    "passed": process.returncode == 0,
                    "returnCode": process.returncode,
                }
            )
    fixture.write_bytes(b"original")
    expected = hashlib.sha256(b"original").hexdigest()
    for mode in ["software", "hardware"]:
        for kind, data, size in [
            ("same-size-mutation", b"modified", 8),
            ("truncated", b"orig", 8),
            ("oversized", b"original!", 8),
        ]:
            fixture.write_bytes(data)
            process = subprocess.run(
                [str(binaries / mode), str(fixture), str(size), expected],
                capture_output=True,
                text=True,
                check=False,
            )
            report["contractChecks"].append(
                {
                    "mode": mode,
                    "kind": kind,
                    "passed": process.returncode != 0
                    and "integrity" in process.stderr
                    or process.returncode != 0
                    and "exceeds" in process.stderr,
                    "returnCode": process.returncode,
                }
            )
    output.write_text(json.dumps(report, indent=2) + "\n")
    if any(r["returnCode"] for r in report["runs"]) or not all(
        r["passed"] for r in report["contractChecks"]
    ):
        raise SystemExit("integrity comparison has failures; preserved in output")


if __name__ == "__main__":
    main()
