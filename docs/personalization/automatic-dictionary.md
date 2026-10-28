# Automatic dictionary admission

Implementation: [core learning policy](../../crates/vesper-core/src/learning.rs), [existing memory guards](../../crates/vesper-core/src/memory.rs), and [evidence/admission transactions](../../crates/vesper-data/src/learning.rs). Policy version: **6**. These deterministic rules are not calibrated probabilities or acoustic confidence.

Learning is automatic desktop behavior: no enable switch or separate Remember action is needed. Completed edits and result observations use the existing background workers. This changes when the existing policy is available, not its admission thresholds or evidence requirements.

Settings shows one Vocabulary list with three row kinds. Proposals Vesper is
still learning appear first with a Learning badge: they are not applied yet.
Words you add and words Vesper already learned are applied: each listed word is
used until you remove it, and there is no per-word off switch. Unticked
automatic words apply when the transcript matches their supporting context;
ticked (confirmed) words always apply. Words you add are always confirmed.
Repeated raw observations and Copy do not teach an alias.

Row actions, as implemented (`PersonalizationPanel.svelte`, `learning.rs`):

- Tick a proposal to adopt it immediately as a manual, confirmed word.
- Tick a learned word to confirm it: it stays automatic (Forget still clears
  it) but skips the per-utterance context gate. Unticking restores the gate.
- Edit a row's spelling or language to convert it to a manual word. The
  candidate evidence is consumed, so it cannot reactivate a duplicate.
- Remove a learned word or a proposal to suspend its evidence: same-phrase
  observations are suspended too, so future edits never re-learn it. Removing
  a word you added carries no such block: a later eligible edit can teach that
  spelling again.

## Evidence flow

A completed ordinary edit is compared against the displayed text, including any memory already applied. The existing contiguous-difference extractor proposes a heard/intended pair. The automatic wrapper permits at most three heard tokens and three intended tokens, with the existing 128-byte phrase limit and an 8,192-byte comparison budget. A target with multiple words must contain exactly the same joined letters as the source, ignoring case and whitespace: for example, `BrooksidePark → Brookside Park`. This supports word-boundary repairs without turning a phrase rewrite into a dictionary rule.

It rejects protected numbers, dates, negation, uncertainty and common words, pure insertions/deletions, broad rewrites, initialism-only punctuation changes, and spans crossing sentence boundaries. The common-word exclusions include demonstratives and question words; `This → These` is a grammatical change, not an automatic spelling rule. A trial through the real deterministic memory stage must apply the proposed alias and reproduce the edited token sequence. Admission can disregard case, spaces/tabs, commas and a single terminal period. Quotes, newlines, other sentence punctuation, lexical order and word boundaries must still match after that trial. These presentation allowances do not learn formatting preferences or change the live replacement algorithm. Case-only edits still supply no candidate. Multiple unrelated corrections in one edit remain unsupported.

A term must look like an identifier or a related spelling. Identifier evidence requires allowed identifier characters and a dot, underscore, double colon or a lowercase-to-uppercase transition. Other spellings normally need at least four normalized Unicode characters and character edit distance no greater than half the longer string. A narrow exception permits equal-length ASCII uppercase acronyms of three to eight characters with exactly one changed letter, such as `QTX → QTY`; protected/common words remain excluded. Lowercase short words do not inherit that exception. Normalization removes whitespace and ASCII punctuation while retaining combining marks. This is a limited lexical heuristic, not phonetic alignment or a multilingual semantic classifier.

## Automatic activation

