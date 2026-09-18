"""Freeze a prospective denoising comparison; never overwrite measured inputs."""

import csv
import hashlib
import importlib.util
import json
import re
import shutil
from pathlib import Path

import numpy as np
import soundfile as sf

ROOT = Path(".cache/benchmarks/results/denoising-followup-2026-09-19")
PRIOR = Path(".cache/benchmarks/results/frontend-integration-2026-09-18")
FROZEN = Path("benchmarks/asr/manifests/denoising-followup-2026-09-19")
spec = importlib.util.spec_from_file_location(
    "frontend_prepare", Path(__file__).with_name("prepare-frontend-integration.py")
)
audio_writer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audio_writer)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def main():
    ROOT.mkdir(parents=True, exist_ok=False)
    corpus = ROOT / "input"
    corpus.mkdir()
    original = json.loads((PRIOR / "quality.json").read_text())
    samples = []
    for sample in original["samples"]:
        source = PRIOR / "corpus" / sample["audioPath"]
        assert digest(source) == sample["sha256"]
        shutil.copy2(source, corpus / sample["audioPath"])
        samples.append({**sample, "evaluationRole": "observed-regression"})
    seen = set()
    for folder in ["benchmarks/asr/manifests", "docs/validation", ".cache/benchmarks/results"]:
        for path in Path(folder).rglob("*.json"):
            if ROOT in path.parents or path.stat().st_size > 30_000_000:
                continue
            seen.update(re.findall(r"en_us-(\d+)", path.read_text(errors="replace")))
    with Path(".cache/benchmarks/downloads/fleurs/data/en_us/test.tsv").open() as source:
        rows = list(csv.reader(source, delimiter="\t", quoting=csv.QUOTE_NONE))
    frozen = json.loads((FROZEN / "protocol.json").read_text()) if FROZEN.exists() else None
    if frozen:
        by_id = {Path(r[1]).stem: r for r in rows if len(r) == 7}
        selected = [by_id[r["baseId"]] for r in frozen["derivations"]]
        seen = set(frozen["excludedBaseIds"])
    else:
        selected = []
        for sex in ["FEMALE", "MALE"]:
            for low, high in [(2, 8), (8, 16), (16, 25)]:
                eligible = [
                    r
                    for r in rows
                    if len(r) == 7
                    and r[6] == sex
                    and low < int(r[5]) / 16000 <= high
                    and Path(r[1]).stem not in seen
                ]
                eligible.sort(
                    key=lambda r: hashlib.sha256(
                        ("denoise-followup-v1:" + r[1]).encode()
                    ).hexdigest()
                )
                assert len(eligible) >= 2
                selected.extend(eligible[:2])
    noise_meta = json.loads(
        Path("benchmarks/asr/manifests/audio-regression-2026-09-16/noise-assets.json").read_text()
    )
    noises = []
    for entry in noise_meta:
        matches = list(Path(".cache/benchmarks/downloads/ms-snsd").rglob(entry["path"]))
        assert len(matches) == 1 and digest(matches[0]) == entry["sha256"]
        audio, rate = sf.read(matches[0], dtype="float32")
        assert rate == 16000 and audio.ndim == 1
        noises.append(audio)
    derivations = []
    for index, row in enumerate(selected):
        source = Path(".cache/benchmarks/corpora/fleurs-en-us/test") / row[1]
        x, rate = sf.read(source, dtype="float32")
        assert rate == 16000 and x.ndim == 1
        noise = np.resize(noises[index % len(noises)], len(x))
        noise_gain = np.sqrt(
            np.mean(x.astype(np.float64) ** 2) / np.mean(noise.astype(np.float64) ** 2)
        )
        mixture = x + noise * noise_gain
        headroom = min(1.0, 0.95 / float(np.max(np.abs(mixture))))
        for condition, audio in [
            ("clean", x),
            ("quiet30", x * 0.03),
            ("noise0", mixture * headroom),
        ]:
            sample_id = f"en_us-{Path(row[1]).stem}--followup-{condition}"
            path = corpus / (sample_id + ".wav")
            audio_writer.write_float_wav(path, audio, {path.name: 0})
            samples.append(
                {
                    "id": sample_id,
                    "sourceId": "fleurs-en_us-test",
                    "split": "test",
                    "language": "en-US",
                    "audioPath": path.name,
                    "sha256": digest(path),
                    "reference": row[2],
                    "durationSeconds": len(audio) / 16000,
                    "scenario": "read",
                    "replayKind": "natural",
                    "condition": condition,
                    "baseId": Path(row[1]).stem,
                    "sex": row[6],
                    "evaluationRole": "fresh-holdout",
                }
            )
        derivations.append(
            {
                "baseId": Path(row[1]).stem,
                "sourceSha256": digest(source),
                "noise": noise_meta[index % len(noise_meta)]["path"],
                "noiseGain": float(noise_gain),
                "commonHeadroom": headroom,
            }
        )
    manifest = {**original, "samples": samples}
    write(ROOT / "quality.json", manifest)
    stream = json.loads((PRIOR / "streaming.json").read_text())
    for sample in stream["samples"]:
        sample["evaluationRole"] = "observed-regression"
    write(ROOT / "streaming.json", stream)
    protocol = {
        "selectionSeed": "denoise-followup-v1",
        "excludedBaseIds": sorted(seen),
        "derivations": derivations,
        "originalWeight": 0.5,
        "knownAttenuation": 0.03,
        "profiles": ["raw", "dpdfnet4-full", "dpdfnet4-mix50", "dpdfnet8-full", "dpdfnet8-mix50"],
        "mainAttempts": 1830,
        "diagnosticAttempts": 288,
        "streamingAttempts": 35,
        "diagnosticScope": "24 observed quiet clips; known-gain experiment, not automatic gain control",
        "caveats": [
            "Fresh file IDs are not proof of speaker/training-set disjointness",
            "Noise mixtures include common headroom scaling",
            "Synthetic meaning/noise controls are not natural accuracy data",
            "No production default, adaptive route, new runtime or training change",
        ],
    }
    write(ROOT / "protocol.json", protocol)
    if frozen:
        for name in ["quality.json", "streaming.json", "protocol.json"]:
            assert (ROOT / name).read_bytes() == (FROZEN / name).read_bytes(), name
    else:
        FROZEN.mkdir(parents=True, exist_ok=False)
        for name in ["quality.json", "streaming.json", "protocol.json"]:
            shutil.copy2(ROOT / name, FROZEN / name)
    assert len(samples) == 122 and len(selected) == 12
    print("Frozen 122 cases: 86 observed controls and 36 variants of 12 fresh recordings.")


if __name__ == "__main__":
    main()
