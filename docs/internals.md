# Application internals

This document is the canonical description of the current desktop application's state, persistence, and failure boundaries. It describes implemented behavior, not the speech-model research roadmap. Architecture decisions that explain why these boundaries exist are indexed in [ADRs](adrs/README.md).

## Storage map

The application deliberately does not put every kind of state in one database.

| State | Persistence | Logical location | Lifetime |
| --- | --- | --- | --- |
| User settings and selected models | Pretty-printed JSON | Tauri `app_config_dir/settings.json` | Until reset or manually removed |
| Session metadata and review history | SQLite | Tauri `app_data_dir/dataset/sessions.sqlite3` | Until the dataset is manually removed |
| Original imported/captured audio | Ordinary files | Tauri `app_data_dir/dataset/audio/` | Same as its session record |
| Model weights and native sidecars | Ordinary files | Selected model directory | Until manually removed or repaired |
| Current operation, progress, microphone level, live partial, cancellation flag, resident models, and latest result | Process memory | None | Lost when the process exits |
| Browser-preview settings | Web storage | `localStorage` key `transcribe.browser-preview.settings.v1` | Browser-profile lifetime |
| Portable dataset output | JSON, JSONL, and audio files | User-selected export parent | Independent of application state |

Tauri resolves platform-specific configuration and data directories; application code does not hard-code an operating-system home path. The current application identifier is `com.transcribe.desktop`. If that root has no explicit state, the app can continue using an existing sibling root from `com.ukti.desktop` and then `com.stt.desktop`. It does not move, merge, or delete legacy data. A settings entry, nonempty dataset/model tree, or symlink counts as explicit state; empty directories and `.DS_Store` do not.

The config and data roots are resolved independently. Therefore `settings.json`, `dataset/`, and `models/` must be discussed as logical locations rather than assumed to share one physical parent on every platform.

## Settings persistence

`settings.json` stores `AppConfig` schema version 1. It includes the ASR and cleanup selections, compatible custom Hugging Face profiles, cleanup policy, language, model-directory override, requested compute backend, legacy recording-limit field (ignored), shortcut/UI preferences (including separate preset/custom Hold/Toggle bindings with legacy single-shortcut migration and in-app/paste output), notification preference, collection preference, and the future improvement policy. The exact field defaults and constraints live in `transcribe-core`; [development.md](development.md) is the operator-facing configuration guide.

A missing settings file returns validated defaults. A corrupt, unreadable, or unsupported settings file is reported and preserved instead of being silently replaced. Saves follow this sequence:

1. Validate the entire candidate configuration.
2. Serialize it to a temporary file in the destination directory.
3. Write all bytes and call `sync_all` on the temporary file.
4. Atomically persist the temporary file over `settings.json`.

A process-local mutex serializes settings reads and replacements. Settings changes and operation startup also share a gate, so a recording or transcription receives one immutable configuration snapshot. Changing settings while an operation is active is rejected. Resetting settings writes the normal default object; it does not delete the dataset or model files.

Browser preview is intentionally separate. It migrates reads from the old `ukti.browser-preview.settings.v1` and `stt.browser-preview.settings.v1` keys, but writes only the current key. It cannot inspect native files, run inference, record audio, or access the SQLite dataset.

## Dataset database

