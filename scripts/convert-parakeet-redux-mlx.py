#!/usr/bin/env python3
"""Convert pinned moondream/parakeet-redux to Vesper's MLX Parakeet layout.

Redux is a ternary student of NVIDIA Parakeet TDT 0.6B v3 with the same graph,
tensor roles, vocabulary and TDT durations. Its 264 encoder projections store
five base-3 digits per byte plus one fp16 scale per row and 128 inputs:
w = scale * (code - 1). mlx-audio 0.6.1 repacks those codes losslessly into
MLX affine 2-bit words (bias = -scale); this script reuses that repacking and
writes:

- model.safetensors: dense tensors in their source dtypes, plus `weight`
  (uint32 codes), `scales` and `biases` for each ternary module;
- config.json: the pinned mlx-community v3 Parakeet config, which the pinned
  Swift loader parses, with dither disabled as in Redux and the ternary module
  list under `vesper_ternary_modules`;
- README.md with attribution and output hashes.

No weights are retrained or requantized. Run with the isolated project:

    uv run --locked --project benchmarks/asr/python/mlx-convert \
        python scripts/convert-parakeet-redux-mlx.py --out OUTPUT_DIR

Downloads go to a separate cache directory; the output directory must not exist.
"""

import argparse
import hashlib
import importlib.metadata
import json
import sys
from pathlib import Path

import mlx.core as mx
from huggingface_hub import hf_hub_download
from mlx.utils import tree_flatten
from mlx_audio.stt.models.parakeet.redux import ParakeetRedux, map_weight_name

REDUX = ("moondream/parakeet-redux", "2bf128600aac4b16946f7ed8372e56117fe5e23b")
V3_MLX = ("mlx-community/parakeet-tdt-0.6b-v3", "ed2b7e8c15f9aaa0b5772e2efb986255eaef7e15")
REDUX_FILES = ("config.json", "model.safetensors", "ternary.json", "tokenizer.json")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 24), b""):
            digest.update(chunk)
    return digest.hexdigest()


def fetch(repo: str, revision: str, name: str, cache: Path) -> Path:
    return Path(hf_hub_download(repo, name, revision=revision, local_dir=cache / repo / revision))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--cache", type=Path, default=Path(".cache/mlx-convert"))
    args = parser.parse_args()
    if args.out.exists():
        sys.exit(f"{args.out} already exists")

    source = {name: fetch(*REDUX, name, args.cache) for name in REDUX_FILES}
    v3_config_path = fetch(*V3_MLX, "config.json", args.cache)
    redux_config = json.loads(source["config.json"].read_text())
    redux_config["model_path"] = str(source["config.json"].parent)

    # mlx-audio validates the architecture, ternary manifest and every tensor,
    # then repacks the base-3 codes into MLX 2-bit words without requantizing.
    model = ParakeetRedux(redux_config)
    raw = dict(mx.load(str(source["model.safetensors"])))
    converted = model.sanitize(raw)
    model.load_weights(list(converted.items()), strict=True)
    weights = dict(tree_flatten(model.parameters()))
    if weights.keys() != converted.keys():
        sys.exit("converted parameters do not match the model")

    v3_config = json.loads(v3_config_path.read_text())
    if (
        model.durations != v3_config["decoding"]["durations"]
        or list(model.vocabulary) != v3_config["joint"]["vocabulary"]
        or model.max_symbols != v3_config["decoding"]["greedy"]["max_symbols"]
        or model.preprocessor_config.features != v3_config["preprocessor"]["features"]
    ):
        sys.exit("Redux and the v3 MLX config disagree on vocabulary, durations or features")
    modules = []
    for name in redux_config["ternary_modules"]:
        target = map_weight_name(name)
        codes, scales, biases = (
            weights[f"{target}.{part}"] for part in ("weight", "scales", "biases")
        )
        if codes.dtype != mx.uint32 or not mx.array_equal(biases, -scales).item():
            sys.exit(f"unexpected ternary layout for {target}")
        modules.append({"name": target, "shape": [codes.shape[0], scales.shape[1] * 128]})
    config = dict(v3_config)
    config["preprocessor"] = {**v3_config["preprocessor"], "dither": 0.0}
    config["vesper_ternary_modules"] = modules
    config["vesper_conversion"] = {
        "source": {
            "repository": REDUX[0],
            "revision": REDUX[1],
            "files": {name: sha256(path) for name, path in source.items()},
        },
        "config": {
            "repository": V3_MLX[0],
            "revision": V3_MLX[1],
            "sha256": sha256(v3_config_path),
        },
        "tool": {
            "mlx-audio": importlib.metadata.version("mlx-audio"),
            "mlx": importlib.metadata.version("mlx"),
        },
        "groupSize": 128,
        "auxiliaryVadHead": "dropped",
    }

    args.out.mkdir(parents=True)
    mx.save_safetensors(str(args.out / "model.safetensors"), weights)
    (args.out / "config.json").write_text(json.dumps(config, indent=2) + "\n")
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
base_model: {REDUX[0]}
tags: [mlx, parakeet, ternary, vesper]
---

# Parakeet Redux for Vesper's MLX runtime

A lossless repacking of [{REDUX[0]}@{REDUX[1][:8]}](https://huggingface.co/{REDUX[0]}/tree/{REDUX[1]}),
Moondream's ternary student of
[NVIDIA Parakeet TDT 0.6B v3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3), for
[Vesper](https://github.com/rahulsharmavishwakarma/vesper).

The ternary codes and fp16 scales are kept exactly, as MLX affine 2-bit words
with bias = -scale, through mlx-audio {importlib.metadata.version("mlx-audio")}'s repacking.
No weights were retrained or requantized. The configuration follows
[{V3_MLX[0]}]({"https://huggingface.co/" + V3_MLX[0]}) so Vesper's pinned Parakeet
loader can read it. Redux's auxiliary VAD head is not included.

Weights: CC-BY-4.0. Attribution: Moondream (parakeet-redux) and NVIDIA (Parakeet TDT 0.6B v3).

| File | Bytes | SHA-256 |
| --- | ---: | --- |
{rows}
""")
    print(json.dumps(outputs, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
