"""Summarize the locked scoped-policy ablation and counterbalanced timings.

Reuses report helpers from the earlier failure-bank comparison. All variants run
locally in the existing Rust example; no production database is read.
"""

import argparse
import importlib.util
import json
import subprocess
import time
from collections import Counter
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "report_helpers", Path(__file__).with_name("compare-policy-failure-bank.py")
)
helpers = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(helpers)
read, sha, write = helpers.read, helpers.sha, helpers.write
MODES = ("current", "failure_bank", "strong_global", "scoped")
FLAGS = {
    "current": [],
    "failure_bank": ["--failure-bank"],
    "strong_global": ["--strong-global"],
    "scoped": ["--scoped"],
}


def paired_rows(left, right):
    if len(left) != len(right):
        raise ValueError("Paired row count mismatch")
    result = Counter(attempted=len(left), improved=0, regressed=0, tied=0, unscored=0)
    for a, b in zip(left, right):
        if (a["id"], a["phase"], a["rawErrors"], a["baselineErrors"]) != (
            b["id"],
            b["phase"],
            b["rawErrors"],
            b["baselineErrors"],
        ):
            raise ValueError("Paired row/control identity mismatch")
        x, y = a["adaptiveErrors"], b["adaptiveErrors"]
        if x is None or y is None:
            result["unscored"] += 1
        else:
            result["improved"] += y < x
            result["regressed"] += y > x
            result["tied"] += y == x
    return result


def measure(root, binary, lock, repeats):
    selected = {}
    for history in lock["histories"]:
        if history["scenario"] != "capacity":
            selected.setdefault(history["scenario"], history)
    trials, errors = [], []
    for repeat in range(repeats):
        for index, (scenario, h) in enumerate(selected.items()):
            offset = (repeat + index) % len(MODES)
            order = MODES[offset:] + MODES[:offset]
            trial = {
                "repeat": repeat,
                "scenario": scenario,
                "file": h["file"],
                "order": order,
                "variants": {},
            }
            path = root / "manifests" / h["file"]
            if sha(path) != h["sha256"]:
                raise ValueError("Frozen timing input changed")
            for mode in order:
                destination = root / f"timing-{repeat}-{scenario}-{mode}.json"
                started = time.perf_counter()
                try:
                    subprocess.run(
                        [str(binary.resolve()), str(path), str(destination), *FLAGS[mode]],
                        check=True,
                        timeout=600,
                        capture_output=True,
                        text=True,
                    )
                    wall = (time.perf_counter() - started) * 1000
                    actual, expected = read(destination), read(root / mode / h["file"])
                    for key in (
                        "rows",
                        "decisions",
                        "scopedPolicies",
                        "failureBank",
                        "fixedPolicyComparators",
                    ):
                        if actual[key] != expected[key]:
                            raise ValueError(f"Repeat changed {key}")
                    trial["variants"][mode] = {
                        **{
                            k: actual[k]
                            for k in ("timing", "adaptiveAndBaselineReplayMs", "totalReplayMs")
                        },
                        "wallMs": wall,
                    }
                except (subprocess.SubprocessError, OSError, ValueError) as error:
                    errors.append(
                        {
                            "repeat": repeat,
                            "scenario": scenario,
                            "variant": mode,
                            "error": str(error),
                        }
                    )
            trials.append(trial)
        print(f"Timing block {repeat + 1}/{repeats} completed", flush=True)
    result = {"binarySha256": sha(binary), "trials": trials, "errors": errors}
    write(root / "repeated-timing.json", result)
    return result


def verify_previous(root, lock):
    selected = {}
    for history in lock["histories"]:
        if history["scenario"] != "capacity":
            selected.setdefault(history["scenario"], history)
    checks, errors = [], []
    for mode in ("current", "failure_bank"):
        for scenario, h in selected.items():
            destination = root / f"previous-{mode}-{h['file']}"
            try:
                subprocess.run(
                    [
                        str((root / "previous-replay").resolve()),
                        str(root / "manifests" / h["file"]),
                        str(destination),
                        *FLAGS[mode],
                    ],
                    check=True,
                    timeout=600,
                    capture_output=True,
                    text=True,
                )
                old, current = read(destination), read(root / mode / h["file"])
                equal = all(
                    old[k] == current[k]
                    for k in (
                        "rows",
                        "decisions",
                        "failureBank",
                        "allEpisodes",
                        "heldout",
                        "finalTest",
                        "fixedPolicyComparators",
                    )
                )
                checks.append(
                    {
                        "variant": mode,
                        "scenario": scenario,
                        "manifestSha256": h["sha256"],
                        "episodes": h["episodes"],
                        "equal": equal,
                    }
                )
                if not equal:
                    errors.append(
                        {
                            "variant": mode,
                            "scenario": scenario,
                            "error": "previous executable behavior changed",
                        }
                    )
            except (subprocess.SubprocessError, OSError, ValueError) as error:
                errors.append({"variant": mode, "scenario": scenario, "error": str(error)})
    result = {
        "previousBinarySha256": sha(root / "previous-replay"),
        "checks": checks,
        "errors": errors,
    }
    write(root / "previous-equivalence.json", result)
    return result


