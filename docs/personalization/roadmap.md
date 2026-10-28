# Personalization phases and remaining work

The [September RSI survey](../research-rsi-2026-09.md) supplies recent primary sources and an experiment sequence covering retained failure cases, memory-specific outcome attribution, context retrieval, consolidation and qualification of a live policy loop. The failed global failure-bank experiment remains excluded. The fixed teaching-context guard is now integrated; policy proposals also run in isolated application shadow evaluation; active policy selection remains fixed.

This page explains the product and engineering sequence beyond the implemented automatic dictionary. It complements the [component contracts](README.md), [research synthesis](../research-automatic-personalization.md) and [continuous-improvement contract](../self-improvement.md). Architectural proposals, approval decisions and progress belong to [issue #34 and its sub-issues](https://github.com/rahulsharmavishwakarma/vesper/issues/34), shown together in the [RSI Project view](https://github.com/users/rahulsharmavishwakarma/projects/2/views/6). The issue links below identify each scope's owner; current status, next action and evidence stay in GitHub rather than a second local backlog.

The original proposal named **phase A** and **phase B**. **C and D below are organizational names.** Semantic shadow analysis in B and the persistent policy-shadow contract are explicitly approved and implemented; the staged cleanup-first design in D has now been approved. The former TextEdit-only shadow scope in C was implemented and then removed on 29 September 2026 because dictation pastes into any application; see [C below](#c-external-editor-learning-removed). Automatic project rules are implemented, while training/promotion remain gated on exact runtime compatibility and quality evidence. [ADR-0016](../adrs/0016-automatic-local-spelling-learning.md) records the implemented phase-A decision.

## At a glance

| Phase                               | User-visible purpose                                                                                                     | Current boundary                                                                                                                                           | Scope owner                                                                                                                                              |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A — local spelling memory           | Learn eligible spellings and lexical context from ordinary edits; reuse them in relevant dictation across model changes. | Automatic desktop behavior on existing workers, without opt-in; fixed raw-context eligibility is integrated. Broader personal benefit remains unqualified. | [#34](https://github.com/rahulsharmavishwakarma/vesper/issues/34); guard delivered in [PR #52](https://github.com/rahulsharmavishwakarma/vesper/pull/52) |
| A qualification                     | Establish whether learning actually reduces repeated corrections and editing time.                                       | Controlled tests and a public replay exist; longitudinal user evidence is still needed.                                                                    | [#58](https://github.com/rahulsharmavishwakarma/vesper/issues/58)                                                                                        |
| B — semantic personalization        | Learn supported writing preferences and better interpret intent, repairs and relevant context.                           | Bounded [semantic shadow analysis](semantic-shadow.md) is wired to draft edits; activation and personal efficacy remain unqualified.                       | [#59](https://github.com/rahulsharmavishwakarma/vesper/issues/59)                                                                                        |
| C — external-app feedback (removed) | Was: learn from a correction made in the editor where dictation was inserted.                                            | Removed: the TextEdit-only observer covered almost none of real corrections. See the [removal note](external-observer.md).                                 | [#61](https://github.com/rahulsharmavishwakarma/vesper/issues/61); insertion remains in [#4](https://github.com/rahulsharmavishwakarma/vesper/issues/4)  |
| D — optional own-model adaptation   | Improve acoustic recognition or cleanup through a trained and qualified local artifact.                                  | Cleanup-first design approved; exact adapter conversion/native parity proof in progress. No trained candidate is active.                                   | [#5](https://github.com/rahulsharmavishwakarma/vesper/issues/5)                                                                                          |

The separate [adaptive-policy qualification gate (#60)](https://github.com/rahulsharmavishwakarma/vesper/issues/60) owns the evidence and approval needed before promoting the implemented shadow controller to live use. Both live policy promotion and semantic activation depend on A qualification's independently labelled feedback. Native device/permission checks remain in [#7](https://github.com/rahulsharmavishwakarma/vesper/issues/7); #58 owns the learning outcome across sessions. C and D remain optional directions, not prerequisites for automatic in-app learning.

Application memory remains local and independent of model weights throughout. B and C must operate using installed local components without a hosted inference, embedding or memory API. D is optional: vocabulary and preference learning must continue to work without training or waiting for a new model release.

The extension designs are in [automatic project resolution](context-selection.md#automatic-project-routing-boundary)
and [cleanup-first training](../self-improvement.md#proposed-cleanup-first-implementation).
Project routing is an implemented boundary; external spelling admission was removed
(see the [removal note](external-observer.md));
the training diagram still describes a gated proposal. Manual project selection
remains the migration default.

## Will ordinary use produce self-improvement?

**Personal improvement is feasible without changing model weights, but it is conditional on useful feedback, correct memory selection and evidence that later outcomes improve.** Implementing all phases does not guarantee a gain. More usage can supply more observations; only trustworthy evidence should increase the authority of a spelling or preference. Repeated wrong predictions, Copy and missing edits are not independent proof of correctness.

If “RSI” means recursive self-improvement, distinguish two objectives. The current product objective is **personal continual adaptation**: the application updates a user's scoped memory and uses it on future recordings while the learning rules remain fixed. Stronger recursion would improve the learning procedure itself—such as which evidence to trust or which policy to use—and demonstrate that this improves subsequent learning. The [offline policy experiment](policy-experiment.md) tests bounded eligibility-policy selection and rollback. Its scoped variant beats the strong fixed comparator in authored simulations with remaining regressions; it does not establish open-ended algorithm improvement or real-user benefit. The live application still uses fixed learning rules. Automatic checkpoint training alone would not establish the stronger capability either.

The evidence supports a path, with specific limits:

| Evidence class                                                     | What it supports                                                                                                                  | What it does not establish                                                                                                         |
| ------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| Paper: [GEC-RAG, January 2025](https://arxiv.org/abs/2501.10734v1) | Paired ASR errors and independent reference text can improve later correction through retrieval without modifying the recognizer. | Fully passive learning, this local implementation's performance, or the correctness of a repeated ASR guess.                       |
| Paper: [PROSE, May 2025](https://arxiv.org/abs/2505.23815v1)       | Iterative preference inference and checks across writing samples can improve personalized generation.                             | Quality with this app's installed small model, its latency, or improvement of its own learning algorithm.                          |
| Local phase-A replay                                               | Each automatic mode learned two memories and improved five of 27 constructed episode outputs without worsening any.               | A measured reduction in real-user correction burden. The separate public replay learned no active memory and had no held-out gain. |
| Engineering hypothesis                                             | Scoped memory, reliable edits, later-use evaluation and reversible updates can reduce recurring personal mistakes on device.      | Guaranteed, unlimited or monotonic improvement merely from continued use.                                                          |

The intended user experience is to dictate and correct naturally when needed; desktop learning is automatic without an enable step. The app manages eligible memories; a user should not need to tune thresholds or curate every dictionary entry. However, learning needs an observed correction: in-app edits confirmed through ordinary use enter vocabulary. A retry can flag an unresolved problem without revealing the desired spelling. New topics, unseen errors and changing preferences can still need feedback.

Automatic on-device improvement is a core product requirement, not an optional enhancement users must configure. Phase A runs during normal use; the remaining semantic/context/intent work must eventually fit that same low-effort background experience. This requirement does not activate the unqualified offline policy controller or an autonomous trainer. The separately approved cleanup-first training design remains subject to its compatibility and quality gates. Optional external-app observation and own-model training are possible mechanisms, not prerequisites for learning within the app.

The product success criterion is **fewer corrections and less editing time on later independent sessions, while preserving meaning and keeping dictation latency acceptable**. A larger dictionary, fewer detected retries, or the model judging its own output better does not satisfy this criterion. Compare learning enabled with a frozen-memory baseline on the same held-out inputs; measure real editing burden separately in a suitably controlled user study. Include unflagged outputs and retain harmful changes and failures. Later-use feedback can strengthen, suspend or retire a memory only under validated rules; the existing automatic loop has not yet demonstrated this full beneficial cycle in natural use.

The immediate sequence is A qualification, then shadow evaluation of B's semantic candidates. The offline policy trial does not authorize live optimization: that still needs a concrete integration proposal, independently labelled development data, a locked evaluation boundary and reversible deployment. The system must not improve its own reported score by changing the labels, excluding failures or repeatedly tuning against the final test. Research and dictionary growth are inputs to this objective; later-session improvement is the outcome that must be measured.

The [97-history stress evaluation](../../benchmarks/asr/2026-09-18-policy-learning-stress.md) strengthens mechanical validation of the second loop and fixes two evaluator defects. Context/support shifts show gains over the starting policy, but overall results still favor the best fixed comparators. The next policy experiment therefore needs representative feedback and better coverage of older failure cases, with a new locked comparison; increasing the number of simulated repetitions alone does not establish stronger recursive improvement. Live optimization remains deferred.

The [real-audio accent screen](../../benchmarks/asr/2026-09-18-accent-personalization.md) now covers 19 Svarah language-background groups with 304 new recordings and 38 paced replays. Automatic memory showed no gain after the existing corpus calibration; a simulated reviewed dictionary introduced a singular/plural regression. This strengthens the requirement for repeated, correctly scoped personal evidence and negative cases before widening admission. It does not implement an accent detector or acoustically train the recognizer.

## A qualification: prove useful learning

Phase A already supplies editable drafts, conservative spelling admission, usage ranking, uncertain retry signals and inspect/undo/forget controls. It needs evidence of recurring personal benefit before a broader rollout. The [measurement report](../../benchmarks/asr/2026-09-16-automatic-learning.md) records 54 passing controlled automatic assertions, but **zero automatically active memories and no held-out gain** in the public one-off correction replay.

The next evaluation needs independently labelled, chronological interactions across multiple sessions: repeated names, technical vocabulary, changed intentions, untouched outputs, wrong-context uses and model switches. Natural user feedback and constructed simulations must be reported separately. Replaying the same recording cannot manufacture independent correction support.

Exercise the native application from recording through an ordinary draft correction to a new independent recording. Include restart, model changes, undo/Forget and collection-off behavior. Compare with frozen memory in an isolated evaluation without adding a production opt-in requirement. Report mean and p95 stop-to-visible-final latency, with the start and end events defined, alongside separate ASR, cleanup and learning-stage timings; keep cold and resident results separate. A passing file replay or mocked interface does not replace this native session check.

Measure correction actions and editing seconds, recurrence of the same mistake, admission precision/coverage, harmful substitutions and undo rates. Include English varieties, Hindi and code-switching as separate qualification groups; existing support does not establish equal benefit. Keep development/calibration data separate from a locked test and choose numerical release margins before inspecting that test. If admission misses useful corrections, refine it against development evidence and repeat the held-out comparison; do not lower thresholds just to force activations.

This work belongs to the existing core/data/runtime evaluation paths. It does not require a new model. The [evaluation contract](evaluation.md) defines the release evidence and timing boundaries.

The [policy-3 correction follow-up](evaluation.md#correction-admission-follow-up)
expands bounded name/acronym admission and tolerates limited formatting around a
single spelling fix. Constructed later-use controls pass, while the inspected
public corpus supplies only provisional support and the older authored audit
retains a harmful substitution. This is an incremental dictionary fix; recurring
natural-user benefit, broader correction coverage and semantic intent learning
still require their own evidence.

The [policy-4 lifecycle follow-up](evaluation.md#policy-4-lifecycle-fixes-and-recurring-audio)
fixes retracted support, rejected learned targets and later taught context. Its
recurring synthetic-audio benchmark demonstrates six improved scored audit
outputs across three voices, but retains ASR completion failures and cleanup
fallbacks in both arms. That diagnostic gate remains failed; it does not complete
Phase A qualification or establish natural-user/accent benefit.

The [native qualification protocol](native-qualification.md) provides reproducible lifecycle and public-audio tests. The [measured results](evaluation.md#native-multi-session-qualification) preserve the initial cleanup failure and rejected prompt replacements; the [integrated bounded recovery](evaluation.md#integrated-native-recovery-verification) then resolves the six native cleanup warnings with 76 passing requests. The [native WebView replay](evaluation.md#native-webview-replay--20-september-2026) adds production-panel rendering-opportunity measurements and clean-exit checks, with no audit gain from automatic learning. Simulated contradictory-use harm, real microphone stop-to-visible timing and natural editing-effort measurements remain. Passing these mechanism and cleanup gates does not complete A qualification or unblock a live adaptive controller.

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

The existing [retrieval stages and budgets](context-selection.md) are the baseline. Measure relevant-memory coverage, wrong-context application, stale/conflicting memory and prompt cost. Usage frequency can rank an already credible term; it cannot establish its correct spelling. An embedding model or new dependency would need its own concrete justification and approval.

### B's shared implementation prerequisite

The approved [shadow implementation](semantic-shadow.md) adds a separate structured analysis contract and bounded queue to the existing inference owner. It uses an already-resident supported writer, strict evidence validation, a separate bounded SQLite table, cancellation and stale-result guards. The cleanup API remains separate; proposals cannot update active vocabulary or preferences.

Start with shadow analysis that proposes candidates without applying them. Compare its labels against independent references before enabling automatic updates. A stronger model is only a candidate if the installed small model fails; neither a model switch nor extra compute is assumed here. The [semantic qualification](evaluation.md#semantic-shadow-qualification) distinguishes measured analysis/interference from unmeasured microphone-to-visible latency.

The [concrete shadow proposal](https://github.com/rahulsharmavishwakarma/vesper/issues/59#issuecomment-5740928523) specifies the implemented versioned, evidence-linked relation/output-form/context proposals, one cancellable idle job on the existing inference owner, bounded local retention, epoch/undo/Forget handling and foreground-interference checks. Its approved analysis API, scheduling and proposal storage remain isolated from active memory. The installed writer failed semantic quality qualification; activation remains blocked pending better independent evidence and a separate decision.

## C: external-editor learning (removed)

Users often correct text after dictating into another application, so a TextEdit-only adapter covered almost none of real corrections, and per-application adapters do not scale. The optional TextEdit observer was therefore removed on 29 September 2026; the dated design and qualification record is preserved in the [removal note](external-observer.md), and the decision record is [#61](https://github.com/rahulsharmavishwakarma/vesper/issues/61). Copy/paste success is not correction feedback. In-app draft edits remain the app-agnostic learning path; the opt-in [fix read-back](app-learning.md) (27 October 2026) feeds fixes made in the destination app into that same path through generic Accessibility reads rather than per-app adapters.

## D: optional own-model and acoustic adaptation

The proposed first training experiment uses separately reviewed cleanup pairs; acoustic training remains a separate later target. Cleanup needs raw predictions paired with reviewed edited wording; ASR needs audio paired with reviewed verbatim speech. Ordinary dictionary events and inferred retries are not automatically valid training labels.

The future workflow is a versioned dataset snapshot, separate candidate training, held-out evaluation of the deployable artifact, an explicit promotion policy and rollback. It must qualify native conversion, quantization, supported languages, latency, memory and retention of general speech ability. Training uses an appropriate source checkpoint, not in-place edits to the deployed quantized file. Local inference continues while optional training is absent; no cloud API is required by this roadmap. Whether training fits the user's device must be measured on the selected recipe and hardware.

Acoustic adaptation could address recurring pronunciation errors that text aliases cannot fix. Accent identification is a separate optional hypothesis: an accent label is useful only if a compatible downstream choice measurably improves recognition. It is not the same as language identification, code-switch handling or the current pronunciation-confusion aliases. No acoustic detector or automatic acoustic adaptation is implemented by phase A.

The detailed training, deletion/provenance, promotion and rollback contract remains in [continuous improvement](../self-improvement.md); this page does not duplicate or activate it. Application-owned memories must survive candidate swaps and rollback, while measurements stay separated by actual model/runtime/artifact.

## How this relates to the original six capabilities

The six research areas are capabilities, not six successive implementation phases.

| Capability                 | Existing foundation                             | Further development                                                     |
| -------------------------- | ----------------------------------------------- | ----------------------------------------------------------------------- |
| Correction learning        | Ordinary spelling edits and explicit reviews    | A qualification; B for semantic edits; optional C for external edits    |
| Contextual vocabulary      | Scoped aliases and bounded retrieval            | B3 context selection; C only for observable external context            |
| Evidence of improvement    | Usage/undo counts and separate replay reports   | A qualification, then comparisons for every extension                   |
| Useful feedback            | Bounded review selection and retry cues         | B2 better repair interpretation; measure correction effort              |
| Intent and writing         | Explicit forms and reviewed-example suggestions | B1 preferences and B2 intent, with fidelity checks                      |
| Pronunciation/accent needs | Reviewed text-level confusion aliases           | Optional D for acoustic changes; accent labels only with proven utility |

## Evidence and performance boundaries

The recent paper results and official implementations remain classified in the [research synthesis](../research-automatic-personalization.md). The phase-A report is local measurement. B/C/D benefits are unverified hypotheses; this roadmap introduces no new paper result or runtime capability claim.

For inference proposals, A and prospective B use frozen weights; the bounded B shadow API and scheduling proposal is now approved and implemented. Semantic activation still needs evidence and a separate concrete decision. C is application/platform work. Runtime-internal decoder/kernel changes are a separate category and are not part of this sequence. D entails training/checkpoint changes and cannot be presented as a configuration switch. The pins in [ADR-0016](../adrs/0016-automatic-local-spelling-learning.md#evidence-and-inference-boundary) identify the phase-A reference envelope, not automatic qualification of a future prototype.

Measure each addition's mean and p50/p95/p99 for memory application, feedback-to-memory-ready including queue delay, recording interference, stop-to-final and stop-to-insert separately. Preserve failures, language/corpus/hardware groups and exact model/runtime/artifact hashes; include resource use and sustained thermal behavior when inference is involved. Phase A's sub-millisecond component measurements cannot predict B's extra model work or total dictation latency.
