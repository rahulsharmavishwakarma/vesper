"""Verify source patching refuses altered inputs and preserves publisher files."""

import hashlib
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import vibevoice_compat as compat


class CompatibilityPatchTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.repo = Path(self.temp.name) / "repo"
        self.root = Path(self.temp.name) / "artifacts"
        self.source = self.root / "source/example.py"
        self.source.parent.mkdir(parents=True)
        self.source.write_text("value = 4\n")
        self.patch = self.repo / "patches/vibevoice/transformers-5.patch"
        self.patch.parent.mkdir(parents=True)
        self.patch.write_text(
            "--- a/example.py\n+++ b/example.py\n@@ -1 +1 @@\n-value = 4\n+value = 5\n"
        )
        canonical = {
            "sourceRevision": "source",
            "modelRevision": "model",
            "sourceFiles": [{"file": "example.py", "sha256": compat.digest(self.source)}],
        }
        manifest = self.repo / "benchmarks/asr/vibevoice-streaming-artifacts.json"
        manifest.parent.mkdir(parents=True)
        manifest.write_text(json.dumps(canonical))
        (self.root / "verified-artifacts.json").write_text(json.dumps(canonical))
        pins = {
            "sourceRevision": "source",
            "modelRevision": "model",
            "patchSha256": compat.digest(self.patch),
            "files": {
                "example.py": {
                    "before": compat.digest(self.source),
                    "after": hashlib.sha256(b"value = 5\n").hexdigest(),
                }
            },
        }
        (self.patch.parent / "provenance.json").write_text(json.dumps(pins))
        self.override = patch.object(compat, "REPOSITORY", self.repo)
        self.override.start()
        self.addCleanup(self.override.stop)

    def test_applies_in_fresh_tree_and_keeps_original(self):
        prepared, _ = compat.prepare_source(self.root)
        self.addCleanup(prepared.cleanup)
        self.assertEqual((Path(prepared.name) / "example.py").read_text(), "value = 5\n")
        self.assertEqual(self.source.read_text(), "value = 4\n")

    def test_modified_publisher_source_rejected(self):
        self.source.write_text("value = 999\n")
        with self.assertRaises(ValueError):
            compat.prepare_source(self.root)

    def test_modified_patch_rejected(self):
        self.patch.write_text(self.patch.read_text().replace("value = 5", "value = 999"))
        with self.assertRaises(ValueError):
            compat.prepare_source(self.root)

    def test_wrong_output_hash_rejected(self):
        path = self.patch.parent / "provenance.json"
        pins = json.loads(path.read_text())
        pins["files"]["example.py"]["after"] = "0" * 64
        path.write_text(json.dumps(pins))
        with self.assertRaises(ValueError):
            compat.prepare_source(self.root)

    def test_changed_model_or_source_manifest_rejected(self):
        path = self.root / "verified-artifacts.json"
        pins = json.loads(path.read_text())
        pins["modelRevision"] = "other"
        path.write_text(json.dumps(pins))
        with self.assertRaises(ValueError):
            compat.prepare_source(self.root)


if __name__ == "__main__":
    unittest.main()
