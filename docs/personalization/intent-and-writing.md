# Intent and writing preferences

This page distinguishes the current implementation from the semantic-learning research. Source: [intent retrieval](../../crates/vesper-core/src/memory.rs) and [cleanup prompt](../../crates/vesper-inference/src/personalization.rs).

## Implemented output forms

Users can select dictated wording, note, message or technical dictation. These settings guide punctuation and presentation while preserving facts and order. They do not authorize summarization, invented recipients or greetings, date/number changes, commitments or external actions. Preserving fillers and repetition is a separate explicit preference.

A saved, explicitly reviewed cleanup example can be remembered with an output-form label. The example retains its source session and review revision; it is validated against that reviewed target. Ordinary draft edits and inferred retry relationships do not manufacture these labels.

When the existing optional example-based intent setting is enabled, retrieval filters enabled examples by language/project. A match needs at least two shared informative terms and Jaccard overlap of at least one third. Among the top three matches, at least two distinct source sessions must agree on the same output form. Otherwise the selected default is used. This is lexical agreement, not a trained intent classifier or a calibrated confidence estimate.

## What the automatic dictionary adds

Automatically admitted spellings become eligible for bounded in-context vocabulary retrieval and exact final correction. A user who repeatedly supplies the same technical name can therefore benefit without changing the model. The feature does not infer a broad writing-style rule from that single term edit.

A spelling correction, a style edit, a spoken repair and a changed intention are different evidence types. For example, changing Friday to Monday may express a new decision; it must never create a global spelling replacement. A cleaned target may resolve a clearly supplied spoken repair, while a verbatim target preserves what was actually spoken. Personal context cannot fill in facts that the user never supplied.

## Deferred semantic learning

[PROSE and related research](../research-automatic-personalization.md#what-the-research-supports) motivate extracting component preferences across ordinary edits and validating them against independent examples. This is phase B. It requires a new structured analysis API, a bounded local scheduling design and quality evaluation with the actual installed small model. The current `rewrite_personalized` method is a constrained cleanup operation, not a general memory-writing or repair-classification API.

Phase A introduces no extra model calls, embedding model, background semantic agent, acoustic accent detector, training runner or model promotion. Claims about automatic tone learning or understanding why a user retried would exceed the implementation.
