"""Regression checks for failure denominators and the publication boundary."""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "denoising_report", Path(__file__).with_name("summarize-denoising-followup.py")
)
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)


class ReportTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        previous = report.ROOT, report.OUTPUT
        self.addCleanup(lambda: self.restore(previous))
        report.ROOT, report.OUTPUT = self.root, self.root / "published.json"
        (self.root / "runs").mkdir()
        self.source = {
            "id": "one",
            "replayKind": "natural",
            "condition": "clean",
            "evaluationRole": "fresh-holdout",
        }
        self.write("quality.json", {"samples": [self.source, {**self.source, "id": "two"}]})
        self.write("streaming.json", {"samples": []})
        for name in [
            "environment",
            "runtime-env",
            "source-hashes",
            "harness-hashes",
            "protocol",
            "contracts",
        ]:
            self.write(name + ".json", {})
        self.write("preprocessing.json", {"rows": []})
        self.process = {
            "name": "quality-cohere-raw",
            "model": "cohere",
            "profile": "raw",
            "phase": "quality",
            "latencyUsable": False,
            "missingIds": [],
            "returnCode": 0,
            "nativeExceptionLog": False,
            "command": ["/Users/private/command"],
        }
        score = {
            "normalizedReference": "private reference",
            "normalizedHypothesis": "private recognition",
            "wordErrorRate": {"edits": 1, "referenceUnits": 5, "rate": 0.2},
            "graphemeErrorRate": {"edits": 1, "referenceUnits": 20, "rate": 0.05},
        }
        self.row = {
            "sampleId": "one",
            "audioSeconds": 1,
            "residentRun": True,
            "rawHypothesis": "private raw",
            "cleanedHypothesis": None,
            "personalizedHypothesis": "private personalized",
            "error": "fidelity check private error",
            "execution": {},
            "rawScore": score,
            "cleanedScore": None,
            "asrMs": 2,
            "cleanupMs": 3,
            "realTimeFactor": 0.002,
        }
        self.failed = {
            **self.row,
            "sampleId": "two",
            "rawHypothesis": None,
            "rawScore": None,
            "error": "/Users/private/native failure",
        }
        self.write("processes-quality.json", [self.process])
        self.write(
            "runs/quality-cohere-raw.json",
            {"runs": [self.row, self.failed], "modelVersions": {}, "artifactProvenance": {}},
        )

    def restore(self, previous):
        report.ROOT, report.OUTPUT = previous

    def write(self, name, value):
        (self.root / name).write_text(json.dumps(value))

    def test_failures_are_retained_and_content_is_removed(self):
        report.main()
        encoded = report.OUTPUT.read_text()
        self.assertNotIn("private", encoded)
        output = json.loads(encoded)
        a = output["aggregates"][0]
        self.assertEqual(
            (a["attempted"], a["asrFailures"], a["writerFailures"], a["rawScored"]), (2, 1, 1, 1)
        )
        self.assertEqual(a["rawWer"], 0.2)
        self.assertEqual(a["deliveredWer"], 0.2)
        self.assertFalse(output["checks"]["matrixComplete"])
        self.assertTrue(output["checks"]["missingJobs"])

    def test_synthetic_scores_are_never_natural_accuracy(self):
        sanitized = report.sanitized_row(
            self.row, self.process, {**self.source, "replayKind": "synthetic"}
        )
        self.assertIsNone(sanitized["rawScore"])
        self.assertEqual(report.aggregate([sanitized])["rawScored"], 0)

    def test_paired_interval_does_not_hide_failed_pair(self):
        baseline = [
            report.sanitized_row(r, self.process, self.source) for r in [self.row, self.failed]
        ]
        candidate_process = {**self.process, "profile": "dpdfnet4-mix50"}
        candidate = [
            report.sanitized_row({**self.row, "sampleId": i}, candidate_process, self.source)
            for i in ["one", "two"]
        ]
        comparison = report.paired(baseline + candidate)[0]
        self.assertEqual(comparison["pairedScored"], 1)
        self.assertEqual(comparison["unpairedOrFailed"], 1)
        self.assertEqual(comparison["baselineAsrFailures"], 1)
        self.assertEqual(comparison["wordErrorDifferencePoints"], 0)

    def test_failed_noise_case_is_not_counted_as_successful_silence(self):
        source = {**self.source, "reference": ""}
        row = report.sanitized_row(self.failed, self.process, source)
        aggregate = report.aggregate([row])
        self.assertEqual(aggregate["noiseAttempted"], 1)
        self.assertEqual(aggregate["noiseAsrFailures"], 1)
        self.assertEqual(aggregate["noiseCases"], 0)
        self.assertEqual(aggregate["noiseFalseTranscripts"], 0)

    def test_unknown_private_path_in_provenance_blocks_publication(self):
        path = self.root / "runs/quality-cohere-raw.json"
        value = report.read(path)
        value["modelVersions"]["unexpected"] = "/Users/private/model"
        path.write_text(json.dumps(value))
        with self.assertRaises(AssertionError):
            report.main()
        self.assertFalse(report.OUTPUT.exists())


if __name__ == "__main__":
    unittest.main()
