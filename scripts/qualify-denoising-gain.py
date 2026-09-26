"""Fixed whole-recording RMS diagnostic; not a causal gain controller."""

import argparse
import importlib.util
import os
import shutil
import subprocess
import sys
from pathlib import Path

import numpy as np
import soundfile as sf


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


runner = module("denoising_runner", "run-denoising-followup.py")
audio = module("frontend_prepare", "prepare-frontend-integration.py")
read, write, digest = runner.read, runner.common.write, runner.common.digest
PRIOR = Path(".cache/benchmarks/results/denoising-followup-2026-09-19")
ROOT = Path(".cache/benchmarks/results/denoising-gain-matched-2026-09-19")
SIGNALS = [
    "raw",
    "dpdfnet4-full",
    "dpdfnet4-pregain-restore",
    "dpdfnet8-full",
    "dpdfnet8-pregain-restore",
]
LEVELS = {"quiet": 1.0, "restored": 1.0 / 0.03}
PEAK = 0.95
MAX_GAIN = 10000.0
FLOOR = 1e-12


def levels(values):
    values = np.asarray(values, dtype=np.float64)
    if values.ndim != 1 or not values.size or not np.isfinite(values).all():
        raise ValueError("expected finite, nonempty mono PCM")
    rms, peak = float(np.sqrt(np.mean(values * values))), float(np.max(np.abs(values)))
    if rms <= FLOOR:
        raise ValueError("RMS is too small for the fixed matching protocol")
    return {"rms": rms, "peak": peak}


def match(signals, multiplier):
    """Reduce one shared target; never cap individual gains or clip samples."""
    if not np.isfinite(multiplier) or multiplier <= 0:
        raise ValueError("invalid level multiplier")
    stats = {k: levels(v) for k, v in signals.items()}
    if len({len(v) for v in signals.values()}) != 1:
        raise ValueError("unaligned waveform lengths")
    requested = stats["raw"]["rms"] * multiplier
    peak_target = min(PEAK * v["rms"] / v["peak"] for v in stats.values())
    gain_target = min(MAX_GAIN * v["rms"] for v in stats.values())
    target = min(requested, peak_target, gain_target)
    outputs, details = {}, {}
    for name, values in signals.items():
        gain = target / stats[name]["rms"]
        output = (values.astype(np.float64) * gain).astype(np.float32)
        actual = levels(output)
        assert abs(actual["rms"] / target - 1) < 1e-6
        assert actual["peak"] <= PEAK + 1e-7
        outputs[name] = output
        details[name] = {"input": stats[name], "gain": gain, "output": actual}
    return outputs, {
        "requestedRms": requested,
        "commonRms": target,
        "peakLimited": target < requested and peak_target <= gain_target,
        "gainLimited": target < requested and gain_target <= peak_target,
        "signals": details,
    }


