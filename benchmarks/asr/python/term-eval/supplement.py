"""Render the ordinary-word supplement and merge it with the frozen term manifest.

The frozen corpus asserts that no hard negative contains a term spelling, so terms
that are also ordinary words (helm, envoy, calico, parakeet, vesper) are missing
from it. `supplement-ordinary-terms.jsonl` adds them as target-free sentences. It
was written before any term-evaluation output existed. Audio is synthetic macOS
`say` output written to files only (never played).

Usage: python -I supplement.py CACHE_DATASET_DIR
"""

import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path

import soundfile

HERE = Path(__file__).resolve().parent
MANIFESTS = HERE.parent.parent / "manifests" / "term-eval"
VOICES = ["Samantha", "Daniel"]


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def render(text: str, voice: str, target: Path) -> None:
    with tempfile.TemporaryDirectory() as scratch:
        aiff = Path(scratch) / "speech.aiff"
        subprocess.run(["say", "-v", voice, "-o", str(aiff), text], check=True)
        subprocess.run(
            ["afconvert", "-f", "WAVE", "-d", "LEI16@16000", "-c", "1", str(aiff), str(target)],
            check=True,
        )


def main() -> int:
    dataset = Path(sys.argv[1])
    source = dataset / "term-manifest.json"
    manifest = json.loads(source.read_text())
    supplement_path = MANIFESTS / "supplement-ordinary-terms.jsonl"
    rows = [json.loads(line) for line in supplement_path.read_text().splitlines() if line]
    out_dir = dataset / "supplement"
    out_dir.mkdir(exist_ok=True)
    tts_source = next(s["sourceId"] for s in manifest["samples"] if s["category"] == "positive")
    for row in rows:
        base = {
            "sourceId": tts_source,
            "split": "test",
            "language": "en-US",
            "reference": row["reference"],
            "workspaceId": row["workspaceId"],
            "targetTerms": [],
            "category": "hard_negative",
            "subcategory": "ordinary_term",
            "relatedTermId": row["relatedTermId"],
            "synthetic": True,
            "noise": "clean",
            "biasingList": None,
        }
        manifest["samples"].append(
            {**base, "id": row["id"], "audioPath": None, "sha256": None, "durationSeconds": None, "voice": None}
        )
        for voice in VOICES:
            target = out_dir / f"{row['id']}.{voice}.wav"
            if not target.exists():
                render(row["reference"], voice, target)
            info = soundfile.info(str(target))
            manifest["samples"].append(
                {
                    **base,
                    "id": f"{row['id']}.{voice}.clean",
                    "audioPath": f"supplement/{target.name}",
                    "sha256": sha256(target),
                    "durationSeconds": round(info.frames / info.samplerate, 3),
                    "voice": voice,
                }
            )
    manifest["supplement"] = {
        "frozenManifestSha256": sha256(source),
        "supplementSha256": sha256(supplement_path),
        "note": "Ordinary-word uses of term spellings; synthetic macOS say audio.",
    }
    output = dataset / "term-manifest-supplemented.json"
    output.write_text(json.dumps(manifest, indent=1))
    print(f"wrote {output} ({len(rows)} sentences, {len(rows) * len(VOICES)} clips)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
