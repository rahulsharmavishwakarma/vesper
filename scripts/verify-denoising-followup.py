"""Verify frozen source/input integrity and PCM arithmetic after timed inference."""

import hashlib
import importlib.util
import json
import tarfile
import tempfile
from pathlib import Path

import numpy as np
import soundfile as sf

spec = importlib.util.spec_from_file_location(
    "denoising_prepare", Path(__file__).with_name("prepare-denoising-followup.py")
)
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)
ROOT = prepare.ROOT


def read(path):
    return json.loads(path.read_text())


def pcm(path):
    values, rate = sf.read(path, dtype="float32")
    assert rate == 16000 and values.ndim == 1 and np.isfinite(values).all(), path.name
    return values


def main():
    snapshot = read(ROOT / "source-hashes.json")
    for name, expected in snapshot.items():
        assert prepare.digest(ROOT / "source-snapshot" / name) == expected, name
    for path in ROOT.glob("execution-pins-*.json"):
        for name, expected in read(path).items():
            assert prepare.digest(Path(name)) == expected, name
    initial_harness = read(ROOT / "harness-hashes.json")
    assert (
        prepare.digest(ROOT / "run-denoising-followup-initial.py")
        == initial_harness["scripts/run-denoising-followup.py"]
    )
    manifest = read(ROOT / "quality.json")
    sources = {r["id"]: r for r in manifest["samples"]}
    # Confirm unusually low source levels are in the pinned public archive,
    # rather than introduced by an extraction/conversion of the cached files.
    archive = Path(".cache/benchmarks/downloads/fleurs/data/en_us/audio/test.tar.gz")
    revision = next(s["revision"] for s in manifest["sources"] if s["id"] == "fleurs-en_us-test")
    assert prepare.digest(archive) == revision.split("audio-sha256:")[1].split(";")[0]
    expected_files = {
        r["baseId"] + ".wav" for r in sources.values() if r["sourceId"] == "fleurs-en_us-test"
    }
    archive_matches = set()
    with tarfile.open(archive, "r|gz") as pack:
        for member in pack:
            name = Path(member.name).name
            if member.isfile() and name in expected_files:
                payload = pack.extractfile(member).read()
                cached = Path(".cache/benchmarks/corpora/fleurs-en-us/test") / name
                assert hashlib.sha256(payload).hexdigest() == prepare.digest(cached), name
                archive_matches.add(name)
    assert archive_matches == expected_files
    original = {}
    for sample_id, row in sources.items():
        path = ROOT / "input" / row["audioPath"]
        assert prepare.digest(path) == row["sha256"], sample_id
        original[sample_id] = pcm(path)
    waveforms = {}
    for path in sorted((ROOT / "manifests").glob("*.json")):
        for row in read(path)["samples"]:
            audio = ROOT / "corpus" / row["audioPath"]
            assert prepare.digest(audio) == row["sha256"], row["id"]
            values = pcm(audio)
            assert len(values) == len(original[row["id"]]), row["id"]
            waveforms[(path.stem, row["id"])] = values
    for sample_id, raw in original.items():
        assert np.array_equal(raw, waveforms[("raw", sample_id)]), sample_id
        for denoiser in ["dpdfnet4", "dpdfnet8"]:
            full, mixed = (
                waveforms[(f"{denoiser}-{profile}", sample_id)] for profile in ["full", "mix50"]
            )
            assert np.array_equal(mixed, np.float32(0.5) * full + np.float32(0.5) * raw), sample_id
            assert np.all(np.abs(mixed) <= np.maximum(np.abs(raw), np.abs(full))), sample_id
            diagnostic = waveforms.get((f"{denoiser}-postgain", sample_id))
            if diagnostic is not None:
                assert np.array_equal(diagnostic, full / np.float32(0.03)), sample_id
    # Rebuild the frozen selection in a disposable directory; never overwrite
    # measured inputs or update expected hashes after observing differences.
    with tempfile.TemporaryDirectory(prefix="denoising-reproduce-") as temporary:
        prepare.ROOT = Path(temporary) / "reproduced"
        prepare.main()
        for row in sources.values():
            assert prepare.digest(prepare.ROOT / "input" / row["audioPath"]) == row["sha256"]
    result = {
        "passed": True,
        "sourceSnapshotFiles": len(snapshot),
        "publicArchiveOriginalFiles": len(archive_matches),
        "reproducedInputWaveforms": len(original),
        "processedWaveforms": len(waveforms),
        "exactRawPcmComparisons": len(original),
        "exactMixAndConvexBoundComparisons": len(original) * 2,
        "exactPostgainComparisons": sum(p.endswith("postgain") for p, _ in waveforms),
    }
    prepare.write(ROOT / "verification.json", result)
    print(json.dumps(result))


if __name__ == "__main__":
    main()
