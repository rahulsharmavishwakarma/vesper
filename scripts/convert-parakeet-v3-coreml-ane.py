#!/usr/bin/env python3
"""Build Parakeet TDT 0.6B v3 files for Vesper's Neural Engine speech engine.

Vesper runs Parakeet's FastConformer encoder through Core ML on the Neural Engine
and keeps the TDT decoder and joint network in its MLX runtime. This script:

1. builds the encoder from NVIDIA's pinned Transformers-format checkpoint, traces
   it over one fixed 15 s window, converts it to a float16 ML program, and checks
   the Core ML result on the CPU and Neural Engine against the float32 PyTorch
   encoder at full and padded lengths on a synthetic voiced signal;
2. copies the decoder, joint network and subsampling tensors bit for bit from
   Vesper's pinned bfloat16 MLX checkpoint into a small `model.safetensors`, with
   `config.json` set to zero encoder layers, so MLX never loads the encoder that
   Core ML replaces.

Inputs and outputs keep NeMo's channel-first layout:

- `mel` float32 [1, 128, 1501]: Vesper's normalized log-mel frames, zero-padded.
- `mel_length` int32 [1]: valid frames (1...1501).
- `encoder` float16 [1, 1024, 188] and `encoder_length` int32 [1].

Two details keep the export equal to NeMo's encoder. Frames past `mel_length`
are zeroed before subsampling, as NeMo's masked convolution stack does. The
attention mask is clamped to -10000, NeMo's masking value, because float32's
minimum becomes -inf in float16 and turns fully padded rows into NaN.

Run with the isolated project on Apple-silicon macOS:

    uv run --locked --project benchmarks/asr/python/coreml-convert \
        python scripts/convert-parakeet-v3-coreml-ane.py --out OUTPUT_DIR

Downloads go to a separate cache directory; the output directory must not exist.
"""

import argparse
import hashlib
import importlib.metadata
import json
import sys
import tempfile
from pathlib import Path

import coremltools as ct
import numpy as np
import torch
from huggingface_hub import hf_hub_download
from safetensors import safe_open
from safetensors.torch import save_file
from transformers import AttentionInterface, ParakeetForTDT
from transformers.models.parakeet.modeling_parakeet import eager_attention_forward

SOURCE = ("nvidia/parakeet-tdt-0.6b-v3", "541d1f99c6b0c3cd0b11a95167540bb8edefd82b")
# Vesper's bfloat16 MLX checkpoint (models/mlx-artifacts.json).
MLX_SOURCE = (
    "rahulsharmavishwakarma/vesper-parakeet-tdt-0.6b-v3-mlx-bf16",
    "5668bb2ddf29647ad38cecda5f09371a306381e5",
)
# Everything the MLX graph needs once the encoder layers are gone.
MLX_KEPT_PREFIXES = ("decoder.", "joint.", "encoder.pre_encode.")
MEL_BINS, FRAMES = 128, 1501
# Compiled, as apps bundle Core ML models: no on-device compile step, and a stable
# path keeps the Neural Engine's per-path specialization cache valid.
COMPILED = "ParakeetEncoder15s.mlmodelc"
# NeMo's attention masking value; float16 keeps it finite.
MASK_VALUE = -10000.0
CHECK_LENGTHS = (1501, 1200, 901, 457, 120)
# Catches broken exports (layout, masking, NaN), not accuracy: float16 drifts more on
# this synthetic signal than on speech, so speech is qualified by held-out WER instead.
MIN_COSINE = 0.98


def fp16_safe_attention(module, query, key, value, attention_mask, **kwargs):
    if attention_mask is not None:
        attention_mask = attention_mask.clamp(min=MASK_VALUE)
    return eager_attention_forward(module, query, key, value, attention_mask, **kwargs)


