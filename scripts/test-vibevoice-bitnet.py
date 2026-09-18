#!/usr/bin/env python3
"""Check the native benchmark's failure and raw-output reporting contract."""

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "bitnet_benchmark", Path(__file__).with_name("benchmark-vibevoice-bitnet.py")
)
benchmark = importlib.util.module_from_spec(spec)
spec.loader.exec_module(benchmark)


class NativeProtocolTests(unittest.TestCase):
    def run_native(self, script, **collect):
        with tempfile.TemporaryDirectory(prefix="bitnet-contract-") as folder:
            child = benchmark.Native(
                [sys.executable, "-c", script], Path(folder) / "native", 1024**3
            )
            try:
                return child.collect(timeout=5, **collect)
            finally:
                child.stop()

    def test_exit_zero_without_completion_is_not_success(self):
        with self.assertRaisesRegex(RuntimeError, "missing native measurement"):
            self.run_native("print('partial transcription')", require_record=True)

    def test_bracket_text_and_newlines_survive_raw_protocol(self):
        record = self.run_native(
            "import json,sys; "
            "print('VIBEASR_BENCH '+json.dumps({'rawText':'[Noise]\\nnext line',"
            "'stopReason':'eos'}),file=sys.stderr)",
            require_record=True,
        )
        self.assertEqual(record["rawText"], "[Noise]\nnext line")

    def test_cap_status_is_not_replaced_by_exit_zero(self):
        record = self.run_native(
            "import json,sys; "
            "print('VIBEASR_BENCH '+json.dumps({'rawText':'partial',"
            "'stopReason':'token-limit'}),file=sys.stderr)",
            require_record=True,
        )
        self.assertEqual(record["stopReason"], "token-limit")

    def test_response_marker_without_result_is_not_success(self):
        with self.assertRaisesRegex(RuntimeError, "missing native measurement"):
            self.run_native("print('---END---')", marker="---END---", require_record=True)

    def test_stderr_record_may_arrive_after_stdout_marker(self):
        record = self.run_native(
            "import json,sys,time; print('---END---',flush=True); time.sleep(.02); "
            "print('VIBEASR_BENCH '+json.dumps({'rawText':'complete',"
            "'stopReason':'eos'}),file=sys.stderr)",
            marker="---END---",
            require_record=True,
        )
        self.assertEqual(record["rawText"], "complete")

    def test_completed_child_has_peak_memory_evidence_in_bytes(self):
        with tempfile.TemporaryDirectory(prefix="bitnet-resources-") as folder:
            child = benchmark.Native(
                [sys.executable, "-c", "print('done')"], Path(folder) / "native", 1024**3
            )
            try:
                child.collect(timeout=5)
                evidence = child.evidence()
                self.assertEqual(evidence["exitCode"], 0)
                self.assertIsInstance(evidence["peakRssBytes"], int)
                self.assertGreater(evidence["peakRssBytes"], 0)
            finally:
                child.stop()


class ResourcePlatformTests(unittest.TestCase):
    def test_platform_options_preserve_command_arguments(self):
        command = ["/some path/native", "--context", "brackets [x] and spaces"]
        self.assertEqual(
            benchmark.time_command(command, "darwin"), ["/usr/bin/time", "-l", *command]
        )
        self.assertEqual(
            benchmark.time_command(command, "linux"),
            ["/usr/bin/time", "-f", "TRANSCRIBE_PEAK_RSS_KIB %M", *command],
        )
        self.assertEqual(command, ["/some path/native", "--context", "brackets [x] and spaces"])
        with self.assertRaisesRegex(RuntimeError, "unsupported"):
            benchmark.time_command(command, "win32")

    def test_bsd_bytes_and_gnu_kib_produce_equal_byte_measurements(self):
        self.assertEqual(
            benchmark.peak_rss_bytes("  12582912  maximum resident set size\n", "darwin"),
            12582912,
        )
        self.assertEqual(
            benchmark.peak_rss_bytes("TRANSCRIBE_PEAK_RSS_KIB 12288\n", "linux"), 12582912
        )

    def test_missing_or_malformed_measurements_are_not_fabricated(self):
        for platform in ("darwin", "linux"):
            for stderr in ("", "model completed", "TRANSCRIBE_PEAK_RSS_KIB unknown\n"):
                with self.subTest(platform=platform, stderr=stderr):
                    self.assertIsNone(benchmark.peak_rss_bytes(stderr, platform))
        self.assertIsNone(benchmark.peak_rss_bytes("TRANSCRIBE_PEAK_RSS_KIB 12\n", "darwin"))
        self.assertIsNone(benchmark.peak_rss_bytes("  12 maximum resident set size\n", "linux"))


if __name__ == "__main__":
    unittest.main()
