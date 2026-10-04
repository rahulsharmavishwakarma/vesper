# Automatic dictionary admission

Implementation: [core learning policy](../../crates/vesper-core/src/learning.rs), [existing memory guards](../../crates/vesper-core/src/memory.rs), and [evidence/admission transactions](../../crates/vesper-data/src/learning.rs). Policy version: **4**. These deterministic rules are not calibrated probabilities or acoustic confidence.

Learning is automatic desktop behavior: no enable switch or separate Remember action is needed. Completed edits and result observations use the existing background workers. This changes when the existing policy is available, not its admission thresholds or evidence requirements.

Settings shows one Vocabulary list: words you add, words Vesper learned, and
suggested spellings awaiting your decision. Everything listed is used until you
remove it; there is no per-word off switch. Suggestions are proposals, not
vocabulary: adding one makes it yours, dismissing one suspends it so future
edits never re-learn it. Repeated raw observations and Copy do not teach an alias.

## Evidence flow

A completed ordinary edit is compared against the displayed text, including any memory already applied. The existing contiguous-difference extractor proposes a heard/intended pair. The automatic wrapper permits at most three heard tokens and three intended tokens, with the existing 128-byte phrase limit and an 8,192-byte comparison budget. A target with multiple words must contain exactly the same joined letters as the source, ignoring case and whitespace: for example, `BrooksidePark → Brookside Park`. This supports word-boundary repairs without turning a phrase rewrite into a dictionary rule.

It rejects protected numbers, dates, negation, uncertainty and common words, pure insertions/deletions, broad rewrites, initialism-only punctuation changes, and spans crossing sentence boundaries. The common-word exclusions include demonstratives and question words; `This → These` is a grammatical change, not an automatic spelling rule. A trial through the real deterministic memory stage must apply the proposed alias and reproduce the edited token sequence. Admission can disregard case, spaces/tabs, commas and a single terminal period. Quotes, newlines, other sentence punctuation, lexical order and word boundaries must still match after that trial. These presentation allowances do not learn formatting preferences or change the live replacement algorithm. Case-only edits still supply no candidate. Multiple unrelated corrections in one edit remain unsupported.

A term must look like an identifier or a related spelling. Identifier evidence requires allowed identifier characters and a dot, underscore, double colon or a lowercase-to-uppercase transition. Other spellings normally need at least four normalized Unicode characters and character edit distance no greater than half the longer string. A narrow exception permits equal-length ASCII uppercase acronyms of three to eight characters with exactly one changed letter, such as `QTX → QTY`; protected/common words remain excluded. Lowercase short words do not inherit that exception. Normalization removes whitespace and ASCII punctuation while retaining combining marks. This is a limited lexical heuristic, not phonetic alignment or a multilingual semantic classifier.

## Automatic activation

- A supplied identifier spelling in a known project can activate after one completed eligible edit.
- Other eligible spellings require at least two independent user edits. A lowercase command such as `kubectl` follows this second route unless its spelling meets the identifier-shape rule.
- `Language::Auto` cannot activate an automatic alias because the actual language has not been established by this layer. The evidence can remain provisional.
- A candidate needs project or lexical context. Without a project, up to three informative terms from the original context limit its later application.
- Conflicting existing aliases suspend the candidate. The store does not overwrite or re-enable an equivalent manually managed entry. Full vocabulary capacity leaves the candidate unactivated.

The application stores the canonical term and a single observed alias, candidate provenance, language/context and a store-authored activation time. These entries have no forged human-review source. An explicit later reviewed confirmation may take ownership of the same alias through the manual path.

## Context required for post-recognition use

Admission and use are separate. An active automatic word must also match an unquoted alias or canonical spelling in the raw transcript and at least one retained teaching cue. This fixed guard filters the automatic vocabulary supplied to cleanup and the final deterministic correction stage. Both stages reuse selection from the raw hypothesis, so cleanup cannot create context that authorizes its own later correction. Requested language and project must match; automatic language mode, missing candidate provenance, suspended entries, absent cues and oversized raw input fail closed for automatic words. Manual entries retain their existing rules. Optional pre-recognition hints remain separately controlled and unchanged.

