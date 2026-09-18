#!/usr/bin/env python3
"""Freeze paired, deterministic audio regression fixtures before model execution."""

import argparse
import csv
import hashlib
import json
import shutil
import tarfile
from pathlib import Path

import numpy as np
import soundfile as sf

RATE = 16000
VARIANTS = (
    "clean",
    "matchedClean",
    "quiet30",
    "quiet10",
    "natural10",
    "natural0",
    "naturalm5",
    "clip",
    "reverb",
    "drop10",
)
ASR_VARIANTS = {"clean", "quiet30", "natural10", "natural0"}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def rms(x):
    return float(np.sqrt(np.mean(np.asarray(x, dtype=np.float64) ** 2)))


def variants(audio, noise, seed):
    """Common headroom gain preserves paired clean/noisy comparisons and SNR."""
    if not rms(audio) or not rms(noise):
        raise ValueError("speech and noise must have nonzero energy")
    mixtures = {}
    gains = {}
    for name, db in [("natural10", 10), ("natural0", 0), ("naturalm5", -5)]:
        gain = rms(audio) / (rms(noise) * 10 ** (db / 20))
        mixtures[name] = audio + noise * gain
        gains[name] = gain
    common_gain = min(
        1.0, 0.95 / max(float(np.max(np.abs(x))) for x in [audio, *mixtures.values()])
    )
    clean = audio * common_gain
    result = {
        "clean": audio.copy(),
        "matchedClean": clean,
        "quiet30": audio * 0.03,
        "quiet10": audio * 0.01,
    }
    result.update({k: v * common_gain for k, v in mixtures.items()})
    result["clip"] = np.clip(audio * 6, -0.25, 0.25)
    reverberant = audio.copy()
    for delay, gain in [(640, 0.4), (1440, 0.25), (2720, 0.1)]:
        if len(clean) > delay:
            reverberant[delay:] += audio[:-delay] * gain
    result["reverb"] = reverberant / 1.75
    dropped = audio.copy()
    rng = np.random.default_rng(seed)
    blocks = len(clean) // 320
    indices = rng.choice(blocks, size=max(1, blocks // 10), replace=False)
    for index in indices:
        dropped[index * 320 : (index + 1) * 320] = 0
    result["drop10"] = dropped
    evidence = {
        "commonHeadroomGain": common_gain,
        "noiseAmplitudeGains": gains,
        "snrDefinition": "whole-waveform RMS speech/noise; same common gain",
        "measuredSnrDb": {
            k: 20 * np.log10(rms(clean) / rms(v - clean)) for k, v in result.items() if k in gains
        },
        "clipFraction": float(np.mean(np.abs(audio * 6) > 0.25)),
        "packetDropBlocks": sorted(int(i) for i in indices),
        "reverb": "direct + 40/90/170ms echoes .4/.25/.1, divide1.75; truncate tail",
    }
    for name, value in result.items():
        if len(value) != len(audio) or not np.isfinite(value).all() or np.max(abs(value)) > 1:
            raise ValueError(f"invalid generated waveform: {name}")
    return result, evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--prior-ids", type=Path, required=True)
    args = parser.parse_args()
    root = args.root
    corpus = root / "corpus"
    corpus.mkdir(parents=True, exist_ok=True)
    previous = json.loads(Path("benchmarks/asr/manifests/audio-frontends-screen.json").read_text())
    sources = previous["sources"]
    sources.append(
        {
            "id": "ms-snsd-noise-test",
            "name": "MS-SNSD environmental test noise",
            "url": "https://github.com/microsoft/MS-SNSD/tree/fe61c4ba0d9ac8dd7e23d719cc79f8947e1dc742/noise_test",
            "revision": "fe61c4ba0d9ac8dd7e23d719cc79f8947e1dc742; individual hashes in assets manifest",
            "license": "Collection: Freesound CC0 and DEMAND CC-BY-SA3 per official README; code MIT",
            "attribution": "Microsoft MS-SNSD; Reddy et al., Interspeech2019; original sound contributors",
            "redistribution": "Audio retained locally; no assumption of MIT audio licensing",
        }
    )
    excluded = set(json.loads(args.prior_ids.read_text()))
    selected = []
    for lang, per_stratum in [("en_us", 4), ("hi_in", 2)]:
        folder = Path(".cache/benchmarks/downloads/fleurs/data") / lang
        rows = [
            r
            for r in csv.reader(
                (folder / "test.tsv").open(), delimiter="\t", quoting=csv.QUOTE_NONE
            )
            if r
        ]
        if any(len(r) != 7 for r in rows):
            raise ValueError("unexpected FLEURS metadata columns")
        for sex in ["FEMALE", "MALE"]:
            for low, high in [(2, 8), (8, 16), (16, 25)]:
                eligible = [
                    r
                    for r in rows
                    if r[6] == sex
                    and low < int(r[5]) / RATE <= high
                    and not any(f"{lang}-{Path(r[1]).stem}" in v or r[1] == v for v in excluded)
                ]
                eligible.sort(key=lambda r: digest(f"audio-hard-v1:{lang}:{r[1]}".encode()))
                if len(eligible) < per_stratum:
                    raise ValueError("insufficient untouched stratum")
                for row in eligible[:per_stratum]:
                    selected.append(
                        {
                            "id": f"{lang}-{Path(row[1]).stem}",
                            "filename": row[1],
                            "lang": lang,
                            "reference": row[2],
                            "sex": sex,
                            "durationBand": [low, high],
                            "expectedSamples": int(row[5]),
                        }
                    )
        with tarfile.open(folder / "audio/test.tar.gz") as archive:
            for item in [s for s in selected if s["lang"] == lang]:
                # Exact named member; never extract archive paths to the filesystem.
                payload = archive.extractfile(f"test/{item['filename']}").read()
                path = corpus / f"{item['id']}-source.wav"
                path.write_bytes(payload)
                item["sourceAudioSha256"] = digest(payload)
    existing = Path(".cache/benchmarks/results/audio-frontends-2026-09-16/corpus")
    controls = [
        s
        for s in previous["samples"]
        if s["id"] in {"negation-amount", "date-time", "spoken-instruction", "self-correction"}
    ]
    for s in controls:
        shutil.copyfile(existing / s["audioPath"], corpus / f"{s['id']}-source.wav")
        selected.append(
            {
                "id": s["id"],
                "reference": s["reference"],
                "lang": "en_us",
                "synthetic": True,
                "sourceAudioSha256": s["sha256"],
            }
        )
    noise_root = Path(".cache/benchmarks/downloads/ms-snsd")
    assets = json.loads((noise_root / "assets.json").read_text())
    shutil.copyfile(noise_root / "assets.json", root / "noise-assets.json")
    noises = []
    for asset in assets:
        path = noise_root / asset["path"]
        if digest(path.read_bytes()) != asset["sha256"]:
            raise ValueError("noise hash mismatch")
        noise, rate = sf.read(path, dtype="float64")
        if rate != RATE or noise.ndim != 1:
            raise ValueError("expected native mono16k noise")
        noises.append(noise)
    samples = []
    derivations = []

    def save(identifier, audio, reference, language, source, lane, **metadata):
        path = corpus / f"{identifier}.wav"
        sf.write(path, audio, RATE, subtype="FLOAT")
        sample = {
            "id": identifier,
            "sourceId": source,
            "split": "test",
            "language": language,
            "audioPath": path.name,
            "sha256": digest(path.read_bytes()),
            "reference": reference,
            "durationSeconds": len(audio) / RATE,
            "scenario": "read" if reference else "noise",
            "replayKind": lane,
            **metadata,
        }
        samples.append(sample)
        return sample

    for index, item in enumerate(selected):
        path = corpus / f"{item['id']}-source.wav"
        audio, rate = sf.read(path, dtype="float64")
        if rate != RATE or audio.ndim != 1 or not 0 < len(audio) <= 25 * RATE:
            raise ValueError("unexpected source geometry")
        if "expectedSamples" in item and len(audio) != item["expectedSamples"]:
            raise ValueError("FLEURS duration metadata mismatch")
        noise_index = index % len(noises)
        noise = noises[noise_index]
        seed = int(digest(item["id"].encode())[:8], 16)
        offset = seed % len(noise)
        noise = noise[(np.arange(len(audio)) + offset) % len(noise)]
        generated, evidence = variants(audio, noise, seed)
        language = "en-US" if item["lang"] == "en_us" else "hi-IN"
        source = (
            "synthetic-dictation-controls"
            if item.get("synthetic")
            else f"fleurs-{item['lang']}-test"
        )
        for variant, value in generated.items():
            save(
                f"{item['id']}--{variant}",
                value,
                item["reference"],
                language,
                source,
                "synthetic_soak" if item.get("synthetic") else "natural",
                baseId=item["id"],
                variant=variant,
                sourceAudioSha256=item["sourceAudioSha256"],
                noiseId=assets[noise_index]["path"],
                corpusLane="synthetic" if item.get("synthetic") else "natural",
            )
        derivations.append(
            {
                **item,
                **evidence,
                "noiseId": assets[noise_index]["path"],
                "noiseOffsetSamples": offset,
            }
        )
    for index, asset in enumerate(assets):
        save(
            f"noise-natural-{index}",
            noises[index][: 5 * RATE],
            "",
            "en-US",
            "ms-snsd-noise-test",
            "natural",
            baseId=f"noise-natural-{index}",
            variant="noise",
            corpusLane="noise",
        )
    for item in previous["samples"]:
        if item["id"].startswith("noise-"):
            audio, _ = sf.read(existing / item["audioPath"], dtype="float64")
            save(
                item["id"],
                audio,
                "",
                "en-US",
                item["sourceId"],
                "synthetic_soak",
                baseId=item["id"],
                variant="noise",
                corpusLane="noise",
            )
    base_samples = samples.copy()
    reset = next(s for s in samples if s["corpusLane"] == "natural" and s["variant"] == "clean")
    repeated = {**reset, "id": reset["id"] + "--reset", "corpusLane": "reset"}
    samples.append(repeated)

    def manifest(name, rows):
        (root / f"{name}.json").write_text(
            json.dumps(
                {"schemaVersion": 1, "sources": sources, "samples": rows},
                indent=2,
                ensure_ascii=False,
            )
            + "\n"
        )

    manifest("frontend", samples)
    manifest(
        "asr",
        [
            s
            for s in base_samples
            if (s["language"] == "en-US" and s["variant"] in ASR_VARIANTS)
            or s["corpusLane"] == "noise"
        ]
        + [repeated],
    )
    manifest("ear", samples)
    manifest(
        "uhm",
        [
            s
            for s in base_samples
            if s["variant"] == "clean"
            or s["corpusLane"] == "noise"
            or (s["corpusLane"] == "synthetic" and s["variant"] in {"quiet30", "natural0"})
        ]
        + [repeated],
    )
    # Known meaningful/filler controls from the earlier run, kept out of natural accuracy.
    filler_manifest = json.loads(
        Path("benchmarks/asr/manifests/audio-frontends-fillers.json").read_text()
    )
    filler_rows = []
    for item in filler_manifest["samples"]:
        if item["id"] in {
            "filler-um",
            "filler-uh",
            "affirmation-hmm",
            "meaningful-and",
        }:
            shutil.copyfile(existing / item["audioPath"], corpus / item["audioPath"])
            filler_rows.append(item)
    if filler_rows:
        (root / "filler-controls.json").write_text(
            json.dumps({**filler_manifest, "samples": filler_rows}, indent=2) + "\n"
        )
    # Short sample/hop boundaries and an energetic hard cut; no invented word truth.
    clean, _ = sf.read(corpus / reset["audioPath"], dtype="float64")
    start = int(np.argmax(np.convolve(clean**2, np.ones(1600), "valid")))
    boundary_rows = []
    for length in [
        1,
        63,
        64,
        159,
        160,
        255,
        256,
        319,
        320,
        511,
        512,
        513,
        639,
        640,
        641,
        15999,
    ]:
        value = clean[start : start + length]
        boundary_rows.append(
            save(
                f"boundary-{length}",
                value,
                "",
                "en-US",
                reset["sourceId"],
                "synthetic_soak",
                corpusLane="contract",
                variant="boundary",
            )
        )
    manifest("boundaries", boundary_rows + [repeated])
    # Three-minute stitched continuous frontend test; fresh stream follows in same process.
    pieces = [
        sf.read(corpus / s["audioPath"], dtype="float64")[0]
        for s in base_samples
        if s["language"] == "en-US"
        and s["variant"] in {"clean", "natural0", "quiet30"}
        and s["corpusLane"] == "natural"
    ]
    continuous = np.concatenate(pieces)[: 180 * RATE]
    long_row = save(
        "continuous-180s",
        continuous,
        "",
        "en-US",
        reset["sourceId"],
        "synthetic_soak",
        corpusLane="contract",
        variant="continuous",
    )
    manifest("continuous", [reset, long_row, repeated])
    (root / "derivations.json").write_text(
        json.dumps(
            {
                "protocol": "audio-hard-v1",
                "priorIdsSha256": digest(args.prior_ids.read_bytes()),
                "excludedIds": sorted(excluded),
                "sources": sources,
                "derivations": derivations,
                "variants": VARIANTS,
                "selection": "SHA256 order, sex x (2,8]/(8,16]/(16,25]s, 4en/2hi per stratum",
                "limitations": "Unused in repository reports, not speaker-disjoint or checkpoint-training-disjoint; artificial mixtures",
            },
            indent=2,
            ensure_ascii=False,
        )
        + "\n"
    )
    print(
        json.dumps(
            {
                "naturalEnglish": 24,
                "naturalHindi": 12,
                "baseCases": len(selected),
                "frontendCases": len(base_samples) + 1,
                "root": str(root),
            }
        )
    )


if __name__ == "__main__":
    main()
