#!/usr/bin/env python3
"""Small local GA diarization probe; no audio, models or generated reports are versioned.

Run through the repository's uv project. Requires ffmpeg and a separately built,
pinned NeMo-Speech.cpp CLI. Segment-reference DER is diagnostic, not official AMI DER.
"""

import argparse
import hashlib
import itertools
import json
import math
import os
import re
import signal
import subprocess
import time
import wave
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

MODEL_SHA = "08456d9e22cd9a323c0364d98375f3746d6e68507ebb705cd46438c534c7a3a1"
RUNTIME_REV = "97a15afa5caa9bce5baaa86c1184103877af4101"


def sha(path):
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def save(path, value):
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")


def score(reference, hypothesis, duration):
    """Exact interval integration, global one-to-one mapping, overlap, zero collar."""
    for start, end, _speaker in reference + hypothesis:
        if not (math.isfinite(start) and math.isfinite(end) and 0 <= start < end <= duration):
            raise ValueError(f"invalid interval {start}, {end}; duration {duration}")
    refs = sorted({row[2] for row in reference})
    hyps = sorted({row[2] for row in hypothesis})
    bounds = sorted({0, duration, *(x for row in reference + hypothesis for x in row[:2])})
    atoms = []
    weights = {(h, r): 0.0 for h in hyps for r in refs}
    for start, end in itertools.pairwise(bounds):
        midpoint = (start + end) / 2
        active_ref = {s for a, b, s in reference if a <= midpoint < b}
        active_hyp = {s for a, b, s in hypothesis if a <= midpoint < b}
        atoms.append((end - start, active_ref, active_hyp))
        for h in active_hyp:
            for r in active_ref:
                weights[h, r] += end - start
    if max(len(refs), len(hyps)) > 8:
        raise ValueError("This bounded probe supports at most eight speakers")
    targets = refs + [None] * max(0, len(hyps) - len(refs))
    best = max(
        itertools.permutations(targets, len(hyps)),
        key=lambda perm: sum(weights.get((h, r), 0) for h, r in zip(hyps, perm)),
    )
    mapping = dict(zip(hyps, best))
    total = missed = false_alarm = confusion = overlap = 0.0
    for dt, active_ref, active_hyp in atoms:
        nr, nh = len(active_ref), len(active_hyp)
        correct = sum(mapping[h] in active_ref for h in active_hyp)
        total += dt * nr
        missed += dt * max(0, nr - nh)
        false_alarm += dt * max(0, nh - nr)
        confusion += dt * (min(nr, nh) - correct)
        overlap += dt * (nr > 1)
    return {
        "reference_speakers": len(refs),
        "detected_speakers": len(hyps),
        "reference_speaker_seconds": total,
        "reference_overlap_seconds": overlap,
        "missed_speaker_seconds": missed,
        "false_alarm_speaker_seconds": false_alarm,
        "confusion_speaker_seconds": confusion,
        "segment_reference_der": (missed + false_alarm + confusion) / total if total else None,
        "mapping": mapping,
    }


def self_test():
    ref = [[0, 2, "A"], [1, 3, "B"]]
    assert score(ref, [[0, 2, "Y"], [1, 3, "X"]], 3)["segment_reference_der"] == 0
    assert score(ref, [[0, 3, "X"]], 3)["segment_reference_der"] == 0.5
    assert score(ref, [], 3)["segment_reference_der"] == 1
    result = score([], [[0, 2, "X"]], 3)
    assert result["segment_reference_der"] is None
    assert result["false_alarm_speaker_seconds"] == 2
    assert score([[0, 2, "A"], [1, 2, "A"]], [[0, 2, "X"]], 3)["segment_reference_der"] == 0
    assert score([[0, 2, "A"]], [[0, 2, "X"], [0, 2, "Y"]], 3)["segment_reference_der"] == 1
    try:
        score([], [[0, float("nan"), "X"]], 3)
    except ValueError:
        pass
    else:
        raise AssertionError("non-finite interval accepted")
    print("Seven interval-scoring checks passed")


