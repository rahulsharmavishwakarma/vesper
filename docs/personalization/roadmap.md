# Personalization phases and remaining work

The [September RSI survey](../research-rsi-2026-09.md) supplies recent primary sources and a prioritized experiment sequence: retained failure cases, memory-specific outcome attribution, context retrieval, consolidation, then qualification of a live policy loop. These are proposed additions; the current policy experiment remains offline.

This page explains the product and engineering sequence beyond the implemented automatic dictionary. It complements the [component contracts](README.md), [research synthesis](../research-automatic-personalization.md) and [continuous-improvement contract](../self-improvement.md). Architectural proposals, approval decisions and progress remain in [issue #34](https://github.com/rahulsharmavishwakarma/transcribe/issues/34); training belongs to [issue #5](https://github.com/rahulsharmavishwakarma/transcribe/issues/5).

The original proposal named **phase A** and **phase B**. **C and D below are organizational names for separate deferred directions**, not previously approved implementation phases. Documenting this sequence does not approve a semantic-analysis API, external-editor observer, training service or default change. [ADR-0016](../adrs/0016-automatic-local-spelling-learning.md) records the implemented phase-A decision.

## At a glance

| Phase | User-visible purpose | Current boundary |
| --- | --- | --- |
| A — local spelling memory | Learn eligible spellings from ordinary edits; reuse them in relevant dictation across model changes. | Implemented as automatic desktop behavior on existing workers, without opt-in. Broader personal benefit remains unqualified. |
| A qualification | Establish whether learning actually reduces repeated corrections and editing time. | Controlled tests and a public replay exist; longitudinal user evidence is still needed. |
| B — semantic personalization | Learn supported writing preferences and better interpret intent, repairs and relevant context. | Research direction; no general local semantic-analysis task is implemented. |
| C — optional external-app feedback | Learn from a correction made in the editor where dictation was inserted. | Requires a separate platform/permission design; current insertion does not observe later edits. |
| D — optional own-model adaptation | Improve acoustic recognition or cleanup through a trained and qualified local artifact. | Deferred until the own-model project and a specific training proposal. |

Application memory remains local and independent of model weights throughout. B and C must operate using installed local components without a hosted inference, embedding or memory API. D is optional: vocabulary and preference learning must continue to work without training or waiting for a new model release.

## Will ordinary use produce self-improvement?

**Personal improvement is feasible without changing model weights, but it is conditional on useful feedback, correct memory selection and evidence that later outcomes improve.** Implementing all phases does not guarantee a gain. More usage can supply more observations; only trustworthy evidence should increase the authority of a spelling or preference. Repeated wrong predictions, Copy and missing edits are not independent proof of correctness.

If “RSI” means recursive self-improvement, distinguish two objectives. The current product objective is **personal continual adaptation**: the application updates a user's scoped memory and uses it on future recordings while the learning rules remain fixed. Stronger recursion would improve the learning procedure itself—such as which evidence to trust or which policy to use—and demonstrate that this improves subsequent learning. The [offline policy experiment](policy-experiment.md) now tests bounded eligibility-policy selection and rollback, but does not demonstrate open-ended algorithm improvement or benefit beyond the best fixed alternatives. The live application still uses fixed learning rules. Automatic checkpoint training alone would not establish the stronger capability either.

The evidence supports a path, with specific limits:

| Evidence class | What it supports | What it does not establish |
| --- | --- | --- |
| Paper: [GEC-RAG, January 2025](https://arxiv.org/abs/2501.10734v1) | Paired ASR errors and independent reference text can improve later correction through retrieval without modifying the recognizer. | Fully passive learning, this local implementation's performance, or the correctness of a repeated ASR guess. |
| Paper: [PROSE, May 2025](https://arxiv.org/abs/2505.23815v1) | Iterative preference inference and checks across writing samples can improve personalized generation. | Quality with this app's installed small model, its latency, or improvement of its own learning algorithm. |
| Local phase-A replay | Each automatic mode learned two memories and improved five of 27 constructed episode outputs without worsening any. | A measured reduction in real-user correction burden. The separate public replay learned no active memory and had no held-out gain. |
| Engineering hypothesis | Scoped memory, reliable edits, later-use evaluation and reversible updates can reduce recurring personal mistakes on device. | Guaranteed, unlimited or monotonic improvement merely from continued use. |

The intended user experience is to enable learning once, dictate, and correct naturally when needed. The app manages eligible memories; a user should not need to tune thresholds or curate every dictionary entry. However, the app must actually observe the correction: edits in another application remain invisible until C is implemented. A retry can flag an unresolved problem without revealing the desired spelling. New topics, unseen errors and changing preferences can still need feedback.

Automatic on-device improvement is a core product requirement, not an optional enhancement users must configure. Phase A runs during normal use; the remaining semantic/context/intent work must eventually fit that same low-effort background experience. This requirement does not activate the unqualified offline policy controller or authorize model training. Optional external-app observation and own-model training are possible mechanisms, not prerequisites for learning within the app.

The product success criterion is **fewer corrections and less editing time on later independent sessions, while preserving meaning and keeping dictation latency acceptable**. A larger dictionary, fewer detected retries, or the model judging its own output better does not satisfy this criterion. Compare learning enabled with a frozen-memory baseline on the same held-out inputs; measure real editing burden separately in a suitably controlled user study. Include unflagged outputs and retain harmful changes and failures. Later-use feedback can strengthen, suspend or retire a memory only under validated rules; the existing automatic loop has not yet demonstrated this full beneficial cycle in natural use.

The immediate sequence is A qualification, then shadow evaluation of B's semantic candidates. The offline policy trial does not authorize live optimization: that still needs a concrete integration proposal, independently labelled development data, a locked evaluation boundary and reversible deployment. The system must not improve its own reported score by changing the labels, excluding failures or repeatedly tuning against the final test. Research and dictionary growth are inputs to this objective; later-session improvement is the outcome that must be measured.

The [97-history stress evaluation](../../benchmarks/asr/2026-09-18-policy-learning-stress.md) strengthens mechanical validation of the second loop and fixes two evaluator defects. Context/support shifts show gains over the starting policy, but overall results still favor the best fixed comparators. The next policy experiment therefore needs representative feedback and better coverage of older failure cases, with a new locked comparison; increasing the number of simulated repetitions alone does not establish stronger recursive improvement. Live optimization remains deferred.

The [real-audio accent screen](../../benchmarks/asr/2026-09-18-accent-personalization.md) now covers 19 Svarah language-background groups with 304 new recordings and 38 paced replays. Automatic memory showed no gain after the existing corpus calibration; a simulated reviewed dictionary introduced a singular/plural regression. This strengthens the requirement for repeated, correctly scoped personal evidence and negative cases before widening admission. It does not implement an accent detector or acoustically train the recognizer.

## A qualification: prove useful learning

Phase A already supplies editable drafts, conservative spelling admission, usage ranking, uncertain retry signals and inspect/undo/forget controls. It needs evidence of recurring personal benefit before a broader rollout. The [measurement report](../../benchmarks/asr/2026-09-16-automatic-learning.md) records 54 passing controlled automatic assertions, but **zero automatically active memories and no held-out gain** in the public one-off correction replay.

The next evaluation needs independently labelled, chronological interactions across multiple sessions: repeated names, technical vocabulary, changed intentions, untouched outputs, wrong-context uses and model switches. Natural user feedback and constructed simulations must be reported separately. Replaying the same recording cannot manufacture independent correction support.

Measure correction actions and editing seconds, recurrence of the same mistake, admission precision/coverage, harmful substitutions and undo rates. Include English varieties, Hindi and code-switching as separate qualification groups; existing support does not establish equal benefit. Keep development/calibration data separate from a locked test and choose numerical release margins before inspecting that test. If admission misses useful corrections, refine it against development evidence and repeat the held-out comparison; do not lower thresholds just to force activations.

This work belongs to the existing core/data/runtime evaluation paths. It does not require a new model. The [evaluation contract](evaluation.md) defines the release evidence and timing boundaries.

## B: semantic personalization with frozen weights

The desired loop is **ordinary user feedback → bounded local analysis → validated, scoped preference → relevant future use**. This extends phase A's spelling-only evidence. A stored hypothesis must not become a rule merely because a language model generated it.

### B1. Learn writing preferences from edits

Look for consistent presentation choices across independent user-authored changes: for example, repeated removal of greetings from project messages or a preference for short paragraphs. Separate enduring preferences from a one-time rewrite. Unchanged generated text, Copy, or silence does not establish endorsement.

Store a bounded typed preference with scope and supporting edit references. Check contradictions and earlier examples before activation; preserve inspect, suspend, undo and forget behavior. Do not silently convert ordinary edits into reviewed cleanup-training examples. The selected cleanup policy and meaning-preservation guards still constrain what a preference may change.

Affected components would include semantic analysis in the inference layer, typed profile/evidence validation, cleanup context selection and the inspector. Qualification must measure later-context preference adherence, false generalization and added/removed facts, including conflicting and changing preferences. The actual installed model must pass; published larger-model results are not local evidence.

### B2. Interpret intent and repeat dictation

Distinguish a continuation, explicit repair, changed decision, new task and uncertainty more reliably than the current English cue rules. A later utterance may clarify a spelling, but it may also change what the user wants to say. Temporal proximity or a second ASR output alone cannot prove that the first recognizer was wrong.

For example, “Friday—actually Monday” can supply a revised date for this utterance; it must not teach a global Friday-to-Monday replacement. A clearly supplied spelled-out name may supply a candidate spelling, but the application must preserve its actual evidence and abstain when recognition is ambiguous.

Learned output-form selection would stay within supported forms such as note, message and technical dictation, with fallback to the selected default. It would not infer recipients, invent commitments, execute commands or silently rewrite an earlier inserted result. Raw/verbatim speech remains available separately from edited wording.

This extends the runtime's bounded recent context, typed feedback and current [intent](intent-and-writing.md) / [retry](retry-evidence.md) contracts. Evaluation needs independently labelled complete repair chains, false-link rates, unknown/out-of-scope cases, abstention and meaning-preservation checks. Diagnosing whether an error came from ASR, cleanup or changed intent remains uncertain without sufficient evidence.

### B3. Choose context more accurately

Improve which existing terms and validated preferences are retrieved for a particular task. Begin with observable context: selected project, requested language, current text and bounded prior interaction evidence. Test same-user terminology across unrelated projects and changing preferences. Before ASR, the current transcript is unavailable; after ASR it can inform cleanup selection.

The existing [retrieval stages and budgets](context-selection.md) are the baseline. Measure relevant-memory coverage, wrong-context application, stale/conflicting memory and prompt cost. Usage frequency can rank an already credible term; it cannot establish its correct spelling. External-document context belongs to C. An embedding model or new dependency would need its own concrete justification and approval.

### B's shared implementation prerequisite

The current `rewrite_personalized` API performs constrained cleanup; it is not a generic classifier or memory writer. B needs a concrete proposal for a separate structured analysis request/result contract, validated evidence updates and bounded scheduling. Analysis should be idle, cancellable and lower priority than recording, with strict queue/token/time limits and existing model ownership preserved. Those properties require implementation and measurement; they do not exist merely because the writing model is already resident.

Start with shadow analysis that proposes candidates without applying them. Compare its labels against independent references before enabling automatic updates. A stronger model is only a candidate if the installed small model fails; neither a model switch nor extra compute is assumed here. There is no measured B latency yet.

## C: optional learning from external editors

Users often correct text after dictating into another application. The current app can insert final text but cannot attribute a later editor change to that result. Copy/paste success is not correction feedback.

A separate platform design would need reliable insertion-span/document identity, explicit observation controls, narrowly scoped edit capture and exclusion of unrelated text or another author's edits. Unsupported applications, changed focus, moved text and stale callbacks must abstain. Captured edits would enter the same typed learning pipeline with their origin retained; they must not bypass admission or reviewed-target rules.

This affects native insertion/platform integration, feedback correlation and privacy controls. Validation requires real supported editors, undo/redo, delayed edits, focus changes, permissions off and proof that unrelated document content is not collected. It must not retroactively replace text merely because a memory changes. C is optional and can be designed independently once the event contract is stable; B does not require observing external applications.

## D: optional own-model and acoustic adaptation

When the own-model project begins, separately reviewed data can support candidate cleanup or acoustic training. Cleanup needs raw predictions paired with reviewed edited wording; ASR needs audio paired with reviewed verbatim speech. Ordinary dictionary events and inferred retries are not automatically valid training labels.

The future workflow is a versioned dataset snapshot, separate candidate training, held-out evaluation of the deployable artifact, an explicit promotion policy and rollback. It must qualify native conversion, quantization, supported languages, latency, memory and retention of general speech ability. Training uses an appropriate source checkpoint, not in-place edits to the deployed quantized file. Local inference continues while optional training is absent; no cloud API is required by this roadmap. Whether training fits the user's device must be measured on the selected recipe and hardware.

Acoustic adaptation could address recurring pronunciation errors that text aliases cannot fix. Accent identification is a separate optional hypothesis: an accent label is useful only if a compatible downstream choice measurably improves recognition. It is not the same as language identification, code-switch handling or the current pronunciation-confusion aliases. No acoustic detector or automatic acoustic adaptation is implemented by phase A.

The detailed training, deletion/provenance, promotion and rollback contract remains in [continuous improvement](../self-improvement.md); this page does not duplicate or activate it. Application-owned memories must survive candidate swaps and rollback, while measurements stay separated by actual model/runtime/artifact.

## How this relates to the original six capabilities

The six research areas are capabilities, not six successive implementation phases.

| Capability | Existing foundation | Further development |
| --- | --- | --- |
| Correction learning | Ordinary spelling edits and explicit reviews | A qualification; B for semantic edits; optional C for external edits |
| Contextual vocabulary | Scoped aliases and bounded retrieval | B3 context selection; C only for observable external context |
| Evidence of improvement | Usage/undo counts and separate replay reports | A qualification, then comparisons for every extension |
| Useful feedback | Bounded review selection and retry cues | B2 better repair interpretation; measure correction effort |
| Intent and writing | Explicit forms and reviewed-example suggestions | B1 preferences and B2 intent, with fidelity checks |
| Pronunciation/accent needs | Reviewed text-level confusion aliases | Optional D for acoustic changes; accent labels only with proven utility |

## Evidence and performance boundaries

The recent paper results and official implementations remain classified in the [research synthesis](../research-automatic-personalization.md). The phase-A report is local measurement. B/C/D benefits are unverified hypotheses; this roadmap introduces no new paper result or runtime capability claim.

For inference proposals, A and prospective B use frozen weights; B still needs a new application API and scheduling approval. C is application/platform work. Runtime-internal decoder/kernel changes are a separate category and are not part of this sequence. D entails training/checkpoint changes and cannot be presented as a configuration switch. The pins in [ADR-0016](../adrs/0016-automatic-local-spelling-learning.md#evidence-and-inference-boundary) identify the phase-A reference envelope, not automatic qualification of a future prototype.

Measure each addition's mean and p50/p95/p99 for memory application, feedback-to-memory-ready including queue delay, recording interference, stop-to-final and stop-to-insert separately. Preserve failures, language/corpus/hardware groups and exact model/runtime/artifact hashes; include resource use and sustained thermal behavior when inference is involved. Phase A's sub-millisecond component measurements cannot predict B's extra model work or total dictation latency.
