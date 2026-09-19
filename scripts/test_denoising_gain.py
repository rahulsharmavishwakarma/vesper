"""Numerical contracts for an offline diagnostic, not an automatic gain policy."""

import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import numpy as np
import soundfile as sf

spec = importlib.util.spec_from_file_location(
    "gain", Path(__file__).with_name("qualify-denoising-gain.py")
)
gain = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gain)


class MatchingTest(unittest.TestCase):
    def test_scalar_only_difference_disappears_and_raw_quiet_is_unchanged(self):
        x = np.array([0.001, -0.002, 0.003, 0.0], dtype=np.float32)
        outputs, detail = gain.match({"raw": x, "scaled": x * np.float32(0.25)}, 1.0)
        np.testing.assert_array_equal(outputs["raw"], x)
        np.testing.assert_allclose(outputs["raw"], outputs["scaled"], rtol=1e-6, atol=0)
        self.assertEqual(detail["signals"]["scaled"]["gain"], 4.0)

    def test_peak_headroom_reduces_the_target_for_every_signal(self):
        raw = np.full(1000, 0.1, dtype=np.float32)
        impulse = np.zeros_like(raw)
        impulse[0] = 1
        outputs, detail = gain.match({"raw": raw, "impulse": impulse}, 10.0)
        self.assertTrue(detail["peakLimited"])
        self.assertLess(detail["commonRms"], detail["requestedRms"])
        for values in outputs.values():
            self.assertLessEqual(np.max(np.abs(values)), gain.PEAK + 1e-7)
            self.assertAlmostEqual(gain.levels(values)["rms"] / detail["commonRms"], 1, 6)
        self.assertTrue(np.array_equal(np.flatnonzero(outputs["impulse"]), [0]))

    def test_gain_bound_is_shared_without_independent_clipping(self):
        x = np.array([0.01, -0.01], dtype=np.float32)
        outputs, detail = gain.match({"raw": x, "tiny": x * np.float32(1e-6)}, 1.0)
        self.assertTrue(detail["gainLimited"])
        self.assertLessEqual(detail["signals"]["tiny"]["gain"], gain.MAX_GAIN)
        np.testing.assert_allclose(outputs["raw"], outputs["tiny"], rtol=1e-6, atol=0)

    def test_invalid_and_unmatchable_inputs_are_rejected(self):
        valid = np.array([0.1, -0.1], dtype=np.float32)
        for bad in [[], [0, 0], [1e-13, -1e-13], [np.nan, 0], [np.inf, 0], [[0.1]], [0.1]]:
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                gain.match({"raw": valid, "bad": np.array(bad, dtype=np.float32)}, 1.0)
        for multiplier in [0, -1, np.nan, np.inf]:
            with self.assertRaises(ValueError):
                gain.match({"raw": valid}, multiplier)

    def test_serialized_float_pcm_and_hash_are_reproducible(self):
        values = np.array([0.0, -0.01, 0.02, 0.003], dtype=np.float32)
        with tempfile.TemporaryDirectory() as folder:
            a, b = Path(folder) / "a.wav", Path(folder) / "b.wav"
            for p in [a, b]:
                gain.audio.write_float_wav(p, values, {p.name: 0})
                restored, rate = sf.read(p, dtype="float32")
                self.assertEqual(rate, 16000)
                np.testing.assert_array_equal(values, restored)
            self.assertEqual(gain.digest(a), gain.digest(b))

    def test_execution_uses_frozen_imports_without_altering_the_worktree(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "harness").mkdir()
            with patch.object(gain, "ROOT", root), patch.object(gain.subprocess, "run") as spawn:
                gain.run()
                expected = (root / "execution-cwd").resolve()
                self.assertEqual((expected / "scripts").resolve(), (root / "harness").resolve())
                self.assertEqual(spawn.call_args.kwargs["cwd"], expected)
                self.assertTrue(spawn.call_args.kwargs["check"])
                self.assertEqual(
                    spawn.call_args.args[0][1:], ["scripts/qualify-denoising-gain.py", "run"]
                )
                self.assertEqual(gain.execution_directory(), expected)


if __name__ == "__main__":
    unittest.main()
