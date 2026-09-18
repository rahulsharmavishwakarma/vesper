"""Seeded text/feedback simulation of the offline policy replay. Python stdlib only.

Prepare freezes every manifest before scoring. Run verifies those hashes and retains
every result/failure. No audio, ASR, hosted service, or live user database is involved.
"""

import argparse
import copy
import hashlib
import json
import random
import statistics
import subprocess
import time
from collections import Counter
from pathlib import Path

SCENARIOS = (
    "stable",
    "context_shift",
    "support_shift",
    "noisy_feedback",
    "sparse_feedback",
    "correlated_retries",
    "scope_language",
    "no_feedback",
)
TOPICS = ("onboarding", "rendering", "networking", "indexing", "deployment", "packaging")
DISTRACTORS = ("linguistics", "poetry", "etymology", "storytelling", "pronunciation")
PROTECTED = (
    "Do not send 15 files on Friday",
    "Actually make it Monday instead",
    "कृपया कल बैठक का समय बताएं",
    "Keep the value at 42.5 percent",
)


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode()


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def episodes(manifest):
    return [
        e
        for r in manifest["rounds"]
        for phase in ("learn", "develop", "validate", "audit")
        for e in r[phase]
    ] + manifest["finalTest"]


class History:
    def __init__(self, seed, scenario):
        # Explicit strings avoid Python's process-randomized hash().
        self.rng = random.Random(f"policy-stress-v1:{seed}:{scenario}")
        self.seed, self.scenario = seed, scenario
        self.serial = 0
        self.time = 0
        self.used = set()

    def term(self, technical=True):
        while True:
            word = "".join(self.rng.choice("abcdefghijklmnoprstuv") for _ in range(7))
            if word not in self.used:
                self.used.add(word)
                break
        return (
            (f"{word} sdk", f"{word.title()}SDK")
            if technical
            else (word.title(), word.title() + "a")
        )

    def event(self, raw, desired, feedback=True, project="workspace", language="en"):
        self.serial += 1
        self.time += self.rng.randint(1000, 300000)
        key = f"{self.scenario}-{self.seed}-{self.serial}"
        edit = desired if feedback else None
        if self.scenario == "no_feedback" or (
            self.scenario == "sparse_feedback" and self.rng.random() < 0.8
        ):
            edit = None
        return {
            "input": {
                "id": key,
                "sourceKey": key,
                "episodeId": key,
                "atMs": self.time,
                "project": project,
                "language": language,
                "modelId": self.rng.choice(("sim-model-a", "sim-model-b", "sim-model-c")),
                "text": raw,
            },
            "edit": edit,
            "reference": desired,
        }

    def round(self, number):
        topic = self.rng.choice(TOPICS)
        # New vocabulary per round and per evaluation partition; protect against
        # the development edit itself resolving all later validation examples.
        terms = [self.term(technical=self.rng.random() < 0.65) for _ in range(12)]
        learn = []
        for alias, phrase in terms:
            for _ in range(1 if " " in alias else 2):
                e = self.event(f"Open {alias} for {topic}", f"Open {phrase} for {topic}")
                if self.scenario == "noisy_feedback" and e["edit"] and self.rng.random() < 0.25:
                    e["edit"] = e["edit"].replace(phrase, phrase + "X")
                learn.append(e)

        def use(pairs, phase):
            result = []
            for alias, phrase in pairs:
                positive = True
                query_topic = topic
                if self.scenario == "context_shift":
                    positive = number % 3 == 2
                    if not positive:
                        query_topic = self.rng.choice(DISTRACTORS)
                elif self.scenario == "support_shift":
                    positive = " " not in alias or number % 3 == 2
                elif self.scenario not in ("stable", "no_feedback"):
                    positive = self.rng.random() < 0.6
                    if not positive and self.rng.random() < 0.5:
                        query_topic = self.rng.choice(DISTRACTORS)
                project, language = "workspace", "en"
                if self.scenario == "scope_language":
                    project = self.rng.choice(("workspace", "workspace", "unrelated"))
                    language = self.rng.choice(("en", "hi", "auto"))
                    positive &= project == "workspace" and language == "en"
                raw = f"Review {alias} during {query_topic}"
                desired = raw.replace(alias, phrase) if positive else raw
                e = self.event(raw, desired, phase != "audit", project, language)
                if self.scenario == "noisy_feedback" and e["edit"] and self.rng.random() < 0.25:
                    e["edit"] = raw if positive else raw.replace(alias, phrase)
                result.append(e)
                # Same vocabulary in a protected quoted mention; target stays raw.
                quote = self.rng.choice(("'", '"', "`"))
                raw = f"Keep {quote}{alias}{quote} during {topic}"
                result.append(self.event(raw, raw, phase != "audit"))
            for raw in PROTECTED:
                result.append(self.event(raw, raw, phase != "audit"))
            if self.scenario == "correlated_retries":
                # Every six events form a transitive source/repair component.
                # The phase has two components, regardless of its raw event count.
                for n, e in enumerate(result):
                    prefix = f"retry-{number}-{phase}-{n // 6}"
                    e["input"]["sourceKey"] = f"{prefix}-source-{(n % 6) // 2}"
                    e["input"]["episodeId"] = f"{prefix}-chain-{((n % 6) + 1) // 2}"
            return result

        round_data = {
            "id": f"round-{number}",
            "learn": learn,
            "develop": use(terms[:4], "develop"),
            "validate": use(terms[4:8], "validate"),
            "audit": use(terms[8:], "audit"),
        }
        return round_data, terms, topic

    def manifest(self):
        rounds, previous = [], []
        for n in range(4):
            r, terms, topic = self.round(n)
            rounds.append(r)
            previous.extend((alias, phrase, topic) for alias, phrase in terms)
        final = []
        # Independently randomized final contexts include older memories and
        # conditions absent from the immediately preceding validation partition.
        for alias, phrase, topic in self.rng.sample(previous, 24):
            positive = self.rng.random() < 0.5
            context = topic if positive else self.rng.choice(DISTRACTORS)
            raw = f"Review {alias} during {context}"
            final.append(self.event(raw, raw.replace(alias, phrase) if positive else raw, False))
        for raw in PROTECTED:
            final.append(self.event(raw, raw, False))
        return {
            "schemaVersion": 1,
            "source": {
                "kind": "authored seeded text and externally supplied feedback simulation",
                "suiteVersion": 1,
                "scenario": self.scenario,
                "seed": self.seed,
                "limitation": "Not real user sessions, acoustic noise, accent evaluation or causal benefit. Timestamps order events; SQLite retention uses the real clock.",
            },
            "rounds": rounds,
            "finalTest": final,
        }


