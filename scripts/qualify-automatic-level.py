"""Frozen application-path automatic-level qualification; generated evidence stays ignored.

Prepare once, run development before inspecting quality, then run quality and
streaming with a stable release binary. This is a bounded English qualification,
not physical-microphone, natural-whisper or target-device certification.
"""

import argparse
import csv
import hashlib
import importlib.util
import json
import os
import re
import shutil
import subprocess
from collections import defaultdict
from pathlib import Path

import numpy as np
import soundfile as sf

ROOT = Path(".cache/benchmarks/results/automatic-level-2026-09-19")
PRIOR = Path(".cache/benchmarks/results/frontend-integration-2026-09-18")
PROFILES = {
    "raw": [],
    "level": ["--auto-level"],
    "dpdfnet4": ["--denoiser", "dpdfnet4"],
    "level-dpdfnet4": ["--auto-level", "--denoiser", "dpdfnet4"],
    "dpdfnet8": ["--denoiser", "dpdfnet8"],
    "level-dpdfnet8": ["--auto-level", "--denoiser", "dpdfnet8"],
}
MODELS = {
    "cohere": "CohereLabs/cohere-transcribe-03-2026",
    "small": "openai/whisper-small",
    "turbo": "openai/whisper-large-v3-turbo",
}


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


runner = module("level_runner", "run-denoising-followup.py")
report = module("level_report", "summarize-denoising-followup.py")
writer = module("level_writer", "prepare-frontend-integration.py")
read = runner.read
write = runner.common.write
digest = runner.common.digest