def prepare(root, cache):
    plan_path = cache / "samples.json"
    if plan_path.exists():
        raise FileExistsError("Preserve the existing sample plan; choose a new cache to reprepare")
    corpus = cache / "corpus"
    corpus.mkdir(parents=True, exist_ok=True)
    source = root / ".cache/benchmarks/downloads/ami/ES2004a.Mix-Headset.wav"
    annotation = source.with_name("ami_public_manual_1.6.2.zip")
    cases = []
    with zipfile.ZipFile(annotation) as archive:
        for start, end in [(600, 645), (675, 705)]:
            name = f"ami-{start}-{end}"
            audio = corpus / f"{name}.wav"
            subprocess.run(
                [
                    "ffmpeg",
                    "-v",
                    "error",
                    "-i",
                    str(source),
                    "-ss",
                    str(start),
                    "-t",
                    str(end - start),
                    "-ar",
                    "16000",
                    "-ac",
                    "1",
                    "-c:a",
                    "pcm_s16le",
                    str(audio),
                ],
                check=True,
            )
            reference = []
            for speaker in "ABCD":
                tree = ET.fromstring(archive.read(f"segments/ES2004a.{speaker}.segments.xml"))
                for segment in tree.findall("segment"):
                    a = max(start, float(segment.attrib["transcriber_start"]))
                    b = min(end, float(segment.attrib["transcriber_end"]))
                    if a < b:
                        reference.append([a - start, b - start, speaker])
            cases.append(
                {
                    "id": name,
                    "audio": str(audio.relative_to(cache)),
                    "reference": reference,
                    "reference_kind": "AMI coarse segments",
                    "duration": end - start,
                    "sha256": sha(audio),
                }
            )
    single = root / (
        ".cache/benchmarks/results/audio-regression-2026-09-16/corpus/"
        "en_us-11974497390699501411--clean.wav"
    )
    target = corpus / "fleurs-single.wav"
    subprocess.run(
        [
            "ffmpeg",
            "-v",
            "error",
            "-i",
            str(single),
            "-ar",
            "16000",
            "-ac",
            "1",
            "-c:a",
            "pcm_s16le",
            str(target),
        ],
        check=True,
    )
    with wave.open(str(target)) as wav:
        duration = wav.getnframes() / wav.getframerate()
    cases.append(
        {
            "id": "fleurs-single",
            "audio": str(target.relative_to(cache)),
            "duration": duration,
            "reference": None,
            "expected_speakers": 1,
            "reference_kind": "Single speaker; no reference activity boundaries",
            "source_sha256": sha(single),
            "sha256": sha(target),
        }
    )
    silence = corpus / "silence.wav"
    with wave.open(str(silence), "wb") as wav:
        wav.setparams((1, 2, 16000, 160000, "NONE", "not compressed"))
        wav.writeframes(bytes(320000))
    cases.append(
        {
            "id": "silence",
            "audio": str(silence.relative_to(cache)),
            "duration": 10,
            "reference": [],
            "expected_speakers": 0,
            "reference_kind": "Synthetic exact zero PCM",
            "sha256": sha(silence),
        }
    )
    save(
        plan_path,
        {
            "ami_audio_sha256": sha(source),
            "ami_annotations_sha256": sha(annotation),
            "cases": cases,
        },
    )
    print(f"Prepared {len(cases)} fixed samples")


