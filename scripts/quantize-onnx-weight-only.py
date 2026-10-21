#!/usr/bin/env python3
"""Quantize a pinned sherpa-onnx FP32 transducer export to weight-only 4-bit.

ONNX Runtime `MatMulNBits` stores MatMul weights in blocks of `--bits` bits and
keeps activations in FP32, unlike sherpa's dynamic INT8 exports, which also
quantize activations at run time. Convolutions, LSTMs and other operators stay
FP32. The encoder's sherpa metadata is preserved.

`--rewrite-pointwise-conv` first rewrites 1-D pointwise convolutions (kernel 1,
stride 1, no padding, group 1) as Transpose -> MatMul (+ Add) -> Transpose, so
`MatMulNBits` also quantizes their weights. In the Parakeet encoder those weights
are otherwise left FP32. `--self-test` checks the rewrite on a synthetic graph.

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

import numpy as np
import onnx
from huggingface_hub import hf_hub_download
from onnx import TensorProto, helper, numpy_helper
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


def pointwise_conv_to_matmul(model):
    """Rewrite pointwise Conv nodes with a constant weight as Transpose -> MatMul (+ Add) -> Transpose.

    Mutates and returns the model with counts of rewritten and skipped nodes.
    The model must have its external data loaded.
    """
    graph = model.graph
    inits = {t.name: t for t in graph.initializer}
    uses = {}
    for node in graph.node:
        for name in node.input:
            uses[name] = uses.get(name, 0) + 1
    counts = {"rewritten": 0, "skippedNotPointwise": 0, "skippedNonConstant": 0}
    nodes, new_inits, drop = [], [], set()
    for node in graph.node:
        if node.op_type != "Conv":
            nodes.append(node)
            continue
        attrs = {a.name: helper.get_attribute_value(a) for a in node.attribute}
        weight = inits.get(node.input[1])
        if weight is None:
            counts["skippedNonConstant"] += 1
            nodes.append(node)
            continue
        dims = list(weight.dims)
        pointwise = (
            len(dims) == 3
            and dims[2] == 1
            and attrs.get("group", 1) == 1
            and all(v == 1 for v in attrs.get("strides", [1]))
            and all(v == 1 for v in attrs.get("dilations", [1]))
            and all(v == 0 for v in attrs.get("pads", [0, 0]))
            and attrs.get("auto_pad", b"NOTSET") in (b"NOTSET", "NOTSET")
        )
        if not pointwise:
            counts["skippedNotPointwise"] += 1
            nodes.append(node)
            continue
        tag = node.name or node.output[0]
        matrix = numpy_helper.to_array(weight)[:, :, 0].T.copy()  # (Cin, Cout)
        weight_name = f"{tag}.pw_weight"
        new_inits.append(numpy_helper.from_array(matrix, weight_name))
        before, product, after = f"{tag}.pw_in", f"{tag}.pw_mm", f"{tag}.pw_out"
        has_bias = len(node.input) > 2 and node.input[2]
        nodes.append(
            helper.make_node(
                "Transpose", [node.input[0]], [before], perm=[0, 2, 1], name=f"{tag}.pw_t1"
            )
        )
        nodes.append(
            helper.make_node(
                "MatMul",
                [before, weight_name],
                [product if has_bias else after],
                name=f"{tag}.pw_matmul",
            )
        )
        if has_bias:
            nodes.append(
                helper.make_node("Add", [product, node.input[2]], [after], name=f"{tag}.pw_add")
            )
        nodes.append(
            helper.make_node(
                "Transpose", [after], [node.output[0]], perm=[0, 2, 1], name=f"{tag}.pw_t2"
            )
        )
        uses[node.input[1]] -= 1
        if uses[node.input[1]] == 0:
            drop.add(node.input[1])
        counts["rewritten"] += 1
    del graph.node[:]
    graph.node.extend(nodes)
    kept = [t for t in graph.initializer if t.name not in drop]
    del graph.initializer[:]
    graph.initializer.extend(kept + new_inits)
    del graph.value_info[:]  # shape hints for rewritten tensors would be stale
    return model, counts


def self_test() -> int:
    """Check the rewrite against ONNX Runtime on a synthetic graph."""
    import onnxruntime as ort

    rng = np.random.default_rng(1)
    cin, mid, cout, steps = 6, 10, 4, 7
    arrays = {
        "w1": rng.standard_normal((mid, cin, 1)).astype(np.float32),
        "b1": rng.standard_normal(mid).astype(np.float32),
        "w2": rng.standard_normal((cout, mid, 1)).astype(np.float32),
        "wd": rng.standard_normal((cout, 1, 3)).astype(np.float32),
    }
    graph = helper.make_graph(
        [
            helper.make_node("Conv", ["x", "w1", "b1"], ["c1"], name="c1", kernel_shape=[1]),
            helper.make_node("Relu", ["c1"], ["r1"], name="r1"),
            helper.make_node("Conv", ["r1", "w2"], ["c2"], name="c2", kernel_shape=[1]),
            helper.make_node(
                "Conv", ["c2", "wd"], ["y"], name="c3", kernel_shape=[3], group=cout, pads=[1, 1]
            ),
        ],
        "self_test",
        [helper.make_tensor_value_info("x", TensorProto.FLOAT, [1, cin, steps])],
        [helper.make_tensor_value_info("y", TensorProto.FLOAT, [1, cout, steps])],
        [numpy_helper.from_array(value, name) for name, value in arrays.items()],
    )
    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)])
    model.ir_version = 9
    x = rng.standard_normal((1, cin, steps)).astype(np.float32)

    def run(candidate):
        session = ort.InferenceSession(
            candidate.SerializeToString(), providers=["CPUExecutionProvider"]
        )
        return session.run(None, {"x": x})[0]

    want = run(model)
    rewritten, counts = pointwise_conv_to_matmul(
        onnx.ModelProto.FromString(model.SerializeToString())
    )
    got = run(rewritten)
    expected = {"rewritten": 2, "skippedNotPointwise": 1, "skippedNonConstant": 0}
    if counts != expected or not np.allclose(want, got, atol=1e-5):
        print(
            f"self-test failed: counts {counts}, max difference {float(np.abs(want - got).max())}"
        )
        return 1
    print(f"self-test passed: {counts}")
    return 0


def fetch(repo: str, revision: str, name: str, cache: Path) -> Path:
    # Plain files, not cache symlinks: ONNX refuses symlinked external data.
    folder = cache / repo / revision
    return Path(hf_hub_download(repo, name, revision=revision, local_dir=folder))


def model_card(args, provenance) -> str:
    changes = "MatMul weights quantized as described"
    if args.rewrite_pointwise_conv:
        changes = (
            "pointwise (1x1) convolutions rewritten as Transpose/MatMul/Transpose so their weights are "
            "quantized too, then MatMul weights quantized as described"
        )
    extra = args.card_extra.read_text().strip() + "\n" if args.card_extra else ""
    title_suffix = ", pointwise convolutions as MatMul" if args.rewrite_pointwise_conv else ""
    rows = "\n".join(
        f"| `{item['path']}` | {item['sizeBytes']:,} | `{item['sha256']}` |"
        for item in provenance["outputs"]
    )
    license_lines = f"license: {args.license}\n"
    if args.license == "other":
        license_lines += f"license_name: {args.license_name}\nlicense_link: {args.license_link}\n"
    return f"""---
{license_lines}base_model: {args.source_model}
tags: [sherpa-onnx, onnx, speech-recognition, vesper]
---

