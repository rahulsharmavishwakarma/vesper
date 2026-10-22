#!/usr/bin/env python3
"""Research tool: quantize the pinned MLX Parakeet TDT 0.6B v3 export and score the variants.

Subcommands, all run with the isolated MLX project on Apple silicon:

    uv run --locked --project benchmarks/asr/python/mlx-convert \
        python scripts/quantize-mlx-parakeet.py sensitivity --work WORK --out sensitivity.json
    ... build --work WORK --scheme affine --bits 4 --out OUT_DIR
    ... build --work WORK --scheme mixed --sensitivity sensitivity.json --high-fraction 0.25 --out OUT_DIR
    ... eval --work WORK --out results.json LABEL=DIR [LABEL=DIR ...]

Only `Linear` layers are quantized (MLX `Conv1d` has no quantized form), so the output loads with the
per-layer quantization support of the app's Swift Parakeet loader without code changes. Other float
weights are stored as `--float-dtype`. Sensitivity uses public FLEURS en-US dev clips; evaluation uses
the first 100 unique 4-20 s sentences of the test split, so calibration and scoring never overlap.
Outputs are research artifacts, not qualified application models.
"""

import argparse
import hashlib
import json
import re
import shutil
import statistics
import struct
import sys
import tarfile
import time
from pathlib import Path

import mlx.core as mx
import mlx.nn as nn
import numpy as np
from huggingface_hub import hf_hub_download, snapshot_download
from mlx.utils import tree_flatten

SOURCE_REPO = "mlx-community/parakeet-tdt-0.6b-v3"
SOURCE_REVISION = "ed2b7e8c15f9aaa0b5772e2efb986255eaef7e15"
FLEURS_REVISION = "70bb2e84b976b7e960aa89f1c648e09c59f894dd"
TEST_PINS = {
    "data/en_us/audio/test.tar.gz": "d9c2e37b41aacd41bc283554a0a82b5476b36887049774ecb2819dcaaa55a356",
    "data/en_us/test.tsv": "74c046239374deeb60fa63f258f907388093a32bcaa3140965f70ef05c79f7ca",
}
DEV_FILES = ("data/en_us/audio/dev.tar.gz", "data/en_us/dev.tsv")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 24), b""):
            digest.update(chunk)
    return digest.hexdigest()


def source_dir(work: Path) -> Path:
    return Path(
        snapshot_download(
            SOURCE_REPO, revision=SOURCE_REVISION, local_dir=work / "parakeet-tdt-0.6b-v3"
        )
    )


def load(path: Path):
    # mlx-audio infers the model type from the folder name when config.json has none,
    # so source and output folders must contain "parakeet".
    if "parakeet" not in path.name:
        sys.exit(
            f"{path}: folder name must contain 'parakeet' for mlx-audio to detect the model type"
        )
    from mlx_audio.stt.utils import load_model

    return load_model(str(path))


def read_wav(data: bytes, name: str) -> np.ndarray:
    """Decode a 16 kHz mono WAV holding 16-bit PCM or 32-bit float samples."""
    if data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        sys.exit(f"{name}: not a WAV file")
    offset, fmt, samples = 12, None, None
    while offset + 8 <= len(data):
        chunk, size = data[offset : offset + 4], struct.unpack_from("<I", data, offset + 4)[0]
        body = data[offset + 8 : offset + 8 + size]
        if chunk == b"fmt ":
            fmt = struct.unpack_from("<HHIIHH", body)
        elif chunk == b"data":
            samples = body
        offset += 8 + size + (size & 1)
    if fmt is None or samples is None:
        sys.exit(f"{name}: missing fmt or data chunk")
    tag, channels, rate, _, _, width = fmt
    if channels != 1 or rate != 16000:
        sys.exit(f"{name}: expected 16 kHz mono, got {channels} channels at {rate} Hz")
    if tag == 3 and width == 32:
        return np.frombuffer(samples, dtype="<f4").astype(np.float32)
    if tag == 1 and width == 16:
        return np.frombuffer(samples, dtype="<i2").astype(np.float32) / 32768.0
    sys.exit(f"{name}: unsupported WAV format {tag} with {width}-bit samples")


