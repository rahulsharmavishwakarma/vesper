#!/usr/bin/env python3
"""Verify or explicitly download pinned Whisper exports for the native screen."""

import argparse
import hashlib
import json
import urllib.request
from pathlib import Path


def verified(path, entry):
    if not path.is_file() or path.stat().st_size != entry["size"]:
        return False
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest() == entry["sha256"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("--download", action="store_true")
    args = parser.parse_args()
    pins = json.loads(Path("benchmarks/asr/whisper-artifacts.json").read_text())
    for model in pins["models"]:
        directory = args.root / model["model"]
        directory.mkdir(parents=True, exist_ok=True)
        output = {k: model[k] for k in ["model", "publisher", "revision", "license"]}
        for entry in model["files"]:
            path = directory / entry["filename"]
            if not verified(path, entry):
                if not args.download:
                    raise SystemExit(f"Missing/mismatched {path}; use --download explicitly")
                url = f"https://huggingface.co/{model['publisher']}/resolve/{model['revision']}/{entry['filename']}"
                partial = path.with_suffix(path.suffix + ".partial")
                # Fetch data only: no remote Python code, pickle loading or archive extraction.
                with (
                    urllib.request.urlopen(url, timeout=60) as response,
                    partial.open("wb") as file,
                ):
                    count = 0
                    while chunk := response.read(1024 * 1024):
                        count += len(chunk)
                        if count > entry["size"]:
                            raise ValueError(f"Artifact exceeds pinned size: {path}")
                        file.write(chunk)
                if not verified(partial, entry):
                    raise ValueError(f"Downloaded artifact integrity failure: {path}")
                partial.replace(path)
            output[entry["role"]] = {
                "path": str(path),
                "size": entry["size"],
                "sha256": entry["sha256"],
            }
        descriptor = args.root / f"{model['model']}.json"
        if descriptor.exists():
            previous = json.loads(descriptor.read_text())
            if all(previous.get(k) == v for k, v in output.items() if k != "license"):
                print("verified", model["model"])
                continue
            raise ValueError(
                f"Existing descriptor differs; preserve it and use a new root: {descriptor}"
            )
        descriptor.write_text(json.dumps(output, indent=2) + "\n")
        print("verified", model["model"])


if __name__ == "__main__":
    main()