The dataset engine is SQLite through `rusqlite`. Opening a store creates and canonicalizes `dataset/` and `dataset/audio/`, rejects an audio directory that escapes the dataset root, opens `sessions.sqlite3`, applies a five-second busy timeout, and executes:

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = FULL;

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY NOT NULL,
    created_at TEXT NOT NULL,
    review_status TEXT NOT NULL
        CHECK(review_status IN ('pending', 'approved')),
    record_json TEXT NOT NULL
);
```

This is intentionally a small relational envelope around a version-tolerant JSON record:

- `id` provides stable identity.
- `created_at` provides newest-first ordering, with `id` as the tie-breaker.
- `review_status` supports a constrained, directly queryable review state.
- `record_json` is the complete session object returned across the Tauri boundary and exported to JSONL.

There are currently no foreign keys, secondary indexes, triggers, schema-version table, automatic migrations, vacuum schedule, or application-managed backup. Startup only uses `CREATE TABLE IF NOT EXISTS`. Newly introduced optional JSON fields use Serde defaults where declared. A future incompatible SQL or JSON change must add an explicit migration before release.

WAL mode means `sessions.sqlite3-wal` and `sessions.sqlite3-shm` may exist while the database is open. Copying only `sessions.sqlite3` from a running application is not a reliable backup. Use the application export, or close the application before copying the complete dataset directory.

### Session JSON fields

| Field | Meaning |
| --- | --- |
| `id` | UUID for the session and audio filename stem |
| `createdAt` | UTC RFC 3339 timestamp with millisecond precision |
| `originalFilename` | Original leaf filename shown to the user |
| `audioRelativePath` | Managed path in the exact form `audio/<session UUID>.<extension>` |
| `audioSha256` / `audioBytes` | Integrity and length of the archived original |
| `rawTranscript` | First machine ASR prediction, or `null` before inference |
| `cleanedTranscript` | First accepted machine cleanup result, or `null` |
| `verifiedTranscript` | Latest human-approved verbatim speech label, or `null` |
| `correctedTranscript` | Latest human-approved edited cleanup target, or `null` |
| `reviewHistory` | Append-only review revisions with UUID, target kind, text, and timestamp |
| `reviewStatus` | `pending` or `approved` |
| `configSnapshot` | Full validated configuration used for the operation |
| `modelVersions` | JSON object containing pinned artifacts and actual inference runtime provenance |
| `pipelineEvents` | Timestamped stage/message records such as completion, interruption, or cleanup error |

Machine outputs and human labels are separate by design. `set_transcripts` writes original predictions once. `complete_cleanup` requires an existing raw transcript and can write cleanup once. Approval can update the current human label, but every approval appends a new revision. Model output is never inferred to be ground truth.

Every record mutation reads the JSON inside a SQLite transaction, changes it, updates the relational review column and JSON together, then commits. Dataset commands run as blocking work outside the UI async executor; separate openings are coordinated by SQLite WAL and the busy timeout.

## Audio lifecycle and path safety

Collection is controlled by the saved configuration snapshot. When collection is disabled, transcription still runs but no dataset directory, database record, or archived audio is required. Disabling collection later does not delete older sessions.

An imported file must be a nonempty regular file no larger than 512 MiB and have an allow-listed extension: `wav`, `mp3`, `m4a`, `flac`, `ogg`, `opus`, `aac`, `aif`, `aiff`, `webm`, or `mp4`. Archive acceptance only means the original bytes can be retained; it does not guarantee the contained codec can be decoded. Opus is currently archival-only.

Import publication is failure-aware:

1. Generate a session UUID and a hidden staging file under `audio/`.
2. Copy in 64 KiB chunks while calculating SHA-256 and enforcing the size limit again.
3. Synchronize the staging file.
4. Rename it to `audio/<uuid>.<extension>` and synchronize the audio directory on Unix.
5. Insert the initial SQLite record.
6. Remove staging and final files if publication fails.

Imported audio keeps its exact bytes. Microphone capture is written as native-rate, native-channel 32-bit float WAV before mono conversion and filtered 16 kHz resampling for inference. During capture the not-yet-stopped buffer is memory-only. Explicit cancellation discards it. Normal application close attempts to archive it when collection is enabled; if archiving a stopped capture fails, the temporary WAV is retained and its recovery path is surfaced. Force termination or power loss cannot recover samples that still exist only in memory.

Before export, a stored relative path must contain exactly the `audio` directory and one filename, the filename stem must match the record UUID, the extension must be ASCII alphanumeric, and the canonical file must remain inside the managed audio directory. Export re-hashes the audio and rejects a byte-count or SHA-256 mismatch.

There is currently no session-delete API, retention timer, trash workflow, or orphan-reconciliation job. Users who remove individual database or audio files manually can create an inconsistent dataset; export will report missing or corrupt audio rather than hiding it.

## Inference and transient state

The Rust runtime allows one recording, transcription, or model installation operation at a time. It keeps these values only in memory:

- busy/stage/message status;
- current recording `WorkContext` and unique operation ID;
- cancellation and closing flags;
- current microphone level while recording;
- latest operation-scoped streaming partial and sequence number;
- latest `PipelineResult`.

Each `WorkContext` snapshots the validated settings, dataset root, selected model root, and runtime handle. Old recording watchers cannot claim a newer operation because they must match the operation ID.

It also freezes an optional local personalization profile once at inference startup. An absent database uses an empty profile without creating a store. Reviewed examples require a separate explicit remember action; profile revision checks prevent stale saves from restoring removed entries. Native phrase hints are per request. Profile schema 3 retains model-independent aliases, project scope and reviewed sources, and adds optional automatic learning, permission epochs, automatic provenance and usage counts. A bounded `automatic_learning` SQLite row stores ordinary-edit candidates and usage/retry evidence separately from human reviews. Editable drafts have result/revision guards and never become supervised targets implicitly. Automatic learning defaults off; eligible ordinary edits need no second Remember action when enabled. [ADR-0016](adrs/0016-automatic-local-spelling-learning.md) and the [component guide](personalization/README.md) define this extension.

A deterministic final stage records personalized output separately; undo restores raw/cleanup fallback. The reviewed-memory report separates usage from leakage-checked text replay and a bounded review queue; automatic entries are excluded from that reviewed-source replay. [ADR-0015](adrs/0015-model-independent-local-memory.md) retains the earlier manual and inference contracts. The static adapter registry declares the actual application capabilities and preserves existing native lifetimes. Execution evidence and per-cleanup-pass timings are available in diagnostics and, when collection is enabled, session provenance. See [ADR-0012](adrs/0012-adapter-registry-and-reviewed-personalization.md).

For selected Nemotron microphone capture, the order is important:

1. Verify Nemotron artifacts on the resident-model cache miss and load one recognizer.
2. Create fresh per-recording stream state before starting capture.
3. Send bounded, non-overlapping mono chunks and publish operation-ID/sequence-checked partials; partials remain transient.
4. On Stop, flush the capture tail, signal end of input, and drain native results.
5. Persist final raw ASR plus selected model/runtime provenance when collection is enabled, then publish it to the UI.
6. Load/reuse Qwen weights, create a fresh cleanup context, and process final raw text only.
7. Persist and publish cleanup, or retain raw text with a warning if cleanup fails or is cancelled.

Imported files and non-streaming adapters enter at final ASR instead. Full artifact hashes are attached to a newly loaded resident handle rather than recalculated for every request. Saving model/backend settings, replacing model files through the installer, or clearing a custom profile explicitly unloads resident handles.

The latest UI result is not itself durable. With collection enabled its session record is durable; without collection, exiting the application loses the result unless the user copies it elsewhere.

The native Tauri boundary exposes commands for settings/profile management, dataset import/review/export, model catalog/status/installation, audio-device enumeration, operation status/cancellation, recording, and file transcription. Notifications contain only success/failure wording and never transcript text.

## Model files and runtime ownership

Models are files, not database blobs. The effective model directory uses a saved `modelDirectory` when present; otherwise startup resolves `TRANSCRIBE_MODEL_DIR`, then the legacy `UKTI_MODEL_DIR`, then `STT_MODEL_DIR`, and finally the Tauri data-root `models/` directory. Debug development can use the repository's `models/downloads/` directory as the last legacy fallback.

`models/artifacts.json` is the executable built-in artifact manifest. An entry pins its model ID, source URL/revision, relative path, byte size, SHA-256, role, license description, and where relevant platform, backend, archive type, and quantization. Custom profiles are data-only and are restricted to an already implemented runtime contract; importing a profile never executes repository code.

Installation only accepts catalog entries. Downloads are streamed to a temporary file in the target directory, checked for declared length and SHA-256, synchronized, and atomically persisted. Descendant symlinks below the configured model root are rejected. UI inventory uses fast existence/size checks; installation and pre-inference verification calculate full hashes.

LFM2.5 Audio uses the compiled llama.cpp/mtmd adapter with an English-only CPU contract. Its backbone and projector share the existing worker lifetime; actual frontend/context/encoding-prefill/generation metrics are attached to execution evidence. See [ADR-0013](adrs/0013-lfm-audio-native-adapter.md).

Native runtime ownership is split:

- Rust owns settings, lifecycle, cancellation, storage, policy, and adapter selection.
- A bounded Rust inference worker owns at most one resident speech recognizer (Cohere, Whisper, Nemotron or LFM) and one resident writing model. Streams and decode contexts are request-local; LFM creates a fresh context per completed-audio segment.
- `transcribe-nemo` isolates the unsafe dynamic C ABI; NeMo-Speech.cpp C++, sherpa-onnx, and llama.cpp provide architecture-specific inference engines and kernels.
- Nemotron runtime archives are platform/backend-specific and unpacked under a versioned runtime directory.

Cohere Transcribe is the user-selected default ASR with Qwen writing cleanup enabled and explicit English for new settings. Existing valid saved selections remain authoritative. Cohere processes completed recordings; Nemotron is the selectable multilingual ASR with native streaming. Parakeet is an offline fallback, Qwen3-ASR is a completed-audio multilingual CPU option, and LFM2.5 Audio is an English completed-audio CPU option. Qwen3.5-0.8B is the implemented resident-weight cleanup model, with Q4 and Q8 installations exposed separately. Qwen3-ASR 0.6B uses per-job CPU ownership; Cohere retains verified sessions in the worker under [ADR-0017](adrs/0017-resident-cohere-recognizer.md). Both use internal bounded segments, processing progress and native completion reporting; LFM uses resident weights with fresh per-segment contexts. None has a recording/import time quota; see [offline ASR](offline-asr.md) and [LFM audio](lfm-audio.md). Kyutai, Voxtral, VibeVoice, and Moshi remain research/history or integration candidates; a name in research documentation is not equivalent to a runnable adapter or downloadable manifest entry. See [GitHub issue #48](https://github.com/rahulsharmavishwakarma/transcribe/issues/48) and the [canonical language and recommendation matrix](language-support-and-recommendations.md).

Whisper Small and Turbo share one architecture adapter and the selected resident Sherpa wrapper with Cohere under [ADR-0018](adrs/0018-shared-whisper-adapter.md). Native completion/confidence reporting, 29-second bounded segments and pinned INT8 installation are implemented. English/French completed-audio evaluation is available, but the product picker does not expose Whisper while its noise release gate fails. There are no live Whisper partials or qualified Hindi/code-switching/singing modes. See the [adapter validation](validation/whisper-adapter-2026-09-18.md).

VibeVoice-ASR-BitNet appears in benchmark capability metadata with `executable: false`. It has no application adapter or installable artifact entry. The [quantized CPU development benchmark](validation/vibevoice-bitnet-2026-09-18.md) verifies actual native execution and records memory growth, hotword/JSON failures, a Cohere comparison and a separate decoder-position experiment; it does not qualify production integration. The separate Streaming-1.5B checkpoint has a [49-case Python CPU reference screen](validation/vibevoice-2026-09-18.md), including paced and completed recordings. The [family inventory](research-vibevoice.md) found no complete VibeVoice ASR below two billion total parameters; BitNet was subsequently selected for quantized testing despite that count. The optional CPU runtime remains a separate [architectural proposal](https://github.com/rahulsharmavishwakarma/transcribe/issues/15); benchmark execution does not enable application model selection or inference.

Benchmark data is deliberately separate from application persistence. Public archives, extracted corpora, prepared PCM, machine environment files, raw reports, and detailed generated JSON live under the gitignored `.cache/benchmarks/` tree; they are not inserted into SQLite and never appear in user history. Checked-in benchmark material is limited to reproducible code, input manifests, compact capability metadata and fixtures, tests, and human-reviewed Markdown conclusions. Local detailed summaries use `.cache/benchmarks/results/<run-id>/debug-summary.json` and an ignored hash index maintained by `scripts/archive-debug-reports.py`. A generated manifest and every local report carry source/license metadata, sample hashes, application/runtime/model revisions, and the requested execution envelope. See the [ASR benchmark contract](../benchmarks/asr/README.md).

## Export format and durability

The export command creates a unique `transcribe-export-<uuid>` child in a user-selected existing directory. The storage layer itself requires a new or empty destination outside the live dataset. It builds a sibling hidden staging directory containing:

```text
transcribe-export-<uuid>/
├── metadata.json
├── manifest.jsonl
└── audio/
    └── <session UUID>.<original extension>
