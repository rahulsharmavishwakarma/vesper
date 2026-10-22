#!/usr/bin/env python3
"""Research tool: quantize the float32 pointwise convolutions of an already-quantized ONNX model.

Some published int8 exports (for example Cohere Transcribe) quantize their MatMul weights but
leave pointwise (1x1) convolutions in float32. This rewrites those convolutions as
Transpose -> MatMul -> Transpose (shared code in `scripts/quantize-onnx-weight-only.py`) and
quantizes only the new MatMul nodes, leaving every other tensor byte for byte as published:

- `int8`: ONNX Runtime dynamic int8 (`MatMulInteger`), matching the rest of such exports.
- `wo4`: weight-only 4-bit `MatMulNBits`, block 32, symmetric, float activations.

The source folder must hold the files of one `models/artifacts.json` install folder; their
hashes are checked against the manifest first. Other files are copied unchanged.

    uv run --locked --group evaluation --project benchmarks/asr/python/onnx-quantize \
        python scripts/rewrite-onnx-pointwise.py --folder cohere-transcribe \
        --source SRC --model encoder.int8.onnx --mode wo4 --out OUT
"""

import argparse
import hashlib
import importlib.util
import json
import shutil
import sys
from pathlib import Path

import onnx

HERE = Path(__file__).resolve().parent


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 24), b""):
            digest.update(chunk)
    return digest.hexdigest()


def rewrite_module():
    spec = importlib.util.spec_from_file_location(
        "weight_only", HERE / "quantize-onnx-weight-only.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "--folder", required=True, help="Install folder name in models/artifacts.json"
    )
    parser.add_argument(
        "--source", type=Path, required=True, help="Local copy of that folder's files"
    )
    parser.add_argument(
        "--model", required=True, help="ONNX file to rewrite, relative to the folder"
    )
    parser.add_argument("--mode", required=True, choices=("int8", "wo4"))
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, default=Path("models/artifacts.json"))
    args = parser.parse_args()
    if args.out.exists():
        sys.exit(f"{args.out} already exists")

    entries = [
        a
        for a in json.loads(args.manifest.read_text())
        if a["relativePath"].split("/")[0] == args.folder
    ]
    if not entries:
        sys.exit(f"no artifacts for {args.folder}")
    sources = []
    for entry in entries:
        name = entry["relativePath"].split("/", 1)[1]
        path = args.source / name
        if not path.exists() or sha256(path) != entry["sha256"]:
            sys.exit(f"{path}: missing or not the pinned file")
        sources.append({"path": name, "sha256": entry["sha256"], "url": entry["url"]})

    model = onnx.load(str(args.source / args.model))
    metadata = [(item.key, item.value) for item in model.metadata_props]
    model, counts = rewrite_module().pointwise_conv_to_matmul(model)
    added = [
        n.name for n in model.graph.node if n.op_type == "MatMul" and n.name.endswith(".pw_matmul")
    ]
    print(f"rewrite {counts}; quantizing {len(added)} new MatMul nodes", flush=True)

    args.out.mkdir(parents=True)
    staged = args.out / "staged.onnx"
    onnx.save_model(
        model,
        str(staged),
        save_as_external_data=True,
        all_tensors_to_one_file=True,
        location="staged.onnx.data",
        size_threshold=1024,
    )
    del model
    target = args.out / args.model
    data_name = f"{Path(args.model).name}.data"
    if args.mode == "int8":
        from onnxruntime.quantization import QuantType, quantize_dynamic

        quantize_dynamic(
            str(staged),
            str(target),
            op_types_to_quantize=["MatMul"],
            nodes_to_quantize=added,
            weight_type=QuantType.QInt8,
            use_external_data_format=True,
        )
    else:
        from onnxruntime.quantization.matmul_nbits_quantizer import (
            DefaultWeightOnlyQuantConfig,
            MatMulNBitsQuantizer,
        )

        quantizer = MatMulNBitsQuantizer(
            onnx.load(str(staged)),
            nodes_to_include=added,
            algo_config=DefaultWeightOnlyQuantConfig(block_size=32, is_symmetric=True, bits=4),
        )
        quantizer.process()
        onnx.save_model(
            quantizer.model.model,
            str(target),
            save_as_external_data=True,
            all_tensors_to_one_file=True,
            location=data_name,
            size_threshold=1024,
        )
    final = onnx.load(str(target), load_external_data=False)
    del final.metadata_props[:]
    for key, value in metadata:
        entry = final.metadata_props.add()
        entry.key, entry.value = key, value
    onnx.save_model(final, str(target))
    staged.unlink()
    (args.out / "staged.onnx.data").unlink()

    for source in sources:
        name = source["path"]
        if name not in (args.model, f"{args.model}.data") and not (args.out / name).exists():
            (args.out / name).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(args.source / name, args.out / name)
    outputs = [
        {"path": str(p.relative_to(args.out)), "sizeBytes": p.stat().st_size, "sha256": sha256(p)}
        for p in sorted(args.out.rglob("*"))
        if p.is_file()
    ]
    provenance = {
        "schemaVersion": 1,
        "folder": args.folder,
        "sources": sources,
        "rewrite": {
            "model": args.model,
            "counts": counts,
            "quantizedNewMatMuls": len(added),
            "mode": args.mode,
        },
        "outputs": outputs,
    }
    (args.out / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
    print(json.dumps(outputs, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
