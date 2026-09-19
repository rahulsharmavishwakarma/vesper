#!/usr/bin/env python3
"""Development-only, pinned Microsoft VibeVoice streaming CPU probe.

Uses publisher step APIs with their trained chunk/lookahead geometry. This is
not an application adapter, microphone test, or persistent speaker enrollment.
"""

import argparse
import hashlib
import importlib.metadata
import json
import os
import re
import resource
import signal
import subprocess
import sys
import time
from pathlib import Path

from vibevoice_compat import prepare_source


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(8 * 1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def cache_length(cache):
    if hasattr(cache, "get_seq_length"):
        return int(cache.get_seq_length())
    return int(cache[0][0].shape[-2])


def speech_text(text):
    return re.sub(r"Speaker\s+\d+\s*:", "", text).strip()


def check_artifacts(root):
    provenance = json.loads((root / "verified-artifacts.json").read_text())
    for entry in provenance["artifacts"]:
        path = root / "model" / entry["file"]
        if path.stat().st_size != entry["size"] or digest(path) != entry["sha256"]:
            raise ValueError(f"Artifact verification failed: {entry['file']}")
    for entry in provenance["sourceFiles"]:
        if digest(root / "source" / entry["file"]) != entry["sha256"]:
            raise ValueError(f"Source verification failed: {entry['file']}")
    return provenance


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--threads", type=int, default=8, choices=range(1, 9))
    parser.add_argument("--case-timeout", type=int, default=300)
    parser.add_argument("--max-tokens", type=int, default=256)
    args = parser.parse_args()
    if args.case_timeout <= 0 or not 1 <= args.max_tokens <= 256:
        parser.error("invalid timeout or token budget")
    # Exclusive creation also protects evidence from accidental reruns.
    with args.output.open("x") as stream:
        stream.write("{}\n")
    plan = json.loads(args.plan.read_text())
    report = {
        "schemaVersion": 1,
        "status": "preflight",
        "plan": plan,
        "runnerSha256": digest(Path(__file__)),
        "planSha256": digest(args.plan),
        "threads": args.threads,
        "device": "cpu",
        "precision": "float32",
        "attention": "sdpa",
        "maxTokensPerChunk": args.max_tokens,
        "cases": [],
        "unmeasured": [
            "word-aligned emission latency",
            "automatic endpoint latency",
            "microphone queues",
            "power/thermal telemetry",
            "application writer",
        ],
    }

    def save():
        args.output.write_text(json.dumps(report, indent=2) + "\n")

    def timeout(_signum, _frame):
        raise TimeoutError("per-case wall-clock budget exceeded")

    signal.signal(signal.SIGALRM, timeout)
    try:
        started = time.perf_counter()
        report["provenance"] = check_artifacts(args.root)
        report["verificationMs"] = (time.perf_counter() - started) * 1000
        prepared_source, compatibility = prepare_source(args.root)
        report["compatibilityPatch"] = compatibility
        if importlib.metadata.version("transformers") != compatibility["transformersVersion"]:
            raise ValueError("Use the locked VibeVoice environment for this compatibility patch")
        sys.path.insert(0, prepared_source.name)
        import numpy as np
        import torch
        from vibevoice.modular.modeling_vibevoice_asr import (
            VibeVoiceASRForConditionalGeneration,
        )
        from vibevoice.processor.vibevoice_asr_processor import VibeVoiceASRProcessor

        torch.set_num_threads(args.threads)
        torch.set_num_interop_threads(1)
        report["packages"] = {
            name: importlib.metadata.version(name)
            for name in (
                "torch",
                "transformers",
                "diffusers",
                "numpy",
                "safetensors",
                "tokenizers",
            )
        }
        model_path = args.root / "model"
        config = json.loads((model_path / "preprocessor_config.json").read_text())
        rate = config["target_sample_rate"]
        advance = config["chunk_frames"] * config["speech_tok_compress_ratio"]
        lookahead = config["lookahead_frames"] * config["speech_tok_compress_ratio"]
        window = advance + lookahead
        if (rate, advance, lookahead) != (24000, 70400, 12800):
            raise ValueError("Unexpected pinned checkpoint chunk geometry")
        report["frameConfig"] = config
        started = time.perf_counter()
        processor = VibeVoiceASRProcessor.from_pretrained(
            str(model_path),
            local_files_only=True,
        )
        model, loading_info = VibeVoiceASRForConditionalGeneration.from_pretrained(
            str(model_path),
            dtype=torch.float32,
            attn_implementation="sdpa",
            local_files_only=True,
            output_loading_info=True,
        )
        report["loadingInfo"] = {
            key: sorted(value) if isinstance(value, set) else value
            for key, value in loading_info.items()
        }
        if any(
            loading_info.get(key)
            for key in ("missing_keys", "unexpected_keys", "mismatched_keys", "error_msgs")
        ):
            raise ValueError("Checkpoint loading has unexplained missing or incompatible weights")
        model = model.to("cpu").eval()
        from safetensors import safe_open

        # A major loader upgrade must not silently reinitialize trained weights.
        # Check every tensor, including the duplicated tied embeddings, exactly.
        checked_tensors = 0
        weight_map = json.loads((model_path / "model.safetensors.index.json").read_text())[
            "weight_map"
        ]
        loaded_state = model.state_dict()
        for shard in sorted(set(weight_map.values())):
            with safe_open(model_path / shard, framework="pt", device="cpu") as checkpoint:
                for name in checkpoint.keys():
                    actual = loaded_state[name]
                    expected = checkpoint.get_tensor(name).to(actual.dtype)
                    if not torch.equal(actual, expected):
                        raise ValueError(f"Loaded tensor differs from pinned checkpoint: {name}")
                    checked_tensors += 1
        del loaded_state, actual, expected
        report["checkpointTensorsVerified"] = checked_tensors
        report["tiedEmbeddingIdentity"] = (
            model.get_input_embeddings().weight is model.get_output_embeddings().weight
        )
        if model.config.tie_word_embeddings and not report["tiedEmbeddingIdentity"]:
            raise ValueError("Expected tied embeddings do not share parameter identity")
        acoustic = model.model.acoustic_tokenizer
        report["acousticSamplingStd"] = acoustic.fix_std.item()
        if report["acousticSamplingStd"] != float(acoustic.config.fix_std):
            raise ValueError("Acoustic sampling buffer differs from the pinned configuration")
        report["loadMs"] = (time.perf_counter() - started) * 1000
        report["loadedUniqueParameters"] = sum(p.numel() for p in model.parameters())
        report["loadedParameterBytes"] = sum(
            p.numel() * p.element_size() for p in model.parameters()
        )
        report["status"] = "running"
        save()
        print("Verified model loaded on CPU", flush=True)
        for item in plan["cases"]:
            row = {
                "id": item["id"],
                "delivery": item.get("delivery", "offline"),
                "seed": item.get("seed", 0),
                "chunks": [],
                "error": None,
            }
            report["cases"].append(row)
            try:
                path = Path(item["audioPath"])
                if digest(path) != item["sha256"]:
                    raise ValueError("audio checksum mismatch")
                prep_start = time.perf_counter()
                pcm = subprocess.run(
                    [
                        "ffmpeg",
                        "-v",
                        "error",
                        "-nostdin",
                        "-i",
                        str(path),
                        "-f",
                        "s16le",
                        "-acodec",
                        "pcm_s16le",
                        "-ac",
                        "1",
                        "-ar",
                        str(rate),
                        "-",
                    ],
                    check=True,
                    capture_output=True,
                    timeout=30,
                ).stdout
                audio = np.frombuffer(pcm, dtype="<i2").astype(np.float32) / 32768.0
                duration = len(audio) / rate
                if not 0 < duration <= 480:
                    raise ValueError("outside released positive <=480s envelope")
                row.update(
                    audioSeconds=duration,
                    resampledPcmSha256=hashlib.sha256(pcm).hexdigest(),
                    preparationMs=(time.perf_counter() - prep_start) * 1000,
                )
                torch.manual_seed(row["seed"])
                signal.alarm(args.case_timeout)
                request_start = time.perf_counter()
                session = model.init_streaming_state(
                    processor.tokenizer,
                    context_info=item.get("hotwords"),
                )
                row["initialCacheLength"] = cache_length(session["past_key_values"])
                row["promptPrefillMs"] = (time.perf_counter() - request_start) * 1000
                first_partial = None
                for position in range(0, len(audio), advance):
                    ready = min(position + window, len(audio)) / rate
                    if row["delivery"] == "paced":
                        delay = request_start + ready - time.perf_counter()
                        if delay > 0:
                            time.sleep(delay)
                    elif row["delivery"] != "offline":
                        raise ValueError("unknown delivery mode")
                    chunk_start = time.perf_counter()
                    values = np.zeros(window, dtype=np.float32)
                    section = audio[position : position + window]
                    values[: len(section)] = section
                    features = model.encode_speech(torch.from_numpy(values)[None])
                    encoded = time.perf_counter()
                    before = cache_length(session["past_key_values"])
                    first_token = []
                    text, session = model.streaming_generate_step(
                        audio_features=features,
                        streaming_state=session,
                        tokenizer=processor.tokenizer,
                        max_new_tokens=args.max_tokens,
                        temperature=0.0,
                        on_first_token=lambda _token, times=first_token, start=request_start: (
                            times.append((time.perf_counter() - start) * 1000)
                        ),
                    )
                    finished = time.perf_counter()
                    after = cache_length(session["past_key_values"])
                    if after <= before:
                        raise ValueError("Streaming cache did not advance")
                    count = after - before - features.shape[1] - 3
                    chunk = {
                        "startSample": position,
                        "advancedSamples": min(advance, len(audio) - position),
                        "inputWindowSamples": len(section),
                        "paddedSamples": window - len(section),
                        "audioReadyMs": ready * 1000,
                        "encoderMs": (encoded - chunk_start) * 1000,
                        "decoderMs": (finished - encoded) * 1000,
                        "cacheBefore": before,
                        "cacheAfter": after,
                        "emissionMs": (finished - request_start) * 1000,
                        "firstGeneratedTokenMs": first_token[0] if first_token else None,
                        "backlogBeforeComputeMs": max(
                            0, (chunk_start - request_start - ready) * 1000
                        )
                        if row["delivery"] == "paced"
                        else None,
                        "cacheTokens": after,
                        "generatedTokens": count,
                        "reachedTokenCap": count >= args.max_tokens,
                        "text": text,
                        "rssPeakBytes": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
                    }
                    row["chunks"].append(chunk)
                    if first_partial is None and speech_text(text):
                        first_partial = chunk["emissionMs"]
                    save()
                    print(
                        f"{item['id']} chunk {len(row['chunks'])}: "
                        f"encode={chunk['encoderMs']:.0f}ms "
                        f"decode={chunk['decoderMs']:.0f}ms tokens={count}",
                        flush=True,
                    )
                    if chunk["reachedTokenCap"]:
                        raise RuntimeError("chunk exhausted token budget; incomplete output")
                elapsed = time.perf_counter() - request_start
                row.update(
                    elapsedMs=elapsed * 1000,
                    firstPartialMs=first_partial,
                    stopToFinalMs=max(0, elapsed - (duration if row["delivery"] == "paced" else 0))
                    * 1000,
                    computeRtf=(
                        row["promptPrefillMs"]
                        + sum(c["encoderMs"] + c["decoderMs"] for c in row["chunks"])
                    )
                    / (duration * 1000),
                    completeAudioCoverage=sum(c["advancedSamples"] for c in row["chunks"])
                    == len(audio),
                    completionReason="chunk-end-or-eos-not-distinguished-by-public-api",
                )
                del session, features
            except Exception as error:  # noqa: BLE001 - retain every failed case
                row["error"] = f"{type(error).__name__}: {error}"
                print(f"{item['id']}: {row['error']}", flush=True)
            finally:
                signal.alarm(0)
                row["raw"] = "".join(chunk["text"] for chunk in row["chunks"])
                row["speech"] = speech_text(row["raw"])
                row["speechSha256"] = hashlib.sha256(row["speech"].encode()).hexdigest()
                row["rssPeakBytes"] = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
                save()
        report["status"] = "completed"
    except Exception as error:
        report["status"] = "failed"
        report["fatalError"] = f"{type(error).__name__}: {error}"
        raise
    finally:
        report["rssPeakBytes"] = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
        save()


if __name__ == "__main__":
    # Network is disabled for inference; preparation fetches immutable revisions.
    os.environ["HF_HUB_OFFLINE"] = "1"
    os.environ["TRANSFORMERS_OFFLINE"] = "1"
    main()
