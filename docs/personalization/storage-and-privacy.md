# Learning storage, lifecycle and privacy

Implementation: [data-store schema](../../crates/vesper-data/src/lib.rs), [automatic store](../../crates/vesper-data/src/learning.rs), [profile validation](../../crates/vesper-data/src/personalization.rs), and [core profile](../../crates/vesper-core/src/personalization.rs).

## Schemas and provenance

Profile schema **4** adds bounded project mode/rules and defaults legacy profiles to manual scope. Schema 4 also gained `automaticWorkspaces` (off when absent; the user opts in). While it is on, the `workspace_detection` table keeps per-app detection counts (bundle identifier, result kind, latency bucket; never what was found), deleted when detection is turned off; results may now record the `workspace` or `category` selection reason. Results retain the effective project, selection reason and rule ID, never the observed document URL. User-selected folder roots remain explicit local preferences. Schema **3** introduced `automaticLearning`, a store-controlled `learningEpoch`, and vocabulary `automaticSource`/`usageCount`. The desktop reads profiles through `dictation_personalization()`: new profiles and schema 1/2/3/4 profiles carrying the retired off switch are saved with learning enabled. This advances the existing profile revision and learning epoch once, preserving manual words, disabled entries, application/hint choices and reviews. Repeated reads do not write or advance revisions. Desktop saves also enforce learning enabled, including requests from an older client. The generic `personalization()`/`save_personalization()` APIs retain explicit off/frozen controls for offline experiments and legacy-context regression tests; they are not the desktop learning preference. Unknown future schemas are rejected. Existing optimistic profile revisions remain mandatory.