def prepare():
    ROOT.mkdir(parents=True, exist_ok=False)
    for folder in ["corpus", "manifests", "harness"]:
        (ROOT / folder).mkdir()
    selection = read(PRIOR / "quality.json")
    frozen_selection = Path("benchmarks/asr/manifests/denoising-followup-2026-09-19/quality.json")
    assert digest(PRIOR / "quality.json") == digest(frozen_selection), "Source selection changed"
    prior_pins = read(PRIOR / "execution-pins-quality.json")
    selected = [
        r
        for r in selection["samples"]
        if r["evaluationRole"] == "observed-regression" and r["condition"] == "quiet30"
    ]
    assert len(selected) == 24 and all(r["replayKind"] == "natural" for r in selected)
    input_manifests = {name: read(PRIOR / "manifests" / (name + ".json")) for name in SIGNALS}
    for name in SIGNALS:
        path = PRIOR / "manifests" / (name + ".json")
        assert digest(path) == prior_pins[str(path)], "Source manifest changed"
    by_id = {name: {r["id"]: r for r in data["samples"]} for name, data in input_manifests.items()}
    profiles = {f"{level}-{signal}": [] for level in LEVELS for signal in SIGNALS}
    records, source_pins, failures = [], {}, []
    for row in selected:
        sample_id = row["id"]
        try:
            signals = {}
            for name in SIGNALS:
                source = by_id[name][sample_id]
                path = PRIOR / "corpus" / source["audioPath"]
                assert digest(path) == source["sha256"]
                source_pins[str(path)] = source["sha256"]
                values, rate = sf.read(path, dtype="float32")
                if rate != 16000:
                    raise ValueError("expected canonical 16 kHz PCM")
                signals[name] = values
            for label, multiplier in LEVELS.items():
                output, detail = match(signals, multiplier)
                records.append({"sampleId": sample_id, "level": label, **detail})
                for name, values in output.items():
                    profile = f"{label}-{name}"
                    filename = f"{profile}--{sample_id}.wav"
                    path = ROOT / "corpus" / filename
                    audio.write_float_wav(path, values, {filename: 0})
                    reloaded, rate = sf.read(path, dtype="float32")
                    assert rate == 16000 and np.array_equal(values, reloaded)
                    profiles[profile].append({**row, "audioPath": filename, "sha256": digest(path)})
        except (OSError, ValueError, KeyError, AssertionError) as error:
            failures.append({"sampleId": sample_id, "errorType": type(error).__name__})
    write(ROOT / "preprocessing.json", {"rows": records, "failures": failures})
    if failures:
        raise RuntimeError("Matching failed; preserve unsuccessful preprocessing records")
    for profile, rows in profiles.items():
        write(ROOT / "manifests" / (profile + ".json"), {**selection, "samples": rows})
    write(ROOT / "quality.json", {**selection, "samples": selected})
    write(ROOT / "source-input-pins.json", source_pins)
    write(
        ROOT / "protocol.json",
        {
            "signals": SIGNALS,
            "levels": LEVELS,
            "peak": PEAK,
            "maxGain": MAX_GAIN,
            "rmsFloor": FLOOR,
            "expectedAttempts": 720,
            "expectedProcesses": 30,
            "sourceSelectionSha256": digest(PRIOR / "quality.json"),
            "sourceResultSha256": digest(PRIOR / "debug-summary.json"),
            "driverRevision": subprocess.check_output(
                ["git", "rev-parse", "HEAD"], text=True
            ).strip(),
            "scope": "24 observed artificially quiet English clips; whole-clip future information; no causal AGC or fresh holdout",
        },
    )
    old_pins = read(PRIOR / "execution-pins-quality.json")
    binary = PRIOR / "vesper-eval"
    assert digest(binary) == old_pins[str(binary)]
    shutil.copy2(binary, ROOT / "vesper-eval")
    for name in ["source-hashes.json", "source-revision.txt", "runtime-env.json"]:
        shutil.copy2(PRIOR / name, ROOT / name)
    environment = read(PRIOR / "environment.json")
    environment.update(
        label="denoising-gain-matched-2026-09-19",
        notes="Frozen preceding native evaluator/source; whole-clip RMS diagnostic. CPU8, writer Q4_0, ASR INT8. No streaming, new training or production gain controller. Per-process power and contention are authoritative.",
    )
    write(ROOT / "environment.json", environment)
    harness = [
        "qualify-denoising-gain.py",
        "run-denoising-followup.py",
        "qualify-frontend-integration.py",
        "prepare-frontend-integration.py",
    ]
    harness_pins = {}
    for name in harness:
        path = Path("scripts") / name
        harness_pins[str(path)] = digest(path)
        shutil.copy2(path, ROOT / "harness" / name)
    write(ROOT / "harness-hashes.json", harness_pins)
    print(f"Prepared {len(profiles)} profiles x 24 clips; {len(records)} matched sets")


def execution_directory():
    """Pin imports as well as binaries against concurrent worktree changes."""
    directory = ROOT.resolve() / "execution-cwd"
    directory.mkdir(exist_ok=True)
    for name, target in [
        ("scripts", ROOT / "harness"),
        (".cache", Path(".cache")),
        ("models", Path("models")),
        (".tooling", Path(".tooling")),
    ]:
        link, expected = directory / name, target.resolve()
        if link.is_symlink():
            assert link.resolve() == expected, "Execution-directory target changed"
        else:
            link.symlink_to(expected, target_is_directory=True)
    return directory


def run():
    directory = execution_directory()
    if Path.cwd().resolve() != directory:
        subprocess.run(
            [sys.executable, "scripts/qualify-denoising-gain.py", "run"],
            cwd=directory,
            check=True,
        )
        return
    harness = read(ROOT / "harness-hashes.json")
    assert all(digest(Path(p)) == value for p, value in harness.items()), "Harness changed"
    sources = read(ROOT / "source-input-pins.json")
    assert all(digest(Path(p)) == value for p, value in sources.items()), "Source PCM changed"
    profiles = [f"{level}-{signal}" for level in LEVELS for signal in SIGNALS]
    jobs = []
    for index, (model, model_id) in enumerate(runner.MODELS.items()):
        order = profiles[index:] + profiles[:index]
        for profile in order:
            name = f"quality-{model}-{profile}"
            manifest = ROOT / "manifests" / (profile + ".json")
            command = [
                str(ROOT / "vesper-eval"),
                "run",
                str(manifest),
                str(ROOT / "corpus"),
                "models/downloads",
                str(ROOT / "environment.json"),
                str(ROOT / "runs" / (name + ".json")),
                "--model",
                model_id,
                "--language",
                "en",
                "--cleanup",
            ]
            jobs.append((name, model, profile, command, manifest))
    pinned = {ROOT / "vesper-eval", ROOT / "environment.json", ROOT / "protocol.json"}
    pinned.update(Path(p) for p in harness)
    pinned.update(ROOT.glob("manifests/*.json"))
    pinned.update(ROOT.glob("corpus/*.wav"))
    # Experimental process switches are not inherited from another workload.
    env = {
        k: v
        for k, v in os.environ.items()
        if not k.startswith(("TRANSCRIBE_", "UKTI_", "STT_", "GGML_", "LLAMA_"))
    }
    env.update(read(ROOT / "runtime-env.json"))
    runner.run_jobs(ROOT, "quality", jobs, env, pinned)


