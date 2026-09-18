# Editable drafts and ordinary feedback

Implementation: [TranscriptionPanel](../../apps/desktop/src/lib/components/TranscriptionPanel.svelte), [runtime](../../apps/desktop/src-tauri/src/runtime.rs), [commands](../../apps/desktop/src-tauri/src/commands.rs), and [draft transaction](../../crates/transcribe-data/src/learning.rs).

## Text ownership

Each result retains separate raw ASR, optional machine cleanup, optional personalized output and an optional user draft. **Your text** starts with the best available output. Editing it does not mutate any machine prediction. Original outputs remain inspectable in a disclosure.

A completed changed draft invokes `edit_latest_draft(resultId, revision, text)` when the field loses focus or Copy is clicked. Identical text is a no-op. IME composition prevents committing an unfinished composition. Copy waits for the current save promise and copies the edited draft; it does not submit a second learning event after blur already started the save. A clipboard write is not a correctness label.

The frontend preserves the visible draft on failure and offers Retry saving edit. A new recording arriving before an old unsaved draft is committed preserves that draft in a recovery field. That recovery field holds the latest displaced unsaved draft in memory; it is not a durable editing history. Closing the app can lose unretained drafts. The main recording controls wait while a draft save is active.

## Authoritative checks

The backend uses its current result as the before-text. The frontend cannot supply a fabricated machine baseline. A runtime mutex serializes result/revision validation, local persistence and publication against starting a new operation. Edits during an active recording or against an old result/revision fail. Late responses cannot replace a newer UI result.

A saved session also checks its draft revision and authoritative displayed baseline. Updating the saved draft and admitting its learning evidence occur in one SQLite transaction. A failed profile write rolls back both evidence and draft changes. Runtime publication happens after successful persistence. Draft revisions are separate from profile revisions; multiple independent results may teach the same profile without pretending to be revisions of one recording.

## Collection and labels

With audio collection enabled, the session stores `draftTranscript` and `draftRevision`. History preview and search include this draft. With collection disabled, the draft stays in runtime memory; automatic learning persists compact correction evidence without archiving the full draft or source audio. The retired desktop learning opt-in is no longer required. Generic disabled contexts remain supported for offline controls and stale-result isolation.

Drafts may be empty and are bounded to 100,000 characters. An empty draft must not fall back to the original text in History. Ordinary drafts do not update `verifiedTranscript`, `correctedTranscript`, `reviewStatus` or `reviewHistory`. Training eligibility still requires the existing explicit review path; unreviewed drafts can appear as archival metadata but never become supervised targets automatically.

**Undo learning** pauses the inferred spelling while preserving authored text. **Undo remembered edits** rejects the additional machine-personalization result. An existing authored draft takes precedence over that machine fallback; undo does not silently discard the user's own writing. The application never re-inserts an edited draft into another application automatically.
