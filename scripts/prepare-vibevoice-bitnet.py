#!/usr/bin/env python3
"""Download and verify only the pinned BitNet GGUF pair for native benchmarks."""

import argparse
import concurrent.futures
import hashlib
import json
import shutil
import time
import urllib.request
from pathlib import Path


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(8 * 1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("--download", action="store_true")
    args = parser.parse_args()
    pins = json.loads(
        (
            Path(__file__).resolve().parents[1] / "benchmarks/asr/vibevoice-bitnet-artifacts.json"
        ).read_text()
    )
    models = args.root / "models"
    models.mkdir(parents=True, exist_ok=True)
    parts = args.root / "downloads" / "parts"
    jobs = []
    pending = []
    for entry in pins["artifacts"]:
        target = models / entry["file"]
        if target.exists():
            if target.stat().st_size != entry["bytes"] or digest(target) != entry["sha256"]:
                raise ValueError(f"Refusing to replace mismatched {entry['file']}")
            continue
        if not args.download:
            raise ValueError(f"Missing {entry['file']}; use --download")
        pending.append(entry)
        for start in range(0, entry["bytes"], 32 * 1024 * 1024):
            end = min(start + 32 * 1024 * 1024, entry["bytes"]) - 1
            jobs.append((entry, start, end))

    def fetch(job):
        entry, start, end = job
        target = parts / f"{entry['file']}.{start}"
        url = (
            f"https://huggingface.co/{pins['model']}/resolve/"
            f"{pins['modelRevision']}/{entry['file']}"
        )
        for attempt in range(4):
            try:
                req = urllib.request.Request(url, headers={"Range": f"bytes={start}-{end}"})
                with urllib.request.urlopen(req, timeout=120) as source:
                    expected = f"bytes {start}-{end}/{entry['bytes']}"
                    if source.status != 206 or source.headers.get("Content-Range") != expected:
                        raise ValueError("Server did not honor exact byte range")
                    with target.open("wb") as stream:
                        shutil.copyfileobj(source, stream, 1024 * 1024)
                if target.stat().st_size != end - start + 1:
                    raise ValueError("Short range download")
                return
            except Exception:
                if attempt == 3:
                    raise
                time.sleep(attempt + 1)

    if jobs:
        parts.mkdir(parents=True, exist_ok=True)
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            list(pool.map(fetch, jobs))
    for entry in pending:
        target = models / entry["file"]
        partial = target.with_suffix(".partial")
        with partial.open("wb") as stream:
            for start in range(0, entry["bytes"], 32 * 1024 * 1024):
                with (parts / f"{entry['file']}.{start}").open("rb") as source:
                    shutil.copyfileobj(source, stream, 8 * 1024 * 1024)
        if digest(partial) != entry["sha256"]:
            raise ValueError(f"Checksum mismatch: {entry['file']}")
        partial.rename(target)
        for start in range(0, entry["bytes"], 32 * 1024 * 1024):
            (parts / f"{entry['file']}.{start}").unlink()
        print(f"Verified {entry['file']}", flush=True)
    (args.root / "verified-artifacts.json").write_text(json.dumps(pins, indent=2) + "\n")
    print("Pinned BitNet GGUF pair verified.", flush=True)


if __name__ == "__main__":
    main()
