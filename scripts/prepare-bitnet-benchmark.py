#!/usr/bin/env python3
"""Reuse VibeVoice reference cases with identical FFmpeg 24 kHz PCM inputs."""

import argparse
import hashlib
import json
import subprocess
import wave
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("reference_root", type=Path)
    args = parser.parse_args()
    corpus = args.root / "corpus"
    corpus.mkdir(parents=True, exist_ok=False)
    offline_path = args.reference_root / "plan-offline.json"
    stream_path = args.reference_root / "plan-streaming.json"
    offline = json.loads(offline_path.read_text())
    streaming = json.loads(stream_path.read_text())
    meeting = next(x for x in streaming["cases"] if x["id"] == "ami-600-645")
    long = next(x for x in streaming["cases"] if x["durationSeconds"] == 180)
    samples = offline["cases"] + [
        dict(meeting, id="ami-text", delivery="offline", promptFormat="text"),
        dict(meeting, id="ami-json", delivery="offline", promptFormat="json"),
        dict(long, id="continuous-180s-offline", delivery="offline"),
    ]
    prepared = {}
    cases = []
    for sample in samples:
        path = Path(sample["audioPath"])
        if digest(path) != sample["sha256"]:
            raise ValueError("source audio checksum mismatch")
        source_sha = sample["sha256"]
        target = corpus / (source_sha[:20] + ".wav")
        if source_sha not in prepared:
            pcm = subprocess.check_output(
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
                    "24000",
                    "-",
                ]
            )
            with wave.open(str(target), "wb") as output:
                output.setnchannels(1)
                output.setsampwidth(2)
                output.setframerate(24000)
                output.writeframes(pcm)
            prepared[source_sha] = {
                "audioPath": str(target),
                "sha256": digest(target),
                "resampledPcmSha256": hashlib.sha256(pcm).hexdigest(),
                "preparedSamples": len(pcm) // 2,
            }
        cases.append(dict(sample, **prepared[source_sha], sourceAudioSha256=source_sha))
    plan = {
        "cases": cases,
        "sources": streaming["sources"],
        "sourcePlanSha256": [digest(offline_path), digest(stream_path)],
        "preprocessing": "FFmpeg 24kHz mono PCM16, matching reference; native RMS normalization separately recorded",
    }
    for name, selected in [
        ("main", cases),
        ("smoke", cases[:1]),
        ("thread", [cases[0], max(cases[:24], key=lambda x: x["durationSeconds"])]),
        ("normalization", [x for x in cases if x["lane"] in ("noise", "quiet")]),
    ]:
        if name == "thread":
            selected = [
                dict(x, id=x["id"] + f"--repeat{repeat}") for repeat in range(3) for x in selected
            ]
        with (args.root / f"plan-{name}.json").open("x") as output:
            output.write(json.dumps(dict(plan, cases=selected), indent=2) + "\n")
    print(f"Prepared {len(cases)} cases; three-minute case remains subject to memory guard.")


if __name__ == "__main__":
    main()
