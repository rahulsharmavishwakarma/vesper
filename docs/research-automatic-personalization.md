# Automatic personalization from dictation usage

The [September RSI review](research-rsi-2026-09.md) adds Dream-RSI and six related papers, dated lab/engineering posts, implementation-availability checks and prioritized local experiments. It distinguishes the tested automatic memory flow from the offline recursive-policy experiment.

Reviewed: **16 September 2026**. Recent research window: **16 September 2024–16 September 2026**, measured from first public publication, not a later arXiv upload. This document preserves the research and proposal rationale. **Phase A was subsequently approved and implemented**; the [implementation guide](personalization/README.md) and [ADR-0016](adrs/0016-automatic-local-spelling-learning.md) describe the actual behavior, automatic desktop policy and narrower limits. Semantic phase B remains deferred. The original proposal is in [issue #34](https://github.com/rahulsharmavishwakarma/transcribe/issues/34#issuecomment-5688658223).

The intended experience is: **dictate normally, correct naturally, and encounter fewer recurring mistakes over time**. Users should not have to maintain a dictionary or press “Remember” after each correction. All inference and memory updates stay on device; the application owns the memory across model changes. Training remains deferred until a separately approved own-model project.

This extends [the six-capability research](research-personal-dictation.md). It concentrates on the missing automatic feedback loop rather than repeating that document's accent, adapter and competitor survey.

The [phase roadmap](personalization/roadmap.md) organizes the remaining implementation directions and their validation requirements. It preserves A/B from this proposal and distinguishes optional external-editor observation and own-model training from the current on-device memory loop.

## What the research supports

These are **paper results in their original settings**. The application recommendations below are our engineering synthesis. No reported accuracy, speed or confidence value transfers to this application's checkpoints, languages, hardware or short dictation workflow.

| Primary source; first public date and version reviewed | Evidence and supervision | Useful transfer and limitation |
| --- | --- | --- |
| [PROSE: Aligning LLMs by Predicting Preferences from User Writing Samples](https://arxiv.org/html/2505.23815v1), 2025-05-27, v1; ICML 2025. [Official Apple code](https://github.com/apple-aiml-research/ml-predict). | Infers component preferences from writing demonstrations; refines them and checks consistency across previous examples without fine-tuning. Tests include Qwen2.5-7B/72B; GPT-4o simulates users. | Supports automatically learning style from ordinary user-authored examples. Multiple generation/judging passes cost inference time. It does not validate this app's 0.8B model, silent acceptance, or a ready-to-run offline integration. |
| [GEC-RAG](https://arxiv.org/html/2501.10734v1), 2025-01-18, v1. | Retrieves paired ASR predictions and independent ground-truth transcripts using lexical similarity, then supplies correction examples to GPT-4o. Experiments use Whisper-large-v3 and Persian speech. | Supports retaining useful corrections outside ASR weights. The desired spelling comes from the reference, not repetition of a model output. Replacing its hosted correction model with local cleanup requires evaluation. |
| [Retrieval Augmented Generation based context discovery for ASR](https://arxiv.org/html/2509.19567v2), first 2025-09-23; v2 2025-11-19. [Official proceedings](https://aclanthology.org/2025.findings-emnlp.768/). | Uses preceding ASR segments to retrieve relevant context words from an existing inventory with pretrained embeddings; recognizer weights stay fixed. | Supports automatic vocabulary selection. Discovering relevant inventory entries differs from discovering the correct spelling of a new name. Requires contextual-biasing support; English long-form results do not establish short Hinglish dictation performance. No author implementation was identified in the paper. |
| [ReSpect: Retrospective Learning from Interactions](https://arxiv.org/html/2410.13852v2), first 2024-10-17; v2 2025-05-20, ACL 2025. [Official code/data](https://lil-lab.github.io/respect/). | Decodes implicit feedback from subsequent conversation, then **trains** an action policy. Studies human interaction over tangram selection with IDEFICS2-8B. | Follow-up language can reveal failure without a rating button. Full method is training, not a memory configuration. Moving to a new target and continuing after a mistake complicate feedback inference. It provides no direct ASR retry detector. |
| [Error and Recovery Analysis of Intent-based and LLM-based SDS](https://www.isca-archive.org/slate_2025/pepe25_slate.pdf), SLaTE 2025-08-22–24; proceedings version, no earlier version located. | Human coding of 50 L2 English speakers separates ASR errors, dialogue errors, repetition, rephrasing, changed orders and topic shifts. Recognizer is the same in both dialogue conditions. | Supplies useful repair categories. Repeated failures and uncorrected misunderstandings show why retries and apparent acceptance need interpretation. Narrow café roleplay; no public interaction dataset/code link identified. Lower WER across conditions is not evidence of an adapted recognizer. |
| [AdaMem](https://arxiv.org/html/2606.21144v2), first 2026-06-19; v2 2026-09-02. [Official code](https://github.com/galaxyChen/AdaMem). | Updates context-specific natural-language memory-writing policies without parameter updates. Both feedback modes include answer confirmation or the correct answer after an error; “implicit” omits the abstract preference rule. | Supports selective, scoped memory creation. Synthetic histories and large-model results do not validate passive dictation learning. Learning a good policy and reliably following it are separate problems. |
| [MemoryBench](https://arxiv.org/html/2510.17281v1), first 2025-10-20; v1 protocol reviewed. [Current paper, v7 2026-06-03](https://arxiv.org/abs/2510.17281), [official code](https://github.com/THUIR/MemoryBench), [data](https://huggingface.co/datasets/THUIR/MemoryBench). | Accumulates feedback in batches and evaluates on a separate test set after each batch. Includes simulated verbal/action feedback. | Useful longitudinal evaluation pattern. Simulated “copy” and “like” events are not evidence that actual copies mean correct dictation. Published infrastructure is not an on-device latency result. |

Relevant older background is deliberately outside the window:

- [CIPHER/PRELUDE](https://arxiv.org/abs/2404.15269), first **2024-04-23**, with [official code](https://github.com/gao-g/prelude): infers latent preferences from edits without personal weight updates. Useful precursor to PROSE, not newly published late-2024 evidence.
- [An Efficient Self-Learning Framework for Interactive Spoken Dialog Systems](https://proceedings.mlr.press/v235/tulsiani24a.html), **ICML 2024-07-21–27**: uses reformulations, contextual teacher training and distillation to a streaming ASR student. Its September 16 arXiv upload does not make it recent within this window. Timing and text similarity help mine candidates; the full method requires training. [Official OD3 data](https://github.com/amazon-science/amazon-od3) is semi-synthetic dialogue, not natural personal dictation history.

## Application capability when the proposal was written

This section is **local source inspection**, not a paper claim or a new experiment. The worktree contains substantial uncommitted source beyond HEAD `592e0084fe5cf486b4d56c575423e9a4f219bfb8`; that commit alone cannot reproduce the current application.

| Existing component | What it provides | What automatic learning still needs |
| --- | --- | --- |
| [Core profile](../crates/transcribe-core/src/personalization.rs) and [memory application](../crates/transcribe-core/src/memory.rs) | Model-independent vocabulary, exact aliases, scope, bounded retrieval/application and guarded fallback. | Candidate evidence, usage ranks, promotion/suspension states and source types beyond reviewed revisions. |
| [Data store](../crates/transcribe-data/src/personalization.rs) | Reviewed targets, source validation, optimistic revisions, explicit remember, uses/undos and bounded replay. | Versioned interaction events and retry relationships; automatic admission without relabeling inferred evidence as a human review. |
| [Main transcript](../apps/desktop/src/lib/components/TranscriptionPanel.svelte) | Read-only results, Copy and undo. History provides explicit correction/review. | Editable final draft and an observable, completed user edit. Copy currently sends text to the clipboard; it does not establish accuracy. |
| [Capture orchestration](../apps/desktop/src-tauri/src/dictation.rs) and [native platform bridge](../crates/transcribe-desktop-platform/src/lib.rs) | Per-operation capture and native final insertion with target revalidation. | Semantic relationships between recordings. The bridge does not expose later edits made in the receiving application or a persistent document identity for learning. |
| [Inference manager](../crates/transcribe-inference/src/manager.rs) | Native ASR and a constrained `rewrite_personalized` cleanup task. | A structured repair/preference analysis task would be a new API path. The cleanup method cannot simply be treated as an existing generic classifier. |

The accepted loop still requires **save review → separately confirm memory**; see [ADR-0015](adrs/0015-model-independent-local-memory.md). A button labelled retry after a recording failure is not evidence that an earlier successful transcript was semantically wrong. Importing a file twice or regenerating cleanup is not a fresh user correction.

The [existing public-audio regression screen](../benchmarks/asr/2026-09-16-memory-regressions.md) tests frozen remembered corrections and application guards. It does **not** test automatic vocabulary discovery, retry attribution, evolving preferences or reduced user effort. Those capabilities have no local quality or average-latency measurement yet.

## Recommended product behavior

The broader semantic design remains an **unverified engineering proposal**. Phase A and the automatic desktop policy are implemented as recorded in issue #34; the component guide owns their exact contracts.

The user's 2026-09-18 requirement supersedes the original **“Learn as I dictate”** opt-in proposal: automatic local learning is core desktop behavior, explained once, with no extra enable or per-entry Remember step. Keep existing dictionary editing as an optional inspector. Occasionally show a quiet “Remembered `kubectl` · Undo” message, not a modal confirmation for every word. Retain removal, undo and forgetting separately from applying already saved memory; the desktop has no learning enable/disable switch. See [ADR-0016](adrs/0016-automatic-local-spelling-learning.md) and the [implementation contract](personalization/README.md) for current execution and evidence limits.

The original one-time migration choice is also superseded: the desktop enables learning for new and existing profiles automatically, preserving disabled vocabulary and audio-retention choices. Existing revision and epoch checks protect stale results and deletion. The change does not widen spelling admission or qualify automatic semantic learning.

| Signal available during ordinary use | What can be learned automatically | What it does not establish |
| --- | --- | --- |
| User edits an in-app term to a particular spelling | Canonical spelling candidate; possibly a narrowly scoped confusion alias after bounded alignment and admission checks. No second memory confirmation. | Every edit is not an acoustic correction: it can change facts or writing intent. |
| A term recurs in independent, user-authored changed spans | Stronger evidence for spelling and the contexts where it is useful. | An unchanged surrounding model-generated paragraph does not become authored evidence because one word was edited. |
| A word recurs in raw dictation output | Usage/topic statistics and ranking of already credible vocabulary; new observed candidates. | The recognizer may repeat the same misspelling. Frequency alone cannot validate a permanent replacement. |
| A user undoes an applied memory | Evidence against that application; suspend its inferred alias pending reconciliation. | Whether the cause was a wrong spelling, wrong scope or a changed intention. |
| A user records again soon after a result | Candidate repair relationship and possible dissatisfaction. | Same intent, which stage failed, or that the second transcript is correct. |
| Explicit repair language or spelled letters | Stronger local evidence for a repair and its intended term, if recognized correctly. | Infallible transcription of the repair itself; unrestricted global aliases. |
| Copy, paste success, no edit, or moving on | Exposure/usage information when observable. | Positive correctness labels, true intent or proof that a vocabulary entry helped. |
| Repeated wording/punctuation edits in comparable contexts | Candidates for bounded presentation preferences and output-form selection. | Permission to add recipients, facts, dates, commitments or actions. |

If the app repeatedly hears an unfamiliar name incorrectly, passive usage cannot reveal an unknown spelling by itself. A naturally typed correction, correctly recognized spelling, or another credible user-authored occurrence supplies the missing evidence. The common path should learn automatically when that evidence arrives; rare ambiguity can remain unresolved without interrupting every dictation.

### Understand the retry before extracting a correction

Keep **relationship**, **error cause** and **desired correction** separate. Proposed relationship labels are `same_intent_repair`, `continuation`, `new_task`, `changed_intent` and `uncertain`. Cause labels are `capture`, `asr`, `cleanup`, `presentation` and `unknown`. A repair can contain multiple causes; do not force a single explanation when the evidence disagrees.

Synthetic examples, not user transcripts:

| Observed sequence | Appropriate interpretation |
| --- | --- |
| “Use cube control” → “I said kubectl; k-u-b-e-c-t-l” | Possible recognition/spelling repair. Align the term and retain where the spelling evidence came from. |
| “Send it Friday” → “Actually, make it Monday” | Changed intention. Never learn a dictionary alias `Friday → Monday`. |
| Dictation → “Make that less formal” | Presentation feedback; original words may have been recognized correctly. |
| Message → “Also add the meeting link” | Continuation, not evidence that the earlier message was wrong. |
| Two similar messages for different purposes | Possibly a new task, despite short elapsed time and lexical overlap. |
| Two recordings return the same wrong name | Correlated predictions, not two independent confirmations. |
| Recording fails to capture audio → user tries again | Capture recovery; no spelling pair exists. |

Use a bounded recent window plus actual repair cues and known context to find candidates. A time gap or similarity threshold alone must not cause a persistent replacement. Existing project/language selections are usable context; automatic document context is currently unavailable. Preserve both attempts and their relationship when retained. Do not silently overwrite the earlier result or automatically replace text already inserted elsewhere.

Within-recording repair (“Friday, sorry, Monday”) and cross-recording retry are different tasks. Verbatim references preserve the spoken words; a cleaned target may resolve an explicit repair. Neither creates a global date or negation substitution. Accent-related recurring confusions can become text memory, but this does not detect an accent or adapt acoustic weights.

### Let memory accumulate evidence without reinforcing its own guesses

Use a small lifecycle: **observed → provisional → active → suspended/expired**. Record the evidence type and the permitted action independently. An observed word can help rank existing context without becoming an active replacement rule.

Proposed initial admission rules, to test rather than describe as calibrated confidence:

1. A completed user edit that supplies a guarded, unambiguous technical spelling in a known project can activate a scoped alias automatically. One ordinary edit is enough in this narrow case; no extra confirmation.
2. Other spelling/name candidates need agreement from at least two independent completed user edits in compatible scope before automatic alias activation. A canonical term may be eligible for retrieval before any alias is justified. Common words, quoted content, numbers, dates, negation, conflicting mappings and broad rewrites remain outside automatic replacement.
3. Machine-only recurrence changes observed usage counts. It cannot satisfy user-authored support. Retry-derived wording without independent spelling evidence stays provisional; it may help identify relevant existing memory, but cannot rewrite future unrelated dictation.
4. An undo of an inferred application suspends that alias and restores the current machine fallback. An ordinary edit reversing the alias records negative evidence too. Do not keep applying it until the user finds the dictionary screen.
5. Distinct models, cleanup reruns, imported duplicates and multiple attempts within one repair episode count as one correlated episode, not independent teaching examples. Tag memory-applied spans so later unchanged output cannot corroborate the same memory.

The one/two-edit thresholds above are conservative starting hypotheses, not paper-derived guarantees. Calibration must measure false activation and coverage; lexical guards alone do not establish semantic correctness in every language.

Keep canonical terms across model switches. Record model/runtime provenance for diagnosis and comparison; a model switch does not transfer memory ownership or erase vocabulary. Error aliases can lose usefulness or become harmful with another recognizer, so application evidence and qualification stay separated by model/runtime/language envelope.

Apply recency to context selection and decay unused provisional entries. Conflicting spellings in different scopes can coexist; a global conflict should abstain, not silently choose whichever was seen last. User deletions must remove derived candidates and prevent resurrection from retained support. Retention expiry can compact supporting evidence into a minimal active-entry provenance record; an explicit “forget” action has stronger semantics than ordinary expiry.

### Keep storage and feedback proportional to the feature

Extend the existing SQLite store rather than introducing a memory service or vector database. The proposed additional records are:

- Interaction events: operation/result IDs, event sequence/time, source (`user_edit`, `raw_observation`, `retry_candidate`, `memory_undo`), changed spans or bounded term evidence, applicable language/project, and profile revision. Commit an edit only after an actual text change is finished; focus/blur alone is not feedback. Use result revision checks so late frontend events cannot edit a newer result.
- Repair episodes: references to attempts, inferred relationship/cause, supporting cues, uncertainty and analyzer version. An ephemeral capture identity does not establish a shared external document.
- Candidates: canonical spelling/alias, scope, evidence references, independent-episode count, contradiction/undo evidence, state, last-supported time and admission-rule version.

Initial resource envelope to validate: 256 pending candidates, 1,024 recent evidence events, and 30-day expiry for unsupported candidates/events; keep the existing 128 active vocabulary and 64 example limits. Store compact support for active entries without preserving an unlimited transcript history. These are proposed engineering limits, not measured optima. Enforce a byte budget per event and transactional quota eviction during implementation.

Treat learning permission separately from full audio retention. With collection off, do not silently retain recordings or whole transcript histories. An enabled local-learning choice can retain minimal spelling/edit evidence; when disabled, do not persist those events either. Do not repurpose `improvementMode = automatic`, which currently describes a future training-promotion policy.

Keep inferred interaction evidence separate from the reviewed training dataset. A desired edited sentence can teach presentation without becoming a verified audio transcript. Automatic candidates and repaired guesses must never silently enter a supervised export. No raw keylogging, clipboard polling, screen scraping, gaze tracking or external-app document scanning is needed for the first implementation.

The limitation is concrete: edits made in an external editor after insertion are invisible today. Supporting that workflow would need a separate scoped platform observation design and evaluation. Its absence should be visible in product claims about what the app learns from.

## Implementation boundaries and delivery order

| Category required by the project | Proposed work and exact boundary |
| --- | --- |
| **1. Frozen-weight application/runtime work supported by existing public inference APIs** | Add ordinary-edit observation, local usage/candidate records, deterministic admission, scoped retrieval ranking, undo/suspension and chronological evaluation. Existing text correction works across adapters; existing optional Nemotron hints and Qwen example retrieval can consume eligible entries within their current budgets. These storage/UI changes still require architectural approval. |
| **1, but a new application API is required** | A later bounded local task for semantic retry classification and style-component extraction, returning structured evidence spans and allowlisted preferences. This can use installed frozen weights, but the current cleanup API does not provide it. Requires quality evaluation, cancellable scheduling and a reviewed API addition; there is no assumed 0.8B capability or free inference time. |
| **2. Runtime-internal decoder/kernel changes** | No such work is required for the proposed first phase. New native biasing for an unsupported architecture, phonetic decoding or different beam search would need exact runtime support and separate evidence. |
| **3. Training/checkpoint changes** | ReSpect policy learning, contextual ASR teacher/student adaptation, trained repair models and acoustic accent adaptation remain deferred. They are not options that can be switched on for downloaded weights. |

**Recommended phase A:** editable final drafts; automatic spelling/candidate learning from ordinary edits; usage-based selection of credible terms; local candidate retry linking without promoting guessed corrections; evidence lifecycle, undo and temporal replay. Reuse Rust, SQLite, existing memory guards and inference lifetimes. No additional inference model or dependency is required.

**Research phase B:** semantic retry/cause analysis, recognition of spelled repairs, and consistent writing/intent preferences across examples. Compare a small structured local analysis task with deterministic rules and with simply retrieving user-authored examples. Only allow supported output-form/style values; do not let an extracted free-text rule authorize arbitrary actions. Start with shadow evaluation, then qualify automatic application. No change to user-selected output mode is implied by phase A.

For phase B, inference should run only when the existing worker is idle, with one bounded cancellable analysis job, a frozen input/profile snapshot, and fresh sequence state. New dictation takes priority; cancelled/stale analysis must not publish memory. If cancellation latency is inadequate, defer analysis instead of moving or duplicating model ownership. This scheduling proposal itself needs validation.

Official capability anchors remain those in [ADR-0015](adrs/0015-model-independent-local-memory.md#evidence-and-boundaries): NeMo-Speech.cpp v0.1.0 `4f9676226f667d14608487df744f375db87127f8`; sherpa-onnx Rust 1.13.8/native `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`; llama-cpp-2 0.1.156/native `e79e4bf660e19f2ad851e06c6913f7a8c5852621`. Nemotron model revision `ea30d66debe3740a08b573244286791d423d6b3e`, Cohere `156a470cf08eefe706a0004f3c52d9ee567ca7a0`, Qwen GGUF `8fea620810c4afa23dd6443f999a48574c1611a3`; artifact hashes are recorded in that decision and benchmark manifests. No runtime/model revision is changed here. Nemotron's phrase hints stay opt-in and bounded; Cohere's selected adapter has no native phrase hints.

## How to establish that it improves with use

The unit of evaluation must be a **chronological interaction episode**, not just an isolated audio file. Public FLEURS/Svarah speech can test recognition and dictionary interference; it does not say whether two recordings represent a retry, changed intention or new task.

Candidate evidence resources:

| Resource | Permitted inference from an experiment | Access/qualification work before reuse |
| --- | --- | --- |
| PROSE's PLUME/PRELUDE examples, linked above | Controlled preference acquisition and later-context application. | Pin code/data; simulations are not observed user editing behavior. Do not run hosted API defaults for local evaluation. |
| MemoryBench code/data, linked above | Controlled accumulation of feedback and held-out learning curves. | Official repository/data advertise MIT; upstream datasets retain their own terms. Pin the selected release. Keep simulated action feedback labelled synthetic. |
| [LongMemEval](https://arxiv.org/abs/2410.10813v2), first 2024-10-14, v2 2025-03-04; [official code](https://github.com/xiaowu0162/LongMemEval), [cleaned data](https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned) | Temporal updates, stale knowledge and abstention test categories. | Constructed conversational histories, not real dictation logs. Official code/data advertise MIT; replace the default hosted judge with deterministic/local human scoring and disclose that deviation. Answer-level forgetting is not proof of physical deletion. |
| OD3, linked as older background | Controlled spoken reformulations with known injected failures. | Semi-synthetic/TTS material; inspect the chosen distribution's license and manifest before downloading or redistributing. |
| New labelled synthetic sequences | Deterministic edge cases, including changed dates, quotes, repeated mistakes, duplicate audio and cancelled operations. | Explicitly synthetic; lock labels before prediction. These establish behavior, not real-user benefit. |

None of the sources reviewed supplies a qualified dataset of actual dictation retries for this application. A subsequent optional, local real-user study must label intended relationships and desired outputs; corpus provenance and consent stay separate from automated inference.

Use the following protocol after implementation:

1. Freeze rule versions and development/calibration/test assignments before evaluation. Group by speaker/user, source document/audio and complete repair chain. Use later episodes for testing; include separate unseen-user evaluation for the admission policy. Deduplicate reruns and near-duplicates.
2. At episode `t`, generate with memory available **before** `t`; score against withheld references; only then expose the feedback that would actually be observable to the app. Do not expose future corrections or hidden retry labels. A repeated untouched holdout may be scored after each calibration batch, but never used to tune thresholds or populate memory.
3. Replay identical streams with learning off, current manual-confirmation memory, automatic edits only, edits plus usage ranking, and edits plus retry analysis. Separate the recognition, deterministic memory and optional cleanup contributions. Include unflagged successful-looking episodes, not only corrections selected for review.
4. Measure canonical/alias admission precision, coverage, false retry links, correction recurrence on later uses, term recall, WER/CER, and harmful replacements per 1,000 words. Score names, numbers, dates, negation, quoted text and meaning separately. Wrong automatic changes must stay in aggregates.
5. Measure effort: actual correction actions and active editing seconds per 1,000 words, repeat attempts, interruptions, undo/suspension and time until a useful correction is learned. Report observed denominators and missing feedback; a copy rate or growing dictionary is not an accuracy score. A replay cannot establish causal engagement improvement.
6. Exercise conflicting contexts, language/code-switching, changing terminology, model switches, disabled learning/collection, quota eviction, restart, deletion and no-resurrection. Inject duplicate/out-of-order events, stale edit revisions, cancellation, database write failure and an interrupted admission transaction.
7. Test network-disabled operation with installed artifacts. Verify evidence is not exported as a reviewed target; no private text/audio appears in ordinary diagnostics. For a prospective usability study, counterbalance/randomize available conditions and obtain independent outcome labels rather than assuming apparent acceptance.

Report effects by user/language/model/backend/corpus envelope, with paired episode/user-level uncertainty and explicit failure counts. A successful narrow synthetic case does not qualify broad automatic learning. Before enabling automatic admission, predeclare numeric precision/coverage and latency gates from development evidence, and keep a zero-tolerance regression set for protected meaning changes. Threshold tuning and release-gate selection must precede the locked test; they remain qualification work, not values proven by this document.

### Latency definitions

At proposal time there was **no measured average latency for the automatic loop**. Subsequent local component measurements are in the [phase-A report](../benchmarks/asr/2026-09-16-automatic-learning.md). Do not reuse the earlier ASR or cleanup average as its measurement. Phase A is designed to use bounded local computation without another model call, but that is a design property, not a timing result.

Record these separately, using a monotonic clock and reporting count, arithmetic mean, p50/p95/p99 and failures:

- **Stop-to-final:** recording stop request to final text visible, including relevant persistence and rendering.
- **Stop-to-insert:** stop request to confirmed native insertion completion; keep failed/changed-target cases explicit.
- **Memory application:** eligible lookup and final correction time within a dictation operation.
- **Feedback-to-memory-ready:** completed edit/feedback event to committed profile revision, including any queue delay. An operation already using an older frozen profile retains it.
- **Analysis interference:** additional record-start delay, stop-to-final delay and cancellation time while optional background analysis is queued/running.

Compare on/off on identical episode streams, at empty/typical/full memory sizes, cold and resident state. Record application snapshot, artifact hashes, runtime revisions, hardware/OS/backend/precision/threads, language/corpus, power mode, RAM and sustained thermals using the [ASR qualification protocol](../benchmarks/asr/README.md). Streaming measurements retain its first-partial, word-emission, endpoint, queue/drop and finalization definitions. A low text-lookup time does not imply low end-to-end latency.

## Evidence status

- **Paper evidence:** methods and limits in the source table; implementation repositories were inspected as references, not installed or reproduced here.
- **Official/local API capability:** existing reviewed memory, adapter-specific hints, constrained cleanup and native insertion; no external-edit observer or generic semantic-analysis task.
- **Local measurement:** prior frozen-memory regression/native corpus results remain separately linked. The research follow-up itself added no tests. Subsequent phase-A behavior and replay measurements are linked in the implementation guide; no longitudinal benefit is established.
- **Unverified hypotheses:** automatic admission, retry interpretation, stable style inference, lower correction burden and acceptable incremental latency. Issue #34 owns the concrete proposal and approval decision before their implementation.
