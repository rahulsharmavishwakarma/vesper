# Writing-model polishing and ASR word error — 16 October 2026

Question ([#151](https://github.com/rahulsharmavishwakarma/vesper/issues/151)):
can the writing model that already runs after ASR also reduce word error rate?

**Result: not on this set.** The Minimal preset (the default) left WER
unchanged. The Balanced preset, which may correct "obvious" recognition errors,
made WER slightly worse: it fixed one real ASR error, added words or rewrote
words in five other recordings, and was rejected three times. These are local
measurements on 52 public read-speech clips. They do not show that
LLM correction can never help, and they do not cover other models, languages
or personal vocabulary.

## Setup

- Application source: `fix/cleanup-fixture-fallback` at `4f0b744` (PR #148,
  without the retired `README.txt` fixture prompt or fallback).
- ASR: Cohere Transcribe INT8 (the default), explicit English, CPU, one fresh
  process per clip (`examples/pipeline --asr-only`).
- Writer: Qwen3.5-0.8B Q4_0, SHA-256
  `57d1997790d1744fba5b40a7317df71ea5e2acee28c47e78f0cce39c0703f8cf`, resident
  in one process per preset (`examples/cleanup --minimal|--balanced`), with
  greedy decoding and production fidelity checks.
- Hardware: Apple M5 Pro, 18 logical CPUs, 64 GiB, macOS 27.0.1, battery power.
- Corpus: the public FLEURS en-US natural clips and synthetic controls from the
  [16 September regression](audio-regression-2026-09-16.md): 24 clean, the same
  24 mixed with environmental noise at 0 dB, and 4 synthetic meaning controls.
  All three arms score the same ASR output against the same verbatim
  references.
- Scoring: lower-cased words with punctuation and hyphens removed, and
  Levenshtein word edits. A fidelity rejection keeps the raw text, as the
  application does, so it counts at the raw WER.

## Word error

| Lane                                  |     Raw ASR |     Minimal |    Balanced | Balanced better / worse recordings |
| ------------------------------------- | ----------: | ----------: | ----------: | ---------------------------------: |
| Clean, 24 clips, 608 words            | 61 (10.03%) | 63 (10.36%) | 65 (10.69%) |                              1 / 5 |
| Noise at 0 dB, 24 clips, 608 words    | 91 (14.97%) | 91 (14.97%) | 93 (15.30%) |                              0 / 2 |
| Synthetic controls, 4 clips, 52 words |  8 (15.38%) |  8 (15.38%) |  8 (15.38%) |                              0 / 0 |

Minimal's +2 on the clean lane is a scoring artifact: it changed a typographic
apostrophe (`isn’t` → `isn't`), which this normalization treats as a
different word. Its word-match guard otherwise keeps output equal to raw.
Median writer time per clip was 436 ms for Minimal and 473 ms for Balanced.

## What Balanced changed

- **Fixed:** a split sentence, `in which was. It was made famous` →
  `in which was made famous`.
- **Added or rewrote words:** `Television reports` → `The television reports`
  (three recordings); `isn’t` → `is not`; and in one sentence the meaningful
  word `First` was dropped.
- **Did not fix:** `free to two` for "3:2", or `top` for "plant" in noise.
  The writer sees only one text hypothesis. Nothing in the text shows that
  these words are wrong.
- **Rejected:** twice because the output hit its token limit, once because a
  number changed, and once because negation changed (synthetic control).
  Raw text was kept each time.

## Interpretation

The default Minimal pass cannot reduce WER by design, because it may only add
punctuation and capitalization. Balanced gives the model room to change words,
but a 0.8B model reading one 1-best transcript has no acoustic evidence for
choosing between plausible words. On this set it introduced more errors than
it fixed.

Published generative error correction work ([HyPoradise](https://arxiv.org/abs/2309.15701), [task-activating
prompting](https://arxiv.org/abs/2309.15649)) reports gains for larger LLMs that receive **N-best ASR
hypotheses**. Those are paper results for other models and inputs. They do not
transfer to this writer or this pipeline. Passing N-best lists from ASR into
the writer would change inference data flow, so it would need its own
approved proposal and the same held-out comparison.

The routes most likely to lower WER here are on the ASR side: a stronger or
better-matched recognizer, user vocabulary hints, and reviewed personal
corrections ([self-improvement](../self-improvement.md)). Polishing remains a
readability step, not an accuracy step.

## Not covered

Only English read speech and only the Cohere recognizer were tested. Not
covered: Q8 writer, custom writers, conversational or technical dictation,
named entities from the user's vocabulary, Hindi or French, or other ASR
models with higher raw WER. Clip-level transcripts stay in the ignored local
benchmark cache.