Policy 6 (28 October 2026; policy 5 plus the ordinary-word row below) derives the number of independent edits from an asymmetric promotion rule ([`evidence.rs`](../../crates/vesper-core/src/evidence.rs)). A spelling activates only when the posterior mean that it is intended exceeds `t = r / (1 + r)`, where `r` is the cost of a wrong automatic change divided by the cost of a missed one. How much harm a wrong change can do depends on what the recognizer heard, which the [correction gate's lexicon](correction-gate.md#lexicon) classifies:

| Heard form                                                                             | Example                                               | Prior          | `r` (`t`) | Independent edits                                                                                                      |
| -------------------------------------------------------------------------------------- | ----------------------------------------------------- | -------------- | --------- | ---------------------------------------------------------------------------------------------------------------------- |
| Contains a non-word, or rejoins the same letters; identifier target in a known project | `sherpa onnx → sherpa_onnx`, `aster tool → AsterTool` | Beta(2, 0.5)   | 4 (0.80)  | 1                                                                                                                      |
| Contains a non-word, other target                                                      | `kube ctl → kubectl`                                  | Beta(0.5, 0.5) | 4 (0.80)  | 2                                                                                                                      |
| Several ordinary words                                                                 | `read me → README.md`                                 | Beta(0.5, 0.5) | 4 (0.80)  | 2, and teaching cues must match at use                                                                                 |
| One capitalized word → capitalized spelling (name repair)                              | `Jonathon → Jonathan`                                 | Beta(0.5, 0.5) | 4 (0.80)  | 2, and teaching cues must match at use                                                                                 |
| One ordinary word → a term (capitalized, acronym or identifier) in a known project     | `data → Detta`, `rack → RAG`, `west → Vesper`         | Beta(0.5, 0.5) | 6 (0.86)  | 3, and at use the text must share two topical teaching cues (function words and words under four letters do not count) |
| One ordinary word → another lowercase word, or outside a project                       | `write → right`, `data → date`                        | —              | ∞         | never; add the word in Vocabulary instead                                                                              |

The priors and cost ratios are design assumptions, not calibrated probabilities. With `r = 9` (`t = 0.90`), every route above would need five independent edits under the Jeffreys prior, which a single user who rarely edits would almost never supply. The strict "never" row is where the review found real harm (`write → Wright`, `notes → Nots`). Policy 4 activated `read me → README.md` after one edit. Policy 5 needs two, because `read me` is ordinary speech ("read me the list").

Two inputs to the ordinary-word row are normalized:

- A capital that only marks a sentence start counts as the lowercase word (`Data is…`), so it adds to the same rule.
- A misheard short acronym counts as a spelling repair when it starts with the same letter and is at most two edits away (`rack → RAG`), not `use → ISO`.

**Evaluation of the ordinary-word row** (local, 28 October 2026). `policy6_replay` taught three realistic corrections each for `data → Detta` and `rack → RAG`, then ran 465 local transcripts and 20 held-out sentences through the real learned-alias path.

| Cue rule at use                         | Wrong changes in history (38 `data`, 1 `rack`)              | Held-out: fixed / wrongly changed                                                                                                                                        |
| --------------------------------------- | ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| One shared cue (as for phrases)         | 7 wrong; 4 correct (the Detta message 3×, `rack system` 1×) | not run                                                                                                                                                                  |
| Two shared cues, function words counted | 2 wrong (`from` + `how`, `how` + `what`); 3 correct         | not run                                                                                                                                                                  |
| Two topical cues (shipped)              | 0                                                           | 10 / 10 positives fixed; 3 / 10 adversarial negatives changed (ordinary `data` sentences sharing two topic words, such as `Our data comes from Fermion research papers`) |

The history was used to choose the cue rule, so its 0 is a development figure; the held-out sentences were written by the developer. The rule fixes an ordinary-word error when the speaker is talking about the term's topic. It can be fooled by an ordinary sentence that shares two of those topic words, and one change-back then stops that rule.

Any rejection of an applied rule suspends it outright: one strike, with no posterior update.

- `Language::Auto` cannot activate an automatic alias because the actual language has not been established by this layer. The evidence can remain provisional.
- `Language::Auto` cannot activate an automatic alias because the actual language has not been established by this layer. The evidence can remain provisional.
- A candidate needs project or lexical context. Without a project, up to three informative terms from the original context limit its later application.
- Conflicting existing aliases suspend the candidate. The store does not overwrite or re-enable an equivalent manually managed entry. Full vocabulary capacity leaves the candidate unactivated.

The application stores the canonical term and a single observed alias, candidate provenance, language/context and a store-authored activation time. These entries have no forged human-review source. An explicit later reviewed confirmation may take ownership of the same alias through the manual path.

## Context required for post-recognition use

Admission and use are separate. An active automatic word must also match an unquoted alias or canonical spelling in the raw transcript and at least one retained teaching cue. This fixed guard filters the automatic vocabulary supplied to cleanup and the final deterministic correction stage. Both stages reuse selection from the raw hypothesis, so cleanup cannot create context that authorizes its own later correction. Requested language and project must match; automatic language mode, missing candidate provenance, suspended entries, absent cues and oversized raw input fail closed for automatic words. Manual entries retain their existing rules. Optional pre-recognition hints remain separately controlled and unchanged.

Each independent supporting edit retains at most **16 deduplicated lexical terms, 64 UTF-8 bytes each**, from the displayed before-text. Extraction excludes the heard/intended terms, project words, a fixed English function-word list, purely numeric and quoted terms. It uses a bounded lexical match, not semantic intent or accent detection. Words beyond the 8,192-byte input budget have no automatic context eligibility. An eligible edit still does not label the surrounding transcript as verified speech.

Legacy support with no teaching cues stays stored but cannot satisfy this added guard. A fresh independent edit can supply cues. At the 16-support cap, a fresh cue-bearing record can replace one cue-less legacy record without increasing the support count; repeated audio/episode support cannot add cues this way. Once all records carry cues, new context is not appended beyond that bound. Adding cues to an active entry advances the profile revision in the same transaction. Undo/suspension clears cues. Removing an automatic entry suspends its candidate as negative evidence while keeping past events; review adoption consumes the candidate and its support into the manual entry. Forget deletes all automatic evidence. Policy 4 clears the first-edit context tags only on automatic entries enabled in the operation-local post-ASR snapshot: the validated raw teaching cues authorize both cleanup retrieval and final application. Stored tags and optional pre-ASR hint scope stay unchanged; manual entries keep their existing context rules. This resolves the first-context veto reproduced in the [pipeline review](evaluation.md#pipeline-review-and-learning-opportunities).

This integrates the stronger fixed-context idea from the benchmarks. It does **not** enable the experimental adaptive controller or transfer its reported gains to real use. It can prevent unrelated replacements and can also miss useful paraphrases; [evaluation](evaluation.md) records both.

## Recognizer-keyed aliases

Each supporting edit records the recognizer family that produced the heard form, such as `cohere`, `parakeet`, `nemotron`, `qwen3-asr` or `whisper`. Runtimes of the same model share a family; fine-tunes such as Phonon-2 or Parakeet Redux have their own. An unconfirmed automatic alias applies only while a recognizer from one of its supporting families is selected, because a confusion learned from one model's errors says little about another's.

The learned _spelling_ is still model-independent: it joins the working set's references, so the [correction gate](correction-gate.md#near-miss-repair) can repair a non-word one letter away from it (`kubernets` → `kubernetes`) under any recognizer. Ordinary-word aliases stay with their family.

Support recorded before policy 5 has no family and keeps applying to all models. Confirmed (ticked) and manual words are model-independent. Switching models therefore never deletes evidence: the alias waits in shadow until that model is selected again, or until fresh edits support it for the new one.

## Policy breaker

When the user rejects applied output from **two distinct** automatic rules within seven days (Undo on a result, or a saved draft that restores the heard form or picks a third spelling), the breaker trips. Every unconfirmed automatic alias then stops applying, and no new alias activates. This protects against a faulty policy rather than one bad word. Manual and confirmed words keep working.

The breaker clears when the user resumes automatic spellings (`resume_automatic_spellings`), after Forget, or when a newer learning policy version is installed. Removing a word from the Vocabulary list is housekeeping and does not count as a rejection.

## Independence and self-reinforcement

Support is deduplicated by source-audio fingerprint and correlated interaction episode, with at most 16 compact support records. Re-running a file under another model does not become independent spelling evidence. Repeated ASR output, repeated attempts in one episode, unchanged generated text and copied text cannot satisfy the two-edit gate.

Usage observations update ranking of already credible entries. Phrases the recognizer was biased toward in that decode are excluded, so recognition hints cannot confirm themselves. Conspicuous identifier forms can also enter the observed-candidate pool, with no alias and no user-authored support. They do not become active replacement rules through frequency alone. Only the actual changed span is corrective evidence; the surrounding machine-generated paragraph is not relabeled as user-authored.

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
