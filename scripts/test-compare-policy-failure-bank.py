"""Checks for weighted reporting and preserving incomplete comparisons."""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "comparison", Path(__file__).with_name("compare-policy-failure-bank.py")
)
comparison = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(comparison)
PREP_SPEC = importlib.util.spec_from_file_location(
    "preparer", Path(__file__).with_name("prepare-policy-learning-replay.py")
)
preparer = importlib.util.module_from_spec(PREP_SPEC)
PREP_SPEC.loader.exec_module(preparer)


class ComparisonTests(unittest.TestCase):
    def test_public_converter_requires_explicit_size_and_hides_all_accent_targets(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "input.json"
            episodes = [
                {
                    "id": str(i),
                    "sourceKey": str(i),
                    "atMs": i + 1,
                    "project": "",
                    "language": "en",
                    "modelId": "cached-model",
                    "input": "raw",
                    "edited": "feedback",
                    "reference": "hidden",
                }
                for i in range(496)
            ]
            comparison.write(path, {"source": {"learningEpisodes": 192}, "episodes": episodes})
            with self.assertRaises(ValueError):
                preparer.public(path)
            result = preparer.public(path, 304)
            self.assertEqual(len(result["finalTest"]), 304)
            self.assertTrue(all(e["edit"] is None for e in result["finalTest"]))
            self.assertEqual(
                sum(len(r[k]) for r in result["rounds"] for k in ("learn", "develop", "validate")),
                192,
            )

    def test_timing_weights_invocations_and_does_not_average_percentiles(self):
        histories = [
            {"timing": {"apply": {"count": 1, "meanMs": 10, "p99Ms": 10}, "empty": None}},
            {"timing": {"apply": {"count": 9, "meanMs": 2, "p99Ms": 4}, "empty": None}},
        ]
        result = comparison.timing_summary(histories)
        self.assertEqual(result["apply"], {"count": 10, "meanMs": 2.8, "largestHistoryP99Ms": 10})
        self.assertIsNone(result["empty"])

    def test_incomplete_runs_keep_errors_without_producing_quality_claims(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "manifests").mkdir()
            lock = root / "manifests/locked-suite.json"
            comparison.write(lock, {"histories": [{"file": "missing.json"}]})
            for mode in ("original", "current", "bank"):
                (root / mode).mkdir()
                comparison.write(
                    root / mode / "summary.json",
                    {
                        "historiesCompleted": 0,
                        "suiteLockSha256": comparison.sha(lock),
                        "errors": [{"file": "missing.json", "failureKind": "TimeoutExpired"}],
                    },
                )
            output = root / "result.json"
            self.assertTrue(comparison.compare(root, root / "unused-binary", output))
            result = json.loads(output.read_text())
            self.assertFalse(result["qualityComparisonAvailable"])
            self.assertEqual(len(result["validation"]["errors"]), 6)
            self.assertEqual(set(result["incompleteRuns"]), {"original", "current", "bank"})


if __name__ == "__main__":
    unittest.main()
