"""Ensure automatic-level qualification cannot hide failures or expose text."""

import contextlib
import importlib.util
import io
import json
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "automatic_level", Path(__file__).with_name("qualify-automatic-level.py")
)
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)


class ReportingTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.original_root = driver.ROOT
        self.addCleanup(setattr, driver, "ROOT", self.original_root)
        driver.ROOT = Path(self.directory.name)
        driver.write(driver.ROOT / "protocol.json", {})

    def summarize(self):
        with contextlib.redirect_stdout(io.StringIO()):
            driver.summarize()
        return driver.read(driver.ROOT / "summary.json")

    def quality_fixture(self, result):
        root = driver.ROOT / "quality"
        (root / "runs").mkdir(parents=True)
        process = {
            "name": "quality-cohere-level",
            "model": "cohere",
            "profile": "level",
            "latencyUsable": False,
            "returnCode": 0,
            "changedInputHashes": {},
        }
        source = {
            "id": "sample",
            "durationSeconds": 2,
            "reference": "PRIVATE REFERENCE",
            "replayKind": "natural",
            "condition": "quiet30",
            "evaluationRole": "fresh-holdout",
        }
        driver.write(root / "processes-quality.json", [process])
        driver.write(driver.ROOT / "quality.json", {"samples": [source]})
        if result is not None:
            driver.write(root / "runs" / "quality-cohere-level.json", {"runs": result})
        return root

    def test_absent_phases_are_missing_jobs(self):
        summary = self.summarize()
        self.assertEqual(summary["checks"]["missingJobs"], 28)

    def test_missing_result_remains_failed_attempt(self):
        self.quality_fixture(None)
        summary = self.summarize()
        self.assertEqual(summary["checks"]["missingRows"], 1)
        self.assertEqual(summary["checks"]["asrFailures"], 1)
        self.assertEqual(summary["rows"][0]["failureKind"], "missing_result")

    def test_unreadable_report_is_not_success(self):
        root = self.quality_fixture(None)
        (root / "runs" / "quality-cohere-level.json").write_text("{")
        summary = self.summarize()
        self.assertEqual(summary["checks"]["unreadableReports"], 1)
        self.assertEqual(summary["checks"]["asrFailures"], 1)

    def test_successful_soak_retry_preserves_preflight_failure(self):
        self.quality_fixture(None)
        source = driver.read(driver.ROOT / "quality.json")
        for folder, failed in [("soak-preflight", True), ("soak", False)]:
            root = driver.ROOT / folder
            (root / "runs").mkdir(parents=True)
            driver.write(driver.ROOT / (folder + ".json"), source)
            process = driver.read(driver.ROOT / "quality" / "processes-quality.json")[0]
            process.update(name="soak-nemotron-raw", model="nemotron", returnCode=int(failed))
            driver.write(root / "processes-soak.json", [process])
            if not failed:
                driver.write(
                    root / "runs" / "soak-nemotron-raw.json",
                    {
                        "runs": [
                            {
                                "sampleId": "sample",
                                "rawHypothesis": "",
                                "error": None,
                                "execution": {},
                            }
                        ]
                    },
                )
        summary = self.summarize()
        self.assertEqual(summary["checks"]["failedProcesses"], 1)
        soak_rows = {r["phase"]: r for r in summary["rows"] if r["model"] == "nemotron"}
        self.assertTrue(soak_rows["soak-preflight"]["asrFailed"])
        self.assertFalse(soak_rows["soak"]["asrFailed"])

    def test_duplicate_rows_are_flagged_and_text_is_redacted(self):
        score = {
            "normalizedReference": "PRIVATE REFERENCE",
            "normalizedHypothesis": "PRIVATE OUTPUT",
            "wordErrorRate": {"edits": 1, "referenceUnits": 2, "rate": 0.5},
            "graphemeErrorRate": {"edits": 1, "referenceUnits": 10, "rate": 0.1},
        }
        row = {
            "sampleId": "sample",
            "rawHypothesis": "PRIVATE OUTPUT",
            "cleanedHypothesis": None,
            "error": "fidelity check",
            "execution": {},
            "rawScore": score,
        }
        self.quality_fixture([row, row])
        summary = self.summarize()
        self.assertEqual(summary["checks"]["duplicateRows"], 1)
        self.assertEqual(summary["checks"]["writerFailures"], 1)
        self.assertNotIn("PRIVATE", json.dumps(summary))
        self.assertEqual(summary["aggregates"][0]["deliveredWer"], 0.5)


if __name__ == "__main__":
    unittest.main()
