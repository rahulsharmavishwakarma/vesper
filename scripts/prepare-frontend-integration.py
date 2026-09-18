"""Freeze previously unlisted FLEURS cases and existing regression controls.

Uses the existing audio-eval NumPy/soundfile environment and cached public audio.
No model execution, downloads, or private recordings.
"""

import csv
import hashlib
import json
import re
import shutil
import struct
from pathlib import Path

import numpy as np
import soundfile as sf

ROOT = Path(".cache/benchmarks/results/frontend-integration-2026-09-18")
PRIOR = Path(".cache/benchmarks/results/audio-regression-2026-09-16")
FROZEN = Path("benchmarks/asr/manifests/frontend-integration-2026-09-18")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def write_float_wav(path, audio, peak_timestamps):
    sf.write(path, audio, 16000, subtype="FLOAT")
    timestamp = peak_timestamps.get(path.name)
    if timestamp is None:
        return
    # libsndfile's FLOAT WAV PEAK chunk includes wall-clock creation time.
    # Restore that pinned container metadata, then verify the entire file hash.
    # No PCM, peak amplitude or peak position is rewritten.
    with path.open("r+b") as output:
        if output.read(4) != b"RIFF":
            raise ValueError("expected a RIFF WAV container")
        output.seek(12)
        while header := output.read(8):
            tag, size = struct.unpack("<4sI", header)
            if tag == b"PEAK" and size >= 8:
                output.seek(4, 1)
                output.write(struct.pack("<I", timestamp))
                return
            output.seek(size + size % 2, 1)
    raise ValueError(f"Missing pinned PEAK metadata for {path.name}")


