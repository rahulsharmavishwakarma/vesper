#!/usr/bin/env python3
"""Isolated CPU screening of pinned Ear/Uhm; never edits or removes source audio.

See docs/research-audio-quality.md for source contracts and evidence limitations.
Ear here tests single windows <=30s, not the SDK's long-file window selector.
Uhm tests raw PCM and SDK normalization separately against the actual 6-class graph.
"""

import argparse
import hashlib
import importlib.metadata
import json
import math
import os
import struct
import time
from pathlib import Path

# Also set these before launching; cap any host BLAS parallelism.
os.environ.setdefault("OPENBLAS_NUM_THREADS", "1")
os.environ.setdefault("OMP_NUM_THREADS", "1")
os.environ.setdefault("VECLIB_MAXIMUM_THREADS", "1")

import numpy as np
import soundfile as sf


def digest(data):
    return hashlib.sha256(data).hexdigest()


def milliseconds(start):
    return (time.perf_counter() - start) * 1000


def logmel(samples, filters):
    signal = np.zeros(480000, dtype=np.float32)
    signal[: min(len(samples), len(signal))] = samples[: len(signal)]
    centered = np.pad(signal, (200, 200), mode="reflect")
    frames = np.lib.stride_tricks.sliding_window_view(centered, 400)[::160][:3000]
    hann = (0.5 - 0.5 * np.cos(2 * np.pi * np.arange(400) / 400)).astype(np.float32)
    spectrum = np.fft.rfft(frames * hann, axis=1)
    power = (spectrum.real**2 + spectrum.imag**2).astype(np.float32)
    logs = np.log10(np.maximum(filters @ power.T, np.float32(1e-10)))
    return ((np.maximum(logs, logs.max() - 8) + 4) / 4)[None].astype(np.float32)


def check_golden(filters, path):
    golden = json.loads(path.read_text())
    t = np.arange(480000, dtype=np.float64) / 16000
    signal = (
        0.5 * np.sin(2 * np.pi * 440 * t)
        + 0.25 * np.sin(2 * np.pi * 1234.5 * t)
        + 0.05 * np.sin(2 * np.pi * 97 * t)
    ).astype(np.float32)
    features = logmel(signal, filters)[0]
    errors = [abs(float(features[s["mel"], s["frame"]]) - s["value"]) for s in golden["spots"]]
    errors.extend(np.abs(features.mean(axis=1) - golden["rowMeans"]).tolist())
    errors.extend(
        [
            abs(float(features.min()) - golden["min"]),
            abs(float(features.max()) - golden["max"]),
        ]
    )
    if max(errors) >= 0.002:
        raise ValueError(f"official frontend golden mismatch: {max(errors)}")
    return {
        "sha256": digest(path.read_bytes()),
        "maxAbsoluteError": max(errors),
        "tolerance": 0.002,
        "sourceRevision": "2aed9c6d6459fc1843ab59553965035954e40199",
    }


