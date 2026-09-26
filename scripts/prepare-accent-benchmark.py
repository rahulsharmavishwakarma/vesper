#!/usr/bin/env python3
"""Select cached Svarah primary-language cohorts without using ASR predictions.

PyArrow only decodes the existing Parquet corpus; inference uses vesper-eval.
No downloads, training, live profiles or private audio are accessed.
"""

import argparse
import hashlib
import importlib.util
import io
import json
import wave
from collections import Counter, defaultdict
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "personal_corpus", Path(__file__).with_name("prepare-personalization-benchmark.py")
)
corpus = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(corpus)


def ordered(value):
    return hashlib.sha256(("svarah-accent-screen-v1:" + value).encode()).hexdigest()


def select(rows, excluded_groups, excluded_references, per_group):
    strata = defaultdict(list)
    for row in rows:
        if (
            4 <= row["duration"] <= 20
            and row["group"] not in excluded_groups
            and corpus.normalized(row["text"]) not in excluded_references
        ):
            strata[row["primary_language"]].append(row)
    selected, groups, references = [], set(excluded_groups), set(excluded_references)
    for label, candidates in sorted(strata.items()):
        count = 0
        for row in sorted(candidates, key=lambda r: ordered(r["path"])):
            reference = corpus.normalized(row["text"])
            if row["group"] in groups or reference in references:
                continue
            selected.append(row)
            groups.add(row["group"])
            references.add(reference)
            count += 1
            if count == per_group:
                break
    return selected


