"""Restricted MLX LoRALinear -> PEFT export for the pinned cleanup checkpoint.

No base weights are fused or rewritten. Conversion is not model qualification.
"""

import argparse
import hashlib
import json
from pathlib import Path

import numpy as np
from safetensors.numpy import load_file, save_file

BASE_REPO = "Qwen/Qwen3.5-0.8B"
BASE_REVISION = "2fc06364715b967f1860aea9cf38778875588b17"
LLAMA_REVISION = "e79e4bf660e19f2ad851e06c6913f7a8c5852621"
MLX_REVISION = "ed1fca4cef15a824c5f1702c80f70b4cffc8e4dd"
MLX_PREFIX = "language_model.model.layers.23.mlp.down_proj"
PEFT_PREFIX = "base_model.model.model.language_model.layers.23.mlp.down_proj"


def peft_tensors(tensors: dict[str, np.ndarray], rank: int, scale: float):
    if isinstance(rank, bool) or not isinstance(rank, int) or not 1 <= rank <= 64:
        raise ValueError("Rank must be an integer from 1 to 64")
    if isinstance(scale, bool) or not np.isfinite(scale) or not 0 < scale <= 64:
        raise ValueError("Invalid MLX scale")
    names = {f"{MLX_PREFIX}.lora_a", f"{MLX_PREFIX}.lora_b"}
    if set(tensors) != names:
        raise ValueError("This proof supports only paired final-layer down-projection LoRA tensors")
    a, b = (tensors[f"{MLX_PREFIX}.{name}"] for name in ["lora_a", "lora_b"])
    if a.shape != (3584, rank) or b.shape != (rank, 1024):
        raise ValueError("Adapter shape does not match the pinned Qwen3.5-0.8B checkpoint")
    if any(t.dtype not in (np.float16, np.float32) or not np.isfinite(t).all() for t in [a, b]):
        raise ValueError("Adapter must contain finite float16/float32 tensors")
    return {
        f"{PEFT_PREFIX}.lora_A.weight": np.ascontiguousarray(a.T, dtype=np.float32),
        f"{PEFT_PREFIX}.lora_B.weight": np.ascontiguousarray(b.T, dtype=np.float32),
    }, {
        "base_model_name_or_path": BASE_REPO,
        "revision": BASE_REVISION,
        "peft_type": "LORA",
        "task_type": "CAUSAL_LM",
        "inference_mode": True,
        "r": rank,
        "lora_alpha": rank * scale,
        "lora_dropout": 0.0,
        "bias": "none",
        "target_modules": ["mlp.down_proj"],
        "layers_to_transform": [23],
        "layers_pattern": "layers",
        "fan_in_fan_out": False,
        "use_rslora": False,
    }


def export(source: Path, destination: Path):
    weights = source / "adapters.safetensors"
    config_path = source / "adapter_config.json"
    if weights.stat().st_size > 2 * 1024 * 1024 or config_path.stat().st_size > 64 * 1024:
        raise ValueError("Adapter input exceeds the restricted export budget")
    config = json.loads(config_path.read_text())
    params = config.get("lora_parameters", {})
    if (
        config.get("fine_tune_type") != "lora"
        or config.get("num_layers") != 1
        or params.get("keys") != ["mlp.down_proj"]
        or params.get("dropout") != 0
    ):
        raise ValueError("Only the declared one-layer LoRALinear proof configuration is accepted")
    tensors, peft = peft_tensors(load_file(weights), params["rank"], params["scale"])
    destination.mkdir(parents=True, exist_ok=False)
    save_file(tensors, destination / "adapter_model.safetensors")
    (destination / "adapter_config.json").write_text(json.dumps(peft, indent=2) + "\n")
    manifest = {
        "schemaVersion": 1,
        "qualified": False,
        "baseRepository": BASE_REPO,
        "baseRevision": BASE_REVISION,
        "mlxLmRevision": MLX_REVISION,
        "llamaRevision": LLAMA_REVISION,
        "sourceSha256": hashlib.sha256(weights.read_bytes()).hexdigest(),
        "peftSha256": hashlib.sha256(
            (destination / "adapter_model.safetensors").read_bytes()
        ).hexdigest(),
        "targets": [MLX_PREFIX],
        "nativeScale": 1.0,
    }
    (destination / "conversion.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    print(json.dumps(export(args.source, args.destination), indent=2))
