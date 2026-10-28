#!/usr/bin/env python3
"""Regenerate the correction gate's English plausibility lexicon from pinned CMUdict.

Usage: uv run --no-project python scripts/generate-english-lexicon.py CMUDICT_PATH OUTPUT
The input must be cmusphinx/cmudict `cmudict.dict` at the pinned commit (hash checked).
Output: sorted lowercase ASCII words, front-coded as `<shared prefix 0-9><suffix>` lines.
"""

import hashlib
import re
import sys

COMMIT = "74790861f652b15e4ac49015a90074ad62a27690"
SHA256 = "81917843c7f44ce2b094ac63873c2c7a4cf802040792c455ba3ca406891c3d22"


def main(source: str, output: str) -> None:
    data = open(source, "rb").read()
    if hashlib.sha256(data).hexdigest() != SHA256:
        sys.exit(f"cmudict.dict does not match commit {COMMIT}")
    words = set()
    for line in data.decode("utf-8").splitlines():
        word = re.sub(r"\(\d+\)$", "", line.split(" ", 1)[0])
        if re.fullmatch(r"[a-z]+", word):
            words.add(word)
    lines, previous = [], ""
    for word in sorted(words):
        shared = 0
        while shared < min(len(word), len(previous), 9) and word[shared] == previous[shared]:
            shared += 1
        lines.append(f"{shared}{word[shared:]}")
        previous = word
    with open(output, "w", encoding="ascii", newline="\n") as handle:
        handle.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main(*sys.argv[1:3])
