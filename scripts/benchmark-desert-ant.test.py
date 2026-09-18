"""Regression checks for consequential filler-span boundaries and SDK merging."""

import importlib.util
import unittest
from pathlib import Path

import numpy as np

SPEC = importlib.util.spec_from_file_location(
    "desert_ant_benchmark", Path(__file__).with_name("benchmark-desert-ant.py")
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class FillerSpanTests(unittest.TestCase):
    def test_low_confidence_and_brief_positives_do_not_become_deletions(self):
        self.assertEqual(MODULE.spans(np.array([0.6] * 50), 1.0), [])
        self.assertEqual(MODULE.spans(np.array([0.9] * 4 + [0.0] * 10), 0.28), [])

    def test_sdk_merges_short_following_run_and_uses_max_run_confidence(self):
        values = np.array([0.8] * 6 + [0.0] * 4 + [0.95] * 2)
        result = MODULE.spans(values, 0.24)
        self.assertEqual(len(result), 1)
        self.assertAlmostEqual(result[0]["endSeconds"], 0.24)
        self.assertAlmostEqual(result[0]["confidence"], 0.95)

    def test_padding_cannot_extend_span_past_audio(self):
        result = MODULE.spans(np.ones(30), 0.137)
        self.assertEqual(result[0]["endSeconds"], 0.137)
        self.assertEqual(MODULE.spans(np.ones(30), 0.1), [])

    def test_separated_runs_remain_separate(self):
        values = np.array([0.9] * 6 + [0.0] * 6 + [0.9] * 6)
        result = MODULE.spans(values, 0.36)
        self.assertEqual(len(result), 2)


if __name__ == "__main__":
    unittest.main()
