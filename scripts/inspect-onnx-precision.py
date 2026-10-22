#!/usr/bin/env python3
"""Report where the bytes of the app's pinned ONNX speech models go.

For each ONNX file of the selected `models/artifacts.json` folders, download the pinned
revision (hash-checked), then group initializer bytes by the operator that consumes them
and by element type. Convolutions are split into pointwise, depthwise and other. ONNX
Runtime `.ort` files are skipped because they are not ONNX protobufs.

    uv run --locked --project benchmarks/asr/python/onnx-quantize \
        python scripts/inspect-onnx-precision.py --cache CACHE --out report.json \
        cohere-transcribe qwen3-asr-0.6b whisper-turbo
"""

import argparse
import hashlib
import json
import sys
from collections import defaultdict
from pathlib import Path

import onnx
from huggingface_hub import hf_hub_download
from onnx import TensorProto, helper

ELEMENT_BYTES = {
    TensorProto.FLOAT: 4,
    TensorProto.FLOAT16: 2,
    TensorProto.BFLOAT16: 2,
    TensorProto.DOUBLE: 8,
    TensorProto.INT8: 1,
    TensorProto.UINT8: 1,
    TensorProto.INT16: 2,
    TensorProto.INT32: 4,
    TensorProto.INT64: 8,
    TensorProto.BOOL: 1,
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 24), b""):
            digest.update(chunk)
    return digest.hexdigest()


def tensor_bytes(tensor) -> int:
    if tensor.data_location == TensorProto.EXTERNAL:
        info = {entry.key: entry.value for entry in tensor.external_data}
        if "length" in info:
            return int(info["length"])
    if tensor.raw_data:
        return len(tensor.raw_data)
    count = 1
    for dim in tensor.dims:
        count *= dim
    return count * ELEMENT_BYTES.get(tensor.data_type, 4)


def label(node, inits) -> str:
    if (
        node.op_type not in ("Conv", "ConvInteger")
        or len(node.input) < 2
        or node.input[1] not in inits
    ):
        return node.op_type
    dims = list(inits[node.input[1]].dims)
    group = next((helper.get_attribute_value(a) for a in node.attribute if a.name == "group"), 1)
    if all(d == 1 for d in dims[2:]) and group == 1:
        return f"{node.op_type}:pointwise"
    if group > 1 and group == dims[0]:
        return f"{node.op_type}:depthwise"
    return f"{node.op_type}:other"


def walk(graph, inits, owner):
    for node in graph.node:
        name = label(node, inits)
        for value in node.input:
            if value in inits and value not in owner:
                owner[value] = name
        for attribute in node.attribute:
            if attribute.type == onnx.AttributeProto.GRAPH:
                walk(attribute.g, inits, owner)
            elif attribute.type == onnx.AttributeProto.GRAPHS:
                for sub in attribute.graphs:
                    walk(sub, inits, owner)


def breakdown(path: Path) -> dict:
    model = onnx.load(str(path), load_external_data=False)
    inits = {t.name: t for t in model.graph.initializer}
    owner = {}
    walk(model.graph, inits, owner)
    rows = defaultdict(int)
    for name, tensor in inits.items():
        dtype = TensorProto.DataType.Name(tensor.data_type)
        rows[(owner.get(name, "(unconsumed)"), dtype)] += tensor_bytes(tensor)
    total = sum(rows.values())
    ops = defaultdict(int)
    for node in model.graph.node:
        ops[node.op_type] += 1
    return {
        "initializer_bytes": total,
        "rows": [
            {"consumer": c, "dtype": d, "bytes": b, "share": round(b / total, 4) if total else 0}
            for (c, d), b in sorted(rows.items(), key=lambda kv: -kv[1])
        ],
        "node_counts": dict(sorted(ops.items(), key=lambda kv: -kv[1])),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "folders", nargs="+", help="Install folder names from models/artifacts.json"
    )
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, default=Path("models/artifacts.json"))
    args = parser.parse_args()
    artifacts = json.loads(args.manifest.read_text())
    report = {}
    for folder in args.folders:
        entries = [a for a in artifacts if a["relativePath"].split("/")[0] == folder]
        if not entries:
            sys.exit(f"no artifacts for {folder}")
        local = {}
        for entry in entries:
            repo = entry["url"].split("huggingface.co/", 1)[1].split("/resolve/", 1)[0]
            filename = entry["url"].split(f"/resolve/{entry['revision']}/", 1)[1]
            path = Path(
                hf_hub_download(
                    repo, filename, revision=entry["revision"], local_dir=args.cache / folder
                )
            )
            if sha256(path) != entry["sha256"]:
                sys.exit(f"{folder}/{filename}: hash does not match the manifest")
            local[filename] = (path, entry, repo)
        report[folder] = {}
        for filename, (path, entry, repo) in local.items():
            if not filename.endswith(".onnx"):
                continue
            result = breakdown(path)
            result.update(
                {
                    "repository": repo,
                    "revision": entry["revision"],
                    "file_bytes": entry["sizeBytes"],
                }
            )
            report[folder][filename] = result
            print(
                f"\n{folder}/{filename}: {result['initializer_bytes'] / 1e6:,.0f} MB of initializers"
            )
            for row in result["rows"][:8]:
                print(
                    f"  {row['consumer']:26} {row['dtype']:9} {row['bytes'] / 1e6:9.1f} MB {row['share'] * 100:5.1f}%"
                )
    args.out.write_text(json.dumps(report, indent=2) + "\n")
    print("\nwrote", args.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
