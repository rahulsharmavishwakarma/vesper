"""Compare locked current/failure-bank replay results; optionally repeat paired timings.

Standard library only. Requires the three completed stress runs and two public
controls produced by the documented protocol. Does not open an application store.
"""

import argparse
import hashlib
import json
import platform
import subprocess
import time
from collections import Counter
from pathlib import Path


def read(path):
    return json.loads(path.read_text())


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def timing_summary(histories):
    result = {}
    for component in histories[0]["timing"]:
        values = [h["timing"][component] for h in histories if h["timing"][component]]
        count = sum(v["count"] for v in values)
        result[component] = (
            None
            if not count
            else {
                "count": count,
                "meanMs": sum(v["meanMs"] * v["count"] for v in values) / count,
                "largestHistoryP99Ms": max(v["p99Ms"] for v in values),
            }
        )
    return result


def repeat_timings(root, binary, repeats):
    lock = read(root / "manifests/locked-suite.json")
    selected = {}
    for h in lock["histories"]:
        if h["scenario"] != "capacity":
            selected.setdefault(h["scenario"], h)
    pairs = []
    errors = []
    for repeat in range(repeats):
        for index, (scenario, h) in enumerate(selected.items()):
            manifest = root / "manifests" / h["file"]
            if sha(manifest) != h["sha256"]:
                raise ValueError("Frozen timing manifest changed")
            order = ("current", "bank") if (repeat + index) % 2 == 0 else ("bank", "current")
            pair = {"repeat": repeat, "scenario": scenario, "file": h["file"], "order": order}
            for mode in order:
                destination = root / f"timing-{repeat}-{scenario}-{mode}.json"
                command = [str(binary.resolve()), str(manifest), str(destination)]
                if mode == "bank":
                    command.append("--failure-bank")
                started = time.perf_counter()
                try:
                    subprocess.run(command, capture_output=True, text=True, timeout=600, check=True)
                    wall = (time.perf_counter() - started) * 1000
                    report = read(destination)
                    expected = read(root / mode / h["file"])
                    if any(
                        report[k] != expected[k]
                        for k in (
                            "allEpisodes",
                            "heldout",
                            "finalTest",
                            "decisions",
                            "failureBank",
                            "fixedPolicyComparators",
                        )
                    ):
                        raise ValueError("Repeated run changed quality or policy decisions")
                    pair[mode] = {
                        "wallMs": wall,
                        "adaptiveAndBaselineReplayMs": report["adaptiveAndBaselineReplayMs"],
                        "totalReplayMs": report["totalReplayMs"],
                        "timing": report["timing"],
                    }
                except (subprocess.SubprocessError, OSError, ValueError) as error:
                    errors.append(
                        {"repeat": repeat, "scenario": scenario, "mode": mode, "error": str(error)}
                    )
            pairs.append(pair)
        print(f"Timing repeat {repeat + 1}/{repeats} completed", flush=True)
    result = {"binarySha256": sha(binary), "repeats": repeats, "pairs": pairs, "errors": errors}
    write(root / "paired-timing.json", result)
    return result


