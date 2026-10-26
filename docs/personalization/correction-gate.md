# Correction gate

Implementation: [`crates/vesper-core/src/gate.rs`](../../crates/vesper-core/src/gate.rs). Contracts: [`terms.rs`](../../crates/vesper-core/src/terms.rs).

The correction gate is the text-level safety layer shared by every speech model. It runs after recognition, after optional cleanup and after the existing [saved-correction aliases](automatic-dictionary.md). Its input is the per-recording working set's **normalizations**: exact spoken-form → spelling pairs built from the term inventory, such as `sherpa onnx` → `sherpa_onnx`. Sound-alike or fuzzy matching never reaches the gate as a change. It may only propose candidates elsewhere.

## Rules

A normalization applies only when all of these hold:

1. The recording language is English. Other languages and automatic language detection fail closed (`unsupported_language`).
2. The spoken form occurs as an exact token sequence (ignoring case). It must not cross punctuation, a sentence or line boundary, or sit inside quotes (`boundary`).
3. Neither side contains a protected class: numbers, weekdays, months, dates, negation, uncertainty, pronouns or auxiliaries (`protected_class`).
4. **Heard-plausibility gate.** If every heard token is an ordinary English word, the span already makes sense as speech, and the gate refuses to change it into a different word (`ordinary_language`). The one exception is a *user-asserted* spelling with exactly the same letters, such as `type script` → `TypeScript`. A rejoin into an ordinary word is still refused.
5. Overlapping candidates abstain (`ambiguous`). Replacement is single-pass and does not cascade.

Every declined candidate is returned with its reason, so evaluation and diagnostics can count what the gate blocked.

The heard-plausibility gate is what blocks the failures found in the [proposer review](evaluation.md): "I see the point" → "Cedar Point", "Monday" → "Mandy", "May" → "Mae", "write" → "Wright", "notes" → "Nots". Each heard span is ordinary language, so none of them can be rewritten by any tier. Phonetic distance cannot make this decision: "write", "right" and "Wright" are pronounced identically.

## Near-miss repair (experimental, shadow by default)

Exact normalizations cannot repair a misrecognition such as `vasper` → `Vesper`, `Envidia` → `NVIDIA` or `GEMA` → `Gemma`. Those misrecognitions do occur in local dictation history. Learned aliases need user edits, and in-app edits are rare.

The gate therefore has an experimental near-miss rule over the working set's **references**: the up to 16 spellings relevant to this workspace. It proposes a repair only when all of these hold:

- the heard token is not an ordinary word, has at least four letters, and is not protected or quoted;
- it is within one letter edit of exactly one reference (two edits from eight letters), with a unique best match;
- it is not merely an inflection of that reference (`LLMs` stays `LLMs`).

`NearMiss::Shadow` (the default) records proposals in `GateResult::near_misses` without changing text. `NearMiss::Apply` applies them. The default changes only on evaluator evidence: zero would-fire changes on correct words across the hard-negative bank and the retained-history replay. See the [term evaluation](term-evaluation.md).

`crates/vesper-data/examples/would_fire_replay.rs` runs the gate over a **copy** of a dataset with a frozen working set. It reports every exact and near-miss change with surrounding context, for private review. It never runs a model or writes to the source store.

## Lexicon

"Ordinary English word" means membership in a lexicon derived from the [CMU Pronouncing Dictionary](https://github.com/cmusphinx/cmudict). It is pinned at commit `74790861f652b15e4ac49015a90074ad62a27690`, and `cmudict.dict` has SHA-256 `81917843…3d22`.

The bundled file `crates/vesper-core/data/english-lexicon.fc` holds 117,493 lowercase ASCII words, front-coded in 522 KB. `scripts/generate-english-lexicon.py` regenerates it and checks the source hash. CMUdict is redistributed under its BSD-style license (`crates/vesper-core/data/CMUDICT-LICENSE`); keep that notice with any binary distribution's notices.

The lexicon includes inflections and many proper names. This makes the gate *more* conservative: a heard name such as "Mandy" also counts as ordinary and is never rewritten. It does not contain most technical tokens (`onnx`, `kubectl`, `etcd`, `ctl`), and those are the spans the gate exists to repair.

## Limitations

- English only. Supporting other languages needs a per-language lexicon and its own evaluation.
- A term whose spoken form is entirely ordinary words (`user id` → `user_id`, `cube control` → `kubectl`) is not normalized. It improves only through a recognizer hint, or through a correction the user taught (exact aliases in the [automatic dictionary](automatic-dictionary.md)). Results are in the [term evaluation](term-evaluation.md).
- Membership is not frequency. Rare dictionary words count as ordinary, which favors missed corrections over wrong ones.