class Encoder(torch.nn.Module):
    """NeMo-layout wrapper with NeMo's input masking."""

    def __init__(self, encoder: torch.nn.Module):
        super().__init__()
        self.encoder = encoder

    def forward(self, mel: torch.Tensor, mel_length: torch.Tensor):
        valid = torch.arange(mel.shape[-1], device=mel.device)[None] < mel_length[:, None]
        features = mel.transpose(1, 2) * valid[..., None].to(mel.dtype)
        output = self.encoder(input_features=features, attention_mask=valid.to(torch.int32))
        return (
            output.last_hidden_state.transpose(1, 2),
            output.attention_mask.sum(-1).to(torch.int32),
        )


def synthetic_speech(seconds: int = 15, rate: int = 16000) -> np.ndarray:
    """A deterministic voiced signal: a gliding pitch through two moving formants."""
    t = np.arange(seconds * rate) / rate
    pitch = 140 + 40 * np.sin(2 * np.pi * 0.7 * t)
    phase = 2 * np.pi * np.cumsum(pitch) / rate
    formants = (700 + 300 * np.sin(2 * np.pi * 1.3 * t), 1200 + 500 * np.sin(2 * np.pi * 0.9 * t))
    voice = sum(
        np.sin(k * phase) / k * sum(np.exp(-(((k * pitch - f) / 150) ** 2)) for f in formants)
        for k in range(1, 30)
    )
    envelope = 0.5 + 0.5 * np.sin(2 * np.pi * 3 * t) ** 2
    return 0.3 * voice * envelope / np.abs(voice).max()