def compare(root, binary, output, repeats=0):
    reports = {mode: read(root / mode / "summary.json") for mode in ("original", "current", "bank")}
    lock = read(root / "manifests/locked-suite.json")
    errors = [{"run": mode, "error": e} for mode, r in reports.items() for e in r["errors"]]
    for mode, report in reports.items():
        if report["historiesCompleted"] != len(lock["histories"]) or report[
            "suiteLockSha256"
        ] != sha(root / "manifests/locked-suite.json"):
            errors.append({"run": mode, "error": "incomplete or mismatched locked run"})
    if any(r["historiesCompleted"] != len(lock["histories"]) for r in reports.values()):
        write(
            output,
            {
                "schemaVersion": 1,
                "validation": {"errors": errors},
                "incompleteRuns": reports,
                "qualityComparisonAvailable": False,
            },
        )
        return True
    if reports["current"]["binarySha256"] != sha(binary) or reports["bank"]["binarySha256"] != sha(
        binary
    ):
        raise ValueError("Comparison binary differs from measured binary")
    histories = []
    unchanged = 0
    paired_counts = {p: Counter() for p in ("allEpisodes", "heldout", "finalTest")}
    for h in lock["histories"]:
        data = {mode: read(root / mode / h["file"]) for mode in reports}
        old, current, bank = (data[k] for k in ("original", "current", "bank"))
        same = all(
            old[k] == current[k]
            for k in (
                "allEpisodes",
                "heldout",
                "finalTest",
                "finalPolicy",
                "rows",
                "fixedPolicyComparators",
            )
        )
        same &= len(old["decisions"]) == len(current["decisions"]) and all(
            all(a[k] == b[k] for k in a) for a, b in zip(old["decisions"], current["decisions"])
        )
        unchanged += same
        if not same:
            errors.append({"file": h["file"], "error": "original/current behavior mismatch"})
        if current["fixedPolicyComparators"] != bank["fixedPolicyComparators"]:
            errors.append({"file": h["file"], "error": "fixed comparator mismatch"})
        if len(current["rows"]) != len(bank["rows"]):
            errors.append({"file": h["file"], "error": "row count mismatch"})
        for a, b in zip(current["rows"], bank["rows"]):
            if (a["id"], a["phase"], a["baselineErrors"], a["rawErrors"]) != (
                b["id"],
                b["phase"],
                b["baselineErrors"],
                b["rawErrors"],
            ):
                errors.append({"file": h["file"], "error": "paired row identity/control mismatch"})
            partitions = ["allEpisodes"]
            if a["phase"] in ("audit", "final_test"):
                partitions.append("heldout")
            if a["phase"] == "final_test":
                partitions.append("finalTest")
            for p in partitions:
                counts = paired_counts[p]
                counts["attempted"] += 1
                x, y = a["adaptiveErrors"], b["adaptiveErrors"]
                if x is None or y is None:
                    counts["unscored"] += 1
                else:
                    counts["improved"] += y < x
                    counts["regressed"] += y > x
                    counts["tied"] += y == x
        histories.append(
            {
                **h,
                "heldoutDeltaBankMinusCurrent": bank["heldout"]["adaptiveWordEdits"]
                - current["heldout"]["adaptiveWordEdits"],
                "currentResultSha256": sha(root / "current" / h["file"]),
                "bankResultSha256": sha(root / "bank" / h["file"]),
            }
        )
    totals = {}
    for mode in ("current", "bank"):
        hs = reports[mode]["histories"]
        totals[mode] = {
            p: {key: sum(h[p][key] for h in hs) for key in hs[0][p]} for p in paired_counts
        }
        totals[mode]["actions"] = dict(sum((Counter(h["actions"]) for h in hs), Counter()))
        totals[mode]["timingOrdinary"] = timing_summary(
            [h for h in hs if h["scenario"] != "capacity"]
        )
        totals[mode]["bankAdmitted"] = sum(h["failureBank"]["admitted"] for h in hs)
        totals[mode]["bankEvicted"] = sum(h["failureBank"]["evicted"] for h in hs)
        totals[mode]["bankMaxRetained"] = max(h["failureBank"]["retained"] for h in hs)
        totals[mode]["protectedChecks"] = sum(h["protectedChecks"] for h in hs)
    fixed = []
    for index in range(6):
        values = [h["fixedPolicyComparators"][index] for h in reports["current"]["histories"]]
        fixed.append(
            {
                "policy": values[0]["policy"],
                "heldout": {
                    key: sum(v["heldout"][key] for v in values) for key in values[0]["heldout"]
                },
            }
        )
    isolation = {mode: read(root / mode / "isolation-summary.json") for mode in ("current", "bank")}
    errors.extend(
        {"run": mode, "error": "isolation failed"}
        for mode, checks in isolation.items()
        if not all(c["passed"] for c in checks)
    )
    public = {}
    for mode in ("current", "bank"):
        r = read(root / f"public-{mode}.json")
        public[mode] = {
            k: r[k]
            for k in (
                "manifestSha256",
                "allEpisodes",
                "heldout",
                "decisions",
                "failureBank",
                "fixedPolicyComparators",
            )
        }
    accent = {}
    accent_rows = {}
    accent_hash = sha(root / "accent.json")
    if accent_hash != read(root / "accent-lock.json")["manifestSha256"]:
        errors.append({"error": "accent manifest changed after freeze"})
    for mode in ("current", "bank"):
        r = read(root / f"accent-{mode}.json")
        accent[mode] = {
            k: r[k]
            for k in (
                "manifestSha256",
                "allEpisodes",
                "heldout",
                "decisions",
                "failureBank",
                "adaptiveMemoryCount",
                "fixedPolicyComparators",
            )
        }
        accent_rows[mode] = [row for row in r["rows"] if row["phase"] == "final_test"]
        if (
            r["manifestSha256"] != accent_hash
            or r["heldout"]["attempted"] != 304
            or r["allEpisodes"]["failureRows"]
        ):
            errors.append({"run": mode, "error": "accent control coverage/hash/failure mismatch"})
    accent["identicalHeldoutOutputs"] = sum(
        (a["id"], a["actual"]) == (b["id"], b["actual"])
        for a, b in zip(accent_rows["current"], accent_rows["bank"])
    )
    timings = (
        repeat_timings(root, binary, repeats)
        if repeats
        else read(root / "paired-timing.json")
        if (root / "paired-timing.json").exists()
        else None
    )
    if timings:
        errors.extend(timings["errors"])
        if timings["binarySha256"] != sha(binary):
            errors.append({"error": "paired timing binary mismatch"})
    environment_path = root / "environment.json"
    environment = (
        read(environment_path)
        if environment_path.exists()
        else {
            "os": platform.mac_ver()[0],
            "architecture": platform.machine(),
            "hardware": None,
            "note": "Hardware was not recorded; supply environment.json for reproducibility.",
        }
    )
    sources = [
        Path("Cargo.lock"),
        Path("scripts/stress-policy-learning.py"),
        Path("scripts/prepare-policy-learning-replay.py"),
        Path(__file__),
        *sorted(Path("crates/vesper-core/src").rglob("*.rs")),
        *sorted(Path("crates/vesper-data/src").rglob("*.rs")),
        Path("crates/vesper-data/examples/policy_learning_replay.rs"),
    ]
    result = {
        "schemaVersion": 1,
        "experiment": "bounded observed-failure replay vs existing controller; not Z.ai agent reproduction",
        "suiteLock": lock,
        "suiteLockSha256": sha(root / "manifests/locked-suite.json"),
        "binarySha256": sha(binary),
        "originalBinarySha256": reports["original"]["binarySha256"],
        "sourceHashes": {str(p): sha(p) for p in sources},
        "environment": environment,
        "validation": {
            "originalBehaviorUnchangedHistories": unchanged,
            "errors": errors,
            "isolation": isolation,
        },
        "totals": totals,
        "pairedEpisodes": paired_counts,
        "fixedPolicies": fixed,
        "scenarios": {mode: reports[mode]["scenarios"] for mode in ("current", "bank")},
        "histories": histories,
        "publicControl": public,
        "accentControl": accent,
        "pairedTiming": timings,
        "limitations": [
            "Fresh seeds from an existing authored generator, not new real-user data or independent scenario design.",
            "Original capacity control is reused.",
            "Historical feedback can be noisy or stale; bank uses frozen pre-event snapshots and is not a full future-trajectory counterfactual.",
            "Public control reuses cached ASR hypotheses with simulated corrections; no fresh acoustic inference.",
            "Word-edit totals are not normalized WER or independently judged meaning preservation.",
            "Replay/component timings exclude actual ASR, UI and insertion; no battery or thermal qualification.",
        ],
    }
    write(output, result)
    print(
        json.dumps(
            {"validationErrors": errors, "totals": totals, "pairedEpisodes": paired_counts},
            indent=2,
        )
    )
    return bool(errors)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument(
        "--binary", type=Path, default=Path("target/release/examples/policy_learning_replay")
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--timing-repeats", type=int, default=0)
    args = parser.parse_args()
    if not 0 <= args.timing_repeats <= 10:
        parser.error("--timing-repeats must be between 0 and 10")
    raise SystemExit(compare(args.directory, args.binary, args.output, args.timing_repeats))