def main():
    ROOT.mkdir(exist_ok=False)
    corpus = ROOT / "corpus"
    corpus.mkdir()
    seen = set()
    # Exclude base IDs from recorded manifests/results, not only one prior run.
    for directory in [
        Path("benchmarks/asr/manifests"),
        Path("docs/validation"),
        Path(".cache/benchmarks/results"),
        Path(".cache/benchmarks/datasets"),
    ]:
        for path in directory.rglob("*.json"):
            if ROOT in path.parents or path.stat().st_size > 30_000_000:
                continue
            seen.update(re.findall(r"en_us-(\d+)", path.read_text(errors="replace")))
    prior = json.loads((PRIOR / "asr.json").read_text())
    metadata = Path(".cache/benchmarks/downloads/fleurs/data/en_us/test.tsv")
    rows = list(csv.reader(metadata.open(), delimiter="\t", quoting=csv.QUOTE_NONE))
    selected = []
    # Once published, replay that exact selection; the new report itself must
    # not silently become an exclusion that changes the reproduced corpus.
    frozen_protocol = (
        json.loads((FROZEN / "protocol.json").read_text()) if FROZEN.exists() else None
    )
    peak_timestamps = (
        json.loads((FROZEN / "wav-container-metadata.json").read_text())["peakTimestamps"]
        if frozen_protocol
        else {}
    )
    if frozen_protocol:
        by_id = {Path(r[1]).stem: r for r in rows if len(r) == 7}
        selected = [by_id[d["baseId"]] for d in frozen_protocol["derivations"]]
        seen = set(frozen_protocol["excludedBaseIds"])
    for sex in [] if frozen_protocol else ["FEMALE", "MALE"]:
        for low, high in [(2, 8), (8, 16), (16, 25)]:
            eligible = [
                r
                for r in rows
                if len(r) == 7
                and r[6] == sex
                and low < int(r[5]) / 16000 <= high
                and Path(r[1]).stem not in seen
            ]
            eligible.sort(key=lambda r: digest(("frontend-integration-v1:" + r[1]).encode()))
            if len(eligible) < 4:
                raise ValueError("insufficient previously unlisted recordings")
            selected.extend(eligible[:4])
    noise_meta = json.loads(
        Path("benchmarks/asr/manifests/audio-regression-2026-09-16/noise-assets.json").read_text()
    )
    noise_root = Path(".cache/benchmarks/downloads/ms-snsd")
    noise_arrays = []
    for entry in noise_meta:
        candidates = list(noise_root.rglob(entry["path"]))
        if len(candidates) != 1:
            raise ValueError("missing/ambiguous noise asset")
        assert digest(candidates[0].read_bytes()) == entry["sha256"]
        x, rate = sf.read(candidates[0], dtype="float32")
        assert rate == 16000 and x.ndim == 1
        noise_arrays.append(x)
    samples = []
    derivations = []
    for index, row in enumerate(selected):
        path = Path(".cache/benchmarks/corpora/fleurs-en-us/test") / row[1]
        x, rate = sf.read(path, dtype="float32")
        assert rate == 16000 and x.ndim == 1
        n = np.resize(noise_arrays[index % len(noise_arrays)], len(x))
        xrms = np.sqrt(np.mean(x.astype(np.float64) ** 2))
        nrms = np.sqrt(np.mean(n.astype(np.float64) ** 2))
        mixed = x + n * (xrms / nrms)
        gain = min(1.0, 0.95 / float(np.max(np.abs(mixed))))
        for variant, audio in [
            ("clean", x),
            ("quiet30", x * 0.03),
            ("noise0", mixed * gain),
        ]:
            id = f"en_us-{Path(row[1]).stem}--integration-{variant}"
            target = corpus / (id + ".wav")
            write_float_wav(target, audio, peak_timestamps)
            samples.append(
                {
                    "id": id,
                    "sourceId": "fleurs-en_us-test",
                    "split": "test",
                    "language": "en-US",
                    "audioPath": target.name,
                    "sha256": digest(target.read_bytes()),
                    "reference": row[2],
                    "durationSeconds": len(audio) / 16000,
                    "scenario": "read",
                    "replayKind": "natural",
                    "condition": variant,
                    "baseId": Path(row[1]).stem,
                    "evaluationRole": "holdout",
                    "sex": row[6],
                }
            )
        derivations.append(
            {
                "baseId": Path(row[1]).stem,
                "sourceSha256": digest(path.read_bytes()),
                "noise": noise_meta[index % len(noise_meta)]["path"],
                "noiseGain": float(xrms / nrms),
                "commonHeadroom": float(gain),
            }
        )
    controls = [
        r
        for r in prior["samples"]
        if r.get("variant") == "noise"
        or r.get("variant") == "clean"
        and r["sourceId"] == "synthetic-dictation-controls"
    ]
    ami = json.loads((PRIOR / "ami.json").read_text())
    sources = prior["sources"] + [
        s for s in ami["sources"] if s["id"] not in {x["id"] for x in prior["sources"]}
    ]
    controls += ami["samples"][:2]
    for row in controls:
        path = PRIOR / "corpus" / row["audioPath"]
        shutil.copy2(path, corpus / path.name)
        samples.append(
            {
                **row,
                "audioPath": path.name,
                "evaluationRole": "observed-regression-control",
                "condition": "control",
            }
        )
    write_json(
        ROOT / "quality.json",
        {"schemaVersion": 1, "sources": sources, "samples": samples},
    )
    # Sustained replay is a state/queue test, never a natural accuracy denominator.
    clean = next(r for r in samples if r.get("condition") == "clean")
    x, _ = sf.read(corpus / clean["audioPath"], dtype="float32")
    soak = np.resize(x, 1800 * 16000)
    write_float_wav(corpus / "soak-30min.wav", soak, peak_timestamps)
    warm = {**clean, "id": "soak-warmup"}
    long = {
        **clean,
        "id": "frontend-soak-30min",
        "audioPath": "soak-30min.wav",
        "sha256": digest((corpus / "soak-30min.wav").read_bytes()),
        "durationSeconds": 1800.0,
        "replayKind": "synthetic_soak",
        "reference": clean["reference"],
        "evaluationRole": "state-only-no-accuracy",
    }
    write_json(
        ROOT / "soak.json",
        {
            "schemaVersion": 1,
            "sources": sources,
            "samples": [warm, long, {**clean, "id": "soak-reset"}],
        },
    )
    # Short paired live runs include quiet/noisy samples and A/B/A reset.
    live = samples[:6]
    write_json(
        ROOT / "streaming.json",
        {
            "schemaVersion": 1,
            "sources": sources,
            "samples": live + [dict(live[0], id="stream-reset")],
        },
    )
    write_json(
        ROOT / "protocol.json",
        {
            "version": 1,
            "selection": "sha256 frontend-integration-v1 order; 4 per sex x duration stratum",
            "excludedBaseIds": sorted(seen),
            "derivations": derivations,
            "frontendSettings": {
                "threshold": 0.5,
                "minSilenceSeconds": 0.25,
                "minSpeechSeconds": 0.064,
                "frameSamples": 512,
                "initialContextSamples": 64,
                "bookkeepingFrames": 320,
                "denoiserDelaySamples": 640,
                "nativeThreads": 1,
            },
            "schedules": ["raw", "vad", "vad-dpdfnet4", "vad-dpdfnet8"],
            "models": ["cohere", "whisper-small", "whisper-turbo"],
            "caveats": [
                "FLEURS speaker IDs are unavailable; no speaker-disjointness claim",
                "Prior regression controls are not fresh holdout",
                "No physical microphone, edge hardware, power telemetry or singing qualification",
                "Synthetic soak excluded from accuracy aggregates",
            ],
            "qualityManifestSha256": digest((ROOT / "quality.json").read_bytes()),
            "soakManifestSha256": digest((ROOT / "soak.json").read_bytes()),
        },
    )
    if frozen_protocol:
        for name in ["quality.json", "streaming.json", "soak.json"]:
            for sample in json.loads((FROZEN / name).read_text())["samples"]:
                if digest((corpus / sample["audioPath"]).read_bytes()) != sample["sha256"]:
                    raise ValueError(
                        f"Reproduction differs for {sample['id']}; preserve the pinned corpus"
                    )
            shutil.copy2(FROZEN / name, ROOT / name)
        shutil.copy2(FROZEN / "protocol.json", ROOT / "protocol.json")
    print(
        f"Frozen {len(samples)} cases: {len(selected)} new base recordings x3 conditions, {len(controls)} existing controls; 30-minute state replay."
    )


if __name__ == "__main__":
    main()
