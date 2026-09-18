#!/usr/bin/env python3
"""Prepare a fixed local corpus-memory experiment; PyArrow is only a Parquet decoder.
No inference, downloads, user audio, or application-profile writes happen here.
"""

import argparse
import csv
import hashlib
import json
import tarfile
import unicodedata
from collections import defaultdict, deque
from pathlib import Path

FLEURS_REV = "70bb2e84b976b7e960aa89f1c648e09c59f894dd"
SVARAH_REV = "ebbf7777fe771490696a3f7b007097606fa8c924"
SVARAH_HASHES = [
    "ac8fc08edabe47dd0ef915276bc0fab51ec1037c65794c7be8f98c5bc746d1db",
    "2adf4c4ce1e69e457f44d3af0756b3965bd05fca409b4cd27528ef5804502fba",
    "1cb3f5c6aa0c18e8c6d106f0be6129a35ddaeeb9fc6977ecb059b8f36033fd23",
]


def digest(path):
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def normalized(text):
    return " ".join(
        "".join(
            " " if unicodedata.category(c)[0] in "PS" or c.isspace() else c
            for c in unicodedata.normalize("NFKC", text).lower()
        ).split()
    )


def order(value):
    return hashlib.sha256(("local-memory-screen-v1:" + value).encode()).hexdigest()


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cache", type=Path, default=Path(".cache/benchmarks"))
    parser.add_argument(
        "--output", type=Path, default=Path(".cache/benchmarks/personalization-2026-09-16")
    )
    args = parser.parse_args()
    root, out = args.cache, args.output
    audio_out = out / "audio"
    audio_out.mkdir(parents=True, exist_ok=True)
    sources, calibration, holdout = [], [], []
    audit = {
        "selection": "fixed SHA-256 order, lengths 4–20s; no model scores used",
        "calibration": [],
        "holdout": [],
        "sourceHashes": {},
        "speakerDisjointSvarah": False,
    }
    used_audio, used_references = set(), set()
    previous = set()
    for path in (root / "datasets").glob("fleurs-*/manifest.json"):
        previous.update(normalized(s["reference"]) for s in json.loads(path.read_text())["samples"])

    def add(sample, audio_bytes, target, group):
        checksum = hashlib.sha256(audio_bytes).hexdigest()
        key = normalized(sample["reference"])
        if checksum in used_audio or key in used_references:
            return False
        used_audio.add(checksum)
        used_references.add(key)
        sample["sha256"] = checksum
        sample["audioPath"] = "audio/" + sample["id"] + ".wav"
        (out / sample["audioPath"]).write_bytes(audio_bytes)
        target.append(sample)
        audit["calibration" if target is calibration else "holdout"].append(
            {"id": sample["id"], "group": group}
        )
        return True

    for locale, language in [("en_us", "en-US"), ("hi_in", "hi-IN")]:
        for split, count, target in [("dev", 64, calibration), ("test", 32, holdout)]:
            metadata = (
                out / "downloads" / f"{locale}-dev.tsv"
                if split == "dev"
                else root / "downloads/fleurs/data" / locale / "test.tsv"
            )
            archive = (
                out / "downloads" / f"{locale}-dev.tar.gz"
                if split == "dev"
                else root / "downloads/fleurs/data" / locale / "audio/test.tar.gz"
            )
            sid = f"fleurs-{locale}-{split}"
            checks = {"metadata": digest(metadata), "audio": digest(archive)}
            audit["sourceHashes"][sid] = checks
            sources.append(
                {
                    "id": sid,
                    "name": f"FLEURS {locale} {split}",
                    "url": f"https://huggingface.co/datasets/google/fleurs/tree/{FLEURS_REV}/data/{locale}",
                    "revision": f"{FLEURS_REV}; audio-sha256:{checks['audio']}; metadata-sha256:{checks['metadata']}",
                    "license": "CC-BY-4.0",
                    "attribution": "Google FLEURS; FLoRes sentences",
                    "redistribution": "Retain CC-BY-4.0 attribution.",
                }
            )
            with metadata.open() as handle:
                rows = sorted(
                    list(csv.reader(handle, delimiter="\t", quoting=csv.QUOTE_NONE)),
                    key=lambda r: order(locale + r[0] + r[1]),
                )
            sentence_ids = set()
            before = len(target)
            with tarfile.open(archive) as tar:
                members = {
                    Path(m.name).name: m
                    for m in tar.getmembers()
                    if m.isfile() and Path(m.name).suffix == ".wav"
                }
                for row in rows:
                    duration = int(row[5]) / 16000
                    if (
                        not 4 <= duration <= 20
                        or row[0] in sentence_ids
                        or normalized(row[2]) in previous
                    ):
                        continue
                    sample = {
                        "id": f"{locale}-{split}-{Path(row[1]).stem}",
                        "sourceId": sid,
                        "split": "validation" if split == "dev" else "test",
                        "language": language,
                        "reference": row[2],
                        "durationSeconds": duration,
                        "scenario": "read",
                        "replayKind": "natural",
                    }
                    if add(
                        sample,
                        tar.extractfile(members[row[1]]).read(),
                        target,
                        f"{locale}-sentence-{row[0]}",
                    ):
                        sentence_ids.add(row[0])
                    if len(target) - before == count:
                        break
            if len(target) - before != count:
                raise ValueError(f"insufficient independent {sid} samples")

    import pyarrow.parquet as pq

    shards = sorted((root / "downloads/svarah/data").glob("*.parquet"))
    rows = []
    for shard_index, path in enumerate(shards):
        checksum = digest(path)
        if checksum != SVARAH_HASHES[shard_index]:
            raise ValueError(f"Svarah shard {shard_index} checksum mismatch")
        audit["sourceHashes"][path.name] = checksum
        for row_index, row in enumerate(
            pq.read_table(
                path, columns=["audio_filepath.path", "duration", "text", "primary_language"]
            ).to_pylist()
        ):
            row.update(shard=shard_index, row=row_index, group=row["path"].split("_chunk_")[0])
            if 4 <= row["duration"] <= 20 and normalized(row["text"]) not in used_references:
                rows.append(row)
    selected = {}
    # The source-recording proxy assigns all neighboring fragments to one side.
    for is_calibration, count, target in [(True, 64, calibration), (False, 48, holdout)]:
        strata = defaultdict(list)
        for row in rows:
            if (int(order(row["group"])[:8], 16) % 4 == 0) == is_calibration:
                strata[row["primary_language"]].append(row)
        queues = {
            key: deque(sorted(value, key=lambda r: order(r["path"])))
            for key, value in sorted(strata.items())
        }
        groups, refs = set(), set()
        chosen = 0
        while chosen < count:
            progress = False
            for label, queue in queues.items():
                while queue:
                    row = queue.popleft()
                    key = normalized(row["text"])
                    if row["group"] in groups or key in refs or key in used_references:
                        continue
                    groups.add(row["group"])
                    refs.add(key)
                    selected[(row["shard"], row["row"])] = (row, target)
                    chosen += 1
                    progress = True
                    break
                if chosen == count:
                    break
            if not progress:
                raise ValueError("insufficient Svarah groups")
    sources.append(
        {
            "id": "svarah",
            "name": "Svarah Indian English mixed read/spontaneous corpus",
            "url": "https://huggingface.co/datasets/ai4bharat/Svarah",
            "revision": SVARAH_REV,
            "license": "CC-BY-4.0",
            "attribution": "AI4Bharat Svarah",
            "redistribution": "Retain CC-BY-4.0 attribution; local source accessed under dataset terms.",
        }
    )
    for shard_index, path in enumerate(shards):
        offset = 0
        for batch in pq.ParquetFile(path).iter_batches(batch_size=32, columns=["audio_filepath"]):
            for index, value in enumerate(batch.column(0).to_pylist()):
                chosen = selected.get((shard_index, offset + index))
                if chosen is None:
                    continue
                row, target = chosen
                sample = {
                    "id": "svarah-" + Path(row["path"]).stem,
                    "sourceId": "svarah",
                    "split": "test",
                    "language": "en-IN",
                    "reference": row["text"],
                    "durationSeconds": row["duration"],
                    "scenario": "mixed_speech",
                    "replayKind": "natural",
                }
                if not add(sample, value["bytes"], target, row["group"]):
                    raise ValueError(
                        "Svarah duplicate found after selection; refuse silent substitution"
                    )
            offset += len(batch)
    for name, samples in [("calibration", calibration), ("holdout", holdout)]:
        ids = {s["sourceId"] for s in samples}
        write_json(
            out / f"{name}.json",
            {
                "schemaVersion": 1,
                "sources": [s for s in sources if s["id"] in ids],
                "samples": samples,
            },
        )
    # Established code-switch screen is evaluation-only, never a calibration source.
    mucs_root = root / "datasets/mucs-hi-en-phase1"
    mucs = json.loads((mucs_root / "manifest.json").read_text())
    for sample in mucs["samples"]:
        data = (mucs_root / sample["audioPath"]).read_bytes()
        if hashlib.sha256(data).hexdigest() != sample["sha256"]:
            raise ValueError("MUCS checksum mismatch")
        (audio_out / (sample["id"] + ".wav")).write_bytes(data)
        sample["audioPath"] = "audio/" + sample["id"] + ".wav"
    write_json(out / "hinglish.json", mucs)
    for language, label in [("en-US", "us"), ("en-IN", "india"), ("hi-IN", "hindi")]:
        samples = [s for s in holdout if s["language"] == language]
        ids = {s["sourceId"] for s in samples}
        for name, chosen in [
            (label, samples),
            (label + "-realtime", samples[:4]),
            (label + "-cleanup", samples[:8]),
        ]:
            write_json(
                out / f"{name}.json",
                {
                    "schemaVersion": 1,
                    "sources": [s for s in sources if s["id"] in ids],
                    "samples": chosen,
                },
            )
    audit["note"] = (
        "Svarah calibration borrows a disjoint subset of its test-only corpus; this is a new memory experiment, not the official full-test score. No reliable speaker IDs or speech-type labels in cached shards. FLEURS prior prepared test prompts excluded. MUCS remains an existing evaluation screen. No accuracy-based filtering."
    )
    write_json(out / "split-audit.json", audit)
    print(
        json.dumps(
            {
                "calibration": len(calibration),
                "holdout": len(holdout),
                "hinglishScreen": len(mucs["samples"]),
                "calibrationSeconds": sum(s["durationSeconds"] for s in calibration),
                "holdoutSeconds": sum(s["durationSeconds"] for s in holdout),
            }
        )
    )


if __name__ == "__main__":
    main()
