# Retry evidence and correlated attempts

Implementation: [core cue rules](../../crates/vesper-core/src/learning.rs), [runtime recent context](../../apps/desktop/src-tauri/src/runtime.rs), and [event storage](../../crates/vesper-data/src/learning.rs).

## What is observable

For completed microphone captures, the runtime keeps one bounded prior raw transcript and context in memory. It considers that prior attempt relevant for at most 120 seconds. The retained text is replaced on the next eligible capture and cleared on file import, disabled learning, preference save/forget or process exit; it is not persisted in the feedback table. No timer claims to securely erase RAM exactly at 120 seconds.

The comparison uses requested language, selected project and learning permission epoch. It does not have a persistent identity for the document receiving the text. Similarity in one project cannot prove that two recordings address the same task. File imports may contribute usage/edit evidence, but are not treated as microphone retries.

## Current labels

| Label             | Version-1 rule                                                                                                                          |
| ----------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `possible_repair` | Same known scope and bounded time, at least two shared informative terms, and an English repair cue such as “I said” or “let me repeat” |
| `continuation`    | Bounded same-scope follow-up starting with an English continuation cue such as “also”                                                   |
| `changed_intent`  | Bounded same-scope follow-up starting with a cue such as “actually” or “instead”                                                        |
| `uncertain`       | Missing evidence, incompatible scope, expired window, oversized input, ordinary repetition or an unsupported pattern                    |

Inputs over 8,192 bytes are not analyzed. These cue rules are intentionally a narrow English baseline. They are not a multilingual classifier; there is no predicted error cause, spelling resolution or new-task certainty. A spoken “actually” can have another meaning, so no persistent correction is derived from this label.

## Independence is a separate decision

Highly overlapping successive attempts can share a correlated episode ID even if their relationship remains uncertain. This requires two shared informative terms and at least half-union lexical overlap within the same time/scope bounds. A possible repair also shares the previous episode. The conservative grouping reduces available independent support; it never increases confidence or asserts that the later wording is correct.

The audio fingerprint independently deduplicates identical 16-kHz PCM across imports/models. Multiple events may retain relationship evidence, but only the first independent observation changes usage counts. An observation never counts a phrase that was in that decode's recognizer hint list ([bias provenance](context-selection.md#typed-recognizer-hooks)). Retry-only data never increments user-edit support or activates an alias.

No earlier result is overwritten or reinserted. Capture failures, empty speech and cancellation do not invent spelling pairs. The current method cannot diagnose whether dissatisfaction came from ASR, cleanup or a changed intention. [The research design](../research-automatic-personalization.md#understand-the-retry-before-extracting-a-correction) describes the later semantic task and why it requires separate validation.
