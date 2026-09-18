#!/usr/bin/env python3
"""Preserve generated report JSON in the ignored local benchmark cache."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT / ".cache" / "benchmarks" / "results"
INDEX = ROOT / ".cache" / "benchmarks" / "debug-report-index.json"


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def checked_source(value: str) -> Path:
    candidate = ROOT / value
    if candidate.is_symlink():
        raise SystemExit(f"report must not be a symbolic link: {value}")
    path = candidate.resolve(strict=True)
    try:
        path.relative_to(ROOT)
    except ValueError as error:
        raise SystemExit(f"report must be inside the repository: {value}") from error
    if not path.is_file() or path.suffix != ".json":
        raise SystemExit(f"report must be a regular JSON file: {value}")
    return path


def load_index() -> dict[str, dict[str, object]]:
    if not INDEX.exists():
        return {}
    value = json.loads(INDEX.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise SystemExit(f"invalid local debug index: {INDEX}")
    return value


def save_index(value: dict[str, dict[str, object]]) -> None:
    INDEX.parent.mkdir(parents=True, exist_ok=True)
    temporary = INDEX.with_suffix(".tmp")
    temporary.write_text(json.dumps(dict(sorted(value.items())), indent=2) + "\n", encoding="utf-8")
    temporary.replace(INDEX)


def archive(values: list[str]) -> None:
    index = load_index()
    for value in values:
        source = checked_source(value)
        relative = source.relative_to(ROOT).as_posix()
        target = CACHE / source.stem / "debug-summary.json"
        target.parent.mkdir(parents=True, exist_ok=True)
        source_digest = digest(source)
        if target.exists() and digest(target) != source_digest:
            raise SystemExit(f"refusing to overwrite different local report: {target}")
        if not target.exists():
            shutil.copy2(source, target)
        index[relative] = {
            "localPath": target.relative_to(ROOT).as_posix(),
            "sha256": source_digest,
            "sizeBytes": source.stat().st_size,
        }
    save_index(index)


def verify() -> None:
    index = load_index()
    if not index:
        raise SystemExit(f"no archived debug reports recorded in {INDEX}")
    for source, entry in index.items():
        path = ROOT / str(entry["localPath"])
        if not path.is_file():
            raise SystemExit(f"missing archived report for {source}: {path}")
        if path.stat().st_size != entry["sizeBytes"] or digest(path) != entry["sha256"]:
            raise SystemExit(f"archived report failed verification: {path}")
    print(f"verified {len(index)} local debug reports")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reports", nargs="*", help="repository-relative JSON reports")
    parser.add_argument("--verify", action="store_true", help="verify the local index")
    arguments = parser.parse_args()
    if arguments.verify:
        if arguments.reports:
            parser.error("--verify does not accept report paths")
        verify()
    elif arguments.reports:
        archive(arguments.reports)
    else:
        parser.error("provide reports to archive or use --verify")


if __name__ == "__main__":
    main()
