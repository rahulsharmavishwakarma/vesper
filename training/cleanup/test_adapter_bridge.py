import json
import tempfile
import unittest
from pathlib import Path

import numpy as np
from safetensors.numpy import load_file, save_file

from adapter_bridge import MLX_PREFIX, PEFT_PREFIX, export, peft_tensors


class BridgeTests(unittest.TestCase):
    def setUp(self):
        rng = np.random.default_rng(17)
        self.rank = 3
        self.scale = 2.5
        self.a = rng.normal(0, 0.01, (3584, self.rank)).astype(np.float32)
        self.b = rng.normal(0, 0.01, (self.rank, 1024)).astype(np.float32)
        self.tensors = {f"{MLX_PREFIX}.lora_a": self.a, f"{MLX_PREFIX}.lora_b": self.b}

    def test_asymmetric_nonzero_roundtrip_preserves_orientation_and_nonunit_scale(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            save_file(self.tensors, root / "adapters.safetensors")
            (root / "adapter_config.json").write_text(
                json.dumps(
                    {
                        "fine_tune_type": "lora",
                        "num_layers": 1,
                        "lora_parameters": {
                            "keys": ["mlp.down_proj"],
                            "rank": self.rank,
                            "scale": self.scale,
                            "dropout": 0,
                        },
                    }
                )
            )
            result = export(root, root / "peft")
            self.assertFalse(result["qualified"])
            peft = load_file(root / "peft" / "adapter_model.safetensors")
            config = json.loads((root / "peft" / "adapter_config.json").read_text())
            self.assertEqual(config["lora_alpha"], self.rank * self.scale)
            for shape in [(1, 3584), (2, 4, 3584)]:
                x = np.random.default_rng(2).normal(size=shape).astype(np.float32)
                expected = self.scale * ((x @ self.a) @ self.b)
                actual = (config["lora_alpha"] / config["r"]) * (
                    (x @ peft[f"{PEFT_PREFIX}.lora_A.weight"].T)
                    @ peft[f"{PEFT_PREFIX}.lora_B.weight"].T
                )
                np.testing.assert_allclose(expected, actual, rtol=2e-5, atol=1e-6)
                self.assertGreater(float(np.max(np.abs(expected - actual / self.rank))), 1e-4)
            with self.assertRaises(FileExistsError):
                export(root, root / "peft")

    def test_wrong_shape_extra_base_parameters_missing_pairs_and_nonfinite_are_rejected(self):
        for tensors in [
            {f"{MLX_PREFIX}.lora_a": self.a.T, f"{MLX_PREFIX}.lora_b": self.b},
            {f"{MLX_PREFIX}.lora_a": self.a},
            {**self.tensors, "unapproved.weight": np.ones((2, 2), dtype=np.float32)},
            {f"{MLX_PREFIX}.lora_a": self.a * np.nan, f"{MLX_PREFIX}.lora_b": self.b},
        ]:
            with self.subTest(keys=list(tensors)), self.assertRaises(ValueError):
                peft_tensors(tensors, self.rank, self.scale)
        for rank, scale in [(True, 1), (0, 1), (65, 1), (3, float("inf")), (3, 0)]:
            with self.subTest(rank=rank, scale=scale), self.assertRaises(ValueError):
                peft_tensors(self.tensors, rank, scale)


if __name__ == "__main__":
    unittest.main()