def fleurs_clips(work: Path, split: str, count: int):
    """Return (rows, {id: float32 samples}) for the first `count` unique 4-20 s sentences."""
    names = TEST_PINS if split == "test" else dict.fromkeys(DEV_FILES)
    hashes = {}
    paths = {}
    for name, expected in names.items():
        path = Path(
            hf_hub_download(
                "google/fleurs",
                name,
                revision=FLEURS_REVISION,
                repo_type="dataset",
                local_dir=work / "fleurs",
            )
        )
        actual = sha256(path)
        if expected and actual != expected:
            sys.exit(f"{name}: hash {actual} does not match the pinned {expected}")
        hashes[name] = actual
        paths[name] = path
    tsv = next(p for n, p in paths.items() if n.endswith(".tsv"))
    archive_path = next(p for n, p in paths.items() if n.endswith(".tar.gz"))
    rows, seen = [], set()
    for line in tsv.read_text(encoding="utf8").strip().splitlines():
        fields = line.split("\t")
        if fields[0] in seen or not 4 <= int(fields[5]) / 16000 <= 20:
            continue
        seen.add(fields[0])
        rows.append({"id": fields[0], "audio": fields[1], "reference": fields[2]})
        if len(rows) == count:
            break
    wanted = {r["audio"]: r["id"] for r in rows}
    clips = {}
    with tarfile.open(archive_path) as archive:
        for member in archive:
            base = member.name.rsplit("/", 1)[-1]
            if base in wanted:
                clips[wanted[base]] = read_wav(archive.extractfile(member).read(), base)
    if len(clips) != len(rows):
        sys.exit(f"found {len(clips)} of {len(rows)} {split} clips")
    return rows, clips, hashes


def linear_paths(model):
    return [name for name, module in model.named_modules() if isinstance(module, nn.Linear)]


def module_at(model, path):
    node = model
    for part in path.split("."):
        node = node[int(part)] if part.isdigit() else getattr(node, part)
    return node


def encoder_output(model, samples):
    mel = model._log_mel_spectrogram(mx.array(samples))
    if mel.ndim == 2:
        mel = mel[None]
    out = model.encoder(mel)
    out = out[0] if isinstance(out, tuple) else out
    mx.eval(out)
    return out


def sensitivity(args) -> int:
    model = load(source_dir(args.work))
    rows, clips, hashes = fleurs_clips(args.work, "dev", args.clips)
    references = {i: encoder_output(model, clips[i]).astype(mx.float32) for i in clips}
    scores = {}
    paths = linear_paths(model)
    for index, path in enumerate(paths, 1):
        layer = module_at(model, path)
        original = layer.weight
        if original.shape[-1] % args.group:
            scores[path] = None
            continue
        packed = mx.quantize(original, group_size=args.group, bits=args.bits)
        layer.weight = mx.dequantize(*packed, group_size=args.group, bits=args.bits).astype(
            original.dtype
        )
        errors = []
        for clip_id, reference in references.items():
            out = encoder_output(model, clips[clip_id]).astype(mx.float32)
            errors.append((mx.linalg.norm(out - reference) / mx.linalg.norm(reference)).item())
        layer.weight = original
        scores[path] = {"relative_error": statistics.mean(errors), "parameters": int(original.size)}
        if index % 20 == 0:
            print(f"{index}/{len(paths)} layers", flush=True)
    args.out.write_text(
        json.dumps(
            {
                "source": [SOURCE_REPO, SOURCE_REVISION],
                "calibration": {
                    "dataset": "google/fleurs",
                    "revision": FLEURS_REVISION,
                    "split": "en_us dev",
                    "clips": len(clips),
                    "sha256": hashes,
                },
                "probe": {
                    "bits": args.bits,
                    "group_size": args.group,
                    "metric": "mean relative L2 of encoder output",
                },
                "layers": scores,
            },
            indent=2,
        )
        + "\n"
    )
    print("wrote", args.out)
    return 0


