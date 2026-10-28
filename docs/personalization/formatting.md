# Dictation formatting

Status: implemented on the local branch `feature/self-improving-dictation` (28 October 2026); not published. On by default (**Format numbers, times, emails and links** in Writing), English only.

`vesper_core::format_dictation` rewrites spoken forms into written ones after the [correction gate](correction-gate.md). It is fixed rules, not a model, so the result is the same whichever speech model or writer produced the text, and changing either cannot degrade it. A rule only rewrites a span it fully recognizes. Each result records the rules that fired in `personalizationEvidence.formatting`.

| Rule | Example |
| --- | --- |
| Hesitations | `um so basically` → `So basically` (um, uh, er, erm, hmm) |
| Repeated words | `we we should`, `I I think` → one word. `that that`, `had had` and a repeated capitalized word mid-sentence (a name) are kept. English letters only. |
| Cut-off starts | `transcri transcription` → `transcription` when the fragment is not a dictionary word |
| Self-corrections | `Tuesday no Wednesday`, `Friday actually make it Saturday`, `ten thousand sorry twelve thousand dollars`: the earlier value is dropped only when both values are the same kind (weekday, month, number) |
| Emails | `john dot smith at gmail dot com`, `John.smith at gmail.com` → `john.smith@gmail.com`. The local part needs a spoken joiner (`dot`, `underscore`) or a cue such as `to`, `cc`, `email`, `is`. |
| Web addresses | `fermion research dot com slash detta` → `fermionresearch.com/detta`; `github.com slash vesper` → `github.com/vesper` |
| Times | `ten thirty a m`, `10 30am`, `4 15 p.m.` → `10:30 AM`, `4:15 PM`; `nine o'clock` → `9:00` |
| Dates | `the fifteenth of October` → `the 15th of October`; `March 3, 2027` / `March 3,2027` → `March 3rd, 2027`; `on the twenty first` → `on the 21st` |
| Money, percentages | `two thousand five hundred dollars` → `$2,500`; `one point two million dollars` → `$1.2 million`; `three point five percent` → `3.5%` (dollars, rupees, euros) |
| Numbers | 10 and above, decimals and multi-word numbers become digits (`twelve thousand words` → `12,000 words`); one to nine stay words; two numbers in a row (`forty nine ninety nine`) stay words because they are ambiguous |

`like` and `you know` are never removed: they can carry meaning. With **Keep fillers and repetition** on, the first three rules (hesitations, repeated words, cut-off starts) are skipped and the rest still run.

## Evaluation (28 October 2026)

**Detta-style phrases.** 24 phrases: Detta's 8 published examples and 16 of the same kinds, rendered with macOS `say` (Samantha) to 16 kHz files and run through the full desktop pipeline (`runtime_e2e_tests.rs`, managed audio, writer cleanup on, correction gate). Exact match with the written target:

| Recognizer | Before | With formatting |
| --- | --- | --- |
| Phonon-2 (MLX) | 1 / 24 | 15 / 24 |
| Parakeet TDT 0.6B v3 | 7 / 24 | 18 / 24 |

The remaining misses are recognition errors (`Dotsmith`, `pri` for `priya`, `Fermian`, `data` for `detta`, `a man` for `a month`), `like` and `you know` kept by design, and `stand up` versus `standup`.

**Writer comparison.** With the writer alone it changed 0 of 24 in its default Minimal preset, where the fidelity check rejects any word change. In Balanced it changed 2 of 24, one harmfully (`priya underscore k` → `Pri K`). Its fidelity check also rejects every spoken-to-digit change, because it compares number words. That is why formatting is rules rather than a writer prompt.

**Real history.** `format_replay` (a `vesper-core` example) over 465 distinct local transcripts changed 140, mostly hesitations (361) and repeats (46), plus 14 numbers and 3 percentages. Every non-hesitation change was reviewed. Three faults were fixed and are covered by tests:
- a non-English repeat was collapsed
- a kept repeat took the wrong capital
- a repeated name was collapsed

One remaining questionable change is `eleven labs` → `11 labs`, where the recognizer had already lost the company name ElevenLabs.

**Unit tests** cover the formatting cases, 23 ordinary sentences that must stay unchanged (`I said no to that`, `He works at Google now`, `One of them was there`, `It was the first time`, `May I come in?`) and idempotence.

## Limitations

- English only; other languages pass through unchanged.
- A repeated word used deliberately (`small small bugs`) is collapsed.
- Words for numbers inside names can become digits (`eleven labs`).
- Web-address joining takes at most two words before `dot` and stops at common words; an unusual site name may join wrongly.
- Recognition errors inside a spoken address or number are formatted as heard.
