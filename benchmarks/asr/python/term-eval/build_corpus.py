"""Deterministic text corpus builder (terms + sentences) for term-eval.

Pure function of the data modules and the pinned CMUdict file. No model is run
and no model output is read.
"""

from __future__ import annotations

import hashlib
import itertools
import json
import random
import re
from collections import Counter

import sentence_data as SD
import terms_data as TD

SEED = "vesper-term-eval-2026-10-26"
HOMOPHONE_MAX_DISTANCE = 0.34
TARGETS = {"homophone": 1000, "subword": 900, "ordinary": 550, "protected": 700}
PROTECTED_QUOTAS = {"weekday": 140, "month": 130, "number": 130, "time": 110,
                    "money": 100, "negation": 90}
DEV_MODULUS = 10  # sha256(id) % 10 == 0 -> dev (~10%)

CMUDICT = {
    "url": "https://raw.githubusercontent.com/cmusphinx/cmudict/"
           "74790861f652b15e4ac49015a90074ad62a27690/cmudict.dict",
    "repository": "https://github.com/cmusphinx/cmudict",
    "revision": "74790861f652b15e4ac49015a90074ad62a27690",
    "sha256": "81917843c7f44ce2b094ac63873c2c7a4cf802040792c455ba3ca406891c3d22",
    "license": "BSD-2-Clause-style (CMU); LICENSE sha256 "
               "bd4ce8e44170a5f9f481310ca85c51de3c4f851a65e679b40e603b143bd3542a",
}


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_text(text: str) -> str:
    return sha256_bytes(text.encode("utf-8"))


def split_for(sample_id: str) -> str:
    return "dev" if int(sha256_text(sample_id)[:8], 16) % DEV_MODULUS == 0 else "test"


def slug(text: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")


# ------------------------------------------------------------------ terms
def build_terms() -> dict:
    workspaces = []
    for wid, label, rows in TD.WORKSPACES:
        terms = [term_entry(f"{wid}-{slug(sp)}", sp, kind, spoken) for sp, kind, spoken in rows]
        workspaces.append({"id": wid, "label": label, "terms": terms})
    distractors = [term_entry(f"distractor-{slug(sp)}", sp, kind, spoken)
                   for sp, kind, spoken in TD.DISTRACTORS]
    doc = {"schemaVersion": 1,
           "note": "Synthetic evaluation terms; not user data. Distractors pad hint lists only "
                   "and never occur in any sentence.",
           "workspaces": workspaces, "distractors": distractors}
    validate_terms(doc)
    return doc


def term_entry(tid, spelling, kind, spoken):
    return {"id": tid, "spelling": spelling, "spoken": list(spoken), "kind": kind}


def all_terms(doc):
    for ws in doc["workspaces"]:
        for t in ws["terms"]:
            yield ws["id"], t
    for t in doc["distractors"]:
        yield None, t


def validate_terms(doc):
    ids, spellings = set(), set()
    for ws in doc["workspaces"]:
        assert len(ws["terms"]) == 30, (ws["id"], len(ws["terms"]))
    assert len(doc["distractors"]) == 60, len(doc["distractors"])
    for _, t in all_terms(doc):
        sp = t["spelling"]
        assert t["id"] not in ids, t["id"]
        ids.add(t["id"])
        assert sp.lower() not in spellings, sp
        spellings.add(sp.lower())
        assert t["kind"] in {"name", "identifier", "acronym", "phrase"}, t
        assert len(sp.encode()) <= 128 and sp == sp.strip(), sp
        assert not re.search(r"[,$\x00-\x1f\x7f]|<\|", sp), sp
        assert t["spoken"], sp
        for form in t["spoken"]:
            assert re.fullmatch(r"[a-z0-9]+( [a-z0-9]+)*", form), (sp, form)


# ---------------------------------------------------------------- cmudict
def load_cmudict(path: str) -> dict[str, list[tuple[str, ...]]]:
    raw = open(path, "rb").read()
    got = sha256_bytes(raw)
    if got != CMUDICT["sha256"]:
        raise SystemExit(f"CMUdict sha256 mismatch: {got}")
    out: dict[str, list[tuple[str, ...]]] = {}
    for line in raw.decode("utf-8").splitlines():
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        word, *phones = line.split()
        word = re.sub(r"\(\d+\)$", "", word)
        out.setdefault(word, []).append(tuple(re.sub(r"\d", "", p) for p in phones))
    return out


def phrase_phones(cmu, text: str):
    variants = [()]
    for word in re.findall(r"[a-z']+", text.lower()):
        prons = cmu.get(word)
        if prons is None:
            return None
        variants = [v + p for v in variants for p in prons][:256]
    return variants


def levenshtein(a, b) -> int:
    prev = list(range(len(b) + 1))
    for i, x in enumerate(a, 1):
        cur = [i]
        for j, y in enumerate(b, 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (x != y)))
        prev = cur
    return prev[-1]