def build(args) -> int:
    src = source_dir(args.work)
    if args.out.exists():
        sys.exit(f"{args.out} already exists")
    model = load(src)
    float_dtype = getattr(mx, args.float_dtype)
    model.update(
        {
            k: v
            for k, v in _unflatten(
                [
                    (k, v.astype(float_dtype))
                    for k, v in tree_flatten(model.parameters())
                    if mx.issubdtype(v.dtype, mx.floating)
                ]
            ).items()
        }
    )
    config = json.loads((src / "config.json").read_text())
    quantization = None
    if args.scheme in ("affine", "mixed"):
        high = set()
        if args.scheme == "mixed":
            layers = json.loads(args.sensitivity.read_text())["layers"]
            ranked = sorted(
                (p for p, s in layers.items() if s), key=lambda p: -layers[p]["relative_error"]
            )
            high = set(ranked[: round(len(ranked) * args.high_fraction)])
            # The probe only sees the encoder output; keep the small decoder and joint at high bits.
            high |= {p for p in linear_paths(model) if not p.startswith("encoder.")}
        per_layer = {}

        def predicate(path, module):
            if not hasattr(module, "to_quantized"):
                return False
            if not isinstance(module, nn.Linear) or module.weight.shape[-1] % args.group:
                per_layer[path] = False  # listed so loaders do not apply the default to it
                return False
            bits = args.high_bits if path in high else args.bits
            per_layer[path] = {"group_size": args.group, "bits": bits}
            return {"group_size": args.group, "bits": bits}

        nn.quantize(model, group_size=args.group, bits=args.bits, class_predicate=predicate)
        quantization = {"group_size": args.group, "bits": args.bits, "mode": "affine"}
        quantization.update(
            {p: v for p, v in per_layer.items() if v is False or v["bits"] != args.bits}
        )
        config["quantization"] = quantization
        config["quantization_config"] = quantization
    args.out.mkdir(parents=True)
    weights = dict(tree_flatten(model.parameters()))
    mx.save_safetensors(str(args.out / "model.safetensors"), weights, metadata={"format": "mlx"})
    (args.out / "config.json").write_text(json.dumps(config, indent=2) + "\n")
    for name in ("tokenizer.model", "tokenizer.vocab", "vocab.txt"):
        if (src / name).exists():
            shutil.copy(src / name, args.out / name)
    provenance = {
        "source": {"repository": SOURCE_REPO, "revision": SOURCE_REVISION},
        "scheme": args.scheme,
        "floatDtype": args.float_dtype,
        "quantization": quantization,
        "sensitivity": str(args.sensitivity) if args.sensitivity else None,
        "highFraction": args.high_fraction if args.scheme == "mixed" else None,
        "outputs": [
            {"path": p.name, "sizeBytes": p.stat().st_size, "sha256": sha256(p)}
            for p in sorted(args.out.iterdir())
            if p.is_file()
        ],
    }
    (args.out / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
    print(json.dumps(provenance["outputs"], indent=2))
    return 0


def _unflatten(pairs):
    from mlx.utils import tree_unflatten

    return tree_unflatten(pairs)


def words(text):
    return re.sub(r"[^\w\s']", " ", text.lower()).split()


def edit_distance(ref, hyp):
    prev = list(range(len(hyp) + 1))
    for i, r in enumerate(ref, 1):
        cur = [i]
        for j, h in enumerate(hyp, 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (r != h)))
        prev = cur
    return prev[-1]