def capacity_manifest():
    h = History(9001, "capacity")
    learn = []
    # 420 different single observations cross the active and pending budgets.
    for _ in range(420):
        alias, phrase = h.term()
        learn.append(h.event(f"Open {alias} for onboarding", f"Open {phrase} for onboarding"))
    # Non-spelling corrections still retain bounded feedback events, crossing 1024.
    for _ in range(660):
        learn.append(h.event("Write a short report", "Please summarize the entire discussion"))
    final = [h.event(raw, raw, False) for raw in PROTECTED]
    return {
        "schemaVersion": 1,
        "source": {"kind": "capacity simulation", "scenario": "capacity", "seed": 9001},
        "rounds": [{"id": "capacity", "learn": learn, "develop": [], "validate": [], "audit": []}],
        "finalTest": final,
    }


def prepare(directory, seeds, seed_start=0):
    directory.mkdir(parents=True, exist_ok=False)
    records = []
    for scenario in SCENARIOS:
        for seed in range(seed_start, seed_start + seeds):
            manifest = History(seed, scenario).manifest()
            filename = f"{scenario}-{seed:03}.json"
            (directory / filename).write_bytes(encoded(manifest))
            records.append(
                {
                    "file": filename,
                    "sha256": sha(directory / filename),
                    "scenario": scenario,
                    "seed": seed,
                    "episodes": len(episodes(manifest)),
                }
            )
    manifest = capacity_manifest()
    (directory / "capacity.json").write_bytes(encoded(manifest))
    records.append(
        {
            "file": "capacity.json",
            "sha256": sha(directory / "capacity.json"),
            "scenario": "capacity",
            "seed": 9001,
            "episodes": len(episodes(manifest)),
        }
    )
    lock = {
        "suiteVersion": 1,
        "generatorSha256": sha(Path(__file__)),
        "seedsPerScenario": seeds,
        "seedStart": seed_start,
        "histories": records,
        "protocol": "All manifests locked before any run; all seeds scored, no seed filtering or policy tuning on final labels. This suite is development stress evidence, not a pristine deployment qualification set.",
    }
    (directory / "locked-suite.json").write_bytes(encoded(lock))
    print(
        json.dumps(
            {
                "histories": len(records),
                "episodes": sum(r["episodes"] for r in records),
                "lockSha256": sha(directory / "locked-suite.json"),
            }
        )
    )


