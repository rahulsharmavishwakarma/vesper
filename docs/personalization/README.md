# Personalization implementation guide

The application remembers vocabulary on this device independently of the selected speech or writing model. Phase A adds automatic spelling learning from ordinary edits; no model training, cloud inference or embedding service runs in this loop. The separate [semantic shadow path](semantic-shadow.md) analyzes bounded edits locally without activating its guesses.

Implementation approval: [issue #34](https://github.com/rahulsharmavishwakarma/vesper/issues/34#issuecomment-5688739690). Decision: [ADR-0016](../adrs/0016-automatic-local-spelling-learning.md). Research and future work: [automatic-personalization research](../research-automatic-personalization.md).

For what comes next, see [Personalization phases and remaining work](roadmap.md): phase-A qualification, semantic learning in phase B, and the gated own-model path. It separates the six capability areas from implementation phases and describes each extension's evidence and performance requirements.

The [bounded policy-improvement experiment](policy-experiment.md) now exercises an offline second loop: propose an eligibility-policy change, validate it on fresh feedback and optionally roll back. It is not connected to live dictation; its measured limits do not justify deployment.

The separately approved fixed [teaching-context guard](automatic-dictionary.md#context-required-for-post-recognition-use) is integrated into the desktop post-ASR path. Automatic words now need matching context from an independent edit before cleanup vocabulary selection and final correction. Legacy entries without cues need fresh evidence; manual vocabulary and optional pre-ASR hints keep their existing behavior. The adaptive policy controller remains offline.

The [September 2026 RSI research review](../research-rsi-2026-09.md) compares Dream-RSI, related recent papers and lab posts with this implementation. It prioritizes retaining past failure cases and evaluating memory-specific outcomes before integrating a live policy optimizer.

[Dictation formatting](formatting.md) covers the rule-based formatting of numbers, times, dates, emails, links, hesitations and self-corrections.

[Learning from the app you dictate into](app-learning.md) covers the opt-in field words and fix read-back.

The [self-improving dictation system](self-improving-system.md) page summarizes how automatic workspaces, the term inventory, recognizer hooks, the correction gate and policy-5 learning fit together, with the measured status of each.

| Component                                      | Detailed contract                                                                                                                                                                     |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Editing and feedback capture                   | [Editable drafts](draft-editing.md): commit points, Copy, revision checks, failures and review separation                                                                             |
| Automatic vocabulary                           | [Dictionary learning](automatic-dictionary.md): candidate admission, independent support, guards and limitations                                                                      |
| Context and in-context examples                | [Context selection](context-selection.md): pre-ASR hints, post-ASR retrieval, budgets and model support                                                                               |
| Term inventory and working set                 | [Term inventory](term-inventory.md): mined spellings, tiers, background worker, per-recording selection and privacy boundaries                                                        |
| Intent and writing preferences                 | [Intent and writing](intent-and-writing.md): existing explicit examples and output-form routing; automatic semantic proposals are isolated in [shadow evaluation](semantic-shadow.md) |
| Repeat dictation                               | [Retry evidence](retry-evidence.md): temporal/lexical cues, correlated attempts and uncertainty                                                                                       |
| Lifecycle and privacy                          | [Storage and privacy](storage-and-privacy.md): schemas, quotas, epochs, undo and deletion                                                                                             |
| Evidence of benefit                            | [Evaluation](evaluation.md): replay, comparison limits, timing and qualification                                                                                                      |
| Native qualification procedure                 | [Protocol](native-qualification.md): frozen controls, installed-model replay, natural sessions and acceptance gates                                                                   |
| Recognizer hooks and correction gate per model | [Term evaluation](term-evaluation.md): fixed term evaluator, hint conditions, false alarms and hook decisions                                                                         |

Automatic local learning is core desktop behavior, including for existing profiles. There is no **Learn as I dictate** switch and no setup or per-word confirmation is needed. Settings → Your vocabulary and writing explains the behavior and retains inspection, removal and **Forget automatic learning**. Forget clears learned evidence; fresh recordings can teach again, while old result contexts cannot recreate forgotten evidence. This product requirement supersedes the earlier off-by-default rollout choice; broad personal benefit still needs qualification. **Apply my saved corrections** controls the final alias-replacement stage. Recognition hints have a separate opt-in switch and depend on the adapter.

Background execution is event-driven: transcription workers record bounded usage/retry evidence, and completed draft edits invoke the existing blocking-worker command for admission and SQLite persistence. These operations are awaited within their worker; they can contribute to completion/save latency and are not an idle-only scheduler. Phase B also has a single cancellable idle analysis slot on the existing inference owner; it never loads a model for background work or trains weights. See [storage and privacy](storage-and-privacy.md) for migration and deletion semantics.

Automatic spelling learning observes edits made in **Your text**, and bounded usage/retry evidence from the app's results. History and Dictate reviews use the same transcript card: a like approves the polished text (a verified speech label only when it matches what was heard, otherwise a reviewed correction) without teaching, while saving an edited correction teaches the loop from that explicit edit exactly once; **Undo** retracts the approval but not already taught vocabulary. A routine edit never labels an entire recording as verified speech.

Current boundaries include one small replacement per completed edit, explicit language for automatic activation, conservative script-independent spelling similarity, English-only retry cues, and bounded lexical teaching context alongside the selected project. Rich style extraction, semantic intent/repair classification, general external-document understanding and acoustic accent adaptation are not implemented by phase A. Application rules and project routing are described in [context selection](context-selection.md).

The [persistent policy shadow evaluator](policy-experiment.md#persistent-application-shadow-evaluation) checks explicit edits against six frozen policies without changing the active learner. It follows the existing Undo/Forget lifecycle. The former TextEdit observer was removed; see the [removal note](external-observer.md).
