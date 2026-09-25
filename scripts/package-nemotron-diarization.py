#!/usr/bin/env python3
"""Package a pinned local Metal diarization build without replacing the ASR runtime.

Run with uv. Requires Xcode tools and the already built official metal-diar preset.
The generated manifest is local build provenance, not a publisher signature.
"""

import argparse
import hashlib
import json
import platform
import re
import shutil
import subprocess
import tempfile
from pathlib import Path

REVISION = "97a15afa5caa9bce5baaa86c1184103877af4101"
GGML_REVISION = "c03b4e2bcece5134827881af90242086daf75be5"
PATCH_SHA = "d1ff5ea3034b382329c578fc37d55f525a227eebd6508b3af8233994897476b0"
PREFIX = "libvesper_diar_97a15afa_"
MODEL_SHA = "08456d9e22cd9a323c0364d98375f3746d6e68507ebb705cd46438c534c7a3a1"
COMPONENTS = {
    "api": "libnemo_speech_asr_c.1.dylib",  # gitleaks:allow -- public library filename
    "asr": "libnemo_speech_asr.dylib",
    "ggml": "libggml.dylib",
    "base": "libggml-base.dylib",
    "cpu": "libggml-cpu.dylib",
    "blas": "libggml-blas.dylib",
    "metal": "libggml-metal.dylib",
}


def output(*args):
    return subprocess.check_output(args, text=True).strip()


def sha(path):
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def dependencies(path):
    return [
        line.strip().split(" (compatibility")[0]
        for line in output("otool", "-L", str(path)).splitlines()[1:]
    ]


def package(source, root, model):
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        raise ValueError("This source package is qualified only on Apple silicon macOS")
    if output("git", "-C", str(source), "rev-parse", "HEAD") != REVISION:
        raise ValueError("Unexpected native source revision")
    if output("git", "-C", str(source), "diff", "HEAD", "--", ".", ":(exclude)ggml"):
        raise ValueError("Native source has local changes")
    ggml = source / "ggml"
    if output("git", "-C", str(ggml), "rev-parse", "HEAD") != GGML_REVISION:
        raise ValueError("Unexpected ggml revision")
    patch = subprocess.check_output(["git", "-C", str(ggml), "diff", "HEAD"])
    if hashlib.sha256(patch).hexdigest() != PATCH_SHA:
        raise ValueError("Unexpected ggml patches; use pinned scripts/configure.sh metal-diar")
    folder = source / "build/metal-diar/bin"
    paths = {key: (folder / name).resolve(strict=True) for key, name in COMPONENTS.items()}
    if any(not path.is_relative_to(source / "build") for path in paths.values()):
        raise ValueError("Build library escapes its build directory")
    parent = root / "nemotron-diarization-runtime"
    destination = parent / REVISION
    if destination.exists():
        raise FileExistsError(
            "Preserve the installed bundle; choose a fresh model root to repackage"
        )
    parent.mkdir(parents=True, exist_ok=True)
    names = {key: f"{PREFIX}{key}.dylib" for key in paths}
    aliases = {}
    for path in folder.glob("*.dylib"):
        for key, real in paths.items():
            if path.resolve() == real:
                aliases[path.name] = names[key]
    with tempfile.TemporaryDirectory(prefix=".diar-build-", dir=parent) as temporary:
        staging = Path(temporary) / REVISION
        staging.mkdir()
        for key, original in paths.items():
            target = staging / names[key]
            shutil.copy2(original, target)
            if "TWOLEVEL" not in output("otool", "-hv", str(target)):
                raise ValueError("Expected Mach-O two-level namespace")
            changes = ["install_name_tool", "-id", f"@loader_path/{target.name}"]
            for dependency in dependencies(original):
                name = aliases.get(Path(dependency).name)
                if name:
                    changes.extend(["-change", dependency, f"@loader_path/{name}"])
                elif not dependency.startswith(("/usr/lib/", "/System/Library/")):
                    raise ValueError(f"Unexpected non-system dependency: {dependency}")
            commands = output("otool", "-l", str(target))
            for rpath in re.findall(
                r"cmd LC_RPATH\n\s+cmdsize \d+\n\s+path (.*?) \(offset", commands
            ):
                changes.extend(["-delete_rpath", rpath])
            subprocess.run([*changes, str(target)], check=True, capture_output=True)
            subprocess.run(
                ["codesign", "--force", "--sign", "-", "--timestamp=none", str(target)],
                check=True,
                capture_output=True,
            )
            subprocess.run(["codesign", "--verify", "--strict", str(target)], check=True)
            for dependency in dependencies(target):
                if dependency.startswith("@loader_path/"):
                    if Path(dependency).name not in names.values():
                        raise ValueError("Unisolated dependency remained")
                elif not dependency.startswith(("/usr/lib/", "/System/Library/")):
                    raise ValueError("Build path remained in packaged dependency")
        for name in ["LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"]:
            shutil.copy2(source / name, staging / name)
        files = [
            {
                "name": name,
                "sha256": sha(staging / name),
                "sizeBytes": (staging / name).stat().st_size,
            }
            for name in names.values()
        ]
        (staging / "manifest.json").write_text(
            json.dumps({"sourceRevision": REVISION, "files": files}, indent=2) + "\n"
        )
        if model:
            if model.stat().st_size != 107_012_128 or sha(model) != MODEL_SHA:
                raise ValueError("Model does not match the pinned GA artifact")
            target = root / "nemotron-3-diarization/Nemotron-3-Diarization.q8_0.gguf"
            target.parent.mkdir(parents=True, exist_ok=True)
            if target.exists():
                if sha(target) != MODEL_SHA:
                    raise FileExistsError("Existing model differs; refusing to replace it")
            else:
                with target.open("xb") as handle, model.open("rb") as incoming:
                    shutil.copyfileobj(incoming, handle)
        staging.rename(destination)
    print(f"Installed isolated local runtime: {destination}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--model-root", type=Path, required=True)
    parser.add_argument("--model", type=Path, help="Optional already downloaded pinned GA GGUF")
    args = parser.parse_args()
    package(
        args.source.resolve(),
        args.model_root.resolve(),
        args.model.resolve() if args.model else None,
    )
