"""Paired outcome accounting must retain failures and reject misaligned controls."""

import importlib.util
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "scoped_report", Path(__file__).with_name("compare-scoped-policy.py")
)
report = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(report)


def row(key, errors):
    return {
        "id": key,
        "phase": "audit",
        "rawErrors": 2,
        "baselineErrors": 2,
        "adaptiveErrors": errors,
    }


class PairedReportTests(unittest.TestCase):
    def test_all_directions_and_unscored_rows_remain_in_denominator(self):
        a = [row("a", 2), row("b", 0), row("c", 1), row("d", None)]
        b = [row("a", 1), row("b", 1), row("c", 1), row("d", 0)]
        self.assertEqual(
            report.paired_rows(a, b),
            {"attempted": 4, "improved": 1, "regressed": 1, "tied": 1, "unscored": 1},
        )

    def test_changed_row_identity_or_baseline_cannot_be_compared(self):
        for changed in (
            {"id": "different"},
            {"phase": "final_test"},
            {"rawErrors": 1},
            {"baselineErrors": 0},
        ):
            a, b = row("a", 2), row("a", 1)
            b.update(changed)
            with self.assertRaises(ValueError):
                report.paired_rows([a], [b])
        with self.assertRaises(ValueError):
            report.paired_rows([row("a", 1)], [])


if __name__ == "__main__":
    unittest.main()