def spans(probabilities, duration):
    """SDK scalar threshold, minimum run, gap merging, balanced confidence."""
    runs = []
    start = None
    for i, value in enumerate(list(probabilities) + [0.0]):
        if value >= 0.5 and start is None:
            start = i
        elif value < 0.5 and start is not None:
            runs.append([start, i])
            start = None
    merged = []
    for a, b in runs:
        confidence = float(np.mean(probabilities[a:b]))
        if merged and a - merged[-1][1] <= 5:
            merged[-1][1] = b
            merged[-1][2] = max(merged[-1][2], confidence)
        elif b - a >= 5:
            merged.append([a, b, confidence])
    result = []
    for a, b, confidence in merged:
        end = min(b * 0.02, duration)
        if end - a * 0.02 >= 0.12 - 1e-9 and confidence >= 0.65:
            result.append({"startSeconds": a * 0.02, "endSeconds": end, "confidence": confidence})
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("model", choices=["ear", "uhm-raw", "uhm-normalized"])
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--model-root", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--environment", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--golden", type=Path)
    args = parser.parse_args()
    artifacts = json.loads(args.artifacts.read_text())
    selected = [
        a
        for a in artifacts
        if a["id"].startswith("ear-") or (args.model != "ear" and a["id"] == "uhm-fp16")
    ]
    if args.model != "ear":
        selected = [a for a in selected if a["id"] == "uhm-fp16"]
    for artifact in selected:
        data = (args.model_root / artifact["path"]).read_bytes()
        if len(data) != artifact["size"] or digest(data) != artifact["sha256"]:
            raise ValueError(f"artifact integrity failure: {artifact['id']}")
    if not selected:
        raise ValueError("no pinned artifacts")
    manifest = json.loads(args.manifest.read_text())
    report = {
        "model": args.model,
        "artifacts": selected,
        "manifest": manifest,
        "manifestSha256": digest(args.manifest.read_bytes()),
        "environment": json.loads(args.environment.read_text()),
        "sourceSha256": digest(Path(__file__).read_bytes()),
        "source": Path(__file__).read_text(),
        "threads": 1,
        "runs": [],
        "limits": [
            "Diagnostic screen; no natural filler span annotations",
            "Single completed windows <=30s; not causal or live streaming",
            "No production adapter or automatic audio/text deletion",
        ],
    }
    start = time.perf_counter()
    if args.model == "ear":
        from ai_edge_litert.interpreter import Interpreter

        root = args.model_root / "ear"
        table = (root / "mel_filters.f32").read_bytes()
        if struct.unpack("<II", table[:8]) != (80, 201):
            raise ValueError("filter geometry mismatch")
        filters = np.frombuffer(table, dtype="<f4", offset=8).reshape(80, 201)
        languages = json.loads((root / "languages.json").read_text())
        interpreter = Interpreter(model_path=str(root / "ear.tflite"), num_threads=1)
        interpreter.allocate_tensors()
        inputs = interpreter.get_input_details()[0]
        outputs = interpreter.get_output_details()[0]
        if list(inputs["shape"]) != [1, 80, 3000] or inputs["dtype"] != np.float32:
            raise ValueError("unexpected Ear input")
        report["runtime"] = "ai-edge-litert " + importlib.metadata.version("ai-edge-litert")
        report["provider"] = "CPU default interpreter/XNNPACK; no external GPU/NPU delegate"
        report["io"] = {"input": inputs["name"], "output": outputs["name"]}
        report["loadMs"] = milliseconds(start)
        if not args.golden:
            raise ValueError("Ear requires official --golden fixture")
        report["frontendGolden"] = check_golden(filters, args.golden)
    else:
        import onnxruntime as ort

        options = ort.SessionOptions()
        options.intra_op_num_threads = 1
        options.inter_op_num_threads = 1
        session = ort.InferenceSession(
            str(args.model_root / "uhm-web-fp16.onnx"),
            options,
            providers=["CPUExecutionProvider"],
        )
        report["runtime"] = "onnxruntime " + ort.__version__
        report["provider"] = session.get_providers()
        report["io"] = {
            "input": session.get_inputs()[0].shape,
            "output": session.get_outputs()[0].shape,
        }
        if session.get_outputs()[0].shape != [1, 1499, 6]:
            raise ValueError("expected inspected six-class Uhm artifact")
        report["loadMs"] = milliseconds(start)
        report["labels"] = ["not_filler", "uh", "um", "hmm", "and", "other"]
        report["labelCaveat"] = (
            "6-class metadata order; 5-label ONNX sidecar is incompatible. Spans use only 1-p0."
        )
    report["numpy"] = np.__version__
    for index, sample in enumerate(manifest["samples"]):
        row = {
            "id": sample["id"],
            "language": sample["language"],
            "reference": sample["reference"],
            "firstCall": index == 0,
        }
        try:
            path = args.corpus / sample["audioPath"]
            if digest(path.read_bytes()) != sample["sha256"]:
                raise ValueError("audio integrity failure")
            audio, rate = sf.read(path, dtype="float32")
            if rate != 16000 or audio.ndim != 1 or not 0 < len(audio) <= 480000:
                raise ValueError("screen requires nonempty mono16kHz <=30s")
            duration = len(audio) / rate
            if abs(duration - sample["durationSeconds"]) > 0.01 or not np.isfinite(audio).all():
                raise ValueError("audio duration/finite contract failure")
            row["audioSeconds"] = duration
            row["inputPcmSha256"] = digest(audio.tobytes())
            start = time.perf_counter()
            if args.model == "ear":
                features = logmel(audio, filters)
                row["preprocessMs"] = milliseconds(start)
                start = time.perf_counter()
                interpreter.set_tensor(inputs["index"], features)
                interpreter.invoke()
                logits = interpreter.get_tensor(outputs["index"]).reshape(-1)
                row["inferenceMs"] = milliseconds(start)
                if len(logits) != 99 or not np.isfinite(logits).all():
                    raise ValueError("invalid language logits")
                probs = np.exp(logits - logits.max())
                probs /= probs.sum()
                order = np.argsort(-probs)
                top = languages[int(order[0])]
                reliable = float(probs[order[0]] - probs[order[1]]) >= 0.25 and top not in {
                    "no",
                    "sv",
                    "da",
                }
                row.update(
                    languagePrediction=top,
                    isReliable=reliable,
                    probabilities={language: float(p) for language, p in zip(languages, probs)},
                    correct=top == sample["language"].split("-")[0]
                    if sample["reference"]
                    else None,
                )
            else:
                signal = audio.copy()
                if args.model == "uhm-normalized":
                    signal -= np.mean(signal, dtype=np.float64)
                    std = math.sqrt(
                        float(np.sum(signal.astype(np.float64) ** 2)) / max(1, len(signal) - 1)
                    )
                    signal /= std + 1e-7
                padded = np.zeros((1, 480000), dtype=np.float32)
                padded[0, : len(signal)] = signal
                row["preprocessMs"] = milliseconds(start)
                start = time.perf_counter()
                probs = session.run(["probs"], {"audio": padded})[0][0]
                row["inferenceMs"] = milliseconds(start)
                if not np.isfinite(probs).all() or np.max(abs(probs.sum(axis=1) - 1)) > 1e-4:
                    raise ValueError("invalid output probabilities")
                valid = min(len(probs), math.ceil(len(audio) / 320))
                filler = 1 - probs[:valid, 0]
                row.update(
                    validFrames=valid,
                    probabilitiesSha256=digest(probs.tobytes()),
                    fillerProbabilities=filler.tolist(),
                    spans=spans(filler, duration),
                )
                row["uncoveredTailSeconds"] = max(0, duration - valid * 0.02)
            row["error"] = None
        except Exception as error:  # noqa: BLE001 -- preserve per-case runtime failures in results
            row["error"] = f"{type(error).__name__}: {error}"
        report["runs"].append(row)
        args.output.write_text(json.dumps(report, indent=2, allow_nan=False) + "\n")
        print(sample["id"], row["error"], flush=True)


if __name__ == "__main__":
    main()