Each independent supporting edit retains at most **16 deduplicated lexical terms, 64 UTF-8 bytes each**, from the displayed before-text. Extraction excludes the heard/intended terms, project words, a fixed English function-word list, purely numeric and quoted terms. It uses a bounded lexical match, not semantic intent or accent detection. Words beyond the 8,192-byte input budget have no automatic context eligibility. An eligible edit still does not label the surrounding transcript as verified speech.

Legacy support with no teaching cues stays stored but cannot satisfy this added guard. A fresh independent edit can supply cues. At the 16-support cap, a fresh cue-bearing record can replace one cue-less legacy record without increasing the support count; repeated audio/episode support cannot add cues this way. Once all records carry cues, new context is not appended beyond that bound. Adding cues to an active entry advances the profile revision in the same transaction. Undo/suspension clears cues. Removing an automatic entry suspends its candidate as negative evidence while keeping past events; review adoption consumes the candidate and its support into the manual entry. Forget deletes all automatic evidence. Policy 4 clears the first-edit context tags only on automatic entries enabled in the operation-local post-ASR snapshot: the validated raw teaching cues authorize both cleanup retrieval and final application. Stored tags and optional pre-ASR hint scope stay unchanged; manual entries keep their existing context rules. This resolves the first-context veto reproduced in the [pipeline review](evaluation.md#pipeline-review-and-learning-opportunities).

This integrates the stronger fixed-context idea from the benchmarks. It does **not** enable the experimental adaptive controller or transfer its reported gains to real use. It can prevent unrelated replacements and can also miss useful paraphrases; [evaluation](evaluation.md) records both.

## Independence and self-reinforcement

Support is deduplicated by source-audio fingerprint and correlated interaction episode, with at most 16 compact support records. Re-running a file under another model does not become independent spelling evidence. Repeated ASR output, repeated attempts in one episode, unchanged generated text and copied text cannot satisfy the two-edit gate.

Usage observations update ranking of already credible entries. Conspicuous identifier forms can also enter the observed-candidate pool, with no alias and no user-authored support. They do not become active replacement rules through frequency alone. Only the actual changed span is corrective evidence; the surrounding machine-generated paragraph is not relabeled as user-authored.

The expected benefit is fewer repeated corrections on later relevant dictation. That benefit remains a hypothesis outside the measured cases. Policy 2 admitted zero spelling pairs from 18 persisted public edits; policy 3 admits two provisional pairs in the same already-inspected corpus. The [diagnosis and follow-up](evaluation.md#correction-admission-follow-up) distinguish admission coverage from later activation and accuracy. A successfully saved draft does not imply that its correction became a vocabulary candidate. Earlier rejected edits are not automatically reprocessed; new eligible corrections supply evidence. Personal vocabulary survives switching models, while the usefulness of individual error aliases can differ between recognizers.

## Retraction and revised targets

Policy 4 withdraws support owned by the current result when a later saved draft
no longer contains its learned target outside quotes. This applies before
activation as well as afterward. Other results' support is preserved, duplicate
callbacks cannot restore a retracted vote, and retraction does not teach a reverse
alias. If the removed support belongs to an active entry, that entry leaves the vocabulary and its candidate stays suspended, so the spelling is not re-learned.

An active automatic target changed back to its original alias, or changed to a
third spelling through an eligible bounded correction, also removes the rejected
word from the vocabulary while keeping its candidate suspended. That edit supplies no new alias: the displayed intermediate word may have
been generated by the application's own memory. Fresh direct corrections of
the original recognizer alias can then support the replacement spelling under
the normal independent-support threshold. A suspended automatic entry does not
block a different supported target; manual entries, including disabled ones,
retain their conflict protection. The old mapping is not reactivated, and live
replacement remains non-cascading. Arbitrary semantic rewrites are still outside
this bounded spelling policy.
