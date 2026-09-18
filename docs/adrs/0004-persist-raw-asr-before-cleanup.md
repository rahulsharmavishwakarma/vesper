# ADR-0004: Persist raw ASR before cleanup

- Status: Accepted
- Date: 2026-09-14

## Context

Cleanup is a generative transformation. It may fail, be cancelled, repeat text, or change meaning. Users need the recognizer's original output, and ASR evaluation must not confuse rewritten prose with what the recognizer produced.

## Decision

Treat raw ASR as the primary machine result. When collection is enabled, store raw ASR and actual model/runtime provenance before cleanup. Publish that result to the UI before invoking the cleanup model. Store at most the first machine cleanup separately. Keep human verbatim labels and human edited corrections as distinct review targets with append-only revision history.

Minimal cleanup remains the default and must preserve the normalized word sequence. Balanced cleanup may edit more but remains review-required. Baseline guards apply even when the user supplies a custom instruction.

## Consequences

- Cleanup failure or cancellation never erases a successful transcription.
- The UI and dataset have more fields, but their meaning stays unambiguous.
- ASR training uses human-verified verbatim speech only; cleanup training uses raw ASR paired with a human-approved correction.
- Machine output is never automatically promoted to ground truth.
