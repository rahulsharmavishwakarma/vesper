#!/usr/bin/env python3
"""Construct labelled behavioral probes from a frozen public-corpus profile.

These are controlled text simulations, not naturally recorded held-out accuracy.
No network, model or personal database is used.
"""

import argparse
import copy
import hashlib
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("memory", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    data = args.memory.read_bytes()
    memory = json.loads(data)
    cases = []

    def add(category, profile, text, expected, **extra):
        cases.append(
            dict(
                id=f"{category}-{len(cases)}",
                category=category,
                profile=copy.deepcopy(profile),
                input=text,
                language=profile["vocabulary"][0]["language"] if profile["vocabulary"] else "en",
                expected=expected,
                **extra,
            )
        )

    for entry in memory["profile"]["vocabulary"]:
        p = copy.deepcopy(memory["profile"])
        p["vocabulary"], p["examples"] = [copy.deepcopy(entry)], []
        heard, intended = entry["aliases"][0], entry["phrase"]
        # Cross-punctuation aliases are deliberately ineligible in production.
        boundary = any(mark in heard for mark in [", ", "; ", ": ", "\n"])
        expected = heard if boundary else intended
        add("boundary-abstention" if boundary else "positive", p, heard, expected)
        if boundary:
            add("boundary-suggestion", p, heard, heard, reviewed=intended, expectedSuggestions=0)
        add("double-quoted", p, f'"{heard}"', f'"{heard}"')
        add("single-quoted", p, f"'{heard}'", f"'{heard}'")
        add("contraction-in-quote", p, f"'don't change {heard}'", f"'don't change {heard}'")
        add("curly-contraction-in-quote", p, f"‘don’t change {heard}’", f"‘don’t change {heard}’")
        add("escaped-quote", p, f'"say \\"hello\\" {heard}"', f'"say \\"hello\\" {heard}"')
        add("single-escaped-quote", p, f'"say \\" {heard}"', f'"say \\" {heard}"')
        add("unrelated", p, "An ordinary unrelated sentence.", "An ordinary unrelated sentence.")
        disabled = copy.deepcopy(p)
        disabled["vocabulary"][0]["enabled"] = False
        add("paused", disabled, heard, heard)
        disabled["applyCorrections"] = False
        add("disabled", disabled, heard, heard)
        other_language = copy.deepcopy(p)
        other_language["vocabulary"][0]["language"] = "fr" if entry["language"] != "fr" else "hi"
        add("language-mismatch", other_language, heard, heard)
        cases[-1]["language"] = entry["language"]
        scoped = copy.deepcopy(p)
        scoped["vocabulary"][0]["project"] = "alpha"
        scoped["activeProject"] = "beta"
        add("wrong-project", scoped, heard, heard)
        scoped["activeProject"] = "alpha"
        add("boundary-abstention" if boundary else "right-project", scoped, heard, expected)
        scoped["vocabulary"][0]["context"] = "unrelatedcontexttoken"
        add("wrong-context", scoped, heard, heard)
        conflict = copy.deepcopy(entry)
        conflict["id"], conflict["phrase"] = "conflicting-memory", "AlternateSpelling"
        p["vocabulary"].append(conflict)
        add("conflicting-memory", p, heard, heard)

    base = copy.deepcopy(memory["profile"])
    base["vocabulary"], base["examples"] = [], []
    for heard, intended in [
        ("don’t", "do"),
        ("can’t", "can"),
        ("won't", "will"),
        ("wouldn’t", "would"),
        ("UN", "U.N."),
        ("USA", "U.S.A."),
    ]:
        add("unsafe-suggestion", base, heard, heard, reviewed=intended, expectedSuggestions=0)
    phrase = "update atlas deployment"
    for i in range(2):
        base["examples"].append(
            dict(
                id=f"example-{i}",
                sessionId=f"session-{i}",
                reviewRevisionId=f"review-{i}",
                input=phrase,
                output="Update Atlas deployment.",
                language="en",
                preset="balanced",
                intent="technical",
                enabled=True,
                project="",
            )
        )
    add("intent-opt-in-off", base, phrase, phrase, expectedIntent="verbatim")
    base["useRememberedIntent"] = True
    add("intent-agreement", base, phrase, phrase, expectedIntent="technical")
    add("intent-unrelated", base, "Lunch is ready.", "Lunch is ready.", expectedIntent="verbatim")
    base["examples"][1]["intent"] = "message"
    add("intent-conflict", base, phrase, phrase, expectedIntent="verbatim")
    base["examples"][1]["intent"] = "technical"
    base["examples"][1]["sessionId"] = base["examples"][0]["sessionId"]
    add("intent-duplicate-session", base, phrase, phrase, expectedIntent="verbatim")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(
            dict(
                schemaVersion=1,
                source=dict(
                    kind="constructed_from_public_corpus_calibration_aliases",
                    profileSha256=hashlib.sha256(data).hexdigest(),
                    note="Quote wrappers, context switches, conflicts and intent labels are authored simulations, not original corpus labels.",
                ),
                cases=cases,
            ),
            ensure_ascii=False,
            indent=2,
        )
        + "\n"
    )
    print(f"Prepared {len(cases)} controlled cases")


if __name__ == "__main__":
    main()
