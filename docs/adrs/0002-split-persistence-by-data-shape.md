# ADR-0002: Split persistence by data shape

- Status: Accepted
- Date: 2026-09-14

## Context

Preferences, queryable sessions, large immutable audio/model artifacts, and current-process status have different consistency, size, and lifetime needs. Storing all of them in one database would add blob handling and migration complexity without improving the product.

## Decision

Use validated atomic JSON for settings, SQLite for the session index and complete JSON records, ordinary files for original audio and model/runtime artifacts, and memory for current operation state and the latest UI result. Use JSONL plus copied original audio for portable exports.

SQLite uses WAL, full synchronization, a five-second busy timeout, and one `sessions` table. Large audio and model blobs do not enter SQLite. The complete field and durability contract is defined in [Application internals](../internals.md).

## Consequences

- Each storage mechanism stays small and appropriate to its data.
- A portable backup must include more than the SQLite main file; the supported path is export or a closed-app copy of the complete state.
- Audio/database inconsistencies caused by manual file deletion are detected during export but are not automatically repaired.
- There is no automatic migration, retention, or backup framework yet; those require explicit future work rather than speculative infrastructure now.