def term_targets(cmu, term):
    """Spoken forms plus their component words (>=3 phones, not single letters)."""
    targets = []
    for form in term["spoken"]:
        p = phrase_phones(cmu, form)
        if p:
            targets.append((form, p))
        words = form.split()
        if len(words) > 1:
            for w in words:
                pw = phrase_phones(cmu, w)
                if len(w) > 1 and pw and min(len(x) for x in pw) >= 3:
                    targets.append((w, pw))
    return targets


def homophone_distance(cmu, term, phrase):
    cand = phrase_phones(cmu, phrase)
    if cand is None:
        raise SystemExit(f"homophone phrase not in CMUdict: {phrase!r}")
    best = None
    for label, tps in term_targets(cmu, term):
        for tp in tps:
            for cp in cand:
                d = levenshtein(tp, cp) / len(tp)
                if best is None or d < best[0]:
                    best = (round(d, 4), label, " ".join(tp), " ".join(cp))
    return best


# --------------------------------------------------------------- sentences
def term_pattern(spelling: str) -> re.Pattern:
    return re.compile(r"(?<![\w])" + re.escape(spelling) + r"(?![\w])", re.IGNORECASE)


def contains_phrase(text: str, phrase: str) -> bool:
    return term_pattern(phrase).search(text) is not None


def cap(text: str) -> str:
    return text[:1].upper() + text[1:]


def expand(clause: str) -> list[str]:
    """All carrier/subject variants of one authored clause."""
    if clause.startswith("="):
        return [clause[1:]]
    if clause.startswith("V:"):
        vp = clause[2:]
        return [c.replace("{C}", cap(vp)).replace("{c}", vp) for c in SD.VP_CARRIERS]
    bodies = [clause.replace("{S}", s) for s in SD.SUBJECTS] if "{S}" in clause else [clause]
    return [c.replace("{C}", cap(b)).replace("{c}", b) for b in bodies for c in SD.DECL_CARRIERS]


def rng_for(*parts) -> random.Random:
    return random.Random(int(sha256_text("|".join([SEED, *map(str, parts)]))[:16], 16))


def pick_round_robin(items, target, used, label):
    """items: list of (meta, variants). Takes one new variant per item per round."""
    queues = []
    for idx, (meta, variants) in enumerate(items):
        v = list(variants)
        rng_for(label, idx, variants[0]).shuffle(v)
        queues.append([meta, v, 0])
    chosen = []
    while len(chosen) < target:
        progressed = False
        for q in queues:
            meta, v, pos = q
            while pos < len(v) and v[pos].lower() in used:
                pos += 1
            if pos < len(v):
                used.add(v[pos].lower())
                chosen.append((meta, v[pos]))
                pos += 1
                progressed = True
            q[2] = pos
            if len(chosen) >= target:
                break
        if not progressed:
            raise SystemExit(f"{label}: only {len(chosen)} unique sentences, target {target}")
    return chosen