def prepare(cache, output, per_group):
    import pyarrow
    import pyarrow.parquet as pq

    # New directory ensures a completed experiment cannot be overwritten.
    output.mkdir(parents=True, exist_ok=False)
    (output / "audio").mkdir()
    previous = cache / "personalization-2026-09-16"
    excluded_groups, excluded_references = set(), set()
    for name in ("calibration", "holdout"):
        for sample in json.loads((previous / f"{name}.json").read_text())["samples"]:
            excluded_references.add(corpus.normalized(sample["reference"]))
            if sample["sourceId"] == "svarah":
                excluded_groups.add(sample["id"].removeprefix("svarah-").split("_chunk_")[0])
    shards = sorted((cache / "downloads/svarah/data").glob("*.parquet"))
    if len(shards) != len(corpus.SVARAH_HASHES):
        raise ValueError("Expected all three pinned Svarah shards")
    rows, shard_hashes, schemas = [], {}, {}
    for shard_index, path in enumerate(shards):
        checksum = corpus.digest(path)
        if checksum != corpus.SVARAH_HASHES[shard_index]:
            raise ValueError(f"Pinned shard hash mismatch: {path.name}")
        shard_hashes[path.name] = checksum
        schemas[path.name] = pq.ParquetFile(path).schema_arrow.names
        columns = ["audio_filepath.path", "duration", "text", "primary_language"]
        for row_index, row in enumerate(pq.read_table(path, columns=columns).to_pylist()):
            row.update(shard=shard_index, row=row_index, group=row["path"].split("_chunk_")[0])
            if not isinstance(row["primary_language"], str) or not row["primary_language"].strip():
                raise ValueError("Missing cohort label must be handled explicitly")
            rows.append(row)
    selected = select(rows, excluded_groups, excluded_references, per_group)
    if {r["primary_language"] for r in selected} != {r["primary_language"] for r in rows}:
        raise ValueError("Selection lost an entire primary-language cohort")
    selected_by_index = {(r["shard"], r["row"]): r for r in selected}
    data_by_index = {}
    for shard_index, path in enumerate(shards):
        offset = 0
        for batch in pq.ParquetFile(path).iter_batches(batch_size=32, columns=["audio_filepath"]):
            for index, value in enumerate(batch.column(0).to_pylist()):
                key = shard_index, offset + index
                if key in selected_by_index:
                    data_by_index[key] = value["bytes"]
            offset += len(batch)
    samples, audit_rows, seen_audio = [], [], set()
    # Interleave cohorts to avoid making language background a run-order proxy.
    queues = defaultdict(list)
    for row in selected:
        queues[row["primary_language"]].append(row)
    for turn in range(per_group):
        for label, queue in sorted(queues.items()):
            if turn >= len(queue):
                continue
            row = queue[turn]
            data = data_by_index[(row["shard"], row["row"])]
            checksum = hashlib.sha256(data).hexdigest()
            if checksum in seen_audio:
                raise ValueError("Duplicate selected audio; refuse silent replacement")
            seen_audio.add(checksum)
            with wave.open(io.BytesIO(data)) as audio:
                duration = audio.getnframes() / audio.getframerate()
                rate, channels = audio.getframerate(), audio.getnchannels()
            if abs(duration - row["duration"]) > 0.05:
                raise ValueError("Selected audio does not match its duration metadata")
            key = "svarah-" + Path(row["path"]).stem
            path = "audio/" + key + ".wav"
            (output / path).write_bytes(data)
            samples.append(
                {
                    "id": key,
                    "sourceId": "svarah",
                    "split": "test",
                    "language": "en-IN",
                    "audioPath": path,
                    "sha256": checksum,
                    "reference": row["text"],
                    "durationSeconds": duration,
                    "scenario": "mixed_speech",
                    "replayKind": "natural",
                }
            )
            audit_rows.append(
                {
                    "id": key,
                    "cohort": label,
                    "recordingGroupProxy": row["group"],
                    "shard": shards[row["shard"]].name,
                    "row": row["row"],
                    "sha256": checksum,
                    "durationSeconds": duration,
                    "sampleRate": rate,
                    "channels": channels,
                }
            )
    source = {
        "id": "svarah",
        "name": "Svarah Indian English, primary-language cohort screen",
        "url": "https://huggingface.co/datasets/ai4bharat/Svarah",
        "revision": corpus.SVARAH_REV,
        "license": "CC-BY-4.0",
        "attribution": "AI4Bharat Svarah, Javed et al. (2023)",
        "redistribution": "Retain CC-BY-4.0 attribution; selected cached WAV bytes are unchanged.",
    }
    manifest = {"schemaVersion": 1, "sources": [source], "samples": samples}
    corpus.write_json(output / "svarah.json", manifest)
    # Two normal-speed streaming examples per primary-language cohort.
    realtime_ids = {r["id"] for r in audit_rows[: 2 * len(queues)]}
    corpus.write_json(
        output / "svarah-realtime.json",
        {**manifest, "samples": [s for s in samples if s["id"] in realtime_ids]},
    )
    corpus.write_json(output / "smoke.json", {**manifest, "samples": samples[:1]})
    us = json.loads((previous / "us.json").read_text())
    for sample in us["samples"]:
        data = (previous / sample["audioPath"]).read_bytes()
        if hashlib.sha256(data).hexdigest() != sample["sha256"]:
            raise ValueError("FLEURS control audio hash mismatch")
        (output / sample["audioPath"]).write_bytes(data)
    corpus.write_json(output / "us-control.json", us)
    audit = {
        "schemaVersion": 1,
        "selectionVersion": 1,
        "pyarrowVersion": pyarrow.__version__,
        "sourceRevision": corpus.SVARAH_REV,
        "shardSha256": shard_hashes,
        "parquetFields": schemas,
        "selection": "SHA-256 path order within primary-language cohorts; one clip per recording-group proxy; 4-20 seconds; previous calibration/holdout groups and normalized references excluded; no ASR predictions used",
        "speakerDisjoint": False,
        "speakerLimitation": "Primary language and filename groups cannot prove distinct speakers. This is not longitudinal accent learning.",
        "perGroupRequested": per_group,
        "sourceCounts": dict(Counter(r["primary_language"] for r in rows)),
        "selectedCounts": dict(Counter(r["cohort"] for r in audit_rows)),
        "excludedPriorGroups": len(excluded_groups),
        "selected": audit_rows,
        "usControl": "Same 32 cached FLEURS English holdout recordings as the earlier screen, freshly decoded here; do not interpret cross-corpus differences as a causal accent effect.",
        "manifestSha256": {p.name: corpus.digest(p) for p in output.glob("*.json")},
    }
    corpus.write_json(output / "selection-audit.json", audit)
    print(
        json.dumps(
            {
                "cohorts": len(queues),
                "svarahRecordings": len(samples),
                "svarahMinutes": sum(s["durationSeconds"] for s in samples) / 60,
                "realtimeRecordings": len(realtime_ids),
                "usRecordings": len(us["samples"]),
                "counts": audit["selectedCounts"],
            }
        )
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, default=Path(".cache/benchmarks"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--per-group", type=int, default=16)
    args = parser.parse_args()
    if not 1 <= args.per_group <= 100:
        parser.error("--per-group must be 1-100")
    prepare(args.cache, args.output, args.per_group)
