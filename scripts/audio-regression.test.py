#!/usr/bin/env python3
"""Regression tests for paired mixing and failure-sensitive benchmark analysis."""

import importlib.util
import unittest
from pathlib import Path

import numpy as np


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


prepare = load("prepare-audio-regression")
summary = load("summarize-audio-regression")


class RegressionProtocolTest(unittest.TestCase):
    def test_tail_mismatch_and_nonfinite_output_remain_reported_failures(self):
        result = summary.signal_difference(np.ones(319), np.ones(320))
        self.assertFalse(result["sameLength"])
        self.assertEqual(result["otherSamples"], 320)
        self.assertTrue(result["error"])
        self.assertTrue(summary.signal_difference(np.array([float("nan")]), np.ones(1))["error"])

    def test_retained_result_does_not_hide_empty_asr_or_rejected_writing(self):
        rows = [
            {
                "rawScore": {"wordErrorRate": {"edits": 5, "referenceUnits": 5}},
                "raw": "",
            },
            {
                "rawScore": {"wordErrorRate": {"edits": 2, "referenceUnits": 5}},
                "cleanupError": "rejected",
            },
            {
                "rawScore": {"wordErrorRate": {"edits": 1, "referenceUnits": 5}},
                "cleanedScore": {"wordErrorRate": {"edits": 0, "referenceUnits": 5}},
            },
        ]
        score = summary.retained_result_totals(rows)
        self.assertEqual(score["scoredCases"], 3)
        self.assertEqual(score["edits"], 7)
        self.assertEqual(score["referenceUnits"], 15)

    def test_si_sdr_is_scale_invariant_but_penalizes_timing_errors(self):
        rng = np.random.default_rng(3)
        speech = rng.normal(size=16000)
        noisy = speech + rng.normal(size=len(speech)) * 0.1
        before = summary.si_sdr(speech, noisy)
        self.assertAlmostEqual(before, summary.si_sdr(speech, noisy * 0.02), places=10)
        self.assertLess(summary.si_sdr(speech, np.roll(noisy, 1)), before - 20)
        self.assertIsNone(summary.si_sdr(np.zeros(4), np.ones(4)))

    def test_mixer_preserves_original_and_achieves_snr_without_clipping(self):
        rng = np.random.default_rng(11)
        speech = np.sin(np.arange(16000) * 0.05) * 0.8
        noise = rng.uniform(-1, 1, len(speech))
        variants, evidence = prepare.variants(speech, noise, 11)
        np.testing.assert_array_equal(variants["clean"], speech)
        np.testing.assert_array_equal(variants["quiet30"], speech * 0.03)
        np.testing.assert_array_equal(
            variants["matchedClean"], speech * evidence["commonHeadroomGain"]
        )
        self.assertLess(evidence["commonHeadroomGain"], 1)
        for name, expected in [("natural10", 10), ("natural0", 0), ("naturalm5", -5)]:
            self.assertAlmostEqual(evidence["measuredSnrDb"][name], expected, places=10)
            self.assertLessEqual(float(np.max(abs(variants[name]))), 0.95 + 1e-12)
        again, _ = prepare.variants(speech, noise, 11)
        np.testing.assert_array_equal(variants["drop10"], again["drop10"])

    def test_mixer_rejects_zero_energy_instead_of_nan(self):
        with self.assertRaises(ValueError):
            prepare.variants(np.zeros(16000), np.ones(16000), 0)
        with self.assertRaises(ValueError):
            prepare.variants(np.ones(16000), np.zeros(16000), 0)

    def test_micro_wer_counts_words_and_keeps_scored_denominator_visible(self):
        rows = [
            {"rawScore": {"wordErrorRate": {"edits": 1, "referenceUnits": 2}}},
            {"error": "decode failed"},
            {"rawScore": {"wordErrorRate": {"edits": 1, "referenceUnits": 8}}},
        ]
        actual = summary.word_totals(rows)
        self.assertEqual(actual["rate"], 0.2)
        self.assertEqual(actual["scoredCases"], 2)

    def test_bootstrap_is_paired_weighted_and_reproducible(self):
        blocks = [[1, 2, 10], [2, 4, 20], [3, 6, 30]]
        actual = summary.paired_bootstrap(blocks, iterations=500)
        self.assertAlmostEqual(actual["deltaWer"], 0.1)
        np.testing.assert_allclose(actual["percentile95"], [0.1, 0.1])
        self.assertEqual(actual, summary.paired_bootstrap(blocks, iterations=500))

    def test_filler_hit_requires_target_overlap_not_any_context_detection(self):
        sample = {
            "id": "test",
            "durationSeconds": 3,
            "annotation": {
                "source_start_seconds": 10,
                "label": "uh",
                "other_speakers_in_window": [],
                "anchor": {"start": 11, "end": 11.3},
                "reference_filler_events": [{"start": 11, "end": 11.3}],
            },
        }
        rows = [
            {
                "id": "test",
                "error": None,
                "spans": [{"startSeconds": 0, "endSeconds": 0.5}],
            }
        ]
        score = summary.ami_score({"samples": [sample]}, rows)[0]
        self.assertTrue(score["anyPredictedSpan"])
        self.assertEqual(score["eventsWithAnyOverlap"], 0)
        rows[0]["spans"] = [{"startSeconds": 1.1, "endSeconds": 1.4}]
        self.assertEqual(
            summary.ami_score({"samples": [sample]}, rows)[0]["eventsWithAnyOverlap"], 1
        )

    def test_missing_filler_run_is_a_failure_not_a_negative(self):
        sample = {
            "id": "absent",
            "durationSeconds": 3,
            "annotation": {
                "source_start_seconds": 0,
                "label": "ordinary",
                "other_speakers_in_window": [],
                "anchor": {"start": 1, "end": 2},
                "reference_filler_events": [],
            },
        }
        self.assertEqual(summary.ami_score({"samples": [sample]}, [])[0]["error"], "missing")


if __name__ == "__main__":
    unittest.main()
