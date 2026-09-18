# ADR-0007: Require explicit native completion for bounded generative ASR

Status: Superseded by [ADR-0011](0011-segment-long-generative-asr.md)

Date: 2026-09-15

## Context

Qwen3-ASR and Cohere can finish with partial text after a context/output limit or another early exit. Sherpa 1.13.8 does not expose those reasons in its ordinary result. Short clips and returned token counts do not prove completion. The user approved application adapters and subsequently approved native reporting instrumentation and its source build in [issue #14](https://github.com/rahulsharmavishwakarma/transcribe/issues/14).

## Decision

Keep the pinned native API and per-job model ownership. Build a reproducible patched Sherpa library that reports actual stop reason and discarded input context through existing stream options. Accept ordinary text only after EOS with complete input context. Reset reports before each decode and reject missing, incompatible or failed reports. Enforce a ten-second effective limit before capture/import decoding and check cancellation around native calls. Exact digital silence returns empty text before inference.

## Consequences

The application maintains a small C++ reporting patch and verifies the linked libraries and patch identity during its native build. It does not add another inference framework, change sampling/weights, or implement segmentation, streaming, training or model residency. Completion reporting prevents publishing known partial/error results; it does not prove recognition accuracy. Each supported target needs a patched native build and qualification. See [implementation and validation](../offline-asr.md).
