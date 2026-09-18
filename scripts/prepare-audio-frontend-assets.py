#!/usr/bin/env python3
"""Prepare pinned development assets; no ASR, user microphone or app writes.

Supply public FLEURS/audio directories prepared using benchmarks/asr/README.md.
Generated controls must match the manifest hash; a different voice/OS build is
a new benchmark corpus, not a reason to silently replace the reference hash.
"""

import argparse
import hashlib
import json
import math
import shutil
import struct
import subprocess
import urllib.request
import wave
from pathlib import Path


def checksum(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def noise(path, kind):
    state = 17
    values = []
    for index in range(80000):
        state = (1664525 * state + 1013904223) & 0xFFFFFFFF
        if kind == "silence":
            value = 0
        elif kind == "white":
            value = 0.03 * (2 * state / 4294967296 - 1)
        else:
            frequency = {"hum": 120, "tone": 880}[kind]
            value = 0.03 * math.sin(2 * math.pi * frequency * index / 16000)
        values.append(round(value * 32767))
    with wave.open(str(path), "wb") as output:
        output.setnchannels(1)
        output.setsampwidth(2)
        output.setframerate(16000)
        output.writeframes(struct.pack("<" + "h" * len(values), *values))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--source-corpus", type=Path, action="append", default=[])
    parser.add_argument("--generate-controls", action="store_true")
    parser.add_argument("--download-models", action="store_true")
    parser.add_argument(
        "--model-root",
        type=Path,
        default=Path(".cache/benchmarks/models/audio-frontends"),
    )
    parser.add_argument(
        "--artifacts",
        type=Path,
        default=Path("benchmarks/asr/audio-frontend-artifacts.json"),
    )
    args = parser.parse_args()
    args.corpus.mkdir(parents=True, exist_ok=True)
    for sample in json.loads(args.manifest.read_text())["samples"]:
        relative = Path(sample["audioPath"])
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError("unsafe manifest audio path")
        target = args.corpus / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        if not target.exists():
            for root in args.source_corpus:
                candidates = [
                    root / relative,
                    root / relative.name,
                    root / "audio" / relative.name,
                ]
                source = next(
                    (p for p in candidates if p.is_file() and checksum(p) == sample["sha256"]),
                    None,
                )
                if source:
                    shutil.copyfile(source, target)
                    break
        if not target.exists() and args.generate_controls:
            if sample["id"].startswith("noise-"):
                noise(target, sample["id"].removeprefix("noise-"))
            elif sample["sourceId"] == "synthetic-dictation-controls":
                rate = (
                    140
                    if sample["id"]
                    in {"filler-um", "filler-uh", "affirmation-hmm", "meaningful-and"}
                    else 175
                )
                subprocess.run(
                    [
                        "say",
                        "-v",
                        "Samantha",
                        "-r",
                        str(rate),
                        "-o",
                        str(target),
                        "--file-format=WAVE",
                        "--data-format=LEI16@16000",
                        sample["reference"],
                    ],
                    check=True,
                )
        if not target.exists() or checksum(target) != sample["sha256"]:
            raise ValueError(
                f"missing or mismatched audio: {sample['id']}; preserve the pinned manifest"
            )
        print("verified audio", sample["id"])
    if args.download_models:
        for artifact in json.loads(args.artifacts.read_text()):
            target = args.model_root / artifact["path"]
            target.parent.mkdir(parents=True, exist_ok=True)
            if not target.exists():
                temporary = target.with_suffix(target.suffix + ".partial")
                urllib.request.urlretrieve(artifact["url"], temporary)
                if (
                    temporary.stat().st_size != artifact["size"]
                    or checksum(temporary) != artifact["sha256"]
                ):
                    raise ValueError(f"download integrity failure: {artifact['id']}")
                temporary.replace(target)
            if target.stat().st_size != artifact["size"] or checksum(target) != artifact["sha256"]:
                raise ValueError(f"artifact integrity failure: {artifact['id']}")
            print("verified model", artifact["id"])


if __name__ == "__main__":
    main()
