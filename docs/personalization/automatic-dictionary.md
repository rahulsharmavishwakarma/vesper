# Automatic dictionary admission

Implementation: [core learning policy](../../crates/transcribe-core/src/learning.rs), [existing memory guards](../../crates/transcribe-core/src/memory.rs), and [evidence/admission transactions](../../crates/transcribe-data/src/learning.rs). Policy version: **1**. These deterministic rules are not calibrated probabilities or acoustic confidence.

Learning is automatic desktop behavior: no enable switch or separate Remember action is needed. Completed edits and result observations use the existing background workers. This changes when the existing policy is available, not its admission thresholds or evidence requirements.

## Evidence flow

A completed ordinary edit is compared against the displayed text, including any memory already applied. The existing contiguous-difference extractor proposes a heard/intended pair. The automatic wrapper permits at most three heard tokens and one intended token, with the existing 128-byte phrase limit and an 8,192-byte comparison budget.

It rejects protected numbers, dates, negation, uncertainty and common words, pure insertions/deletions, broad rewrites, initialism-only punctuation changes, and spans crossing sentence boundaries. A trial through the real deterministic memory stage must reproduce the entire edited string exactly. This reuses quote, overlap, punctuation and resource guards instead of admitting a rule that the application cannot safely apply. Consequently, an edit combining a spelling fix with unrelated punctuation or wording changes often supplies no alias. Case-only changes are also excluded by the existing extractor.

A term must look like an identifier or a related spelling. Identifier evidence requires allowed identifier characters and a dot, underscore, double colon or a lowercase-to-uppercase transition. For other spellings, both normalized strings need at least four Unicode characters and character edit distance no greater than half the longer string. Normalization removes whitespace and ASCII punctuation while retaining combining marks. This is a limited lexical heuristic, not phonetic alignment or a multilingual semantic classifier.

## Automatic activation

- A supplied identifier spelling in a known project can activate after one completed eligible edit.
- Other eligible spellings require at least two independent user edits. A lowercase command such as `kubectl` follows this second route unless its spelling meets the identifier-shape rule.
- `Language::Auto` cannot activate an automatic alias because the actual language has not been established by this layer. The evidence can remain provisional.
- A candidate needs project or lexical context. Without a project, up to three informative terms from the original context limit its later application.
- Conflicting existing aliases suspend the candidate. The store does not overwrite or re-enable an equivalent manually managed entry. Full vocabulary capacity leaves the candidate unactivated.

The application stores the canonical term and a single observed alias, candidate provenance, language/context and a store-authored activation time. These entries have no forged human-review source. An explicit later reviewed confirmation may take ownership of the same alias through the manual path.

## Independence and self-reinforcement

Support is deduplicated by source-audio fingerprint and correlated interaction episode, with at most 16 compact support records. Re-running a file under another model does not become independent spelling evidence. Repeated ASR output, repeated attempts in one episode, unchanged generated text and copied text cannot satisfy the two-edit gate.

Usage observations update ranking of already credible entries. Conspicuous identifier forms can also enter the observed-candidate pool, with no alias and no user-authored support. They do not become active replacement rules through frequency alone. Only the actual changed span is corrective evidence; the surrounding machine-generated paragraph is not relabeled as user-authored.

The expected benefit is fewer repeated corrections on later relevant dictation. That benefit remains a hypothesis outside the measured cases. The public-corpus replay currently has low admission coverage; [evaluation](evaluation.md) records that result. Personal vocabulary survives switching models, while the usefulness of individual error aliases can differ between recognizers.