def verify():
    """Recheck measured data after inference, using the actual frozen harness."""
    source_pins = read(ROOT / "source-input-pins.json")
    assert digest(PRIOR / "quality.json") == read(ROOT / "protocol.json")["sourceSelectionSha256"]
    assert digest(PRIOR / "quality.json") == digest(
        Path("benchmarks/asr/manifests/denoising-followup-2026-09-19/quality.json")
    )
    for name in SIGNALS:
        path = PRIOR / "manifests" / (name + ".json")
        assert digest(path) == read(PRIOR / "execution-pins-quality.json")[str(path)]
    for name, expected in source_pins.items():
        assert digest(Path(name)) == expected, name
    for name, expected in read(ROOT / "harness-hashes.json").items():
        assert digest(ROOT / "harness" / Path(name).name) == expected, name
    for path in ROOT.glob("execution-pins-*.json"):
        for name, expected in read(path).items():
            source = (
                ROOT / "harness" / Path(name).name if name.startswith("scripts/") else Path(name)
            )
            assert digest(source) == expected, name
    source_hashes = read(ROOT / "source-hashes.json")
    for name, expected in source_hashes.items():
        assert digest(PRIOR / "source-snapshot" / name) == expected, name
    prior = {
        name: {r["id"]: r for r in read(PRIOR / "manifests" / (name + ".json"))["samples"]}
        for name in SIGNALS
    }
    details = {(r["sampleId"], r["level"]): r for r in read(ROOT / "preprocessing.json")["rows"]}
    inputs, targets = {}, {}
    for sample_id, level in details:
        stats = {}
        for signal in SIGNALS:
            key = sample_id, signal
            if key not in inputs:
                path = PRIOR / "corpus" / prior[signal][sample_id]["audioPath"]
                inputs[key], rate = sf.read(path, dtype="float32")
                assert rate == 16000
            stats[signal] = levels(inputs[key])
        target = min(
            stats["raw"]["rms"] * LEVELS[level],
            *(min(PEAK * s["rms"] / s["peak"], MAX_GAIN * s["rms"]) for s in stats.values()),
        )
        assert target == details[(sample_id, level)]["commonRms"]
        targets[(sample_id, level)] = target
    count, raw_identical, largest_relative_rms_error = 0, 0, 0.0
    for level in LEVELS:
        for signal in SIGNALS:
            manifest = read(ROOT / "manifests" / (f"{level}-{signal}.json"))
            for row in manifest["samples"]:
                original = inputs[(row["id"], signal)]
                path = ROOT / "corpus" / row["audioPath"]
                assert digest(path) == row["sha256"]
                output, rate = sf.read(path, dtype="float32")
                detail = details[(row["id"], level)]
                transform = detail["signals"][signal]
                expected_gain = targets[(row["id"], level)] / levels(original)["rms"]
                assert expected_gain == transform["gain"]
                expected = (original.astype(np.float64) * expected_gain).astype(np.float32)
                assert rate == 16000
                assert np.array_equal(output, expected) and len(output) == len(original)
                actual = levels(output)
                error = abs(actual["rms"] / detail["commonRms"] - 1)
                assert error < 1e-6 and actual["peak"] <= PEAK + 1e-7
                largest_relative_rms_error = max(largest_relative_rms_error, error)
                if (
                    level == "quiet"
                    and signal == "raw"
                    and not detail["peakLimited"]
                    and not detail["gainLimited"]
                ):
                    assert np.array_equal(output, original)
                    raw_identical += 1
                count += 1
    result = {
        "passed": True,
        "verifiedSourceWaveforms": len(source_pins),
        "verifiedSourceFiles": len(source_hashes),
        "verifiedOutputWaveforms": count,
        "byteExactRawQuietPcm": raw_identical,
        "maximumRelativeRmsError": largest_relative_rms_error,
        "guardEventsPreserved": sum(
            bool(p["changedInputHashes"]) for p in read(ROOT / "processes-quality.json")
        ),
    }
    write(ROOT / "verification.json", result)
    print(result)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", choices=["prepare", "run", "verify"])
    phase = parser.parse_args().phase
    {"prepare": prepare, "run": run, "verify": verify}[phase]()
