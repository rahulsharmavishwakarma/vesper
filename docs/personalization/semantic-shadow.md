# Local semantic shadow analysis

Phase B adds automatic analysis of committed in-app draft edits using the already-resident local Qwen writer. A proposal can classify an edit/attempt relation, suggest a supported output form and identify relevant supplied vocabulary IDs. **Proposals are evaluation evidence only. They never change dictation, active vocabulary, output preferences or reviewed training labels.** The existing spelling learner continues to control production memory.

The owner approved the [concrete proposal in #59](https://github.com/rahulsharmavishwakarma/transcribe/issues/59#issuecomment-5740928523); the [approval record](https://github.com/rahulsharmavishwakarma/transcribe/issues/59#issuecomment-5741340042) identifies its scope. This mechanism does not establish recursive algorithm improvement or personal efficacy.

## Request and evidence contract

`SemanticAnalysisRequest` schema 1 carries the authoritative before/after edit, committed result revision, result/source/episode identity, learning epoch, project, language and model provenance and explicit `in_app_draft_edit` origin. Authored evaluation requests are a separate origin and cannot reserve application shadow records. At most one earlier microphone attempt is included, within 120 seconds and the same project/language/epoch. Imported files do not supply microphone retry context. Combined text is limited to 2,048 UTF-8 bytes; over-budget edits abstain without truncating the user's text. At most eight enabled, scope-compatible vocabulary IDs/phrases are supplied. Selection is bounded; it is not an exhaustive retrieval evaluation.

The response schema allows `repair`, `continuation`, `changed_intent`, `unrelated` or `unknown`, an optional `verbatim`, `note`, `message` or `technical` form, and supplied memory IDs. Relations compare the earlier attempt to the corrected text when present, otherwise before to after. Every positive claim must cite supplied text spans. UTF-8 boundaries, nonempty spans, required sides, supported enums, duplicate/unknown IDs and extra keys are checked. Markdown, malformed or truncated JSON is rejected. Valid spans establish the existence of evidence, **not the truth of a model's interpretation**.

The prompt supplies full-text byte spans to avoid making the model calculate Unicode offsets. Quoted chat delimiters are JSON-escaped so transcript text cannot create a chat role. Semantic prompt injection can still cause wrong classifications; shadow isolation prevents those guesses from altering output. There is no confidence-based admission.

## Scheduling and resources

Normal draft commits schedule analysis automatically. The desktop first commits the draft through its existing spelling-learning transaction; a shadow failure does not fail that edit. The inference manager retains its existing single native-model owner. Its separate background slot holds one pending request; newer work cancels/coalesces older work. Foreground commands win, and capture, new results, settings changes and Undo invalidate pending application requests. No extra model is loaded for analysis; an absent, different or imported writer produces `unsupported`.

Each supported analysis has a fresh context, at most **768 total prompt tokens** and **192 generated tokens**, deterministic sampling and a **2-second soft deadline**. Checks run between 128-token native prefill batches and generated tokens. Synchronous native decode/context work is not forcibly interruptible. Completion persistence also takes time. It declines a contended runtime lock and uses a 25 ms SQLite lock-wait budget; neither setting bounds filesystem latency. These limits are not a hard 2-second bound on foreground delay. The worker checks pending background work after a 50 ms idle queue interval and sleeps without periodic wakeups when no job is pending. No cloud inference, embeddings, new checkpoint, decoder or kernel is involved.

Queue counters expose submissions, coalescing, cancellation and completion for qualification. Reports preserve terminal status, token counts, inference elapsed time and artifact/runtime/provider identity without raw rejected model output. A coalesced request for a replaced edit is superseded by the newer reservation; pending records left by process exit or completion-lock contention are never automatically replayed. No process-restart success is inferred from their presence.

## Persistence, deletion and isolation

The existing SQLite database gains a separate `semantic_shadow` table, limited to **256 result reservations/proposals** and **30 days**. A later edit replaces the same result's earlier reservation. Pruning occurs on store open and shadow reads/writes; there is no clock-driven secure-erasure service. Stored requests contain bounded before/after text and optional earlier text, even with audio collection off. They contain no audio and create no training session.

Completion updates only the exact still-pending serialized reservation. Runtime result/revision checks and a transactional epoch/project check prevent stale callbacks from publishing useful proposals after changes. Undo learning deletes that result's reservation; Undo personalization also deletes associated saved or unsaved evidence. Saved-session reservations require a matching committed draft/revision and cannot be created after that result was rejected; Forget deletes every shadow record and advances the existing learning epoch. Pending callbacks cannot insert a missing reservation. Changes that invalidate automatic-learning provenance also clear shadow evidence. These are logical SQLite deletions, not forensic erasure guarantees.

Analysis is separate from active profile reads, spelling admission, cleanup examples and dataset export. It cannot grant itself stronger permissions, train a model or promote a live adaptive policy. Model identities are provenance, never the key to a user's memory.

## Qualification and research boundaries

Core/data/runtime tests cover malformed evidence, Unicode spans, forged memory IDs, exact scope, repeated completion, coalescing, capture cancellation, persistence bounds, restart, Undo, Forget and old-connection callbacks. The installed-model screen is [semantic_shadow.rs](../../crates/vesper-inference/examples/semantic_shadow.rs), using separate [development](../../benchmarks/cleanup/semantic-shadow.json) and [holdout](../../benchmarks/cleanup/semantic-shadow-holdout.json) fixtures. These are authored labels, not natural longitudinal feedback or an accent-recognition benchmark.

The first prompt produced **0/24 valid responses** on Metal. A diagnostic found unquoted enum values copied from its schema instructions; that prompt remains a failed result. Prompt version 2 supplies valid JSON examples and explicit spans. A five-class example variant increased valid formatting but reduced relation accuracy and was rejected. Measurements and the final decision are recorded in [evaluation](evaluation.md#semantic-shadow-qualification).

Reproduce with installed artifacts, separately for CPU and Metal:

```sh
. ./scripts/dev-env.sh
cargo run --locked -p vesper-inference --example semantic_shadow -- \
  models/downloads benchmarks/cleanup/semantic-shadow-holdout.json \
  /tmp/semantic-shadow-metal.json metal
```

The example's Metal-compatible ASR selection is configuration only; it runs no acoustic model. Latency pairs measure a resident foreground cleanup request with/without a running semantic job, alternating order. They exclude microphone capture, ASR, Tauri/UI rendering, insertion and desktop persistence. Preserve failures and abstentions in classification denominators. Native app and natural-user qualification remain distinct gates.

Paper evidence: [GEC-RAG](https://arxiv.org/abs/2501.10734v1) motivates retrieving independent correction evidence; [PROSE](https://arxiv.org/abs/2505.23815v1) motivates preference inference from writing. Neither establishes this classifier's accuracy. Official capability: the pinned [Qwen template](https://huggingface.co/Qwen/Qwen3.5-0.8B/blob/f35bff78516d5b109092c941393bb6b124723ab1/chat_template.jinja) supports local non-thinking generation. Local evidence is the tests and benchmark here. Better semantic selection and reduced user correction burden remain hypotheses until separate qualification passes. This is frozen-weight application work, not runtime-internal or checkpoint research.