def variant_args(failure_bank=False, variant=None):
    mode = variant or ("failure_bank" if failure_bank else "current")
    flags = {
        "current": [],
        "failure_bank": ["--failure-bank"],
        "strong_global": ["--strong-global"],
        "scoped": ["--scoped"],
    }
    if mode not in flags or (failure_bank and variant is not None):
        raise ValueError("Select exactly one supported controller variant")
    return mode, flags[mode]


def run(directory, binary, output, failure_bank=False, variant=None):
    variant, flags = variant_args(failure_bank, variant)
    lock_path = directory / "locked-suite.json"
    lock = json.loads(lock_path.read_text())
    for entry in lock["histories"]:
        if sha(directory / entry["file"]) != entry["sha256"]:
            raise ValueError(f"Frozen manifest changed: {entry['file']}")
    output.mkdir(parents=True, exist_ok=False)
    summaries, errors = [], []
    # Sequential execution avoids mixing contention from parallel policy jobs.
    for n, entry in enumerate(lock["histories"]):
        destination = output / entry["file"]
        started = time.perf_counter()
        try:
            result = subprocess.run(
                [str(binary.resolve()), str(directory / entry["file"]), str(destination)] + flags,
                capture_output=True,
                text=True,
                timeout=600,
                check=False,
            )
        except (subprocess.TimeoutExpired, OSError) as error:
            errors.append({**entry, "error": str(error), "failureKind": type(error).__name__})
            continue
        elapsed = (time.perf_counter() - started) * 1000
        if result.returncode:
            errors.append({**entry, "error": result.stderr, "returncode": result.returncode})
            (output / (entry["file"] + ".stderr")).write_text(result.stderr)
            continue
        try:
            report = json.loads(destination.read_text())
        except (OSError, ValueError) as error:
            errors.append({**entry, "error": str(error), "failureKind": "invalid result artifact"})
            continue
        manifest = json.loads((directory / entry["file"]).read_text())
        checks = []
        counts = report["allEpisodes"]
        if (
            counts["attempted"] != entry["episodes"]
            or counts["scored"] != entry["episodes"]
            or counts["failureRows"]
        ):
            checks.append("missing or failed episodes")
        if report["manifestSha256"] != entry["sha256"]:
            checks.append("replay input hash mismatch")
        if len(report["fixedPolicyComparators"]) != 6:
            checks.append("missing fixed comparator")
        if report.get("controllerVariant", "current") != variant:
            checks.append("wrong controller variant")
        if report.get("scopedPolicies", {}).get("retained", 0) > 32:
            checks.append("scoped policy budget exceeded")
        bank = report.get("failureBank", {})
        if bank.get("retained", 0) > 64 or any(r["retained"] > 64 for r in bank.get("rounds", [])):
            checks.append("failure bank bound exceeded")
        heldout = report["heldout"]
        for fixed in report["fixedPolicyComparators"]:
            c = fixed["heldout"]
            if (
                c["attempted"] != heldout["attempted"]
                or c["scored"] != heldout["scored"]
                or c["failureRows"]
            ):
                checks.append("fixed comparator missing or failed episodes")
        start_fixed = report["fixedPolicyComparators"][0]["heldout"]
        if (
            start_fixed["wordEdits"] != heldout["baselineWordEdits"]
            or start_fixed["harmfulComparedWithRaw"] != heldout["baselineHarmfulComparedWithRaw"]
        ):
            checks.append("independently replayed baseline mismatch")
        if entry["scenario"] in ("no_feedback", "correlated_retries"):
            if any(d["action"] in ("promoted", "rolled_back") for d in report["decisions"]):
                checks.append("policy changed without independent feedback")
        if entry["scenario"] == "no_feedback" and (
            report["adaptiveMemoryCount"] or report["baselineMemoryCount"]
        ):
            checks.append("silent output manufactured learning")
        if report["adaptiveMemoryCount"] > 128 or report["baselineMemoryCount"] > 128:
            checks.append("memory capacity exceeded")
        for key in ("adaptiveRetained", "baselineRetained"):
            retained = report[key]
            if (
                retained["events"] > 1024
                or retained["candidates"] > 384
                or retained["maxSupport"] > 16
            ):
                checks.append("retained evidence budget exceeded")
            if entry["scenario"] == "capacity" and (
                retained["vocabulary"] != 128
                or retained["candidates"] != 384
                or retained["events"] != 1024
            ):
                checks.append("capacity simulation did not reach all intended bounds")
        protected_checks = 0
        for e, row in zip(episodes(manifest), report["rows"]):
            i = e["input"]
            if i["id"] != row["id"]:
                checks.append("row ordering mismatch")
            protected = i["text"] in PROTECTED or i["text"].startswith("Keep ")
            wrong_scope = entry["scenario"] == "scope_language" and (
                i["language"] != "en" or i["project"] != "workspace"
            )
            if protected or wrong_scope:
                protected_checks += 1
                if row["actual"] != i["text"] or row["baseline"] != i["text"]:
                    checks.append(f"protected text or scope changed: {i['id']}")
        if checks:
            errors.append({**entry, "invariantFailures": checks})
        best = min(
            report["fixedPolicyComparators"],
            key=lambda f: (f["heldout"]["wordEdits"], f["heldout"]["harmfulComparedWithRaw"]),
        )
        summaries.append(
            {
                **entry,
                "resultSha256": sha(destination),
                "wallMs": elapsed,
                "allEpisodes": counts,
                "heldout": heldout,
                "finalTest": report["finalTest"],
                "actions": dict(Counter(d["action"] for d in report["decisions"])),
                "bestFixed": best,
                "timing": report["timing"],
                "protectedChecks": protected_checks,
                "retained": report["adaptiveRetained"],
                "memoryCount": report["adaptiveMemoryCount"],
                "failureBank": report.get("failureBank"),
                "scopedPolicies": report.get("scopedPolicies"),
                "routedEpisodes": sum(row.get("scope") is not None for row in report["rows"]),
                "fixedPolicyComparators": report["fixedPolicyComparators"],
                "adaptiveAndBaselineReplayMs": report.get("adaptiveAndBaselineReplayMs"),
                "totalReplayMs": report.get("totalReplayMs"),
            }
        )
        if (n + 1) % 8 == 0 or n + 1 == len(lock["histories"]):
            print(
                f"Completed {n + 1}/{len(lock['histories'])} histories; invariant failures={len(errors)}",
                flush=True,
            )
    grouped = {}
    for scenario in (*SCENARIOS, "capacity"):
        rows = [r for r in summaries if r["scenario"] == scenario]
        if not rows:
            continue
        aggregate = {
            partition: {k: sum(r[partition][k] for r in rows) for k in rows[0][partition]}
            for partition in ("allEpisodes", "heldout", "finalTest")
        }
        deltas = [
            r["heldout"]["adaptiveWordEdits"] - r["heldout"]["baselineWordEdits"] for r in rows
        ]
        grouped[scenario] = {
            "histories": len(rows),
            **aggregate,
            "actions": dict(sum((Counter(r["actions"]) for r in rows), Counter())),
            "pairedHeldoutDelta": {
                "min": min(deltas),
                "median": statistics.median(deltas),
                "max": max(deltas),
            },
            "bestFixedWordEdits": sum(r["bestFixed"]["heldout"]["wordEdits"] for r in rows),
            "bestFixedHarmful": sum(
                r["bestFixed"]["heldout"]["harmfulComparedWithRaw"] for r in rows
            ),
        }
    report = {
        "schemaVersion": 1,
        "experimentVersion": 4,
        "controllerVariant": variant,
        "suiteLockSha256": sha(lock_path),
        "binarySha256": sha(binary),
        "runnerSha256": sha(Path(__file__)),
        "historiesAttempted": len(lock["histories"]),
        "historiesCompleted": len(summaries),
        "errors": errors,
        "scenarios": grouped,
        "histories": summaries,
        "limitation": "Text/explicit-feedback stress evidence; synthetic histories and labels are not independent real users. Best fixed is selected post hoc separately per history. Component timings exclude SQLite snapshot reads and are not ASR or stop-to-final latency.",
    }
    (output / "summary.json").write_bytes(encoded(report))
    print(
        json.dumps({k: report[k] for k in ("historiesAttempted", "historiesCompleted", "errors")})
    )
    return bool(errors)


