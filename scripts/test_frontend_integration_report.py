"""Publication boundaries: preserve failures, exclude synthetic accuracy/content."""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "frontend_report", Path(__file__).with_name("summarize-frontend-integration.py")
)
reporter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reporter)


class PublicationTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.previous = reporter.ROOT, reporter.OUTPUT
        self.addCleanup(self.restore)
        reporter.ROOT = self.root
        reporter.OUTPUT = self.root / "published.json"
        (self.root / "runs").mkdir()
        self.write(
            "quality.json",
            {
                "samples": [
                    {"id": "one", "condition": "clean", "evaluationRole": "holdout"},
                    {"id": "two", "condition": "clean", "evaluationRole": "holdout"},
                    {"id": "three", "condition": "clean", "evaluationRole": "holdout"},
                ]
            },
        )
        for name in ["environment", "runtime-env", "sources", "protocol"]:
            self.write(name + ".json", {})
        score = {
            "normalizedReference": "sensitive reference",
            "normalizedHypothesis": "sensitive hypothesis",
            "wordErrorRate": {"edits": 1, "referenceUnits": 5, "rate": 0.2},
            "graphemeErrorRate": {"edits": 2, "referenceUnits": 20, "rate": 0.1},
        }
        row = {
            "sampleId": "one",
            "execution": {},
            "replayKind": "natural",
            "residentRun": True,
            "audioSeconds": 1,
            "rawHypothesis": "sensitive raw",
            "cleanedHypothesis": None,
            "personalizedHypothesis": "sensitive personalization",
            "error": "writer rejected",
            "asrMs": 2,
            "cleanupMs": 3,
            "firstPartialMs": None,
            "stopToFinalMs": None,
            "partialUpdates": 0,
            "partialEditDistance": 0,
            "mode": "offline",
            "pace": None,
            "chunkMs": None,
            "realTimeFactor": 0.002,
            "rawScore": score,
            "cleanedScore": None,
        }
        failed = {
            **row,
            "sampleId": "two",
            "rawHypothesis": None,
            "rawScore": None,
            "error": "native failure with sensitive path",
        }
        fallback = {
            **row,
            "sampleId": "three",
            "execution": {
                "audioFrontend": {
                    "originalAudioFallback": True,
                    "error": "/Users/private/error",
                }
            },
        }
        self.job("quality", [row, failed, fallback])
        self.job("soak", [{**row, "replayKind": "synthetic_soak"}])

    def restore(self):
        reporter.ROOT, reporter.OUTPUT = self.previous

    def write(self, name, value):
        (self.root / name).write_text(json.dumps(value))

    def job(self, phase, rows):
        name = phase + "-cohere-raw"
        self.write(
            "processes-" + phase + ".json",
            [
                {
                    "name": name,
                    "model": "cohere",
                    "profile": "raw",
                    "latencyUsable": False,
                    "command": ["/Users/private/command"],
                }
            ],
        )
        self.write(
            "runs/" + name + ".json",
            {"runs": rows, "modelVersions": {}, "artifactProvenance": {}},
        )

    def test_content_is_removed_and_stage_failures_remain_in_denominators(self):
        reporter.main()
        text = reporter.OUTPUT.read_text()
        self.assertNotIn("sensitive", text)
        self.assertNotIn("/Users/", text)
        output = json.loads(text)
        aggregate = output["aggregates"][0]
        self.assertEqual(aggregate["attempted"], 3)
        self.assertEqual(aggregate["asrFailures"], 1)
        self.assertEqual(aggregate["writerFailures"], 2)
        self.assertEqual(aggregate["frontendFallbacks"], 1)
        self.assertEqual(aggregate["scoredSpeech"], 2)
        self.assertEqual(aggregate["wer"], 0.2)
        self.assertEqual(aggregate["deliveredScoredSpeech"], 2)
        self.assertEqual(aggregate["deliveredWer"], 0.2)
        self.assertIsNone(aggregate["uncontendedResidentAsrMs"])
        soak = next(r for r in output["runs"] if r["phase"] == "soak")
        self.assertIsNone(soak["rawScore"])

    def test_unknown_private_path_in_provenance_blocks_publication(self):
        path = self.root / "runs/quality-cohere-raw.json"
        report = json.loads(path.read_text())
        report["modelVersions"]["unexpectedPath"] = "/Users/private/model"
        path.write_text(json.dumps(report))
        with self.assertRaises(AssertionError):
            reporter.main()
        self.assertFalse(reporter.OUTPUT.exists())


if __name__ == "__main__":
    unittest.main()