def compare(root, binary, destination, repeats=0):
    lock_path = root / "manifests/locked-suite.json"
    lock = read(lock_path)
    reports = {m: read(root / m / "summary.json") for m in MODES}
    errors = [{"variant": m, "error": e} for m, r in reports.items() for e in r["errors"]]
    for mode, report in reports.items():
        if report["suiteLockSha256"] != sha(lock_path) or report["historiesCompleted"] != len(
            lock["histories"]
        ):
            errors.append({"variant": mode, "error": "incomplete or mismatched locked run"})
        if report["binarySha256"] != sha(binary):
            errors.append({"variant": mode, "error": "binary mismatch"})
    if errors:
        write(
            destination,
            {
                "validation": {"errors": errors},
                "qualityComparisonAvailable": False,
                "runs": reports,
            },
        )
        return True
    totals = {}
    for mode, report in reports.items():
        histories = report["histories"]
        totals[mode] = {
            part: {k: sum(h[part][k] for h in histories) for k in histories[0][part]}
            for part in ("allEpisodes", "heldout", "finalTest")
        }
        totals[mode].update(
            {
                "actions": dict(sum((Counter(h["actions"]) for h in histories), Counter())),
                "routedEpisodes": sum(h["routedEpisodes"] for h in histories),
                "maxRetainedScopes": max(h["scopedPolicies"]["retained"] for h in histories),
                "scopeEvictions": sum(h["scopedPolicies"]["evicted"] for h in histories),
                "protectedChecks": sum(h["protectedChecks"] for h in histories),
                "timingOrdinary": helpers.timing_summary(
                    [h for h in histories if h["scenario"] != "capacity"]
                ),
            }
        )
    fixed = []
    for index in range(6):
        rows = [h["fixedPolicyComparators"][index] for h in reports["current"]["histories"]]
        fixed.append(
            {
                "policy": rows[0]["policy"],
                "heldout": {
                    key: sum(r["heldout"][key] for r in rows) for key in rows[0]["heldout"]
                },
            }
        )
    comparisons = {
        m: {p: Counter() for p in ("allEpisodes", "heldout", "finalTest")}
        for m in MODES
        if m != "scoped"
    }
    histories = []
    frozen_equivalent = 0 if (root / "first-run").exists() else None
    for h in lock["histories"]:
        if sha(root / "manifests" / h["file"]) != h["sha256"]:
            raise ValueError("Frozen input changed")
        data = {m: read(root / m / h["file"]) for m in MODES}
        for mode in MODES:
            if frozen_equivalent is not None:
                first = read(root / "first-run" / mode / h["file"])
                if all(
                    first[k] == data[mode][k]
                    for k in (
                        "rows",
                        "decisions",
                        "failureBank",
                        "scopedPolicies",
                        "allEpisodes",
                        "heldout",
                        "finalTest",
                        "fixedPolicyComparators",
                    )
                ):
                    frozen_equivalent += 1
                else:
                    errors.append(
                        {
                            "file": h["file"],
                            "variant": mode,
                            "error": "frozen-source rerun changed behavior",
                        }
                    )
            if data[mode]["fixedPolicyComparators"] != data["current"]["fixedPolicyComparators"]:
                errors.append(
                    {"file": h["file"], "variant": mode, "error": "fixed control mismatch"}
                )
        for mode in comparisons:
            for part, phases in [
                ("allEpisodes", None),
                ("heldout", ("audit", "final_test")),
                ("finalTest", ("final_test",)),
            ]:
                a, b = (
                    [r for r in data[m]["rows"] if phases is None or r["phase"] in phases]
                    for m in (mode, "scoped")
                )
                comparisons[mode][part].update(paired_rows(a, b))
        histories.append(
            {
                **h,
                "heldoutWordEdits": {m: data[m]["heldout"]["adaptiveWordEdits"] for m in MODES},
                "fixedContextWordEdits": data["current"]["fixedPolicyComparators"][1]["heldout"][
                    "wordEdits"
                ],
                "resultHashes": {m: sha(root / m / h["file"]) for m in MODES},
            }
        )
    isolation = {m: read(root / m / "isolation-summary.json") for m in MODES}
    for mode, checks in isolation.items():
        if not all(c["passed"] for c in checks):
            errors.append({"variant": mode, "error": "hidden-label isolation failed"})
    public = {}
    for corpus in ("public", "accent"):
        public[corpus] = {}
        for mode in MODES:
            r = read(root / f"{corpus}-{mode}.json")
            public[corpus][mode] = {
                k: r[k]
                for k in (
                    "manifestSha256",
                    "allEpisodes",
                    "heldout",
                    "decisions",
                    "scopedPolicies",
                    "adaptiveMemoryCount",
                )
            }
            if r["allEpisodes"]["failureRows"]:
                errors.append({"corpus": corpus, "variant": mode, "error": "public replay failure"})
    previous = (
        read(root / "previous-equivalence.json")
        if (root / "previous-equivalence.json").exists()
        else verify_previous(root, lock)
    )
    if previous["errors"]:
        errors.extend(previous["errors"])
    timing = (
        measure(root, binary, lock, repeats)
        if repeats
        else read(root / "repeated-timing.json")
        if (root / "repeated-timing.json").exists()
        else None
    )
    if timing:
        errors.extend(timing["errors"])
        if timing["binarySha256"] != sha(binary):
            errors.append({"error": "timing binary mismatch"})
    sources = [
        Path("Cargo.lock"),
        Path("scripts/compare-scoped-policy.py"),
        Path("scripts/stress-policy-learning.py"),
        Path("scripts/compare-policy-failure-bank.py"),
        Path("scripts/prepare-policy-learning-replay.py"),
        Path("crates/vesper-data/examples/policy_learning_replay.rs"),
        *sorted(Path("crates/vesper-data/examples/policy_learning").rglob("*.rs")),
        *sorted(Path("crates/vesper-core/src").rglob("*.rs")),
        *sorted(Path("crates/vesper-data/src").rglob("*.rs")),
    ]
    source_root = root / "frozen-source"
    recorded_sources = read(root / "frozen-source-manifest.json")
    for name, expected in recorded_sources.items():
        if sha(source_root / name) != expected:
            errors.append({"error": "evaluated source changed during benchmark", "file": name})
    source_hashes = {
        str(p): sha(source_root / p if (source_root / p).exists() else p) for p in sources
    }
    result = {
        "schemaVersion": 1,
        "experiment": "scoped eligibility-policy adaptation with fixed context fallback; offline only",
        "suiteLock": lock,
        "suiteLockSha256": sha(lock_path),
        "binarySha256": sha(binary),
        "sourceHashes": source_hashes,
        "evaluatedSourceHashes": recorded_sources,
        "sourceHashConvention": "Rust/Cargo source paths refer to frozen-source/ in the local benchmark directory; report scripts refer to the working tree.",
        "environment": read(root / "environment.json"),
        "validation": {
            "errors": errors,
            "isolation": isolation,
            "previousEquivalence": previous,
            "frozenRerunEquivalentHistories": frozen_equivalent,
        },
        "totals": totals,
        "fixedPolicies": fixed,
        "scopedVersusComparator": comparisons,
        "histories": histories,
        "scenarios": {m: reports[m]["scenarios"] for m in MODES},
        "publicControls": public,
        "repeatedTiming": timing,
        "limitations": [
            "Authored feedback and simple lexical topics; fresh seeds are not independent real-user qualification.",
            "Fixed context fallback and same-initialization global controller distinguish starting-point gains from scoping gains.",
            "No production integration, new acoustic inference, native UI latency, battery or thermal qualification.",
            "Scope changes preserve existing fresh gates but provide no historical or future non-regression guarantee.",
        ],
    }
    write(destination, result)
    print(
        json.dumps({"validationErrors": errors, "totals": totals, "fixedPolicies": fixed}, indent=2)
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
        parser.error("timing repeats must be between 0 and 10")
    raise SystemExit(compare(args.directory, args.binary, args.output, args.timing_repeats))
