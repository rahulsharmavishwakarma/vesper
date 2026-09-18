#!/usr/bin/env python3
"""Fetch or verify pinned VibeVoice reference-test artifacts; no app installation."""

import argparse
import concurrent.futures
import hashlib
import json
import subprocess
from pathlib import Path


def sha256(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for data in iter(lambda: stream.read(8 * 1024 * 1024), b""):
            value.update(data)
    return value.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("--download", action="store_true")
    args = parser.parse_args()
    manifest = (
        Path(__file__).resolve().parents[1] / "benchmarks/asr/vibevoice-streaming-artifacts.json"
    )
    pins = json.loads(manifest.read_text())
    jobs = []
    for entry in pins["artifacts"]:
        jobs.append(
            (
                args.root / "model" / entry["file"],
                entry,
                (
                    f"https://huggingface.co/{pins['model']}/resolve/"
                    f"{pins['modelRevision']}/{entry['file']}"
                ),
            )
        )
    for entry in pins["sourceFiles"]:
        jobs.append(
            (
                args.root / "source" / entry["file"],
                entry,
                (
                    "https://raw.githubusercontent.com/microsoft/VibeVoice/"
                    f"{pins['sourceRevision']}/{entry['file']}"
                ),
            )
        )

    def prepare(job):
        path, entry, url = job
        if path.exists() and sha256(path) == entry["sha256"]:
            return
        if not args.download:
            raise ValueError(f"Missing or mismatched artifact: {entry['file']}")
        if path.exists():
            raise ValueError(f"Refusing to replace mismatched artifact: {entry['file']}")
        path.parent.mkdir(parents=True, exist_ok=True)
        partial = path.with_name(path.name + ".partial")
        subprocess.run(
            [
                "curl",
                "--fail",
                "--location",
                "--retry",
                "3",
                "--connect-timeout",
                "20",
                "--max-time",
                "1800",
                "--continue-at",
                "-",
                "--silent",
                "--show-error",
                "--output",
                str(partial),
                url,
            ],
            check=True,
        )
        if sha256(partial) != entry["sha256"]:
            raise ValueError(f"Downloaded checksum mismatch: {entry['file']}")
        if "size" in entry and partial.stat().st_size != entry["size"]:
            raise ValueError(f"Downloaded size mismatch: {entry['file']}")
        partial.rename(path)
        print(f"Verified {entry['file']}", flush=True)

    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
        list(pool.map(prepare, jobs))
    (args.root / "verified-artifacts.json").write_text(json.dumps(pins, indent=2) + "\n")
    print("All model and publisher-source artifacts verified.")


if __name__ == "__main__":
    main()