A single-row `automatic_learning` SQLite table holds schema-2 JSON with policy version 5, bounded candidate records and bounded recent events. Policy 5 adds a recognizer family to each support record, plus at most seven days of rule-rejection times for the [policy breaker](automatic-dictionary.md#policy-breaker). Neither contains transcript text. Schemas 1–2 and policy versions 1–5 remain readable: missing `teachingTerms` defaults to an empty list, not reconstructed transcript context. Reads normalize the in-memory version without rewriting the stored row; the next evidence mutation persists schema 2 and the current policy version. Version normalization adds no correction support and does not replay previously rejected edits. Policy 4 withdraws support belonging to a revised result and pauses rejected automatic targets in the existing draft transaction; no additional transcript, audio or model artifact is stored. Unknown future schema/policy versions and oversized context metadata are rejected. It uses the same database, WAL and synchronous settings as the existing profile/session store. This is an application-memory extension, not a new service or a training database.

Events distinguish ordinary user edits from raw observations. They retain result/source/episode identity, language/project, model provenance, timestamp and optional compact changed pair or retry label. Model identity is evidence metadata, not the owner or key of a vocabulary item. Full raw/edited transcripts are not copied into the feedback table. `automaticSource` references a candidate; human-review source fields remain reserved for actual reviews.

Independent support now also retains up to 16 teaching terms of at most 64 UTF-8 bytes each. These unordered lexical cues come from the edit's before-text, excluding alias/target/project, quoted and fixed function words. They are additional locally retained text-derived metadata, not a full transcript or independently reviewed labels. Cue capture and active-profile revision changes commit atomically with the edit. Duplicate source/episode edits do not broaden retained context. A full legacy support list may replace a cue-less record with a fresh independent cue-bearing record while remaining capped at 16. See [dictionary use rules](automatic-dictionary.md#context-required-for-post-recognition-use).

## Bounds and expiry

The store retains at most **128 active vocabulary entries**, **64 reviewed examples**, **256 non-active candidates**, and **1,024 recent events**. Each serialized event is limited to **4,096 bytes**. A candidate has at most **16 compact independent support records**. Unsupported observed/provisional candidates and recent events expire after 30 days when the next learning/profile mutation prunes the store. There is no timer-based retention scheduler. Phase-A spelling learning runs on existing transcription workers and `spawn_blocking` draft commands when an event arrives, without extra model calls. [Phase B shadow analysis](semantic-shadow.md) separately uses the existing resident writer during idle time. A draft command awaits its transaction before confirming save, and result observation is awaited on the transcription worker. Background thread execution does not imply zero completion latency.

Active support remains compact after event expiry. Suspended entries keep their negative decision until explicitly changed, removed or displaced by the bounded candidate quota; their disabled vocabulary entries still prevent automatic re-enabling. Expiry and quota eviction are not evidence that a spelling is wrong. The inspector displays up to 32 recent candidates; the underlying store remains separately bounded.

Teaching cues remain with active compact support after recent-event expiry, so a learned word does not lose all context merely because its event aged out. Suspending a candidate clears its cues. Removing/evicting a candidate removes its owned cues; Forget clears the entire automatic state. The term-content maximum is 16 KiB per candidate (384 possible retained candidates give a 6 MiB theoretical term-byte ceiling), plus identifiers and JSON overhead; this is a data bound, not a measured process-RAM budget. One operation snapshots profile and learning state together and neither reads live evidence during cleanup nor mutates that snapshot.

## Automatic workspaces

The `workspace_store` table holds one row: a random per-install salt and a
bounded JSON state (at most 384 anchor nodes and 64 workspaces). Nodes keep only
salted folder hashes, up to four salted document hashes, up to eight local day
numbers and a count. A workspace row adds its kind, category, dictation count and
first/last day. Its label (a folder's last component or a web host) is written
only when the workspace becomes visible, and is cleared by Remove or Merge.
Raw document paths, URLs, URL paths or queries, window titles and document text
are never stored; the observed path or host exists only in memory during capture
start and the follow-up write. No folder path is stored; a `miningRoot` path left
by the removed folder opt-in is erased when the store is next opened.
Removed workspaces keep a suppressed salted key so they are not detected again.
The salt is never shown, logged (`Debug` redacts it) or included in exports.
Rename and Merge rewrite the project name on scoped vocabulary, examples and
learning candidates in the same transaction; learning events and History keep
their original scope. See [automatic workspaces](context-selection.md#automatic-workspaces).

## Undo, conflicts and deletion

Mutating SQLite transactions reserve the writer before reading their inputs.
This prevents a background shadow write from invalidating a foreground draft's
read snapshot before it saves. Read-only profile/evidence snapshots remain
deferred; shadow-state pruning uses an existing mutation transaction or opens
its own immediate transaction. Background completion keeps its 25-ms lock-wait
limit and may abstain when busy. See SQLite's [WAL isolation contract](https://www.sqlite.org/isolation.html).

Undoing an applied inferred memory disables its vocabulary entry and suspends the candidate transactionally. An ordinary edit reverting the canonical spelling to the known alias is also negative evidence. Neither action learns an automatic reverse rule. Undo learning preserves the user's authored draft.

Clients cannot forge candidate provenance, usage counts or learning epochs. Changing an automatic entry's spelling/language/project/context in the inspector converts it to manually managed text and clears its automatic attribution. Explicitly re-enabling a suspended entry is likewise a manual decision. Existing reviewed sources keep their stricter review validation.

Removing an automatic entry deletes its candidate and matching compact correction evidence. Forget automatic learning removes all automatic vocabulary, candidate/event evidence and usage ranks while preserving manual words and reviewed sessions. It increments the learning epoch, so a late callback from a pre-forget result cannot recreate the deleted evidence. New recordings automatically provide fresh evidence; forgetting does not turn learning off. This is logical deletion from application records; it does not claim forensic erasure of SQLite pages or backups.

The migration from the retired opt-in gate also advances the epoch. An operation must have learning enabled in its frozen context and match the current enabled profile/epoch before it can add evidence. Legacy disabled contexts remain ineligible. Users can disable individual spellings or final correction application without stopping evidence collection; automatic admissions never re-enable a disabled manually managed spelling.

## Collection and exports

Audio retention follows `collectTrainingData`. Automatic local learning does not enable audio collection. With collection off, minimal feedback records still persist and the current draft remains ephemeral. The desktop creates its local SQLite store when preparing its first personalization snapshot even if audio retention is off; no source audio or full draft is archived by that step. A single bounded recent microphone transcript may remain in runtime memory for retry comparison, as described in [retry evidence](retry-evidence.md).

The future training `improvementMode` setting is unchanged. An ordinary draft or inferred candidate does not become a supervised export target. Export eligibility still uses separately reviewed verbatim/cleanup targets. The feedback store is not included as a training-label source. Everything runs locally using installed components. Ordinary in-app learning performs no clipboard polling or external observation.

## Semantic shadow text

The separate local `semantic_shadow` table retains up to 256 bounded edit requests/proposals for 30 days, pruned on store access. It can contain before/after and one earlier-attempt text even when audio collection is off; it stores no audio and creates no training session. Undo removes a result’s evidence; Forget clears all shadow records and invalidates old callbacks. Exact request/revision/epoch checks prevent stale completion from recreating them. Shadow guesses never enter active vocabulary, preferences or reviewed exports. See the [full contract](semantic-shadow.md#persistence-deletion-and-isolation).

Policy shadow storage retains compact pre-event counterfactual scores/source identities, 256 cases, 32 routed scopes and eight decisions per scope for 30 days. It contains no raw transcript/profile snapshot. Its one pending job is never replayed after restart. Neither table enters active vocabulary or reviewed exports. Undo invalidates related evidence; Forget clears shadow tables and pending reservations. The removed TextEdit observer's retained rows are dropped when an existing store is opened; see the [removal note](external-observer.md).

## Term inventory

The [term inventory](term-inventory.md) adds three tables to the same SQLite store. They are created with `CREATE TABLE IF NOT EXISTS` when the store opens; existing data is untouched.

- `term_inventory`: one row per surface spelling. Each row holds:
  - kind, tier and status (`unconfirmed`, `active`, `suspended`, or the `removed` tombstone)
  - up to four spoken forms
  - evidence entries of `(source, workspace scope, count)`
  - document frequency, and first- and last-seen times
- `term_mining_state`: per-workspace scan cursors, paused sources and workspace display labels. A cursor holds a SHA-256 fingerprint over tracked paths, sizes and modification times, a file index, and partial candidate counts. It never holds a path or file text.
- `term_mining_runs`: the last 200 worker runs, with counts and timings only.

Bounds:

- At most **20,000** inventory rows. Beyond that, the unconfirmed tier-B rows with the least evidence and the oldest sighting are evicted. Tombstones and asserted rows are not evicted.
- At most 4,096 asserted values per source per run.

Removing a word writes a tombstone that clears its evidence and spoken forms. Mining never re-adds that spelling in any letter case. **Forget words from this source** deletes that source's evidence and pauses the source until you resume it. Rows left with no evidence are deleted; suspended rows keep their status. Forgetting a tier-B source also clears scan cursors, so a resume rescans from scratch.

The inventory never stores file paths, document text, URLs or window titles. Workspace scopes are opaque keys, and labels are folder names. Folder mining runs only for developer-configured roots. Deletion is logical; it does not claim forensic erasure of SQLite pages.

## Learning from other apps

Both features are opt-in (`appFieldWords`, `learnFromAppEdits`, off when absent). Field words are read before the caret and held in memory for one recording; nothing is stored. A read-back stores only the corrected pasted span, as that result's draft, through the same path as an in-app edit. The `app_learning_coverage` table keeps per-app counts (bundle identifier and counts only) and is deleted when both features are off. See [app learning](app-learning.md).

The `paste_coverage` table counts automatic paste outcomes per bundle identifier (sent, no field, no permission, secure input, field changed, clipboard, other). For a capture that found no field it keeps that app's latest diagnostic: a reason and the focused element's Accessibility role, subrole and editability flags, never text. It is local, bounded to 64 apps and used to find apps where paste fails.
