# ADR-0010: Evidence-gated ASR qualification and recommendations

- Status: Accepted
- Date: 2026-09-15
- Tracking: [GitHub issue #33](https://github.com/rahulsharmavishwakarma/transcribe/issues/33)

## Context

The application supports several architecture-specific ASR adapters and tracks additional research candidates. Publisher metrics are collected with different datasets, hardware, precision, streaming definitions, and runtime paths. Model release date, parameter count, or a claimed language list cannot safely choose a desktop default. We need repeatable application-level evidence, including real-time streaming and long-form behavior, without adding a second inference implementation that can drift from production.

## Decision

Use the development-only `transcribe-eval` crate to run the same audio decoder, `InferenceManager`, native adapters, and optional cleanup path as the desktop backend. Every benchmark manifest pins held-out source metadata, revision, license/attribution, sample SHA-256, reference, language, duration, scenario, and whether the recording is natural or a synthetic soak.

Record the full model/runtime provenance and execution envelope. Score normalized exact WER plus Unicode grapheme error rate. Keep the reference language and configured recognition language separate so automatic-language and explicitly prompted results cannot be pooled. Streaming reports distinguish accelerated replay from wall-clock pacing and record first partial, stop-to-final latency, partial count/stability, cold versus genuinely resident state, failures, and stage timings. Missing resource telemetry remains explicitly null.

Recommendations first apply hard compatibility filters for application support, language, mode, actual backend, duration, automatic-language behavior, code-switching, and singing. Comparable local measurements rank ahead of official capability-only candidates. A researched model whose adapter is not executable cannot be recommended. A newer model never changes the default without running the same qualification manifests.

Recommendation requests declare an `accuracy`, `latency`, or `balanced` objective. Balanced ranking combines median WER and median real-time factor after compatibility and failure accounting; an accuracy-only improvement cannot silently hide a severe interactive-latency regression. A request may also set maximum median WER, median RTF, p95 stop-to-final latency, and failure rate. Comparable local evidence that exceeds any requested ceiling is disqualified rather than relabelled as unmeasured official support. Official-only candidates remain visibly unmeasured because a ceiling cannot be inferred without comparable evidence.

Represent benchmark content with an explicit locale (`hi-IN`, `en-IN`, `en-US`) or a `+`-joined language pair (`hi+en`). Model capability checks use the component base languages, while evidence matching retains the complete profile. This prevents Indian and US English, or Hindi and Hinglish, from being pooled.

Keep corpora, prepared audio, machine descriptions, and raw reports in the gitignored benchmark cache. Check in only preparation/scoring code, capability metadata, examples, tests, and documentation. Do not use held-out benchmark clips for tuning or training.

## Consequences

- Accuracy, latency, and reliability claims are tied to reproducible model, corpus, runtime, and hardware identities.
- The benchmark detects production-path lifecycle costs; it does not hide them behind a separate Python implementation.
- Mixed-script exact WER can penalize correct-sounding transliteration, so qualitative review and grapheme scores remain visible.
- A language pack may intentionally return no qualified model; the engine does not force a recommendation when every locally measured compatible candidate fails policy.
- Cross-hardware leaderboards require controlled runners and peak RSS/VRAM, power, and thermal capture before they are complete.
- Automatically switching a user's model is not authorized by this decision. The application may explain compatible evidence, but changing inference ownership, model lifetime, defaults, or storage remains subject to its own approval and ADR where applicable.
