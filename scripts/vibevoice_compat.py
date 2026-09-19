"""Prepare a verified, isolated Transformers 5 port of the pinned publisher source."""

import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def prepare_source(root):
    canonical = json.loads(
        (REPOSITORY / "benchmarks/asr/vibevoice-streaming-artifacts.json").read_text()
    )
    provenance = json.loads((root / "verified-artifacts.json").read_text())
    if provenance != canonical:
        raise ValueError("VibeVoice artifact manifest differs from the pinned repository manifest")
    patch = REPOSITORY / "patches/vibevoice/transformers-5.patch"
    pins = json.loads((patch.parent / "provenance.json").read_text())
    if (pins["sourceRevision"], pins["modelRevision"]) != (
        canonical["sourceRevision"],
        canonical["modelRevision"],
    ) or digest(patch) != pins["patchSha256"]:
        raise ValueError("VibeVoice compatibility patch provenance mismatch")
    prepared = tempfile.TemporaryDirectory(prefix="transcribe-vibevoice-source-")
    try:
        target = Path(prepared.name)
        expected = {}
        for entry in canonical["sourceFiles"]:
            name = entry["file"]
            original = root / "source" / name
            if digest(original) != entry["sha256"]:
                raise ValueError(f"Publisher source checksum mismatch: {name}")
            destination = target / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(original.read_bytes())
            expected[name] = entry["sha256"]
        for name, hashes in pins["files"].items():
            if expected.get(name) != hashes["before"]:
                raise ValueError(f"Unexpected compatibility patch input: {name}")
            expected[name] = hashes["after"]
        subprocess.run(
            ["git", "apply", "--check", str(patch)], cwd=target, check=True, capture_output=True
        )
        subprocess.run(["git", "apply", str(patch)], cwd=target, check=True, capture_output=True)
        actual = {
            p.relative_to(target).as_posix(): digest(p) for p in target.rglob("*") if p.is_file()
        }
        if actual != expected:
            raise ValueError("Prepared VibeVoice source differs from the reviewed patch")
        return prepared, pins
    except BaseException:
        prepared.cleanup()
        raise