# {args.source_model} — sherpa-onnx, {args.bits}-bit weight-only{title_suffix}

Weight-only {args.bits}-bit quantization (ONNX Runtime `MatMulNBits`, block {args.block_size},
symmetric) of the official sherpa-onnx FP32 export
[`{args.repo}@{args.revision[:8]}`](https://huggingface.co/{args.repo}/tree/{args.revision}).
Activations stay FP32. Prepared for the [Vesper](https://github.com/rahulsharmavishwakarma/vesper)
dictation app; usable with any sherpa-onnx build whose ONNX Runtime provides `MatMulNBits`.

- Model: [{args.source_model}](https://huggingface.co/{args.source_model}) by NVIDIA, {args.license_name or args.license.upper()}.
- ONNX export: [k2-fsa/sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx).
- Changes: {changes}; no retraining. See `provenance.json`.

| File | Bytes | SHA-256 |
| --- | ---: | --- |
{rows}

{extra}"""


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        return self_test()
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--repo", required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--source-model", required=True)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--cache", type=Path, default=Path(".cache/onnx-quantize"))
    parser.add_argument("--bits", type=int, default=4, choices=(4, 8))
    parser.add_argument("--block-size", type=int, default=32)
    parser.add_argument(
        "--license", default="cc-by-4.0", help="Hugging Face license id, or 'other'"
    )
    parser.add_argument("--license-name", default="", help="Required with --license other")
    parser.add_argument("--license-link", default="", help="Required with --license other")
    parser.add_argument(
        "--notice", default="", help="Text for a NOTICE file the source license requires"
    )
    parser.add_argument(
        "--rewrite-pointwise-conv",
        action="store_true",
        help="Rewrite pointwise Conv as MatMul before quantizing, so its weights are quantized too",
    )
    parser.add_argument(
        "--card-extra", type=Path, help="Markdown file appended to the generated model card"
    )
    parser.add_argument(
        "--self-test", action="store_true", help="Check the rewrite on a synthetic graph and exit"
    )
    args = parser.parse_args()
    if len(args.revision) != 40:
        sys.exit("--revision must be a full commit hash")
    if args.license == "other" and not (args.license_name and args.license_link):
        sys.exit("--license other needs --license-name and --license-link")
    if args.out.exists():
        sys.exit(f"{args.out} already exists")
    args.out.mkdir(parents=True)

    sources = []
    rewrite_counts = {}
    tokens = fetch(args.repo, args.revision, "tokens.txt", args.cache)
    for name in MODELS:
        path = fetch(args.repo, args.revision, f"{name}.onnx", args.cache)
        sources.append({"path": f"{name}.onnx", "sha256": sha256(path)})
        if name == "encoder":
            weights = fetch(args.repo, args.revision, "encoder.weights", args.cache)
            sources.append({"path": "encoder.weights", "sha256": sha256(weights)})
        model = onnx.load(str(path))
        metadata = [(item.key, item.value) for item in model.metadata_props]
        if args.rewrite_pointwise_conv:
            model, rewrite_counts[name] = pointwise_conv_to_matmul(model)
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
        **(
            {"pointwiseConvRewrite": {"enabled": True, "counts": rewrite_counts}}
            if args.rewrite_pointwise_conv
            else {}
        ),
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
    if args.notice:
        (args.out / "NOTICE").write_text(
            f"{args.notice}\n\nLicense: {args.license_link or args.license}\n"
        )
    print(json.dumps(provenance["outputs"], indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