def prepare():
    ROOT.mkdir(parents=True, exist_ok=False)
    corpus = ROOT / "corpus"
    corpus.mkdir()
    original = read(PRIOR / "quality.json")
    by_id = {s["id"]: s for s in original["samples"]}
    seen = set()
    for folder in [
        "benchmarks/asr/manifests",
        "docs/validation",
        ".cache/benchmarks/results",
        ".cache/benchmarks/datasets",
    ]:
        for path in Path(folder).rglob("*.json"):
            if ROOT in path.parents or path.stat().st_size > 30_000_000:
                continue
            seen.update(re.findall(r"en_us-(\d+)", path.read_text(errors="replace")))
    metadata = Path(".cache/benchmarks/downloads/fleurs/data/en_us/test.tsv")
    with metadata.open() as source:
        available = list(csv.reader(source, delimiter="\t", quoting=csv.QUOTE_NONE))
    selected = []
    for sex in ["FEMALE", "MALE"]:
        for low, high in [(2, 6), (6, 10), (10, 16)]:
            eligible = [
                r
                for r in available
                if len(r) == 7
                and r[6] == sex
                and low < int(r[5]) / 16000 <= high
                and Path(r[1]).stem not in seen
            ]
            eligible.sort(
                key=lambda r: hashlib.sha256(("automatic-level-v1:" + r[1]).encode()).hexdigest()
            )
            if not eligible:
                raise ValueError("No unused recordings in selection stratum")
            selected.append(eligible[0])

    def store(sample, audio):
        path = corpus / (sample["id"] + ".wav")
        writer.write_float_wav(path, np.asarray(audio, dtype=np.float32), {path.name: 0})
        return {
            **sample,
            "audioPath": path.name,
            "sha256": digest(path),
            "durationSeconds": len(audio) / 16000,
        }

    def copy(sample):
        path = PRIOR / "corpus" / sample["audioPath"]
        if digest(path) != sample["sha256"]:
            raise ValueError("Prior source audio changed")
        shutil.copy2(path, corpus / sample["audioPath"])
        return {**sample, "evaluationRole": "observed-regression"}

    samples, derivations = [], []
    for index, row in enumerate(selected):
        path = Path(".cache/benchmarks/corpora/fleurs-en-us/test") / row[1]
        audio, rate = sf.read(path, dtype="float32")
        assert rate == 16000 and audio.ndim == 1
        noise_row = by_id[f"noise-natural-{index % 4}"]
        noise_path = PRIOR / "corpus" / noise_row["audioPath"]
        assert digest(noise_path) == noise_row["sha256"]
        noise, _ = sf.read(noise_path, dtype="float32")
        noise = np.resize(noise, len(audio))
        noise_gain = np.sqrt(np.mean(audio.astype(float) ** 2) / np.mean(noise.astype(float) ** 2))
        mixture = audio + noise * noise_gain
        headroom = min(1.0, 0.95 / float(np.max(np.abs(mixture))))
        base = {
            "sourceId": "fleurs-en_us-test",
            "split": "test",
            "language": "en-US",
            "reference": row[2],
            "scenario": "read",
            "replayKind": "natural",
            "baseId": Path(row[1]).stem,
            "sex": row[6],
            "evaluationRole": "fresh-holdout",
        }
        for condition, signal in [
            ("clean", audio),
            ("quiet30", audio * 0.03),
            ("noise0", mixture * headroom),
        ]:
            samples.append(
                store(
                    {
                        **base,
                        "id": f"en_us-{base['baseId']}--auto-{condition}",
                        "condition": condition,
                    },
                    signal,
                )
            )
        derivations.append(
            {
                "baseId": base["baseId"],
                "sourceSha256": digest(path),
                "noiseSha256": noise_row["sha256"],
                "noiseGain": float(noise_gain),
                "headroom": headroom,
            }
        )
    controls = [f"noise-natural-{i}" for i in range(4)] + [
        "noise-silence",
        "noise-white",
        "negation-amount--clean",
        "date-time--clean",
    ]
    samples += [copy(by_id[key]) for key in controls]
    development = [copy(s) for s in original["samples"][:6]]
    write(ROOT / "quality.json", {**original, "samples": samples})
    write(ROOT / "development.json", {**original, "samples": development})
    streaming = samples[:3] + [next(s for s in samples if s["id"] == "noise-natural-0")]
    write(ROOT / "streaming.json", {**original, "samples": streaming})
    # Two minutes of continuous public speech with pauses, then repeat the
    # preceding short recording to expose cross-recording state contamination.
    clean = [s for s in samples if s["condition"] == "clean"]
    chunks, refs, total = [], [], 0
    for index in range(100):
        sample = clean[index % len(clean)]
        audio, _ = sf.read(corpus / sample["audioPath"], dtype="float32")
        chunks += [audio, np.zeros(4000, dtype=np.float32)]
        refs.append(sample["reference"])
        total += len(audio) + 4000
        if total >= 120 * 16000:
            break
    continuous = store(
        {
            **clean[0],
            "id": "auto-continuous-120s",
            "reference": " ".join(refs),
            "condition": "continuous",
            "replayKind": "synthetic_soak",
            "evaluationRole": "paced-composite",
        },
        np.concatenate(chunks),
    )
    repeat = {**clean[0], "id": "auto-reset-repeat"}
    write(ROOT / "soak.json", {**original, "samples": [clean[0], continuous, repeat]})
    write(
        ROOT / "protocol.json",
        {
            "selectionSeed": "automatic-level-v1",
            "excludedBaseIds": sorted(seen),
            "derivations": derivations,
            "metadataSha256": digest(metadata),
            "profiles": PROFILES,
            "models": MODELS,
            "qualityAttempts": len(samples) * len(PROFILES) * len(MODELS),
            "limitations": "Six fresh read-speech recordings with derived quiet/noisy versions; not natural whispers or speaker-disjoint. No physical microphone, edge or SOTA qualification.",
        },
    )
    print(
        "Prepared",
        len(samples),
        "quality cases;",
        len(samples) * len(PROFILES) * len(MODELS),
        "quality attempts",
    )