```

For every included record it copies and verifies the original audio, serializes the complete session, and adds:

| Field | Source |
| --- | --- |
| `taskEligibility` | `asr`, `cleanup`, both, or neither based on valid human labels |
| `asrTarget` | Human-verified verbatim speech only |
| `cleanupInput` | Original raw machine prediction when a cleanup pair is eligible |
| `cleanupTarget` | Human-approved edited text when a cleanup pair is eligible |

`metadata.json` currently declares export schema version 1, creation time, filtering mode, session count, manifest/audio paths, and label policy. Files and staging directories are synchronized before the staging directory is renamed into place. Nonempty destinations are never merged or overwritten; failed staging is removed. A reviewed export includes only task-eligible records, while an archive export preserves pending records with null targets.

## Privacy, security, and recovery boundaries

Speech inference and cleanup run locally. Normal operation makes network requests only when the user explicitly installs model artifacts; there is no transcript, audio, review, or telemetry upload path. Imported custom profiles describe downloadable data files and cannot introduce executable repository code.

Settings, SQLite rows, transcripts, exported JSON, audio, and model files are not encrypted by the application. They rely on operating-system file permissions and any device/full-disk encryption the user has enabled. Anyone with access to the application data directories or an export can read their contents. Secrets and credentials are not intentionally stored in the session schema.

Failure behavior is conservative:

- invalid settings are reported and preserved;
- failed audio publication removes partially published managed audio;
- raw text remains visible if cleanup fails;
- corrupt/missing model files block inference and can be reinstalled;
- corrupt/missing audio blocks export of the affected session;
- a failed export does not replace an existing nonempty export;
- SQLite lock contention waits up to five seconds and then surfaces an error.

There is no cloud recovery, automatic backup, database repair UI, session deletion UI, or automatic model/data retention policy. The supported portable recovery path is a successfully completed export. For a complete private backup, close the application and copy the settings file, the entire dataset directory, and—only if avoiding re-download matters—the model directory.

## Enforced limits

| Item | Limit/current behavior |
| --- | --- |
| Imported audio | 512 MiB per file |
| Transcript, label, or pipeline-event message | 100,000 Unicode scalar values |
| Custom cleanup instruction | 4,000 characters |
| Imported model-profile JSON | 32 KiB |
| Recording duration | No application time quota; ADR-0011 uses internal Qwen/Cohere segments and preserves resource bounds |
| Concurrent high-level operations | One |
| SQLite busy wait | Five seconds |
| Settings schema | Version 1 |
| Export schema | Version 1 |

These limits are enforced in code and should be changed there first; documentation must follow the implementation.

## Optional audio processing

[ADR-0019](adrs/0019-shared-audio-frontend.md) adds one shared frontend to the existing inference worker. Models → Audio processing enables advisory Silero speech intervals and optional DPDFNet4/8 noise reduction; both default off. Install the selected audio models through the existing download controls. Full recordings and live-capable ASR use the same stateful resampling/enhancement contract. Completed-only ASR processes the frontend at Stop.

Original recordings remain intact. Missing optional audio models do not block recording when the required speech/writer files are installed. Frontend startup failures use original audio with a warning; mid-stream failures invalidate live drafts in both views and use original audio for completed fallback. The UI reconciles drafts with the active operation even when failure and completion occur between polls. Cancellation is propagated. Execution evidence includes frontend artifact/runtime identities, load costs, resampling/native/finalization times, a bounded dispatch histogram, sample counts and capped advisory intervals. VAD never rejects audio; enhancement remains experimental because clearer audio can produce worse words. Ear/Uhm are not production stages.
