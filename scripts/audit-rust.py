#!/usr/bin/env python3
"""Verify the maintained GLib backport, then audit registry dependencies strictly."""

import argparse
import hashlib
import json
import subprocess
import tempfile
import tomllib
from pathlib import Path

ADVISORY = "RUSTSEC-2024-0429"
ARCHIVE_SHA256 = "233daaf6e83ae6a12a52055f568f9d7cf4671dabb78ff9560ab6da230ce00ee5"
PROVENANCE_SHA256 = "afce59aee5239ed3c48389fa4f089a39ae9efc16a0eabec16706f1aea3ebc7fb"
ROOT = Path(__file__).resolve().parents[1]
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"


def verify_source(root):
    vendor = root / "vendor/glib"
    if vendor.is_symlink() or vendor.parent.is_symlink():
        raise ValueError("Symlinks are forbidden in the verified GLib source path")
    provenance = (root / "patches/glib/provenance.json").read_bytes()
    if hashlib.sha256(provenance).hexdigest() != PROVENANCE_SHA256:
        raise ValueError("GLib provenance differs from the reviewed source manifest")
    pins = json.loads(provenance)
    if (pins["package"], pins["version"], pins["advisory"], pins["archiveSha256"]) != (
        "glib",
        "0.18.5",
        ADVISORY,
        ARCHIVE_SHA256,
    ):
        raise ValueError("Unexpected GLib provenance")
    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    if manifest["patch"]["crates-io"]["glib"] != {"path": "vendor/glib"}:
        raise ValueError("GLib must resolve through the maintained source patch")
    actual = {}
    for path in sorted(vendor.rglob("*")):
        if path.is_symlink():
            raise ValueError("Symlinks are forbidden in the verified GLib source")
        if path.is_file():
            actual[path.relative_to(vendor).as_posix()] = hashlib.sha256(
                path.read_bytes()
            ).hexdigest()
    if actual != pins["files"]:
        raise ValueError("GLib source is missing, modified or contains unexpected files")
    package = tomllib.loads((vendor / "Cargo.toml").read_text())["package"]
    if (package["name"], package["version"]) != ("glib", "0.18.5"):
        raise ValueError("Unexpected patched GLib package")
    lock = tomllib.loads((root / "Cargo.lock").read_text())
    copies = [p for p in lock["package"] if p["name"] == "glib"]
    if (
        len(copies) != 1
        or copies[0]["version"] != "0.18.5"
        or "source" in copies[0]
        or "checksum" in copies[0]
    ):
        raise ValueError("Lockfile contains an unverified or duplicate GLib package")
    regression = tomllib.loads((root / "tests/glib-backport/Cargo.lock").read_text())
    audited = {
        (p["name"], p["version"], p.get("source"), p.get("checksum")) for p in lock["package"]
    }
    for package in regression["package"]:
        if (
            "source" in package
            and (package["name"], package["version"], package["source"], package.get("checksum"))
            not in audited
        ):
            raise ValueError(
                "GLib regression dependency is absent from the audited workspace graph"
            )
    # Independent semantic assertion: editing the hash manifest cannot restore
    # the known immutable-pointer bug while retaining the maintained backport.
    source = (vendor / "src/variant_iter.rs").read_text()
    if "let mut p: *mut libc::c_char" not in source or "                &mut p," not in source:
        raise ValueError("The approved mutable out-pointer fix is absent")


def verify_resolution(root, metadata):
    copies = [p for p in metadata["packages"] if p["name"] == "glib"]
    expected = (root / "vendor/glib/Cargo.toml").resolve()
    if len(copies) != 1 or copies[0]["version"] != "0.18.5" or copies[0]["source"] is not None:
        raise ValueError("Cargo resolved an unverified or duplicate GLib package")
    if Path(copies[0]["manifest_path"]).resolve() != expected:
        raise ValueError("Cargo resolved GLib outside the verified source tree")


def registry_audit_lock(lock_text):
    """Restore only GLib's registry identity so future advisories still match.

    This is audit input, never a build lockfile. The real lock and resolved path
    must pass the source guard before using the one repaired-advisory exception.
    """
    expected = tomllib.loads(lock_text)
    sections = lock_text.split("[[package]]")
    matched = 0
    for index, section in enumerate(sections[1:], start=1):
        package = tomllib.loads(section)
        if package["name"] != "glib":
            continue
        if package["version"] != "0.18.5" or "source" in package or "checksum" in package:
            raise ValueError("Only the verified path GLib may receive the audit identity")
        matched += 1
        sections[index] = section.replace(
            'version = "0.18.5"\n',
            f'version = "0.18.5"\nsource = "{REGISTRY}"\nchecksum = "{ARCHIVE_SHA256}"\n',
            1,
        )
    if matched != 1:
        raise ValueError("Expected exactly one verified GLib package for auditing")
    package = next(p for p in expected["package"] if p["name"] == "glib")
    package.update(source=REGISTRY, checksum=ARCHIVE_SHA256)
    rendered = "[[package]]".join(sections)
    if tomllib.loads(rendered) != expected:
        raise ValueError("Audit lock must restore only GLib's registry identity")
    return rendered


def run_audit(root):
    with tempfile.TemporaryDirectory(prefix="vesper-rust-audit-") as directory:
        shadow = Path(directory) / "Cargo.lock"
        shadow.write_text(registry_audit_lock((root / "Cargo.lock").read_text()))
        subprocess.run(
            ["cargo", "audit", "--file", str(shadow), "--deny", "unsound", "--ignore", ADVISORY],
            cwd=root,
            check=True,
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify-only", action="store_true")
    args = parser.parse_args()
    verify_source(ROOT)
    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--format-version", "1"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    verify_resolution(ROOT, json.loads(result.stdout))
    print(f"Verified GLib 0.18.5 source backport for {ADVISORY}", flush=True)
    if not args.verify_only:
        run_audit(ROOT)


if __name__ == "__main__":
    main()