def run(phase):
    root = ROOT / phase
    root.mkdir(exist_ok=True)
    binary = root / "transcribe-eval"
    if not binary.exists():
        shutil.copy2("target/release/transcribe-eval", binary)
    elif digest(binary) != digest(Path("target/release/transcribe-eval")):
        raise ValueError("Preserve prior attempts: release binary differs")
    inventory = [Path("Cargo.lock"), Path("models/artifacts.json")]
    inventory += [
        Path("scripts") / name
        for name in [
            "qualify-automatic-level.py",
            "run-denoising-followup.py",
            "qualify-frontend-integration.py",
            "summarize-denoising-followup.py",
            "summarize-frontend-integration.py",
            "prepare-frontend-integration.py",
        ]
    ]
    inventory += list(Path("crates").rglob("*.rs"))
    hashes = {str(p): digest(p) for p in sorted(inventory)}
    pins = root / "source-hashes.json"
    if pins.exists() and read(pins) != hashes:
        raise ValueError("Source changed since this phase began")
    write(pins, hashes)
    for path in inventory:
        snapshot = root / "source-snapshot" / path
        if snapshot.exists():
            if digest(snapshot) != hashes[str(path)]:
                raise ValueError("Frozen source snapshot differs")
        else:
            snapshot.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, snapshot)
    env_path = root / "environment.json"
    env = read(PRIOR / "environment.json")
    env.update(
        label=f"automatic-level-{phase}",
        applicationRevision=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
        + "+frozen-source-snapshot",
        powerMode="See each process ledger's powerAtStart; power policy and watts not measured",
        threads=4 if phase in ["streaming", "soak"] else 8,
        notes="Actual application manager/frontend; 32ms level frames; no automatic endpoint; paced file replay, not microphone; per-process contention retained",
    )
    write(env_path, env)
    profiles = list(PROFILES)
    if phase in ["development", "streaming"]:
        profiles = profiles[:4]
    if phase == "soak":
        profiles = ["raw", "level-dpdfnet4"]
    models = (
        {"nemotron": "nvidia/nemotron-3.5-asr-streaming-0.6b"}
        if phase in ["streaming", "soak"]
        else (MODELS if phase == "quality" else {"cohere": MODELS["cohere"]})
    )
    manifest = ROOT / (phase + ".json")
    jobs = []
    for index, (model, model_id) in enumerate(models.items()):
        ordered = profiles[index:] + profiles[:index]
        for profile in ordered:
            name = f"{phase}-{model}-{profile}"
            model_root = (
                ".cache/benchmarks/model-roots/nemotron-cpu"
                if phase in ["streaming", "soak"]
                else "models/downloads"
            )
            command = [
                str(binary),
                "run",
                str(manifest),
                str(ROOT / "corpus"),
                model_root,
                str(env_path),
                str(root / "runs" / (name + ".json")),
                "--model",
                model_id,
                "--language",
                "en",
                *PROFILES[profile],
            ]
            command += (
                ["--streaming", "--realtime", "--chunk-ms", "20"]
                if phase in ["streaming", "soak"]
                else ["--cleanup"]
            )
            jobs.append((name, model, profile, command, manifest))
    overrides = {
        "OMP_NUM_THREADS": "1",
        "OPENBLAS_NUM_THREADS": "1",
        "VECLIB_MAXIMUM_THREADS": "1",
        "MKL_NUM_THREADS": "1",
        "TRANSCRIBE_EXPERIMENTAL_PREFIX_CACHE": "0",
    }
    write(root / "runtime-env.json", overrides)
    pinned = {binary, manifest, pins, env_path, ROOT / "protocol.json"}
    pinned.update(root / "source-snapshot" / path for path in inventory)
    pinned.update(ROOT / "corpus" / s["audioPath"] for s in read(manifest)["samples"])
    runner.run_jobs(root, phase, jobs, {**os.environ, **overrides}, pinned)


def score_counts(value):
    return {key: value[key] for key in ["wordErrorRate", "graphemeErrorRate"]} if value else None