def protected_items():
    items = []
    for kind, templates in SD.PROTECTED_TEMPLATES.items():
        for tmpl in templates:
            slots = re.findall(r"\{(day|month|ordinal|num|letter|ext|time|money|v)\}", tmpl)
            pools = {"day": SD.WEEKDAYS, "month": SD.MONTHS, "ordinal": SD.ORDINALS,
                     "num": SD.NUMBERS, "letter": SD.LETTERS, "ext": SD.EXTENSIONS,
                     "time": SD.TIMES, "money": SD.MONEY, "v": SD.NEGATION_VPS}
            names = sorted(set(slots))
            for combo in itertools.product(*(pools[n] for n in names)):
                filled = tmpl
                for n, val in zip(names, combo):
                    filled = filled.replace("{" + n + "}", val)
                items.append((kind, filled))
    return items


def build_sentences(terms_doc: dict, cmu) -> tuple[list[dict], list[dict]]:
    by_spelling = {}
    ws_of = {}
    for wid, t in all_terms(terms_doc):
        by_spelling[t["spelling"]] = t
        ws_of[t["spelling"]] = wid
    patterns = [(t["spelling"], term_pattern(t["spelling"])) for _, t in all_terms(terms_doc)]
    distractor_spellings = {t["spelling"] for t in terms_doc["distractors"]}

    def found_terms(text):
        return {sp for sp, pat in patterns if pat.search(text)}

    rows: list[dict] = []
    used: set[str] = set()

    # positives
    for wid, per_term in SD.POSITIVES.items():
        ws_terms = {t["spelling"] for t in next(w for w in terms_doc["workspaces"] if w["id"] == wid)["terms"]}
        assert set(per_term) == ws_terms, (wid, set(per_term) ^ ws_terms)
        for spelling, templates in per_term.items():
            assert len(templates) == 2, spelling
            for n, tmpl in enumerate(templates, 1):
                marked = re.findall(r"\{([^{}]+)\}", tmpl)
                assert 1 <= len(marked) <= 2 and spelling in marked, tmpl
                assert all(ws_of[m] == wid for m in marked), tmpl
                ref = re.sub(r"\{([^{}]+)\}", lambda m: m.group(1), tmpl)
                tts = cap(re.sub(r"\{([^{}]+)\}", lambda m: by_spelling[m.group(1)]["spoken"][0], tmpl))
                assert found_terms(ref) == set(marked), (ref, found_terms(ref))
                assert ref.lower() not in used, ref
                used.add(ref.lower())
                sid = f"pos-{by_spelling[spelling]['id']}-{n}"
                rows.append(row(sid, ref, tts, wid, marked, "positive", "carrier",
                                by_spelling[spelling]["id"]))

    # review cases (verbatim, reserved before templated expansion)
    for text, wid, related in SD.REVIEW_CASES:
        used.add(text.lower())
        rows.append(row(f"neg-review-{slug(text)[:40]}", text, text, wid, [], "hard_negative",
                        "review_case", by_spelling[related]["id"] if related else None))

    # homophones (verified against CMUdict)
    evidence = []
    items = []
    for spelling, phrase, clauses in SD.HOMOPHONES:
        term = by_spelling[spelling]
        best = homophone_distance(cmu, term, phrase)
        if best[0] > HOMOPHONE_MAX_DISTANCE:
            raise SystemExit(f"homophone {phrase!r} for {spelling}: distance {best} > threshold")
        evidence.append({"termId": term["id"], "homophone": phrase, "matchedForm": best[1],
                         "termPhones": best[2], "homophonePhones": best[3], "distance": best[0]})
        for clause in clauses:
            assert contains_phrase(clause.lstrip("=").removeprefix("V:"), phrase), (phrase, clause)
            items.append(((ws_of[spelling], term["id"]), expand(clause)))
    rows += negatives(pick_round_robin(items, TARGETS["homophone"], used, "homophone"), "homophone")

    # subwords
    items = []
    for spelling, wid, word, clauses in SD.SUBWORDS:
        if spelling is not None:
            assert len(word) >= 3 and word.lower() in spelling.lower(), (spelling, word)
            assert ws_of[spelling] == wid
        for clause in clauses:
            assert contains_phrase(clause.lstrip("=").removeprefix("V:"), word), (word, clause)
            items.append(((wid, by_spelling[spelling]["id"] if spelling else None), expand(clause)))
    rows += negatives(pick_round_robin(items, TARGETS["subword"], used, "subword"), "subword")

    # ordinary + protected spread round-robin across workspaces
    wids = [w["id"] for w in terms_doc["workspaces"]]
    items = [((wids[i % 3], None), expand(c)) for i, c in enumerate(SD.ORDINARY)]
    rows += negatives(pick_round_robin(items, TARGETS["ordinary"], used, "ordinary"), "ordinary")

    by_kind: dict[str, list] = {}
    for kind, clause in protected_items():
        by_kind.setdefault(kind, []).append(clause)
    assert sum(PROTECTED_QUOTAS.values()) == TARGETS["protected"]
    counter = 0
    for kind, quota in PROTECTED_QUOTAS.items():
        items = []
        for clause in by_kind[kind]:
            related = by_spelling["Mandy"]["id"] if "Monday" in clause else None
            items.append(((wids[counter % 3], related), expand(clause)))
            counter += 1
        rows += negatives(pick_round_robin(items, quota, used, f"protected-{kind}"), "protected")

    # global invariants
    seen_ids, seen_text = set(), set()
    for r in rows:
        assert r["id"] not in seen_ids, r["id"]
        seen_ids.add(r["id"])
        key = r["reference"].lower()
        assert key not in seen_text, r["reference"]
        seen_text.add(key)
        found = found_terms(r["reference"])
        assert not (found & distractor_spellings), (r["reference"], found)
        if r["category"] == "positive":
            assert found == set(r["targetTerms"]), r
        else:
            assert not found, (r["reference"], found)
            assert r["ttsText"] == r["reference"]
        for field in ("reference", "ttsText"):
            assert re.fullmatch(r"[A-Za-z0-9$][\x20-\x7e]*[.?!]", r[field]), r[field]
    texts = {r["reference"] for r in rows}
    for text, _, _ in SD.REVIEW_CASES:
        assert text in texts
    return rows, sorted(evidence, key=lambda e: (e["termId"], e["homophone"]))


