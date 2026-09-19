"""Failure-preserving matched-RMS analysis; detailed output stays in the cache."""

import hashlib
import importlib.util
import json
from pathlib import Path

import numpy as np


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


driver = module("gain_driver", "qualify-denoising-gain.py")
report = module("denoising_report", "summarize-denoising-followup.py")
ROOT = driver.ROOT


def pairs(left, right):
    """Recording-paired descriptive intervals; failed pairs remain counted."""
    a, b = {r["sampleId"]: r for r in left}, {r["sampleId"]: r for r in right}
    valid = [
        (a[k], b[k]) for k in sorted(a.keys() & b.keys()) if a[k]["rawScore"] and b[k]["rawScore"]
    ]
    result = {
        "pairedScored": len(valid),
        "unpairedOrFailed": len(a.keys() | b.keys()) - len(valid),
        "baselineAsrFailures": sum(r["asrFailed"] for r in left),
        "candidateAsrFailures": sum(r["asrFailed"] for r in right),
    }
    if not valid:
        return result
    counts = []
    for x, y in valid:
        sx, sy = x["rawScore"]["wordErrorRate"], y["rawScore"]["wordErrorRate"]
        assert sx["referenceUnits"] == sy["referenceUnits"]
        counts.append([sx["edits"], sy["edits"], sx["referenceUnits"]])
    counts = np.asarray(counts, dtype=float)
    rng = np.random.default_rng(91926)
    draws = counts[rng.integers(0, len(valid), size=(5000, len(valid)))].sum(axis=1)
    difference = 100 * (draws[:, 1] - draws[:, 0]) / draws[:, 2]
    result.update(
        baselineWer=float(counts[:, 0].sum() / counts[:, 2].sum()),
        candidateWer=float(counts[:, 1].sum() / counts[:, 2].sum()),
        differencePoints=float(
            100 * (counts[:, 1].sum() - counts[:, 0].sum()) / counts[:, 2].sum()
        ),
        descriptive95PercentIntervalPoints=[
            float(v) for v in np.percentile(difference, [2.5, 97.5])
        ],
        improved=int(sum(y[1] < y[0] for y in counts)),
        worsened=int(sum(y[1] > y[0] for y in counts)),
        identicalTranscriptHashes=sum(x["rawSha256"] == y["rawSha256"] for x, y in valid),
    )
    return result


def missing(source, process):
    row = {
        "sampleId": source["id"],
        "audioSeconds": source["durationSeconds"],
        "rawHypothesis": None,
        "cleanedHypothesis": None,
        "error": "missing result",
        "execution": {},
        "rawScore": None,
        "cleanedScore": None,
        "asrMs": None,
        "cleanupMs": None,
        "realTimeFactor": None,
    }
    item = report.sanitized_row(row, process, source)
    item["failureKind"] = "missing_result"
    return item