def summarize():
    rows, processes = [], []
    checks = {
        "missingRows": 0,
        "duplicateRows": 0,
        "unexpectedRows": 0,
        "missingJobs": 0,
        "duplicateJobs": 0,
        "unreadableReports": 0,
    }
    for folder in [
        "development-initial",
        "development",
        "quality",
        "streaming",
        "soak-preflight",
        "soak",
    ]:
        phase = {"development-initial": "development", "soak-preflight": "soak"}.get(folder, folder)
        root = ROOT / folder
        ledger = root / f"processes-{phase}.json"
        expected_jobs = {"development": 4, "quality": 18, "streaming": 4, "soak": 2}[phase]
        if not ledger.exists():
            if folder not in ["development-initial", "soak-preflight"]:
                checks["missingJobs"] += expected_jobs
            continue
        manifest = "soak-preflight" if folder == "soak-preflight" else phase
        sources = {s["id"]: s for s in read(ROOT / (manifest + ".json"))["samples"]}
        phase_processes = read(ledger)
        unique_jobs = len({p["name"] for p in phase_processes})
        checks["duplicateJobs"] += len(phase_processes) - unique_jobs
        checks["missingJobs"] += max(0, expected_jobs - unique_jobs)
        for process in phase_processes:
            processes.append(process)
            path = root / "runs" / (process["name"] + ".json")
            try:
                result = read(path)["runs"] if path.exists() else []
            except (OSError, ValueError, KeyError):
                checks["unreadableReports"] += 1
                result = []
            by_id = defaultdict(list)
            for item in result:
                by_id[item["sampleId"]].append(item)
            checks["unexpectedRows"] += len(by_id.keys() - sources.keys())
            for sample_id, source in sources.items():
                candidates = by_id.get(sample_id, [])
                checks["duplicateRows"] += max(0, len(candidates) - 1)
                if not candidates:
                    checks["missingRows"] += 1
                r = (
                    candidates[0]
                    if candidates
                    else {
                        "sampleId": sample_id,
                        "audioSeconds": source["durationSeconds"],
                        "rawHypothesis": None,
                        "error": "missing result",
                        "execution": {},
                    }
                )
                front = r["execution"].get("audioFrontend", {})
                natural = source["replayKind"] == "natural"
                item = {
                    "job": process["name"],
                    "model": process["model"],
                    "profile": process["profile"],
                    "phase": folder,
                    "sampleId": sample_id,
                    "condition": source["condition"],
                    "evaluationRole": source["evaluationRole"],
                    "replayKind": source["replayKind"],
                    "nonSpeechReference": source["reference"] == "",
                    "audioSeconds": source["durationSeconds"],
                    "asrFailed": r["rawHypothesis"] is None,
                    "writerFailed": r["rawHypothesis"] is not None and r["error"] is not None,
                    "failureKind": "missing_result"
                    if not candidates
                    else report.common.failure_kind(r),
                    "rawSha256": report.common.hash_text(r["rawHypothesis"]),
                    "cleanedSha256": report.common.hash_text(r.get("cleanedHypothesis")),
                    "rawScore": score_counts(r.get("rawScore")) if natural else None,
                    "cleanedScore": score_counts(r.get("cleanedScore")) if natural else None,
                    "meaningScore": score_counts(r.get("rawScore")) if not natural else None,
                    "rawNonSpeech": r.get("rawNonSpeechScore"),
                    "resident": r.get("residentRun", False),
                    "latencyUsable": process["latencyUsable"],
                    "frontend": front.get("metrics"),
                    "frontendFallback": front.get("originalAudioFallback", False),
                }
                for key in [
                    "asrMs",
                    "cleanupMs",
                    "firstPartialMs",
                    "stopToFinalMs",
                    "partialUpdates",
                    "partialEditDistance",
                    "realTimeFactor",
                ]:
                    item[key] = r.get(key)
                rows.append(item)
    groups = defaultdict(list)
    for row in rows:
        groups[(row["phase"], row["model"], row["profile"], row["condition"])].append(row)
    aggregates = [
        {"phase": k[0], "model": k[1], "profile": k[2], "condition": k[3], **report.aggregate(v)}
        for k, v in groups.items()
    ]
    checks.update(
        failedProcesses=sum(p["returnCode"] != 0 for p in processes),
        changedInputProcesses=sum(bool(p["changedInputHashes"]) for p in processes),
        retainedAttempts=len(rows),
        asrFailures=sum(r["asrFailed"] for r in rows),
        writerFailures=sum(r["writerFailed"] for r in rows),
        frontendFallbacks=sum(r["frontendFallback"] for r in rows),
    )
    write(
        ROOT / "summary.json",
        {
            "checks": checks,
            "protocol": read(ROOT / "protocol.json"),
            "processes": processes,
            "rows": rows,
            "aggregates": aggregates,
        },
    )
    print(json.dumps(checks, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "phase", choices=["prepare", "development", "quality", "streaming", "soak", "summarize"]
    )
    phase = parser.parse_args().phase
    if phase == "prepare":
        prepare()
    elif phase == "summarize":
        summarize()
    else:
        run(phase)


if __name__ == "__main__":
    main()
