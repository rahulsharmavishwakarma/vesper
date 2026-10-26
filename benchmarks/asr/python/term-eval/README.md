# Term-eval corpus generator

Deterministic generator for the frozen term-recognition evaluation corpus in
[`../../manifests/term-eval/`](../../manifests/term-eval/). No ASR model is run
and no model output is read by any command.

**Freeze rule:** the corpus was generated and frozen before any model output was
seen. `FROZEN.json` pins the SHA-256 of `terms.json` and `sentences.jsonl`, the
generator sources, the seed and the CMUdict revision. `freeze` refuses to write a
different corpus unless `--refreeze` is passed; a refreeze is a new corpus and
needs a new evaluation, not a silent update.

## What is in it

- `terms.json`: three synthetic workspaces of 30 terms each (`devops`,
  `rustaudio`, `people`) plus 60 `distractors` used only to pad hint lists. The
  people and product names are invented; some deliberately collide with ordinary
  words (Wright, Mandy, Cedar Point, Paige, Bree, Niall, Nguyen).
- `sentences.jsonl` (1.2 MB, not committed: `freeze`/`check` regenerate it byte-identically and `FROZEN.json` pins its SHA-256): 180 positives (2 per term) and 3,156 target-free hard
  negatives (`homophone`, `subword`, `protected`, `review_case`, `ordinary`). Every
  negative is asserted to contain no term or distractor spelling as a whole word.
  Homophones are checked against CMUdict (stress removed, phone Levenshtein /
  term-form phones ≤ 0.34); the evidence is listed in `FROZEN.json`.
  `subword` uses an ordinary word that is part of a term's spelling (or a
  domain-adjacent word such as pod or crate); `homophone` uses a different word
  or phrase that sounds like a spoken form or a word of one. A term that is itself
  an ordinary word (Helm, Envoy, Calico) cannot appear in negatives in that sense.
- Split: `sha256(id) mod 10 == 0` is `dev` (pipeline debugging only), the rest `test`.

All sentence text is synthetic and contains no user data.

## Regenerate

From the repository root (uv 0.13.x):

```sh
P=benchmarks/asr/python/term-eval
CACHE=/path/to/.cache/benchmarks   # or set VESPER_BENCH_CACHE
uv run --locked --project $P python -I $P/generate.py --cache-root $CACHE fetch [--librispeech]
uv run --locked --project $P python -I $P/generate.py --cache-root $CACHE check
uv run --locked --project $P python -I $P/generate.py --cache-root $CACHE render [--librispeech]
```

`render` writes `<cache>/datasets/term-eval-2026-10-26/` (never committed): macOS
`say` clips converted to 16 kHz mono PCM16 with `afconvert`, rotated over
Samantha, Daniel, Rishi, Karen, Moira and Tessa; `pink10` and `babble10` (three
FLEURS en-US test clips not used anywhere else) mixes of every positive at 10 dB
SNR; FLEURS real-speech controls; and optionally 300 LibriSpeech test-clean
utterances with the Le et al. (2021) N=100 biasing lists. `say` always writes to a
file. `render-summary.json` records counts, voices and the macOS build.

## Labels and licences

- `say` audio is **synthetic**, labelled `synthetic: true`, generated locally and
  not redistributed. Output can differ across macOS builds; compare runs only on
  the build recorded in `render-summary.json`.
- FLEURS (CC-BY-4.0, Google), LibriSpeech (CC-BY-4.0, openslr.org/12), the
  fbai-speech `is21_deep_bias` lists (MIT, Facebook) and CMUdict (BSD-style, CMU)
  are pinned by revision and SHA-256 in `fetch.py` and the manifest `sources`.

## Ordinary-word supplement

Because the frozen negatives exclude every term spelling, terms that are also
ordinary words (helm, envoy, calico, parakeet, vesper) had no target-free uses.
[`supplement-ordinary-terms.jsonl`](../../manifests/term-eval/supplement-ordinary-terms.jsonl)
adds 36 such sentences, written before any term-evaluation output existed.
`supplement.py` renders each with Samantha and Daniel (files only) and writes
`term-manifest-supplemented.json` beside the frozen manifest, recording both
input hashes. The frozen corpus itself is unchanged.

```sh
uv run --locked --project $P python -I $P/supplement.py $CACHE/datasets/term-eval-2026-10-26
```