def run(cache, backend, case_id=None, run_name=None, verbose=False):
    native = cache / "NeMo-Speech.cpp"
    revision = subprocess.check_output(
        ["git", "-C", str(native), "rev-parse", "HEAD"], text=True
    ).strip()
    if revision != RUNTIME_REV:
        raise ValueError("Runtime revision differs from the predeclared pin")
    binary = native / "build/metal-diar/bin/nemo-speech"
    model = cache / "models/Nemotron-3-Diarization.q8_0.gguf"
    if sha(model) != MODEL_SHA:
        raise ValueError("Model hash mismatch")
    plan = json.loads((cache / "samples.json").read_text())
    if run_name and not re.fullmatch(r"[a-z0-9-]+", run_name):
        raise ValueError("Run name must contain lowercase letters, digits or hyphens")
    cases = [case for case in plan["cases"] if not case_id or case["id"] == case_id]
    if not cases:
        raise ValueError("No sample matches --case")
    output = cache / (f"results-{backend}" + (f"-{run_name}" if run_name else ""))
    output.mkdir()  # Never overwrite an earlier attempt.
    results = []
    for case in cases:
        audio = cache / case["audio"]
        if sha(audio) != case["sha256"]:
            raise ValueError(f"Audio hash mismatch: {case['id']}")
        rttm = output / f"{case['id']}.rttm"
        command = [
            "/usr/bin/time",
            "-l",
            str(binary),
            "diarize",
            str(audio),
            "--model",
            str(model),
            "--device",
            backend,
            "--format",
            "rttm",
            "--output",
            str(rttm),
        ]
        if verbose:
            command.insert(3, "--verbose")
        env = {k: v for k, v in os.environ.items() if not k.startswith("NEMO_SPEECH")}
        start = time.perf_counter()
        process = subprocess.Popen(
            command,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
            start_new_session=True,
        )
        timed_out = False
        try:
            stdout, stderr = process.communicate(timeout=300)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(process.pid, signal.SIGKILL)
            stdout, stderr = process.communicate()
        elapsed = time.perf_counter() - start
        (output / f"{case['id']}.stdout").write_text(stdout)
        (output / f"{case['id']}.stderr").write_text(stderr)
        rss = re.search(r"(\d+)\s+maximum resident set size", stderr)
        row = {
            "id": case["id"],
            "backend_requested": backend,
            "duration": case["duration"],
            "exit_code": process.returncode,
            "timeout": timed_out,
            "wall_seconds_including_load": elapsed,
            "rtf_including_load": elapsed / case["duration"],
            "peak_rss_bytes": int(rss[1]) if rss else None,
            "binary_sha256": sha(binary),
            "model_sha256": MODEL_SHA,
            "audio_sha256": case["sha256"],
        }
        try:
            if process.returncode or timed_out or not rttm.exists():
                raise ValueError("Inference failed, timed out or omitted its output")
            hypothesis = []
            for line in rttm.read_text().splitlines():
                columns = line.split()
                if len(columns) != 10 or columns[0] != "SPEAKER":
                    raise ValueError("Invalid RTTM output")
                begin = float(columns[3])
                finish = begin + float(columns[4])
                # RTTM times are rounded: tolerate <=1ms at the right edge only.
                if finish > case["duration"] and finish <= case["duration"] + 0.001:
                    finish = case["duration"]
                hypothesis.append([begin, finish, columns[7]])
            metrics = score(case["reference"] or [], hypothesis, case["duration"])
            row["detected_speakers"] = metrics["detected_speakers"]
            row["segments"] = len(hypothesis)
            if case["reference"] is not None:
                row["diagnostic"] = metrics
            row["status"] = "ok"
        except ValueError as error:
            row.update(status="failed", error=str(error))
        results.append(row)
        save(output / "summary.json", results)
        print(json.dumps(row), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--cache", type=Path)
    parser.add_argument("--prepare", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--backend", choices=["cpu", "metal"])
    parser.add_argument("--case", help="Restrict confirmation to one existing sample id")
    parser.add_argument("--run-name", help="New output suffix; never overwrites previous attempts")
    parser.add_argument(
        "--verbose", action="store_true", help="Retain upstream provider diagnostics"
    )
    args = parser.parse_args()
    if args.self_test:
        self_test()
    if args.prepare or args.backend:
        if args.cache is None:
            parser.error("--cache is required for sample preparation or inference")
        if args.prepare:
            prepare(args.root.resolve(), args.cache.resolve())
        if args.backend:
            run(args.cache.resolve(), args.backend, args.case, args.run_name, args.verbose)
