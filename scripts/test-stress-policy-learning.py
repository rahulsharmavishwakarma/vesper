"""Failure-retention and reproducibility tests for the offline stress harness."""

import contextlib
import importlib.util
import io
import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "stress", Path(__file__).with_name("stress-policy-learning.py")
)
stress = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(stress)


class StressHarnessTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.binary = self.root / "mock-binary"
        self.binary.write_bytes(b"mock, never executed")
        self.manifests = self.root / "manifests"
        self.manifests.mkdir()
        records = []
        for seed in range(2):
            path = self.manifests / f"sample-{seed}.json"
            manifest = stress.History(seed, "no_feedback").manifest()
            path.write_bytes(stress.encoded(manifest))
            records.append(
                {
                    "file": path.name,
                    "sha256": stress.sha(path),
                    "scenario": "no_feedback",
                    "seed": seed,
                    "episodes": len(stress.episodes(manifest)),
                }
            )
        (self.manifests / "locked-suite.json").write_bytes(stress.encoded({"histories": records}))

    def run_failure(self, effect=None, result=None):
        output = self.root / "results"
        with (
            patch.object(stress.subprocess, "run", side_effect=effect, return_value=result),
            contextlib.redirect_stdout(io.StringIO()),
        ):
            self.assertTrue(stress.run(self.manifests, self.binary, output))
        summary = json.loads((output / "summary.json").read_text())
        self.assertEqual(summary["historiesAttempted"], 2)
        self.assertEqual(summary["historiesCompleted"], 0)
        self.assertEqual(len(summary["errors"]), 2)
        return summary

    def test_nonzero_exit_retains_every_failed_history(self):
        summary = self.run_failure(
            result=subprocess.CompletedProcess([], 7, "", "injected failure")
        )
        self.assertTrue(all(e["returncode"] == 7 for e in summary["errors"]))

    def test_timeout_retains_every_failed_history(self):
        summary = self.run_failure(effect=subprocess.TimeoutExpired("mock replay", 600))
        self.assertTrue(all(e["failureKind"] == "TimeoutExpired" for e in summary["errors"]))

    def test_success_without_result_artifact_is_still_a_failure(self):
        summary = self.run_failure(result=subprocess.CompletedProcess([], 0, "", ""))
        self.assertTrue(
            all(e["failureKind"] == "invalid result artifact" for e in summary["errors"])
        )

    def test_changed_manifest_rejected_before_any_execution(self):
        with (self.manifests / "sample-0.json").open("a") as stream:
            stream.write(" ")
        with patch.object(stress.subprocess, "run") as execute:
            with self.assertRaisesRegex(ValueError, "Frozen manifest changed"):
                stress.run(self.manifests, self.binary, self.root / "results")
            execute.assert_not_called()

    def test_all_scenarios_reproduce_and_hidden_partitions_have_no_feedback(self):
        for scenario in stress.SCENARIOS:
            for seed in (0, 11):
                a = stress.History(seed, scenario).manifest()
                b = stress.History(seed, scenario).manifest()
                self.assertEqual(stress.encoded(a), stress.encoded(b))
                hidden = [e for r in a["rounds"] for e in r["audit"]] + a["finalTest"]
                self.assertTrue(all(e["edit"] is None for e in hidden))
                self.assertLessEqual(len(stress.episodes(a)), 2048)

    def test_fresh_seed_range_is_locked_before_execution(self):
        directory = self.root / "fresh"
        with contextlib.redirect_stdout(io.StringIO()):
            stress.prepare(directory, 2, 10000)
        lock = json.loads((directory / "locked-suite.json").read_text())
        ordinary = [r for r in lock["histories"] if r["scenario"] != "capacity"]
        self.assertEqual({r["seed"] for r in ordinary}, {10000, 10001})
        self.assertEqual(len(ordinary), 16)
        for row in ordinary:
            self.assertEqual(row["sha256"], stress.sha(directory / row["file"]))

    def test_failure_bank_flag_reaches_every_process_and_failures_are_retained(self):
        with (
            patch.object(
                stress.subprocess,
                "run",
                return_value=subprocess.CompletedProcess([], 7, "", "injected"),
            ) as run,
            contextlib.redirect_stdout(io.StringIO()),
        ):
            self.assertTrue(
                stress.run(self.manifests, self.binary, self.root / "bank-results", True)
            )
        self.assertEqual(run.call_count, 2)
        self.assertTrue(all(call.args[0][-1] == "--failure-bank" for call in run.call_args_list))

    def test_ablation_variants_reach_processes_and_conflicts_are_rejected(self):
        for variant, flag in [("strong_global", "--strong-global"), ("scoped", "--scoped")]:
            with (
                patch.object(
                    stress.subprocess,
                    "run",
                    return_value=subprocess.CompletedProcess([], 7, "", "injected"),
                ) as run,
                contextlib.redirect_stdout(io.StringIO()),
            ):
                self.assertTrue(
                    stress.run(self.manifests, self.binary, self.root / variant, variant=variant)
                )
            self.assertTrue(all(call.args[0][-1] == flag for call in run.call_args_list))
            summary = json.loads((self.root / variant / "summary.json").read_text())
            self.assertEqual(summary["controllerVariant"], variant)
        with self.assertRaises(ValueError):
            stress.variant_args(True, "scoped")


if __name__ == "__main__":
    unittest.main()
