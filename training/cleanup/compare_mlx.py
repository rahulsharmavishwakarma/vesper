"""Compare adapter deltas against adapter_probe with the identical float32 base.

This is a numerical compatibility gate, not a writing-quality benchmark.
"""

import argparse
import hashlib
import json
from importlib.metadata import version
from pathlib import Path

import mlx.core as mx
import numpy as np
from mlx_lm.models.qwen3_5 import Model, ModelArgs
from mlx_lm.tuner.utils import linear_to_lora_layers
from safetensors.numpy import load_file

from adapter_bridge import LLAMA_REVISION, peft_tensors

CONFIG_SHA256 = "b90b86f35c8e6925ef74ee04d0e758f0a845c83a42089ad82bbaa948de9b4204"
WEIGHTS_SHA256 = "04b1c301231dd422b8860db31311ab2721511346a32cb1e079c4c4e5f1fe4696"
# Fixed before the first cross-runtime run. Float32 kernel differences are
# allowed; a wrong alpha/rank, transpose, ignored adapter or permutation is not.
LIMITS = {"baseMaxAbs": 0.02, "baseRms": 0.002, "deltaRelativeRms": 0.1, "deltaCosineMin": 0.99}


def file_hash(path: Path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def compare(source: Path, adapter: Path, native: Path):
    config_bytes = (source / "config.json").read_bytes()
    if hashlib.sha256(config_bytes).hexdigest() != CONFIG_SHA256:
        raise ValueError("The source config does not match the pinned checkpoint")
    weight_paths = sorted(source.glob("*.safetensors"))
    if len(weight_paths) != 1 or file_hash(weight_paths[0]) != WEIGHTS_SHA256:
        raise ValueError("The source weights do not match the pinned checkpoint")
    adapter_path = adapter / "adapters.safetensors"
    peft_tensors(load_file(adapter_path), 8, 2.5)
    report_path = native / "report.json"
    report = json.loads(report_path.read_text())
    if (
        report.get("nativeRevision") != LLAMA_REVISION
        or report.get("passed") is not True
        or len(report.get("prompts", [])) != 3
    ):
        raise ValueError("Expected the complete pinned native control report")
    model = Model(ModelArgs.from_dict(json.loads(config_bytes)))
    weights = {}
    for path in weight_paths:
        # The native F32 converter casts before norm-shifting. Match that exact
        # representation instead of comparing sanitized BF16 with native F32.
        weights.update({key: value.astype(mx.float32) for key, value in mx.load(path).items()})
    model.load_weights(list(model.sanitize(weights).items()), strict=True)
    model.eval()
    mx.eval(model.parameters())
    baselines = []
    for row in report["prompts"]:
        values = model(mx.array([row["tokenIds"]]))[0, -1].astype(mx.float32)
        mx.eval(values)
        baselines.append(np.array(values))
    model.freeze()
    linear_to_lora_layers(
        model, 1, {"rank": 8, "dropout": 0.0, "scale": 2.5, "keys": ["mlp.down_proj"]}
    )
    model.load_weights(str(adapter / "adapters.safetensors"), strict=False)
    model.eval()
    rows = []
    for index, row in enumerate(report["prompts"]):
        values = model(mx.array([row["tokenIds"]]))[0, -1].astype(mx.float32)
        mx.eval(values)
        changed = np.array(values)
        off = np.fromfile(native / f"{index}-off.f32", dtype="<f4")
        on = np.fromfile(native / f"{index}-on.f32", dtype="<f4")
        if off.shape != changed.shape or on.shape != changed.shape:
            raise ValueError("Native/MLX vocabulary shapes differ")
        baseline = baselines[index]
        if any(not np.isfinite(values).all() for values in (off, on, baseline, changed)):
            raise ValueError("Nonfinite native/MLX logits")
        native_delta = (on - off).astype(np.float64)
        mlx_delta = (changed - baseline).astype(np.float64)
        denominator = float(np.linalg.norm(native_delta) * np.linalg.norm(mlx_delta))
        if denominator <= 1e-12:
            raise ValueError("Nonzero adapter has no measurable effect")
        metrics = {
            "promptIndex": index,
            "baseMaxAbs": float(np.max(np.abs(baseline - off))),
            "baseRms": float(np.sqrt(np.mean((baseline.astype(np.float64) - off) ** 2))),
            "deltaRelativeRms": float(
                np.linalg.norm(mlx_delta - native_delta) / np.linalg.norm(native_delta)
            ),
            "deltaCosine": float(np.dot(native_delta, mlx_delta) / denominator),
            "nativeEffectMaxAbs": float(np.max(np.abs(native_delta))),
            "mlxEffectMaxAbs": float(np.max(np.abs(mlx_delta))),
        }
        metrics["passed"] = (
            metrics["baseMaxAbs"] <= LIMITS["baseMaxAbs"]
            and metrics["baseRms"] <= LIMITS["baseRms"]
            and metrics["deltaRelativeRms"] <= LIMITS["deltaRelativeRms"]
            and metrics["deltaCosine"] >= LIMITS["deltaCosineMin"]
        )
        rows.append(metrics)
    return {
        "schemaVersion": 1,
        "sourceWeightsSha256": WEIGHTS_SHA256,
        "adapterSha256": file_hash(adapter_path),
        "nativeReportSha256": file_hash(report_path),
        "comparisonSourceSha256": file_hash(Path(__file__)),
        "packages": {name: version(name) for name in ("mlx", "mlx-lm", "numpy")},
        "nativeArtifacts": report.get("artifacts"),
        "limits": LIMITS,
        "baseRepresentation": "source cast to float32 before native/MLX sanitation",
        "prompts": rows,
        "passed": all(row["passed"] for row in rows),
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--adapter", required=True, type=Path)
    parser.add_argument("--native", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    result = compare(args.source, args.adapter, args.native)
    with args.output.open("x") as output:
        json.dump(result, output, indent=2)
        output.write("\n")
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if result["passed"] else 1)
