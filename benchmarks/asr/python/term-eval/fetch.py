"""Download pinned public inputs into the benchmark cache, verifying SHA-256.

Downloads are untrusted data: each source gets its own directory, files are only
hashed and parsed as data, and the generator runs under `python -I`.
"""

from __future__ import annotations

import hashlib
import urllib.request
from pathlib import Path

CMUDICT_REV = "74790861f652b15e4ac49015a90074ad62a27690"
FBAI_REV = "43cdb3ca9644a755d5c6cc55cbcc91132f85ed44"
_CMU = f"https://raw.githubusercontent.com/cmusphinx/cmudict/{CMUDICT_REV}"
_FBAI = f"https://raw.githubusercontent.com/facebookresearch/fbai-speech/{FBAI_REV}"

PINNED = {
    "cmudict-74790861": {
        "cmudict.dict": (f"{_CMU}/cmudict.dict",
                         "81917843c7f44ce2b094ac63873c2c7a4cf802040792c455ba3ca406891c3d22"),
        "LICENSE": (f"{_CMU}/LICENSE",
                    "bd4ce8e44170a5f9f481310ca85c51de3c4f851a65e679b40e603b143bd3542a"),
    },
    "fbai-speech-43cdb3ca": {
        "LICENSE": (f"{_FBAI}/LICENSE",
                    "52412d7bc7ce4157ea628bbaacb8829e0a9cb3c58f57f99176126bc8cf2bfc85"),
        "README.md": (f"{_FBAI}/is21_deep_bias/README.md",
                      "277917b6e1bc923eafa138188470b6b0fe0f010bf29bfd53b2bf633c0d7abc49"),
        "test-clean.biasing_100.tsv": (f"{_FBAI}/is21_deep_bias/ref/test-clean.biasing_100.tsv",
                                       "77f0be388c4f5821c71ee7cab65d1141b40b606e70053c18f0b11346a229d8e2"),
        "all_rare_words.txt": (f"{_FBAI}/is21_deep_bias/words/all_rare_words.txt",
                               "31073783030c105da0d666ff43ac3a243359478e7df55d91edf92f1afb90245d"),
    },
    "librispeech-test-clean": {
        "test-clean.tar.gz": ("https://www.openslr.org/resources/12/test-clean.tar.gz",
                              "39fde525e59672dc6d1551919b1478f724438a95aa55f874b576be21967e6c23"),
    },
}


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def pinned_path(cache_root: Path, group: str, name: str) -> Path:
    path = cache_root / "downloads" / group / name
    expected = PINNED[group][name][1]
    if not path.exists():
        raise SystemExit(f"missing {group}/{name}; run `generate.py fetch`")
    got = sha256_file(path)
    if got != expected:
        raise SystemExit(f"sha256 mismatch for {group}/{name}: {got}")
    return path


def fetch_all(cache_root: Path, librispeech: bool) -> None:
    for group, files in PINNED.items():
        if group.startswith("librispeech") and not librispeech:
            continue
        d = cache_root / "downloads" / group
        d.mkdir(parents=True, exist_ok=True)
        for name, (url, expected) in files.items():
            dest = d / name
            if dest.exists() and sha256_file(dest) == expected:
                print(f"ok   {group}/{name}")
                continue
            if dest.exists():
                raise SystemExit(f"{dest} exists with unexpected content; move it aside")
            tmp = dest.with_suffix(dest.suffix + ".part")
            print(f"get  {url}")
            with urllib.request.urlopen(url) as r, open(tmp, "wb") as f:
                while chunk := r.read(1 << 20):
                    f.write(chunk)
            got = sha256_file(tmp)
            if got != expected:
                tmp.unlink()
                raise SystemExit(f"sha256 mismatch for {url}: {got}")
            tmp.rename(dest)
            print(f"ok   {group}/{name}")