def confirmations(root, main_processes, sources):
    """Retain the failed guard and compare separately recorded recovery attempts."""
    read = report.read
    ledger = root / "processes-confirmation.json"
    processes = read(ledger) if ledger.exists() else []
    expected = {"confirmation-" + p["name"]: p for p in main_processes if p["changedInputHashes"]}
    rows, comparisons = [], []
    for process in processes:
        name = process["name"]
        if name not in expected:
            raise ValueError("unexpected confirmation job")
        original = expected[name]
        before = read(root / "runs" / (original["name"] + ".json"))
        after = read(root / "runs" / (name + ".json"))
        a = {
            r["sampleId"]: report.sanitized_row(r, original, sources[r["sampleId"]])
            for r in before["runs"]
        }
        b = {
            r["sampleId"]: report.sanitized_row(r, process, sources[r["sampleId"]])
            for r in after["runs"]
        }
        keys = ["rawSha256", "cleanedSha256", "failureKind", "rawScore", "cleanedScore"]
        identical = sum(all(a[k][v] == b[k][v] for v in keys) for k in a.keys() & b.keys())
        entry = {
            "originalJob": original["name"],
            "confirmationJob": name,
            "expected": len(sources),
            "originalRows": len(a),
            "confirmationRows": len(b),
            "originalRecordedRows": len(before["runs"]),
            "confirmationRecordedRows": len(after["runs"]),
            "identicalOutcomeRows": identical,
            "sameExecutable": original["executableSha256"] == process["executableSha256"],
            "sameManifest": original["manifestSha256"] == process["manifestSha256"],
            "confirmationIntegrityPassed": process["returnCode"] == 0
            and not process["changedInputHashes"]
            and not process["nativeExceptionLog"],
        }
        entry["confirmed"] = (
            set(a) == set(b) == set(sources)
            and identical == len(sources)
            and len(before["runs"]) == len(after["runs"]) == len(sources)
            and entry["sameExecutable"]
            and entry["sameManifest"]
            and entry["confirmationIntegrityPassed"]
        )
        comparisons.append(entry)
        rows.extend(b.values())
    return {
        "expectedJobs": sorted(expected),
        "processes": processes,
        "rows": rows,
        "comparisons": comparisons,
        "allAffectedOutcomesConfirmed": bool(expected)
        and len(comparisons) == len(expected)
        and all(p["confirmed"] for p in comparisons),
    }