def verify_isolation(directory, binary, output, failure_bank=False, variant=None):
    """Rerun one seed per scenario with poisoned reporting labels and model metadata."""
    variant, flags = variant_args(failure_bank, variant)
    summary = json.loads((output / "summary.json").read_text())
    if summary["controllerVariant"] != variant:
        raise ValueError("Isolation variant differs from measured run")
    if summary["binarySha256"] != sha(binary):
        raise ValueError("Isolation check must use the binary measured in this run")
    checks = []
    for scenario in SCENARIOS:
        filename = next(r["file"] for r in summary["histories"] if r["scenario"] == scenario)
        manifest = copy.deepcopy(json.loads((directory / filename).read_text()))
        manifest["source"] = {
            "expectedDecisionSequence": ["promote this untrusted label"],
            "kind": "deliberate reporting-metadata contamination test",
        }
        for e in episodes(manifest):
            # Valid byte length but deliberately beyond the hidden word scorer's budget.
            e["reference"] = "hidden " * 513
            e["input"]["modelId"] = "different-model-metadata-only"
        changed = output / f"isolation-input-{filename}"
        destination = output / f"isolation-result-{filename}"
        changed.write_bytes(encoded(manifest))
        result = subprocess.run(
            [str(binary.resolve()), str(changed), str(destination)] + flags,
            capture_output=True,
            text=True,
            timeout=600,
            check=False,
        )
        if result.returncode:
            checks.append({"scenario": scenario, "passed": False, "error": result.stderr})
            continue
        baseline = json.loads((output / filename).read_text())
        variant = json.loads(destination.read_text())
        passed = (
            baseline["decisions"] == variant["decisions"]
            and baseline["finalPolicy"] == variant["finalPolicy"]
            and baseline.get("failureBank") == variant.get("failureBank")
            and baseline.get("scopedPolicies") == variant.get("scopedPolicies")
            and all(
                a.get("scope") == b.get("scope") for a, b in zip(baseline["rows"], variant["rows"])
            )
            and len(baseline["rows"]) == len(variant["rows"])
            and all(
                a[k] == b[k]
                for a, b in zip(baseline["rows"], variant["rows"])
                for k in ("id", "actual", "baseline", "policy")
            )
            and variant["allEpisodes"]["failureRows"] == len(variant["rows"])
            and variant["allEpisodes"]["scored"] == 0
        )
        checks.append(
            {
                "scenario": scenario,
                "episodes": len(variant["rows"]),
                "passed": passed,
                "inputSha256": sha(changed),
                "resultSha256": sha(destination),
            }
        )
    (output / "isolation-summary.json").write_bytes(encoded(checks))
    print(json.dumps(checks))
    return any(not c["passed"] for c in checks)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("prepare", "run", "verify-isolation"))
    parser.add_argument("directory", type=Path)
    parser.add_argument("--seeds", type=int, default=12)
    parser.add_argument("--seed-start", type=int, default=0)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--failure-bank", action="store_true")
    modes.add_argument("--variant", choices=("current", "failure_bank", "strong_global", "scoped"))
    parser.add_argument(
        "--binary", type=Path, default=Path("target/release/examples/policy_learning_replay")
    )
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.mode == "prepare":
        if not 1 <= args.seeds <= 100:
            parser.error("--seeds must be between 1 and 100")
        prepare(args.directory, args.seeds, args.seed_start)
    else:
        if args.output is None:
            parser.error("run requires --output (a new results directory)")
        action = run if args.mode == "run" else verify_isolation
        raise SystemExit(
            action(args.directory, args.binary, args.output, args.failure_bank, args.variant)
        )
