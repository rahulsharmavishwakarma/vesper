"""Small selection/summary regression tests; no inference or dataset download."""

import importlib.util
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "accent", Path(__file__).with_name("prepare-accent-benchmark.py")
)
accent = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(accent)
SUMMARY_SPEC = importlib.util.spec_from_file_location(
    "accent_summary", Path(__file__).with_name("summarize-accent-benchmark.py")
)
summary = importlib.util.module_from_spec(SUMMARY_SPEC)
SUMMARY_SPEC.loader.exec_module(summary)


class AccentSelectionTests(unittest.TestCase):
    def row(self, key, label="Hindi", group=None, text=None, duration=5):
        return {
            "path": key + ".wav",
            "primary_language": label,
            "group": group or key,
            "text": text or key,
            "duration": duration,
        }

    def test_source_groups_and_normalized_prompts_never_cross_reserved_boundary(self):
        rows = [
            self.row("source", group="reserved"),
            self.row("prompt", text="Previous, PROMPT!"),
            self.row("fresh"),
        ]
        selected = accent.select(rows, {"reserved"}, {"previous prompt"}, 16)
        self.assertEqual([r["path"] for r in selected], ["fresh.wav"])

    def test_related_fragments_and_duplicate_prompts_do_not_inflate_cohorts(self):
        rows = [
            self.row("a", group="one"),
            self.row("b", group="one"),
            self.row("c", text="Repeated sentence"),
            self.row("d", text="Repeated sentence!"),
        ]
        selected = accent.select(rows, set(), set(), 16)
        self.assertEqual(len(selected), 2)

    def test_selection_is_order_independent_and_ignores_quality_fields(self):
        rows = [self.row(str(n), label=label) for label in ("Hindi", "Tamil") for n in range(8)]
        for n, row in enumerate(rows):
            row["path"] = row["primary_language"] + row["path"]
            row["group"] = row["path"]
            row["text"] = row["path"]
            row["wer"] = n
        expected = accent.select(rows, set(), set(), 3)
        changed = [{**r, "wer": 1000 - r["wer"]} for r in reversed(rows)]
        self.assertEqual(
            [r["path"] for r in expected],
            [r["path"] for r in accent.select(changed, set(), set(), 3)],
        )
        self.assertEqual(len(expected), 6)

    def test_duration_limits_and_small_cohorts_are_explicit(self):
        rows = [self.row(str(n), duration=d) for n, d in enumerate((3.9, 4, 20, 20.1))]
        selected = accent.select(rows, set(), set(), 16)
        self.assertEqual({r["duration"] for r in selected}, {4, 20})

    def test_failed_rows_remain_in_attempts_and_latency_but_not_scored_denominator(
        self,
    ):
        row = {
            "sampleId": "failed",
            "error": "injected native failure",
            "audioSeconds": 5,
            "rawScore": None,
            "personalizedScore": None,
            "residentRun": True,
            "asrMs": 800,
            "realTimeFactor": 0.16,
            "firstPartialMs": None,
            "stopToFinalMs": None,
            "personalizationMs": None,
            "partialUpdates": 0,
            "partialEditDistance": 0,
        }
        result = summary.aggregate([row])
        self.assertEqual(result["attempted"], 1)
        self.assertEqual(result["failures"], 1)
        self.assertEqual(result["rawWer"]["referenceUnits"], 0)
        self.assertIsNone(result["rawWer"]["percent"])
        self.assertEqual(result["timingAllAttempts"]["asrMs"]["mean"], 800)

    def test_error_rates_use_pooled_word_denominators_not_average_clip_wer(self):
        rows = [
            {"rawScore": {"wordErrorRate": {"edits": 1, "referenceUnits": 1}}},
            {"rawScore": {"wordErrorRate": {"edits": 0, "referenceUnits": 99}}},
        ]
        self.assertEqual(summary.error_rate(rows, "rawScore", "wordErrorRate")["percent"], 1)


if __name__ == "__main__":
    unittest.main()