def summarize(root):
    read = report.read
    sources = {r["id"]: r for r in read(root / "quality.json")["samples"]}
    ledger_path = root / "processes-quality.json"
    processes = read(ledger_path) if ledger_path.exists() else []
    by_name = {p["name"]: p for p in processes}
    profiles = [f"{level}-{signal}" for level in driver.LEVELS for signal in driver.SIGNALS]
    rows, aggregates, comparisons, provenance = [], [], [], {}
    checks = {
        "missingJobs": [],
        "missingRows": 0,
        "duplicateRows": 0,
        "unexpectedRows": 0,
        "unreadableReports": 0,
    }
    for model in driver.runner.MODELS:
        for profile in profiles:
            name = f"quality-{model}-{profile}"
            process = by_name.get(
                name,
                {
                    "name": name,
                    "model": model,
                    "profile": profile,
                    "phase": "quality",
                    "latencyUsable": False,
                },
            )
            if name not in by_name:
                checks["missingJobs"].append(name)
            path = root / "runs" / (name + ".json")
            native = {}
            if path.exists():
                try:
                    native = read(path)
                except (OSError, ValueError):
                    checks["unreadableReports"] += 1
            cases = {}
            for item in native.get("runs", []):
                key = item["sampleId"]
                if key not in sources:
                    checks["unexpectedRows"] += 1
                    continue
                if key in cases:
                    checks["duplicateRows"] += 1
                    continue
                cases[key] = item
            provenance[name] = {
                k: native.get(k) for k in ["environment", "modelVersions", "artifactProvenance"]
            }
            group = []
            for sample_id, source in sources.items():
                if sample_id in cases:
                    row = report.sanitized_row(cases[sample_id], process, source)
                else:
                    row = missing(source, process)
                    checks["missingRows"] += 1
                row.update(level=profile.split("-", 1)[0], signal=profile.split("-", 1)[1])
                group.append(row)
            rows.extend(group)
            aggregates.append({"model": model, "profile": profile, **report.aggregate(group)})
    for model in driver.runner.MODELS:
        groups = {
            profile: [r for r in rows if r["model"] == model and r["profile"] == profile]
            for profile in profiles
        }
        for level in driver.LEVELS:
            arms = [(f"{level}-raw", f"{level}-{signal}") for signal in driver.SIGNALS[1:]]
            arms += [
                (f"{level}-{m}-full", f"{level}-{m}-pregain-restore")
                for m in ["dpdfnet4", "dpdfnet8"]
            ]
            for baseline, candidate in arms:
                comparisons.append(
                    {
                        "model": model,
                        "baseline": baseline,
                        "candidate": candidate,
                        **pairs(groups[baseline], groups[candidate]),
                    }
                )
        for signal in driver.SIGNALS:
            a, b = f"quiet-{signal}", f"restored-{signal}"
            comparisons.append(
                {"model": model, "baseline": a, "candidate": b, **pairs(groups[a], groups[b])}
            )
    checks.update(
        expectedAttempts=len(driver.runner.MODELS) * len(profiles) * len(sources),
        retainedRows=len(rows),
        expectedProcesses=len(driver.runner.MODELS) * len(profiles),
        recordedProcesses=len(processes),
        duplicateProcesses=len(processes) - len(by_name),
        failedProcesses=sum(p["returnCode"] != 0 for p in processes),
        changedInputProcesses=sum(bool(p["changedInputHashes"]) for p in processes),
        nativeExceptions=sum(p["nativeExceptionLog"] for p in processes),
        asrFailures=sum(r["asrFailed"] for r in rows),
        writerFailures=sum(r["writerFailed"] for r in rows),
        contendedProcesses=sum(not p["latencyUsable"] for p in processes),
        unexpectedJobs=sorted(
            set(by_name) - {f"quality-{m}-{p}" for m in driver.runner.MODELS for p in profiles}
        ),
    )
    protocol = read(root / "protocol.json")
    checks["configurationMatchesProtocol"] = (
        protocol["signals"] == driver.SIGNALS
        and protocol["levels"] == driver.LEVELS
        and protocol["expectedAttempts"] == checks["expectedAttempts"]
        and protocol["expectedProcesses"] == checks["expectedProcesses"]
    )
    harness = read(root / "harness-hashes.json")
    checks["changedHarnessPaths"] = [
        p
        for p, expected in harness.items()
        if driver.digest(root / "harness" / Path(p).name) != expected
    ]
    checks["matrixComplete"] = checks["configurationMatchesProtocol"] and not any(
        checks[k]
        for k in [
            "missingJobs",
            "missingRows",
            "duplicateRows",
            "unexpectedRows",
            "unreadableReports",
            "duplicateProcesses",
            "failedProcesses",
            "changedInputProcesses",
            "nativeExceptions",
            "unexpectedJobs",
            "changedHarnessPaths",
        ]
    )
    result = {
        "protocol": protocol,
        "checks": checks,
        "preprocessing": read(root / "preprocessing.json"),
        "processes": processes,
        "provenance": provenance,
        "aggregates": aggregates,
        "paired": comparisons,
        "rows": rows,
        "confirmation": confirmations(root, processes, sources),
        "executedHarnessHashes": harness,
        "retainedVerification": {
            name: read(root / (name + ".json"))
            for name in [
                "verification",
                "python-environment",
                "interruption",
                "source-hashes",
                "baseline-parity",
                "validation-checks",
            ]
            if (root / (name + ".json")).exists()
        },
        "phaseExecutionPins": {p.stem: read(p) for p in root.glob("execution-pins-*.json")},
        "analysisHashes": {
            "scripts/" + p.name: hashlib.sha256(p.read_bytes()).hexdigest()
            for p in [
                Path(__file__),
                Path(__file__).with_name("summarize-denoising-followup.py"),
                Path(__file__).with_name("summarize-frontend-integration.py"),
                Path(__file__).with_name("qualify-denoising-gain.py"),
                Path(__file__).with_name("run-denoising-followup.py"),
            ]
        },
    }
    encoded = json.dumps(result, indent=2, allow_nan=False) + "\n"
    assert not any(
        s in encoded
        for s in [
            "/Users/",
            "/private/",
            '"rawHypothesis"',
            '"normalizedReference"',
            '"normalizedHypothesis"',
        ]
    ), "Private content cannot enter the summary"
    (root / "debug-summary.json").write_text(encoded)
    return result


if __name__ == "__main__":
    result = summarize(ROOT)
    print(json.dumps(result["checks"], indent=2))
    for row in result["aggregates"]:
        print(
            row["model"],
            row["profile"],
            f"WER {100 * row['rawWer']:.2f}" if row["rawWer"] is not None else "unscored",
            "failed",
            row["asrFailures"],
        )
