"""Term-eval corpus generator CLI.

  fetch   download pinned CMUdict / LibriSpeech / biasing-list inputs into the cache
  freeze  build terms.json + sentences.jsonl, write FROZEN.json (refuses to change a
          different frozen corpus unless --refreeze)
  check   verify committed files against FROZEN.json
  render  render synthetic audio + controls and write term-manifest.json (local cache only)

Run with `uv run --locked --project benchmarks/asr/python/term-eval python -I <this file> ...`.
No ASR model is run and no model output is read by any command.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))  # -I drops the script dir; add only our own directory

import build_corpus as BC  # noqa: E402

MANIFEST_DIR = HERE.parents[1] / "manifests" / "term-eval"
GENERATOR_FILES = ["build_corpus.py", "terms_data.py", "sentence_data.py", "generate.py"]
FREEZE_STATEMENT = "Generated and frozen before any model output was seen."


def default_cache_root() -> Path:
    env = os.environ.get("VESPER_BENCH_CACHE")
    return Path(env) if env else HERE.parents[3] / ".cache" / "benchmarks"


def cmudict_path(cache_root: Path) -> Path:
    return cache_root / "downloads" / "cmudict-74790861" / "cmudict.dict"


def build(cache_root: Path):
    cmu = BC.load_cmudict(str(cmudict_path(cache_root)))
    terms = BC.build_terms()
    rows, evidence = BC.build_sentences(terms, cmu)
    return terms, rows, evidence


def cmd_freeze(args) -> int:
    terms, rows, evidence = build(args.cache_root)
    terms_text = BC.serialize_terms(terms)
    sent_text = BC.serialize_sentences(rows)
    hashes = {"terms.json": BC.sha256_text(terms_text),
              "sentences.jsonl": BC.sha256_text(sent_text)}
    frozen_path = MANIFEST_DIR / "FROZEN.json"
    if frozen_path.exists():
        old = json.loads(frozen_path.read_text())
        if old["files"] == hashes:
            print("corpus identical to FROZEN.json; nothing to do")
            gen_now = generator_hashes()
            if old["generator"]["files"] != gen_now:
                print("note: generator sources changed since freeze but produce the identical corpus")
            return 0
        if not args.refreeze:
            print("refusing to overwrite a different frozen corpus (pass --refreeze):", file=sys.stderr)
            print(f"  frozen  {old['files']}\n  current {hashes}", file=sys.stderr)
            return 2
    MANIFEST_DIR.mkdir(parents=True, exist_ok=True)
    (MANIFEST_DIR / "terms.json").write_text(terms_text, encoding="utf-8")
    (MANIFEST_DIR / "sentences.jsonl").write_text(sent_text, encoding="utf-8")
    frozen = {
        "schemaVersion": 1,
        "corpus": "term-eval-2026-10-26",
        "frozenDate": dt.date.today().isoformat(),
        "statement": FREEZE_STATEMENT,
        "files": hashes,
        "generator": {"files": generator_hashes(), "seed": BC.SEED,
                      "homophoneMaxDistance": BC.HOMOPHONE_MAX_DISTANCE,
                      "homophoneDistance": "Levenshtein over CMUdict phones (stress removed) / "
                                           "phones of the term spoken form or spoken-form word",
                      "split": "sha256(id) mod 10 == 0 -> dev, else test",
                      "targets": BC.TARGETS, "protectedQuotas": BC.PROTECTED_QUOTAS},
        "cmudict": BC.CMUDICT,
        "termCounts": {**{w["id"]: len(w["terms"]) for w in terms["workspaces"]},
                       "distractors": len(terms["distractors"])},
        "counts": BC.counts(rows),
        "homophoneEvidence": evidence,
    }
    frozen_path.write_text(json.dumps(frozen, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"files": hashes, "counts": frozen["counts"]}, indent=2))
    return 0


def generator_hashes() -> dict:
    return {f: BC.sha256_bytes((HERE / f).read_bytes()) for f in GENERATOR_FILES}


def verify_frozen() -> dict:
    frozen = json.loads((MANIFEST_DIR / "FROZEN.json").read_text())
    for name, digest in frozen["files"].items():
        got = BC.sha256_bytes((MANIFEST_DIR / name).read_bytes())
        if got != digest:
            raise SystemExit(f"{name} does not match FROZEN.json ({got} != {digest})")
    return frozen


def cmd_check(args) -> int:
    frozen = verify_frozen()
    terms, rows, _ = build(args.cache_root)
    assert BC.sha256_text(BC.serialize_terms(terms)) == frozen["files"]["terms.json"]
    assert BC.sha256_text(BC.serialize_sentences(rows)) == frozen["files"]["sentences.jsonl"]
    print("ok: committed corpus matches FROZEN.json and regenerates identically")
    return 0


def cmd_fetch(args) -> int:
    import fetch
    fetch.fetch_all(args.cache_root, librispeech=args.librispeech)
    return 0


def cmd_render(args) -> int:
    frozen = verify_frozen()
    import render
    render.render_all(args.cache_root, MANIFEST_DIR, frozen, librispeech=args.librispeech,
                      force=args.force)
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--cache-root", type=Path, default=default_cache_root(),
                    help="benchmark cache root (default: $VESPER_BENCH_CACHE or <repo>/.cache/benchmarks)")
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("freeze")
    p.add_argument("--refreeze", action="store_true")
    p.set_defaults(func=cmd_freeze)
    sub.add_parser("check").set_defaults(func=cmd_check)
    p = sub.add_parser("fetch")
    p.add_argument("--librispeech", action="store_true")
    p.set_defaults(func=cmd_fetch)
    p = sub.add_parser("render")
    p.add_argument("--librispeech", action="store_true")
    p.add_argument("--force", action="store_true", help="re-render existing clips")
    p.set_defaults(func=cmd_render)
    args = ap.parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
