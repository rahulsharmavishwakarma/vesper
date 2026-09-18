"""Ensure reporting preserves stage failures and excludes invalid soak scores."""

import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "summary", Path(__file__).with_name("summarize-whisper.py")
)
summary = importlib.util.module_from_spec(spec)
spec.loader.exec_module(summary)


class PublicationTests(unittest.TestCase):
    def test_cleanup_failure_retains_successful_asr_including_empty_text(self):
        for raw in ["decoded words", ""]:
            self.assertEqual(
                summary.failure_stage({"rawHypothesis": raw, "error": "fidelity rejected"}),
                "cleanup",
            )
        self.assertEqual(
            summary.failure_stage({"rawHypothesis": None, "error": "token limit"}),
            "asr",
        )
        self.assertEqual(summary.failure_stage({"cleanupError": "legacy probe failure"}), "cleanup")
        self.assertIsNone(summary.failure_stage({"rawHypothesis": ""}))

    def test_soak_annotation_does_not_remove_actual_noise_failures(self):
        soak = {
            "sampleId": "continuous-180s",
            "replayKind": "synthetic_soak",
            "rawNonSpeechScore": {"falseTranscript": True},
            "error": "native error",
        }
        noise = {
            "sampleId": "noise-natural-0",
            "replayKind": "natural",
            "rawNonSpeechScore": {"falseTranscript": True},
        }
        summary.exclude_unreferenced_soak_scores(soak)
        summary.exclude_unreferenced_soak_scores(noise)
        self.assertIsNone(soak["rawNonSpeechScore"])
        self.assertEqual(soak["error"], "native error")
        self.assertIn("qualityMeasurementExcluded", soak)
        self.assertTrue(noise["rawNonSpeechScore"]["falseTranscript"])

    def test_publication_strips_transcripts_and_private_paths(self):
        result = summary.sanitize(
            {
                "rawHypothesis": "private",
                "reference": "private",
                "libDir": "/private",
                "error": str(Path.cwd()) + "/model.onnx",
                "rawHypothesisSha256": "hash",
            }
        )
        self.assertEqual(result, {"error": "<workspace>/model.onnx", "rawHypothesisSha256": "hash"})


if __name__ == "__main__":
    unittest.main()
