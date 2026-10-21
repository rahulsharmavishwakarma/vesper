#!/usr/bin/env python3
"""Quantize a pinned sherpa-onnx FP32 transducer export to weight-only 4-bit.

ONNX Runtime `MatMulNBits` stores MatMul weights in blocks of `--bits` bits and
keeps activations in FP32, unlike sherpa's dynamic INT8 exports, which also
quantize activations at run time. Convolutions, LSTMs and other operators stay
FP32. The encoder's sherpa metadata is preserved.

Run with the isolated project:

    uv run --locked --project benchmarks/asr/python/onnx-quantize \
        python scripts/quantize-onnx-weight-only.py \
        --repo csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3 \
        --revision 1a468a35cbba69418f126de829e75261dea4a4e4 \
        --source-model nvidia/parakeet-tdt-0.6b-v3 --out OUTPUT_DIR

Downloads go to a separate cache directory; the output directory must not exist.
"""

import argparse
import hashlib
import importlib.metadata
import json
import shutil
import sys
from pathlib import Path

import onnx
from huggingface_hub import hf_hub_download
from onnxruntime.quantization.matmul_nbits_quantizer import (
    DefaultWeightOnlyQuantConfig,
    MatMulNBitsQuantizer,
)

MODELS = ("encoder", "decoder", "joiner")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 24), b""):
            digest.update(chunk)
    return digest.hexdigest()


def fetch(repo: str, revision: str, name: str, cache: Path) -> Path:
    # Plain files, not cache symlinks: ONNX refuses symlinked external data.
    folder = cache / repo / revision
    return Path(hf_hub_download(repo, name, revision=revision, local_dir=folder))


def model_card(args, provenance) -> str:
    rows = "\n".join(
        f"| `{item['path']}` | {item['sizeBytes']:,} | `{item['sha256']}` |"
        for item in provenance["outputs"]
    )
    return f"""---
license: cc-by-4.0
base_model: {args.source_model}
tags: [sherpa-onnx, onnx, speech-recognition, vesper]
---

# {args.source_model} — sherpa-onnx, {args.bits}-bit weight-only

Weight-only {args.bits}-bit quantization (ONNX Runtime `MatMulNBits`, block {args.block_size},
symmetric) of the official sherpa-onnx FP32 export
[`{args.repo}@{args.revision[:8]}`](https://huggingface.co/{args.repo}/tree/{args.revision}).
Activations stay FP32. Prepared for the [Vesper](https://github.com/rahulsharmavishwakarma/vesper)
dictation app; usable with any sherpa-onnx build whose ONNX Runtime provides `MatMulNBits`.

- Model: [{args.source_model}](https://huggingface.co/{args.source_model}) by NVIDIA, CC-BY-4.0.
- ONNX export: [k2-fsa/sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx).
- Changes: MatMul weights quantized as described; no retraining. See `provenance.json`.

| File | Bytes | SHA-256 |
| --- | ---: | --- |
{rows}
"""


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--repo", required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--source-model", required=True)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--cache", type=Path, default=Path(".cache/onnx-quantize"))
    parser.add_argument("--bits", type=int, default=4, choices=(4, 8))
    parser.add_argument("--block-size", type=int, default=32)
    args = parser.parse_args()
    if len(args.revision) != 40:
        sys.exit("--revision must be a full commit hash")
    if args.out.exists():
        sys.exit(f"{args.out} already exists")
    args.out.mkdir(parents=True)

    sources = []
    tokens = fetch(args.repo, args.revision, "tokens.txt", args.cache)
    for name in MODELS:
        path = fetch(args.repo, args.revision, f"{name}.onnx", args.cache)
        sources.append({"path": f"{name}.onnx", "sha256": sha256(path)})
        if name == "encoder":
            weights = fetch(args.repo, args.revision, "encoder.weights", args.cache)
            sources.append({"path": "encoder.weights", "sha256": sha256(weights)})
        model = onnx.load(str(path))
        metadata = [(item.key, item.value) for item in model.metadata_props]
        quantizer = MatMulNBitsQuantizer(
            model,
            algo_config=DefaultWeightOnlyQuantConfig(
                block_size=args.block_size, is_symmetric=True, bits=args.bits
            ),
        )
        quantizer.process()
        quantized = quantizer.model.model
        del quantized.metadata_props[:]
        for key, value in metadata:
            entry = quantized.metadata_props.add()
            entry.key, entry.value = key, value
        onnx.save_model(quantized, str(args.out / f"{name}.onnx"))
    sources.append({"path": "tokens.txt", "sha256": sha256(tokens)})
    shutil.copy(tokens, args.out / "tokens.txt")

    provenance = {
        "schemaVersion": 1,
        "source": {"repository": args.repo, "revision": args.revision, "files": sources},
        "model": args.source_model,
        "quantization": {
            "method": "onnxruntime MatMulNBits weight-only",
            "bits": args.bits,
            "blockSize": args.block_size,
            "symmetric": True,
            "onnxruntime": importlib.metadata.version("onnxruntime"),
            "onnx": importlib.metadata.version("onnx"),
        },
        "outputs": [
            {
                "path": name,
                "sizeBytes": (args.out / name).stat().st_size,
                "sha256": sha256(args.out / name),
            }
            for name in (*(f"{m}.onnx" for m in MODELS), "tokens.txt")
        ],
    }
    (args.out / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
    (args.out / "README.md").write_text(model_card(args, provenance))
    print(json.dumps(provenance["outputs"], indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
