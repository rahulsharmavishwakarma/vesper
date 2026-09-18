#!/usr/bin/env python3
"""Extract frozen AMI annotation-selected windows without inspecting model outputs."""

import argparse
import hashlib
import json
from pathlib import Path

import soundfile as sf


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("root", type=Path)
    args = parser.parse_args()
    selected = json.loads((args.source / "candidate-windows.json").read_text())
    for name, expected in [
        (selected["audio_file"], selected["audio_sha256"]),
        (selected["annotation_file"], selected["annotation_sha256"]),
    ]:
        if hashlib.sha256((args.source / name).read_bytes()).hexdigest() != expected:
            raise ValueError("AMI source integrity failure")
    audio, rate = sf.read(args.source / selected["audio_file"], dtype="float32")
    if rate != 16000 or audio.ndim != 1:
        raise ValueError("AMI signal contract failure")
    source = {
        "id": "ami-es2004a",
        "name": "AMI ES2004a mixed-headset conversation",
        "url": selected["original_download_page"],
        "revision": selected["actual_mirror_revision"],
        "license": selected["license"],
        "attribution": selected["attribution"],
        "redistribution": "CC-BY4; source and modifications attributed, audio retained locally",
    }
    rows = []
    for window in selected["windows"]:
        start = round(window["source_start_seconds"] * rate)
        end = round(window["source_end_seconds"] * rate)
        if not 0 <= start < end <= len(audio):
            raise ValueError("AMI window outside source")
        path = args.root / "corpus" / f"{window['id']}.wav"
        sf.write(path, audio[start:end], rate, subtype="FLOAT")
        rows.append(
            {
                "id": window["id"],
                "sourceId": source["id"],
                "split": "validation",
                "language": "en-GB",
                "audioPath": path.name,
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "reference": window["text"],
                "durationSeconds": (end - start) / rate,
                "scenario": "conversational",
                "replayKind": "natural",
                "speakerId": "ES2004a." + window["anchor"]["speaker"],
                "corpusLane": "ami",
                "annotation": window,
                "splitCaveat": "local validation selection, not official AMI test split or model-training holdout",
            }
        )
    (args.root / "ami.json").write_text(
        json.dumps(
            {
                "schemaVersion": 1,
                "sources": [source],
                "samples": rows,
                "provenance": selected,
            },
            indent=2,
            ensure_ascii=False,
        )
        + "\n"
    )
    print(f"Prepared {len(rows)} fixed AMI windows")


if __name__ == "__main__":
    main()
