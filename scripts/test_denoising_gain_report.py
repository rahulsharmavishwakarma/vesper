"""Missing or failed matching attempts cannot turn into successful ASR evidence."""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "gain_report", Path(__file__).with_name("summarize-denoising-gain.py")
)
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)


class ReportingTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / "runs").mkdir()
        self.sources = [
            {
                "id": key,
                "durationSeconds": 1,
                "replayKind": "natural",
                "condition": "quiet30",
                "evaluationRole": "observed-regression",
                "reference": "private source",
            }
            for key in ["one", "two"]
        ]
        self.write("quality.json", {"samples": self.sources})
        self.write(
            "protocol.json",
            {
                "signals": report.driver.SIGNALS,
                "levels": report.driver.LEVELS,
                "expectedAttempts": 60,
                "expectedProcesses": 30,
            },
        )
        self.write("harness-hashes.json", {})
        self.write("preprocessing.json", {"rows": [], "failures": []})
        self.process = {
            "name": "quality-cohere-quiet-raw",
            "model": "cohere",
            "profile": "quiet-raw",
            "phase": "quality",
            "latencyUsable": False,
            "returnCode": 0,
            "changedInputHashes": {},
            "nativeExceptionLog": False,
        }
        self.write("processes-quality.json", [self.process])
        score = {
            "normalizedReference": "private reference",
            "normalizedHypothesis": "private hypothesis",
            "wordErrorRate": {"edits": 1, "referenceUnits": 5, "rate": 0.2},
            "graphemeErrorRate": {"edits": 1, "referenceUnits": 20, "rate": 0.05},
        }
        self.row = {
            "sampleId": "one",
            "audioSeconds": 1,
            "rawHypothesis": "private raw",
            "cleanedHypothesis": None,
            "error": "fidelity check private error",
            "execution": {},
            "rawScore": score,
            "cleanedScore": None,
            "asrMs": 1,
            "cleanupMs": 1,
            "realTimeFactor": 0.001,
        }
        self.failed = {
            **self.row,
            "sampleId": "two",
            "rawHypothesis": None,
            "rawScore": None,
            "error": "private failure",
        }
        self.native = {
            "runs": [self.row, self.failed],
            "modelVersions": {},
            "artifactProvenance": {},
        }
        self.write("runs/quality-cohere-quiet-raw.json", self.native)

    def write(self, name, value):
        (self.root / name).write_text(json.dumps(value))

    def test_missing_and_failed_attempts_remain_in_denominators(self):
        result = report.summarize(self.root)
        checks = result["checks"]
        self.assertEqual(checks["expectedAttempts"], 60)
        self.assertEqual(checks["retainedRows"], 60)
        self.assertEqual(checks["missingRows"], 58)
        self.assertEqual(checks["asrFailures"], 59)
        self.assertEqual(checks["writerFailures"], 1)
        self.assertFalse(checks["matrixComplete"])
        self.assertNotIn("private", (self.root / "debug-summary.json").read_text())
        group = next(
            r
            for r in result["aggregates"]
            if r["model"] == "cohere" and r["profile"] == "quiet-raw"
        )
        self.assertEqual(
            (group["attempted"], group["asrFailures"], group["rawWer"], group["deliveredWer"]),
            (2, 1, 0.2, 0.2),
        )

    def test_failed_pairs_are_excluded_from_scores_but_counted(self):
        left = [
            report.report.sanitized_row(r, self.process, s)
            for r, s in zip([self.row, self.failed], self.sources, strict=True)
        ]
        right = [
            report.report.sanitized_row({**self.row, "sampleId": s["id"]}, self.process, s)
            for s in self.sources
        ]
        paired = report.pairs(left, right)
        self.assertEqual(
            (paired["pairedScored"], paired["unpairedOrFailed"], paired["baselineAsrFailures"]),
            (1, 1, 1),
        )
        self.assertEqual(paired["differencePoints"], 0)

    def test_unknown_machine_path_prevents_summary(self):
        self.native["modelVersions"]["unexpected"] = "/Users/private/model"
        self.write("runs/quality-cohere-quiet-raw.json", self.native)
        with self.assertRaises(AssertionError):
            report.summarize(self.root)
        self.assertFalse((self.root / "debug-summary.json").exists())

    def test_duplicate_and_unexpected_rows_are_not_silent(self):
        self.native["runs"] += [self.row, {**self.row, "sampleId": "unexpected"}]
        self.write("runs/quality-cohere-quiet-raw.json", self.native)
        checks = report.summarize(self.root)["checks"]
        self.assertEqual((checks["duplicateRows"], checks["unexpectedRows"]), (1, 1))
        self.assertFalse(checks["matrixComplete"])

    def test_frozen_cardinality_cannot_silently_change(self):
        protocol = json.loads((self.root / "protocol.json").read_text())
        protocol["expectedAttempts"] = 720
        self.write("protocol.json", protocol)
        checks = report.summarize(self.root)["checks"]
        self.assertFalse(checks["configurationMatchesProtocol"])
        self.assertFalse(checks["matrixComplete"])

    def test_confirmation_preserves_guard_and_detects_changed_outcomes(self):
        self.process.update(executableSha256="binary", manifestSha256="manifest")
        confirmation = {**self.process, "name": "confirmation-" + self.process["name"]}
        self.process["changedInputHashes"] = {"scripts/runner.py": {"before": "a", "after": "b"}}
        self.write("processes-quality.json", [self.process])
        self.write("processes-confirmation.json", [confirmation])
        path = "runs/" + confirmation["name"] + ".json"
        self.write(path, self.native)
        result = report.summarize(self.root)
        self.assertEqual(result["checks"]["changedInputProcesses"], 1)
        self.assertFalse(result["checks"]["matrixComplete"])
        self.assertTrue(result["confirmation"]["allAffectedOutcomesConfirmed"])
        changed = {**self.row, "rawHypothesis": "different"}
        self.write(path, {**self.native, "runs": [changed, self.failed]})
        result = report.summarize(self.root)
        self.assertFalse(result["confirmation"]["allAffectedOutcomesConfirmed"])


if __name__ == "__main__":
    unittest.main()
