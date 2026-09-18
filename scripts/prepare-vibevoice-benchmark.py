#!/usr/bin/env python3
"""Prepare public VibeVoice regression plans from the existing benchmark corpus."""

import argparse
import hashlib
import json
import wave
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--root", type=Path, required=True)
root = parser.parse_args().root
root.mkdir(parents=True, exist_ok=True)
for name in (
    "plan-offline.json",
    "plan-smoke.json",
    "plan-streaming.json",
    "ami-600-645.wav",
):
    if (root / name).exists():
        parser.error(f"Refusing to overwrite {name}; use a fresh output root")
corpus = Path(".cache/benchmarks/results/audio-regression-2026-09-16")
language = Path(".cache/benchmarks/datasets/long-asr-fleurs")
clean = json.loads((corpus / "asr-clean.json").read_text())
noise = json.loads((corpus / "asr-noise.json").read_text())
quiet = json.loads((corpus / "cohere-diagnostic.json").read_text())
cases = []
sources = list(clean["sources"])


def add(sample, folder, lane, suffix="", **extra):
    row = dict(sample)
    row.update(
        id=sample["id"] + suffix,
        audioPath=str(folder / sample["audioPath"]),
        lane=lane,
        **extra,
    )
    assert hashlib.sha256(Path(row["audioPath"]).read_bytes()).hexdigest() == row["sha256"]
    cases.append(row)


for row in clean["samples"]:
    add(
        row,
        corpus / "corpus",
        "english-natural"
        if row.get("corpusLane") == "natural"
        else ("reset" if "--reset" in row["id"] else "english-controls"),
    )
for row in noise["samples"][:3] + noise["samples"][4:5]:
    add(row, corpus / "corpus", "noise")
add(quiet["samples"][1], corpus / "corpus", "quiet")
for lang in ["fr_fr", "hi_in"]:
    d = json.loads((language / (lang + "-manifest.json")).read_text())
    sources += d["sources"]
    for row in [d["samples"][0], d["samples"][-1]]:
        add(row, language, lang)
for hotwords in ["Maya", "Maia", "Zorblax,Quuxcity"]:
    add(
        clean["samples"][24],
        corpus / "corpus",
        "hotword",
        suffix="--hint-" + hotwords.replace(",", "-"),
        hotwords=hotwords,
    )
add(clean["samples"][0], corpus / "corpus", "seed", suffix="--seed1", seed=1)
add(clean["samples"][0], corpus / "corpus", "reset", suffix="--seed0-reset", seed=0)
sources = list({x["id"]: x for x in sources}.values())
(root / "plan-offline.json").write_text(
    json.dumps({"sources": sources, "cases": cases}, indent=2) + "\n"
)
(root / "plan-smoke.json").write_text(
    json.dumps({"sources": sources, "cases": [cases[0]]}, indent=2) + "\n"
)
original = Path(".cache/benchmarks/downloads/ami/ES2004a.Mix-Headset.wav")
target = root / "ami-600-645.wav"
with wave.open(str(original), "rb") as stream:
    params = stream.getparams()
    stream.setpos(600 * params.framerate)
    frames = stream.readframes(45 * params.framerate)
with wave.open(str(target), "wb") as stream:
    stream.setparams(params)
    stream.writeframes(frames)
annotations = Path(".cache/benchmarks/downloads/ami/ami_public_manual_1.6.2.zip")
words = []
with zipfile.ZipFile(annotations) as archive:
    for speaker in "ABCD":
        tree = ET.fromstring(archive.read(f"words/ES2004a.{speaker}.words.xml"))
        for node in tree:
            if node.tag == "w" and node.text and node.get("starttime") and node.get("endtime"):
                start = float(node.get("starttime"))
                end = float(node.get("endtime"))
                if start >= 600 and end <= 645:
                    words.append(
                        {
                            "speaker": speaker,
                            "start": start - 600,
                            "end": end - 600,
                            "text": node.text,
                        }
                    )
words.sort(key=lambda x: x["start"])
ami = {
    "id": "ami-600-645",
    "audioPath": str(target),
    "sha256": hashlib.sha256(target.read_bytes()).hexdigest(),
    "durationSeconds": 45,
    "reference": " ".join(x["text"] for x in words),
    "referenceSpeakers": {
        s: " ".join(x["text"] for x in words if x["speaker"] == s)
        for s in "ABCD"
        if any(x["speaker"] == s for x in words)
    },
    "wordTimes": words,
    "sourceId": "ami-es2004a",
    "lane": "meeting",
    "replayKind": "natural",
    "sourceAudioSha256": hashlib.sha256(original.read_bytes()).hexdigest(),
    "annotationArchiveSha256": hashlib.sha256(annotations.read_bytes()).hexdigest(),
    "sourceStartSeconds": 600,
    "sourceEndSeconds": 645,
}
source = json.loads((corpus / "ami.json").read_text())["sources"]
continuous = json.loads((corpus / "continuous.json").read_text())
streamcases = []
for row in continuous["samples"]:
    streamcases.append(
        dict(
            row,
            audioPath=str(corpus / "corpus" / row["audioPath"]),
            id=row["id"] + "--paced",
            lane="continuous",
            delivery="paced",
        )
    )
streamcases.insert(1, dict(ami, delivery="offline"))
streamcases.insert(2, dict(ami, id=ami["id"] + "--paced", delivery="paced"))
(root / "plan-streaming.json").write_text(
    json.dumps({"sources": sources + source, "cases": streamcases}, indent=2) + "\n"
)
print(
    "Prepared",
    len(cases),
    "offline cases and",
    len(streamcases),
    "streaming/meeting cases;",
    len(words),
    "AMI words;",
    len(ami["referenceSpeakers"]),
    "speakers.",
)