def negatives(chosen, subcategory):
    out = []
    for (wid, related), text in chosen:
        sid = f"neg-{subcategory}-{sha256_text(text)[:12]}"
        out.append(row(sid, text, text, wid, [], "hard_negative", subcategory, related))
    return out


def row(sid, ref, tts, wid, targets, category, subcategory, related):
    return {"id": sid, "text": ref, "reference": ref, "ttsText": tts, "workspaceId": wid,
            "targetTerms": list(targets), "category": category, "subcategory": subcategory,
            "relatedTermId": related, "split": split_for(sid)}


def serialize_terms(doc) -> str:
    return json.dumps(doc, indent=2, ensure_ascii=False) + "\n"


def serialize_sentences(rows) -> str:
    rows = sorted(rows, key=lambda r: r["id"])
    return "".join(json.dumps(r, ensure_ascii=False, sort_keys=False) + "\n" for r in rows)


def counts(rows) -> dict:
    c = {"total": len(rows),
         "byCategory": dict(sorted(Counter(r["category"] for r in rows).items())),
         "bySubcategory": dict(sorted(Counter(f"{r['category']}/{r['subcategory']}" for r in rows).items())),
         "bySplit": dict(sorted(Counter(r["split"] for r in rows).items())),
         "bySubcategorySplit": dict(sorted(Counter(f"{r['subcategory']}/{r['split']}" for r in rows).items())),
         "byWorkspace": dict(sorted(Counter(str(r["workspaceId"]) for r in rows).items()))}
    return c