def normalized_log_mel(audio: np.ndarray, rate: int = 16000, n_fft: int = 512) -> np.ndarray:
    """NeMo-style features: pre-emphasis, 25 ms Hann STFT every 10 ms, 128 mel bins,
    log, then per-bin normalization. Close enough for a conversion check."""
    window, hop = 400, 160
    audio = np.append(audio[0], audio[1:] - 0.97 * audio[:-1])
    padded = np.pad(audio, n_fft // 2)
    frames = 1 + (len(padded) - n_fft) // hop
    taper = np.pad(np.hanning(window + 1)[:-1], (n_fft - window) // 2)
    index = np.arange(n_fft)[None] + hop * np.arange(frames)[:, None]
    power = np.abs(np.fft.rfft(padded[index] * taper, axis=1)) ** 2
    top = 2595 * np.log10(1 + rate / 2 / 700)
    edges = 700 * (10 ** (np.linspace(0, top, MEL_BINS + 2) / 2595) - 1)
    hz = np.linspace(0, rate / 2, n_fft // 2 + 1)
    rising = (hz[None] - edges[:-2, None]) / (edges[1:-1, None] - edges[:-2, None])
    falling = (edges[2:, None] - hz[None]) / (edges[2:, None] - edges[1:-1, None])
    log_mel = np.log(power @ np.maximum(0, np.minimum(rising, falling)).T + 2**-24).T
    log_mel = (log_mel - log_mel.mean(1, keepdims=True)) / (log_mel.std(1, keepdims=True) + 1e-5)
    return log_mel[None, :, :FRAMES].astype(np.float32)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 24), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_decoder(out: Path, cache: Path) -> str:
    """Copy the non-encoder-layer MLX tensors unchanged; return the source hash."""

    def fetch(name: str) -> Path:
        directory = cache / MLX_SOURCE[0] / MLX_SOURCE[1]
        return Path(
            hf_hub_download(MLX_SOURCE[0], name, revision=MLX_SOURCE[1], local_dir=directory)
        )

    weights_path = fetch("model.safetensors")
    with safe_open(weights_path, "pt") as source:
        kept = {
            name: source.get_tensor(name)
            for name in source.keys()
            if name.startswith(MLX_KEPT_PREFIXES)
        }
        dropped = [name for name in source.keys() if name not in kept]
    if any(not name.startswith("encoder.layers.") for name in dropped):
        sys.exit("unexpected tensors outside the encoder layers")
    save_file(kept, out / "model.safetensors")
    with safe_open(out / "model.safetensors", "pt") as saved:
        if set(saved.keys()) != set(kept) or not all(
            saved.get_tensor(name).dtype == value.dtype
            and torch.equal(saved.get_tensor(name), value)
            for name, value in kept.items()
        ):
            sys.exit("the saved decoder tensors differ from the source")

    config = json.loads(fetch("config.json").read_text())
    if config["encoder"]["n_layers"] != 24:
        sys.exit("unexpected encoder depth in the MLX config")
    config["encoder"]["n_layers"] = 0
    (out / "config.json").write_text(json.dumps(config, indent=2) + "\n")
    return sha256(weights_path)


def neural_engine_share(compiled: Path) -> float:
    """Fraction of estimated cost Core ML schedules on the Neural Engine."""
    plan = ct.models.compute_plan.MLComputePlan.load_from_path(
        str(compiled), compute_units=ct.ComputeUnit.CPU_AND_NE
    )
    total = ane = 0.0
    for operation in plan.model_structure.program.functions["main"].block.operations:
        cost = plan.get_estimated_cost_for_mlprogram_operation(operation)
        usage = plan.get_compute_device_usage_for_mlprogram_operation(operation)
        if cost is None or usage is None:
            continue
        total += cost.weight
        if type(usage.preferred_compute_device).__name__ == "MLNeuralEngineComputeDevice":
            ane += cost.weight
    return ane / total if total else 0.0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--cache", type=Path, default=Path(".cache/coreml-convert"))
    args = parser.parse_args()
    if args.out.exists():
        sys.exit(f"{args.out} already exists")

    AttentionInterface.register("vesper_fp16_safe", fp16_safe_attention)
    model = ParakeetForTDT.from_pretrained(
        SOURCE[0],
        revision=SOURCE[1],
        cache_dir=args.cache,
        attn_implementation="vesper_fp16_safe",
        dtype=torch.float32,
    ).eval()
    encoder = Encoder(model.model.encoder if hasattr(model, "model") else model.encoder).eval()

    mel = torch.from_numpy(normalized_log_mel(synthetic_speech()))
    with torch.no_grad():
        traced = torch.jit.trace(encoder, (mel, torch.tensor([FRAMES], dtype=torch.int32)))
    program = ct.convert(
        traced,
        convert_to="mlprogram",
        inputs=[
            ct.TensorType(name="mel", shape=(1, MEL_BINS, FRAMES), dtype=np.float32),
            ct.TensorType(name="mel_length", shape=(1,), dtype=np.int32),
        ],
        outputs=[
            ct.TensorType(name="encoder", dtype=np.float16),
            ct.TensorType(name="encoder_length", dtype=np.int32),
        ],
        minimum_deployment_target=ct.target.macOS14,
        compute_precision=ct.precision.FLOAT16,
    )
    program.author = "Vesper conversion of NVIDIA Parakeet TDT 0.6B v3"
    program.license = "CC-BY-4.0"
    program.short_description = (
        "Parakeet TDT 0.6B v3 FastConformer encoder, float16, one 15 s window. Modified: "
        "exported to Core ML from the Transformers-format checkpoint; decoder not included."
    )
    args.out.mkdir(parents=True)
    compiled = args.out / COMPILED
    with tempfile.TemporaryDirectory() as scratch:
        package = Path(scratch) / "encoder.mlpackage"
        program.save(str(package))
        ct.utils.compile_model(str(package), destination_path=str(compiled))

    loaded = ct.models.CompiledMLModel(str(compiled), compute_units=ct.ComputeUnit.CPU_AND_NE)
    checks = []
    for length in CHECK_LENGTHS:
        lengths = torch.tensor([length], dtype=torch.int32)
        with torch.no_grad():
            reference, reference_length = encoder(mel, lengths)
        result = loaded.predict({"mel": mel.numpy(), "mel_length": lengths.numpy()})
        frames = int(reference_length[0])
        if int(result["encoder_length"][0]) != frames:
            sys.exit(f"length {length}: Core ML returned {result['encoder_length']} frames")
        got = result["encoder"].astype(np.float32)[..., :frames]
        want = reference.numpy()[..., :frames]
        if not np.isfinite(got).all():
            sys.exit(f"length {length}: non-finite Core ML output")
        cosine = float((got * want).sum() / (np.linalg.norm(got) * np.linalg.norm(want)))
        checks.append(
            {
                "melFrames": length,
                "encoderFrames": frames,
                "maxAbsDiff": float(np.abs(got - want).max()),
                "cosine": cosine,
            }
        )
        if cosine < MIN_COSINE:
            sys.exit(f"length {length}: cosine {cosine:.5f} against float32 PyTorch")
    share = neural_engine_share(compiled)
    mlx_source_sha = write_decoder(args.out, args.cache)

    files = sorted(
        [args.out / "config.json", args.out / "model.safetensors"]
        + [path for path in compiled.rglob("*") if path.is_file()]
    )
    outputs = [
        {
            "path": str(path.relative_to(args.out)),
            "sizeBytes": path.stat().st_size,
            "sha256": sha256(path),
        }
        for path in files
    ]
    rows = "\n".join(f"| `{o['path']}` | {o['sizeBytes']:,} | `{o['sha256']}` |" for o in outputs)
    check_rows = "\n".join(
        f"| {c['melFrames']} | {c['encoderFrames']} | {c['maxAbsDiff']:.4f} | {c['cosine']:.6f} |"
        for c in checks
    )
    versions = ", ".join(
        f"{name} {importlib.metadata.version(name)}"
        for name in ("coremltools", "torch", "transformers")
    )
    (args.out / "README.md").write_text(f"""---
license: cc-by-4.0
library_name: coreml
pipeline_tag: automatic-speech-recognition
base_model: nvidia/parakeet-tdt-0.6b-v3
tags: [coreml, neural-engine, parakeet, float16, vesper]
---

# Parakeet TDT 0.6B v3 for the Apple Neural Engine (Core ML encoder + MLX decoder)

[{SOURCE[0]}@{SOURCE[1][:8]}](https://huggingface.co/{SOURCE[0]}/tree/{SOURCE[1]}) split for
[Vesper](https://github.com/rahulsharmavishwakarma/vesper): the FastConformer encoder runs
as a Core ML program on the Neural Engine, and the TDT decoder and joint network run in MLX.

**Modifications.**

- `{COMPILED}`: the encoder, as a compiled Core ML model, loaded from NVIDIA's Transformers-format checkpoint, traced
  over one fixed 15 s window and converted to float16 by {versions}. Frames past
  `mel_length` are zeroed and the attention mask is clamped to -10000, matching NeMo's
  encoder.
- `model.safetensors` and `config.json`: the decoder, joint network and subsampling
  tensors of
  [{MLX_SOURCE[0]}@{MLX_SOURCE[1][:8]}](https://huggingface.co/{MLX_SOURCE[0]}/tree/{MLX_SOURCE[1]})
  (source `model.safetensors` SHA-256 `{mlx_source_sha}`), copied unchanged; the
  configuration is the same except `encoder.n_layers` is 0, because Core ML replaces the
  encoder layers. The subsampling tensors are not used at inference.

Weights: CC-BY-4.0. Attribution: NVIDIA (Parakeet TDT 0.6B v3). This is not an NVIDIA
release.

| Input / output | Type | Shape |
| --- | --- | --- |
| `mel` | float32 | 1 x 128 x 1501 (normalized log-mel, zero-padded) |
| `mel_length` | int32 | 1 |
| `encoder` | float16 | 1 x 1024 x 188 |
| `encoder_length` | int32 | 1 |

Conversion check on the CPU and Neural Engine against float32 PyTorch, using a synthetic
voiced signal. This catches broken exports; float16 drifts more on this signal than on
speech, and speech accuracy is measured in Vesper's validation records. Estimated cost
scheduled on the Neural Engine: {share:.2%}.

| Mel frames | Encoder frames | Max abs diff | Cosine |
| ---: | ---: | ---: | ---: |
{check_rows}

| File | Bytes | SHA-256 |
| --- | ---: | --- |
{rows}
""")
    print(json.dumps({"neuralEngineShare": share, "checks": checks, "files": outputs}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