def evaluate(args) -> int:
    rows, clips, hashes = fleurs_clips(args.work, "test", 100)
    results = {}
    for pair in args.models:
        label, folder = pair.split("=", 1)
        folder = source_dir(args.work) if folder == "source" else Path(folder)
        getattr(mx, "reset_peak_memory", lambda: None)()
        started = time.perf_counter()
        model = load(folder)
        load_seconds = time.perf_counter() - started
        model.generate(mx.array(clips[rows[0]["id"]]))  # warm-up
        errors, total, times = [], 0, []
        for row in rows:
            start = time.perf_counter()
            text = model.generate(mx.array(clips[row["id"]])).text
            times.append((time.perf_counter() - start) * 1000)
            ref = words(row["reference"])
            errors.append(edit_distance(ref, words(text)))
            total += len(ref)
        size = sum(p.stat().st_size for p in folder.glob("*.safetensors"))
        results[label] = {
            "errors": errors,
            "words": total,
            "wer": sum(errors) / total,
            "median_ms": round(statistics.median(times), 1),
            "load_s": round(load_seconds, 2),
            "peak_memory_mb": round(getattr(mx, "get_peak_memory", lambda: 0)() / 1e6),
            "weights_bytes": size,
        }
        print(
            f"{label:22} {size / 1e6:7.0f} MB  WER {results[label]['wer'] * 100:5.2f}%  "
            f"median {results[label]['median_ms']:6.1f} ms  peak {results[label]['peak_memory_mb']} MB",
            flush=True,
        )
        del model
        mx.clear_cache()
    labels = list(results)
    base = np.array(results[labels[0]]["errors"])
    ref_words = np.array([len(words(r["reference"])) for r in rows])
    index = np.random.default_rng(0).integers(0, len(rows), size=(2000, len(rows)))
    for label in labels[1:]:
        e = np.array(results[label]["errors"])
        diff = (e[index].sum(1) - base[index].sum(1)) / ref_words[index].sum(1)
        results[label]["vs_first"] = round(float((e.sum() - base.sum()) / ref_words.sum()), 4)
        results[label]["vs_first_ci95"] = [
            round(float(x), 4) for x in np.percentile(diff, [2.5, 97.5])
        ]
        print(
            f"{label:22} vs {labels[0]}: {results[label]['vs_first'] * 100:+.2f} pts "
            f"CI {[round(x * 100, 2) for x in results[label]['vs_first_ci95']]}"
        )
    args.out.write_text(
        json.dumps(
            {
                "dataset": {
                    "repo": "google/fleurs",
                    "revision": FLEURS_REVISION,
                    "split": "en_us test",
                    "clips": len(rows),
                    "sha256": hashes,
                },
                "mlx": mx.__version__ if hasattr(mx, "__version__") else None,
                "results": {
                    k: {kk: vv for kk, vv in v.items() if kk != "errors"}
                    for k, v in results.items()
                },
            },
            indent=2,
        )
        + "\n"
    )
    print("wrote", args.out)
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("sensitivity", "build", "eval"):
        p = sub.add_parser(name)
        p.add_argument(
            "--work", type=Path, required=True, help="Cache directory for sources and FLEURS"
        )
        p.add_argument("--out", type=Path, required=True)
        if name in ("sensitivity", "build"):
            p.add_argument("--bits", type=int, default=4, choices=(2, 3, 4, 5, 6, 8))
            p.add_argument("--group", type=int, default=64, choices=(32, 64, 128))
        if name == "sensitivity":
            p.add_argument("--clips", type=int, default=16)
        if name == "build":
            p.add_argument("--scheme", required=True, choices=("float", "affine", "mixed"))
            p.add_argument(
                "--float-dtype", default="bfloat16", choices=("float32", "float16", "bfloat16")
            )
            p.add_argument("--high-bits", type=int, default=8, choices=(4, 5, 6, 8))
            p.add_argument("--high-fraction", type=float, default=0.25)
            p.add_argument("--sensitivity", type=Path)
        if name == "eval":
            p.add_argument(
                "models", nargs="+", help="LABEL=DIR, or LABEL=source for the pinned original"
            )
    args = parser.parse_args()
    if args.command == "build" and args.scheme == "mixed" and not args.sensitivity:
        sys.exit("--scheme mixed needs --sensitivity")
    return {"sensitivity": sensitivity, "build": build, "eval": evaluate}[args.command](args)


if __name__ == "__main__":
    sys.exit(main())
