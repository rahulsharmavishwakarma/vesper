#!/usr/bin/env python3
"""Select a fresh FLEURS screen from already cached pinned archives."""

import argparse
import csv
import hashlib
import json
import tarfile
import unicodedata
from pathlib import Path


def normalize(text):
    return " ".join(
        "".join(
            c.lower() if c.isalnum() or unicodedata.category(c).startswith("M") else " "
            for c in text
        ).split()
    )


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1048576), b""):
            digest.update(block)
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("previous", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    cache = args.previous.parent
    memory = json.loads((args.previous / "memory.json").read_text())
    audit = json.loads((args.previous / "split-audit.json").read_text())
    excluded = set()
    for path in [*args.previous.glob("*.json"), *cache.glob("datasets/fleurs-*/manifest.json")]:
        data = json.loads(path.read_text())
        excluded.update(normalize(s["reference"]) for s in data.get("samples", []))
    sources = json.loads((args.previous / "holdout.json").read_text())["sources"]
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "audio").mkdir(exist_ok=True)
    all_samples = []
    selected = []
    for locale, language in [("en_us", "en-US"), ("hi_in", "hi-IN")]:
        base = cache / "downloads/fleurs/data" / locale
        metadata, archive = base / "test.tsv", base / "audio/test.tar.gz"
        expected = audit["sourceHashes"][f"fleurs-{locale}-test"]
        assert sha(metadata) == expected["metadata"] and sha(archive) == expected["audio"]
        terms = [
            normalize(v["phrase"])
            for v in memory["profile"]["vocabulary"]
            if v["language"] == ("en" if locale == "en_us" else "hi")
        ]
        with metadata.open() as handle:
            rows = list(csv.reader(handle, delimiter="\t", quoting=csv.QUOTE_NONE))
        candidates = [
            r for r in rows if 4 <= int(r[5]) / 16000 <= 20 and normalize(r[2]) not in excluded
        ]

        def rank(row):
            return hashlib.sha256(
                ("memory-followup-v1:" + locale + ":" + row[1]).encode()
            ).hexdigest()

        relevant = [
            r
            for r in candidates
            if any(" " + t + " " in " " + normalize(r[2]) + " " for t in terms)
        ]
        ordered = sorted(relevant, key=rank)[:12] + sorted(candidates, key=rank)
        samples = []
        with tarfile.open(archive) as tar:
            members = {
                Path(m.name).name: m
                for m in tar.getmembers()
                if m.isfile() and m.name.endswith(".wav")
            }
            for row in ordered:
                if normalize(row[2]) in excluded:
                    continue
                excluded.add(normalize(row[2]))
                audio = tar.extractfile(members[row[1]]).read()
                sid = f"{locale}-test-{Path(row[1]).stem}"
                path = f"audio/{sid}.wav"
                (args.output / path).write_bytes(audio)
                samples.append(
                    dict(
                        id=sid,
                        sourceId=f"fleurs-{locale}-test",
                        split="test",
                        language=language,
                        audioPath=path,
                        sha256=hashlib.sha256(audio).hexdigest(),
                        reference=row[2],
                        durationSeconds=int(row[5]) / 16000,
                        scenario="read",
                        replayKind="natural",
                    )
                )
                selected.append(
                    dict(
                        id=sid,
                        selection="calibration-term-present" if row in relevant else "hash-order",
                    )
                )
                if len(samples) == 24:
                    break
        assert len(samples) == 24
        source = [s for s in sources if s["id"] == f"fleurs-{locale}-test"]
        (args.output / f"{locale}.json").write_text(
            json.dumps(
                dict(schemaVersion=1, sources=source, samples=samples), ensure_ascii=False, indent=2
            )
            + "\n"
        )
        all_samples.extend(samples)
    (args.output / "holdout.json").write_text(
        json.dumps(
            dict(
                schemaVersion=1,
                sources=[s for s in sources if s["id"].startswith("fleurs")],
                samples=all_samples,
            ),
            ensure_ascii=False,
            indent=2,
        )
        + "\n"
    )
    (args.output / "selection.json").write_text(
        json.dumps(
            dict(
                rule="Up to 12 calibration-canonical-term-containing prompts per language, then fixed hash order; selected before new inference. Prior selected normalized prompts excluded.",
                samples=selected,
            ),
            indent=2,
        )
        + "\n"
    )
    print(
        f"Prepared {len(all_samples)} new recordings; {sum(s['durationSeconds'] for s in all_samples):.2f} seconds"
    )


if __name__ == "__main__":
    main()
