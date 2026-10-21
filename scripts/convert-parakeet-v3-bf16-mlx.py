#!/usr/bin/env python3
"""Store the pinned Parakeet TDT v3 MLX weights in bfloat16.

Vesper's pinned Swift loader (`ParakeetModel.fromDirectory`, compute dtype
bfloat16) casts every floating-point parameter of mlx-community's float32
checkpoint to bfloat16 right after loading, before any computation. This
script performs that same MLX cast ahead of time, so the download and the
load-time memory peak halve while inference sees bit-identical weights. The
config is copied unchanged.

Run with the isolated project:

    uv run --locked --project benchmarks/asr/python/mlx-convert \
        python scripts/convert-parakeet-v3-bf16-mlx.py --out OUTPUT_DIR

Downloads go to a separate cache directory; the output directory must not exist.
"""

import argparse
import hashlib
import importlib.metadata
import json
import shutil
import sys
from pathlib import Path

import mlx.core as mx
from huggingface_hub import hf_hub_download

SOURCE = ("mlx-community/parakeet-tdt-0.6b-v3", "ed2b7e8c15f9aaa0b5772e2efb986255eaef7e15")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 24), b""):
            digest.update(chunk)
    return digest.hexdigest()


def fetch(name: str, cache: Path) -> Path:
    return Path(
        hf_hub_download(
            SOURCE[0], name, revision=SOURCE[1], local_dir=cache / SOURCE[0] / SOURCE[1]
        )
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--cache", type=Path, default=Path(".cache/mlx-convert"))
    args = parser.parse_args()
    if args.out.exists():
        sys.exit(f"{args.out} already exists")

    config, weights_path = fetch("config.json", args.cache), fetch("model.safetensors", args.cache)
    weights = dict(mx.load(str(weights_path)))
    converted = {}
    for name, value in weights.items():
        if value.dtype != mx.float32:
            sys.exit(f"unexpected source dtype for {name}: {value.dtype}")
        # The same MLX cast the loader applies after reading the float32 file.
        converted[name] = value.astype(mx.bfloat16)

    args.out.mkdir(parents=True)
    mx.save_safetensors(str(args.out / "model.safetensors"), converted)
    shutil.copyfile(config, args.out / "config.json")
    saved = dict(mx.load(str(args.out / "model.safetensors")))
    if saved.keys() != converted.keys() or not all(
        saved[name].dtype == mx.bfloat16 and mx.array_equal(saved[name], value).item()
        for name, value in converted.items()
    ):
        sys.exit("the saved bfloat16 weights differ from the cast")
    outputs = [
        {
            "path": name,
            "sizeBytes": (args.out / name).stat().st_size,
            "sha256": sha256(args.out / name),
        }
        for name in ("config.json", "model.safetensors")
    ]
    rows = "\n".join(f"| `{o['path']}` | {o['sizeBytes']:,} | `{o['sha256']}` |" for o in outputs)
    (args.out / "README.md").write_text(f"""---
license: cc-by-4.0
library_name: mlx
pipeline_tag: automatic-speech-recognition
base_model: nvidia/parakeet-tdt-0.6b-v3
tags: [mlx, parakeet, bfloat16, vesper]
---

# Parakeet TDT 0.6B v3 for Vesper's MLX runtime (bfloat16)

[{SOURCE[0]}@{SOURCE[1][:8]}](https://huggingface.co/{SOURCE[0]}/tree/{SOURCE[1]}) with every
float32 tensor cast to bfloat16 by MLX {importlib.metadata.version("mlx")}, for
[Vesper](https://github.com/rahulsharmavishwakarma/vesper). Vesper's loader applies the
same cast to the float32 files at load time, so inference is unchanged; the
download is half the size. The configuration is copied unchanged.

Weights: CC-BY-4.0. Attribution: NVIDIA (Parakeet TDT 0.6B v3) and the mlx-community
conversion.

Source `model.safetensors`: `{sha256(weights_path)}`.

| File | Bytes | SHA-256 |
| --- | ---: | --- |
{rows}
""")
    print(json.dumps(outputs, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
