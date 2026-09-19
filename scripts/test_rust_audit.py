"""Fail closed if a GLib source override would cover any unverified source."""

import copy
import importlib.util
import json
import shutil
import subprocess
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("audit_rust", ROOT / "scripts/audit-rust.py")
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


class RustAuditTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for name in ("Cargo.toml", "Cargo.lock"):
            shutil.copyfile(ROOT / name, self.root / name)
        shutil.copytree(ROOT / "vendor/glib", self.root / "vendor/glib")
        shutil.copytree(ROOT / "patches/glib", self.root / "patches/glib")
        regression = self.root / "tests/glib-backport"
        regression.mkdir(parents=True)
        shutil.copyfile(ROOT / "tests/glib-backport/Cargo.lock", regression / "Cargo.lock")

    def test_verified_source_passes(self):
        AUDIT.verify_source(self.root)

    def test_reverted_fix_fails(self):
        path = self.root / "vendor/glib/src/variant_iter.rs"
        path.write_text(path.read_text().replace("&mut p,", "&p,"))
        with self.assertRaises(ValueError):
            AUDIT.verify_source(self.root)

    def test_modified_other_source_fails(self):
        path = self.root / "vendor/glib/src/lib.rs"
        path.write_text(path.read_text() + "\n// modified\n")
        with self.assertRaises(ValueError):
            AUDIT.verify_source(self.root)

    def test_extra_missing_or_symlinked_source_fails(self):
        extra = self.root / "vendor/glib/src/extra.rs"
        extra.write_text("// unverified")
        with self.assertRaises(ValueError):
            AUDIT.verify_source(self.root)
        extra.unlink()
        source = self.root / "vendor/glib/src/lib.rs"
        source.unlink()
        with self.assertRaises(ValueError):
            AUDIT.verify_source(self.root)
        source.symlink_to(ROOT / "vendor/glib/src/lib.rs")
        with self.assertRaises(ValueError):
            AUDIT.verify_source(self.root)

    def test_registry_duplicate_fails(self):
        lock = self.root / "Cargo.lock"
        lock.write_text(
            lock.read_text()
            + '\n[[package]]\nname = "glib"\nversion = "0.18.4"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\n'
        )
        with self.assertRaises(ValueError):
            AUDIT.verify_source(self.root)

    def test_symlinked_source_root_or_parent_fails(self):
        for relative in ("vendor/glib", "vendor"):
            with self.subTest(relative=relative):
                source = self.root / relative
                moved = self.root / "moved"
                source.rename(moved)
                source.symlink_to(moved, target_is_directory=True)
                with self.assertRaises(ValueError):
                    AUDIT.verify_source(self.root)
                source.unlink()
                moved.rename(source)

    def test_regression_dependencies_must_be_audited(self):
        lock = self.root / "tests/glib-backport/Cargo.lock"
        lock.write_text(
            lock.read_text()
            + '\n[[package]]\nname = "unaudited"\nversion = "1.0.0"\nsource = "registry+crates.io"\n'
        )
        with self.assertRaises(ValueError):
            AUDIT.verify_source(self.root)

    def test_shadow_lock_changes_only_glib_registry_identity(self):
        original = (self.root / "Cargo.lock").read_text()
        expected = tomllib.loads(original)
        package = next(p for p in expected["package"] if p["name"] == "glib")
        package.update(source=AUDIT.REGISTRY, checksum=AUDIT.ARCHIVE_SHA256)
        self.assertEqual(tomllib.loads(AUDIT.registry_audit_lock(original)), expected)
        for bad in (
            original.replace('name = "glib"', 'name = "other"'),
            original + '\n[[package]]\nname = "glib"\nversion = "0.18.5"\n',
            original.replace('name = "glib"', 'name = "glib"\nsource = "registry+crates.io"'),
            original.replace('version = "0.18.5"', 'version = "0.18.5" # valid TOML'),
            original.replace('version = "0.18.5"', "version = '0.18.5'"),
            original.replace('version = "0.18.5"', 'version  =  "0.18.5"'),
        ):
            with self.assertRaises(ValueError):
                AUDIT.registry_audit_lock(bad)

    def test_audit_uses_only_repaired_advisory_exception_and_propagates_failure(self):
        original = (self.root / "Cargo.lock").read_bytes()
        observed = []

        def fail_audit(command, **kwargs):
            self.assertEqual(command[:3], ["cargo", "audit", "--file"])
            self.assertEqual(command[4:], ["--deny", "unsound", "--ignore", AUDIT.ADVISORY])
            self.assertTrue(kwargs["check"])
            shadow = Path(command[3])
            observed.append(shadow)
            package = next(
                p for p in tomllib.loads(shadow.read_text())["package"] if p["name"] == "glib"
            )
            self.assertEqual(package["source"], AUDIT.REGISTRY)
            raise subprocess.CalledProcessError(1, command)

        with patch.object(AUDIT.subprocess, "run", side_effect=fail_audit):
            with self.assertRaises(subprocess.CalledProcessError):
                AUDIT.run_audit(self.root)
        self.assertEqual((self.root / "Cargo.lock").read_bytes(), original)
        self.assertTrue(observed)
        self.assertFalse(observed[0].exists())

    @unittest.skipUnless(
        shutil.which("cargo-audit"), "cargo-audit is installed in the security job"
    )
    def test_new_glib_advisory_remains_fatal_with_real_cargo_audit(self):
        database = self.root / "advisories"
        advisories = database / "crates/glib"
        advisories.mkdir(parents=True)
        subprocess.run(["git", "init", "--quiet", str(database)], check=True)
        new_advisory = "RUSTSEC-2026-9998"
        for identifier in (AUDIT.ADVISORY, new_advisory):
            (advisories / f"{identifier}.md").write_text(
                "```toml\n[advisory]\n"
                f'id = "{identifier}"\npackage = "glib"\ndate = "2026-09-19"\n'
                'url = "https://example.invalid/synthetic-audit-regression"\n'
                "\n[versions]\npatched = []\n```\n\n"
                "# Synthetic security regression\n\nThis is not a real vulnerability report.\n"
            )
        real_run = subprocess.run
        reports = []

        def audit_offline(command, **kwargs):
            result = real_run(
                command + ["--db", str(database), "--no-fetch", "--stale", "--no-yanked", "--json"],
                cwd=kwargs["cwd"],
                capture_output=True,
                text=True,
            )
            reports.append(json.loads(result.stdout))
            if kwargs["check"]:
                result.check_returncode()
            return result

        with patch.object(AUDIT.subprocess, "run", side_effect=audit_offline):
            with self.assertRaises(subprocess.CalledProcessError):
                AUDIT.run_audit(self.root)
        found = [row["advisory"]["id"] for row in reports[0]["vulnerabilities"]["list"]]
        self.assertEqual(found, [new_advisory])

    def test_patch_redirection_fails(self):
        path = self.root / "Cargo.toml"
        path.write_text(path.read_text().replace('path = "vendor/glib"', 'path = "other/glib"'))
        with self.assertRaises(ValueError):
            AUDIT.verify_source(self.root)

    def test_metadata_resolution_and_duplicates(self):
        package = {
            "name": "glib",
            "version": "0.18.5",
            "source": None,
            "manifest_path": str(self.root / "vendor/glib/Cargo.toml"),
        }
        AUDIT.verify_resolution(self.root, {"packages": [package]})
        for bad in (
            dict(package, source="registry+crates.io"),
            dict(package, manifest_path=str(self.root / "other/Cargo.toml")),
        ):
            with self.assertRaises(ValueError):
                AUDIT.verify_resolution(self.root, {"packages": [bad]})
        with self.assertRaises(ValueError):
            AUDIT.verify_resolution(self.root, {"packages": [package, copy.copy(package)]})

    def test_changed_hash_manifest_cannot_restore_known_bug(self):
        source = self.root / "vendor/glib/src/variant_iter.rs"
        source.write_text(source.read_text().replace("&mut p,", "&p,"))
        pins_path = self.root / "patches/glib/provenance.json"
        pins = json.loads(pins_path.read_text())
        pins["files"]["src/variant_iter.rs"] = AUDIT.hashlib.sha256(source.read_bytes()).hexdigest()
        pins_path.write_text(json.dumps(pins))
        with self.assertRaises(ValueError):
            AUDIT.verify_source(self.root)


if __name__ == "__main__":
    unittest.main()
