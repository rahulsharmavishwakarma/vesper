# Intent and writing preferences

This page distinguishes the current implementation from the semantic-learning research. Source: [intent retrieval](../../crates/vesper-core/src/memory.rs) and [cleanup prompt](../../crates/vesper-inference/src/personalization.rs).

## Implemented output forms

Users can select dictated wording, note, message or technical dictation. These settings guide punctuation and presentation while preserving facts and order. They do not authorize summarization, invented recipients or greetings, date/number changes, commitments or external actions. Preserving fillers and repetition is a separate explicit preference.

A saved, explicitly reviewed cleanup example can be remembered with an output-form label. The example retains its source session and review revision; it is validated against that reviewed target. Ordinary draft edits and inferred retry relationships do not manufacture these labels.

When the existing optional example-based intent setting is enabled, retrieval filters enabled examples by language/project. A match needs at least two shared informative terms and Jaccard overlap of at least one third. Among the top three matches, at least two distinct source sessions must agree on the same output form. Otherwise the selected default is used. This is lexical agreement, not a trained intent classifier or a calibrated confidence estimate.

## What the automatic dictionary adds

Writing runs automatically for each completed recording or imported file when
cleanup is enabled; it does not wait for a correction. However, its output-form
selection currently uses the explicit default or the reviewed-example matching
described above. It does not run a general semantic intent classifier on each
utterance. The writing model receives ASR text, not the original audio or prosody.
Consequently, fluent wording alone cannot establish what was actually said.

Edits provide optional feedback for persistent memory. They are not a required
step before producing ordinary cleaned text. The separate semantic evaluator
currently needs a committed edit as its input; that trigger must not be confused
with automatic interpretation during the recording's writing pass.

Automatically admitted spellings become eligible for bounded in-context vocabulary retrieval and exact final correction. A user who repeatedly supplies the same technical name can therefore benefit without changing the model. The feature does not infer a broad writing-style rule from that single term edit.

A spelling correction, a style edit, a spoken repair and a changed intention are different evidence types. For example, changing Friday to Monday may express a new decision; it must never create a global spelling replacement. A cleaned target may resolve a clearly supplied spoken repair, while a verbatim target preserves what was actually spoken. Personal context cannot fill in facts that the user never supplied.

## Semantic analysis and current limits

[PROSE and related research](../research-automatic-personalization.md#what-the-research-supports) motivate extracting component preferences across ordinary edits and validating them against independent examples. Phase B's [structured analysis API, bounded scheduling and isolated proposal storage](semantic-shadow.md) are implemented. Automatic activation remains blocked on [failed installed-model quality qualification](evaluation.md#semantic-shadow-qualification). The measured edit-relation task is different from per-recording intent-aware writing; it is not a benchmark of an unimplemented writing mode. The current `rewrite_personalized` method remains constrained cleanup, not a general memory-writing API.

Phase A introduces no extra model calls, embedding model, background semantic agent, acoustic accent detector, training runner or model promotion. Claims about automatic tone learning or understanding why a user retried would exceed the implementation.

Recent research also distinguishes plausible rewriting from recognition accuracy.
[ASR-EC (November 2025)](https://aclanthology.org/2025.emnlp-industry.110/)
found prompting ineffective on its Chinese error-correction benchmark; audio and
text together performed best in that evaluation. [Confidence-guided correction
(September 2025)](https://arxiv.org/abs/2509.25048v1) studies fine-tuning with ASR
uncertainty for disordered speech. That is a training change, not a prompt switch
for the installed writer. Neither result qualifies Vesper's languages, checkpoint
or latency. Automatic intent-writing improvements need their own held-out
meaning-preservation and native latency comparison before activation.
