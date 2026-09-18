#!/usr/bin/env python3
"""Run every existing Python test naming convention in isolated processes."""

import subprocess
import sys
from pathlib import Path


def main():
    directory = Path(__file__).resolve().parent
    paths = sorted(
        {
            path
            for pattern in ("test-*.py", "test_*.py", "*.test.py")
            for path in directory.glob(pattern)
        }
    )
    if not paths:
        raise SystemExit("No Python test suites found")
    failed = []
    for path in paths:
        print(f"Running {path.name}", flush=True)
        result = subprocess.run([sys.executable, str(path)], cwd=directory.parent)
        if result.returncode:
            failed.append(path.name)
    print(f"Python suites: {len(paths) - len(failed)} passed, {len(failed)} failed")
    if failed:
        raise SystemExit("Failed: " + ", ".join(failed))


if __name__ == "__main__":
    main()
