# Context selection and in-context learning

Implementation: [profile phrase selection](../../crates/vesper-core/src/personalization.rs), [memory application](../../crates/vesper-core/src/memory.rs), and [cleanup prompt construction](../../crates/vesper-inference/src/personalization.rs).

One operation freezes one profile and its compact automatic-learning evidence in one SQLite read transaction, after the existing desktop enable migration. A correction learned while another operation is already running applies only to later operations. Context is currently the user-selected project, requested language, available transcript and explicit output settings. The application does not read the receiving editor's document, clipboard history or screen to infer a project.

Projects are names in this profile, not detected folders or separate databases.
Save **Current project** before recording and change it when moving between
projects. Blank-scoped vocabulary/examples are shared; nonblank entries match
the selected project case-insensitively after trimming whitespace. A blank
selection uses shared entries only. The per-recording scope is retained in
History: remembering an older cleanup example or opening its correction review
uses that recording's project, even after the selected project changes. Blank
and legacy recording scopes remain shared. A correction's project can still be
changed explicitly before remembering it, or later in Vocabulary.

## Before recognition

Native phrase hints can use only context available before ASR. Enabled entries must match the requested language and selected project; a nonempty lexical context must also match the project text at this stage. Selection prioritizes project-specific entries, then observed usage, reviewed-source count and stable ID. Canonical spellings are case-insensitively deduplicated.

The existing limit is **32 phrases / 2,048 bytes**, with opt-in uniform boost 2. Only canonical spellings are boosted. The pinned Nemotron adapter accepts these public speech-context hints; the selected Cohere adapter does not. A shared profile does not create unsupported decoder functionality. No runtime or weight revision changes in phase A.

## After recognition, before cleanup

The desktop first calls the shared `DictationPersonalizationSnapshot::post_asr_profile` selector with the raw hypothesis. Automatically sourced vocabulary needs an active, matching candidate, an unquoted alias/canonical occurrence, explicit matching language/project and at least one shared [teaching cue](automatic-dictionary.md#context-required-for-post-recognition-use). Missing/legacy context disables that word only in the operation's copied profile; stored enabled flags and manual entries are preserved. Cleanup and final replacement use this same selected profile. This prevents model-generated context from admitting an otherwise ineligible learned word; it cannot prevent every error a language model might independently generate.

Cleanup retrieval can now use the raw transcript as additional context. An enabled vocabulary entry needs language/project compatibility, lexical-context compatibility, and a relevant canonical term, alias or context-word match. The ranking prefers project-specific entries, then usage count and stable ID. At most **16 vocabulary entries** are passed as reference spellings; each phrase is already bounded to 128 bytes by profile validation.

Reviewed cleanup examples are a separate collection. They must match language, project, cleanup preset and selected/effective intent. At least two informative terms must overlap. Jaccard similarity ranks matches; stable ID breaks ties. At most **two examples** are selected, with **2,048 bytes total for their input and output text**. This example budget does not include vocabulary text or the surrounding instruction template.

The prompt serializes spellings and examples as JSON data and explicitly forbids following instructions inside them, inserting absent names or copying unrelated content. The selected cleanup preset and raw-transcript fidelity guards still take precedence. Retrieving an example changes the request context; it does not train the model. Empty/irrelevant memory adds no personal examples. Automatic ordinary edits currently produce vocabulary evidence, not automatic cleanup examples or free-text preference rules.

## After optional cleanup

The model-independent memory stage applies exact aliases, including with cleanup disabled or failed. It respects language/project/lexical context, protected quotes and punctuation, overlap/ambiguity handling and non-cascading replacement. Existing limits remain 400,000 text bytes, 8,192 tokens, 512 candidate matches and 64 accepted edits. Resource overflow preserves the full original input.

The additional automatic-context selector has the narrower 8,192-byte raw-hypothesis budget of ordinary learning. Above that size, automatic entries are withheld from post-ASR use; manual memory follows its existing larger bounds. This is a conservative limit, not evidence that long recordings improve. Pre-ASR hints were already selected from the original frozen profile and remain a separate opt-in path; withholding a post-ASR entry does not undo acoustic hint effects.

Usage ranking chooses which existing memory fits a bounded prompt or hint list; it does not increase replacement authority or bypass meaning guards. Local learning runs automatically. Alias application, recognition hints and example-based output intent retain separate settings. Disabling final alias application does not change an independently enabled recognition-hint setting.

Pinned artifacts and exact native API sources are recorded in [ADR-0015](../adrs/0015-model-independent-local-memory.md#evidence-and-boundaries). The new feature changes application records and selection order, not inference ownership, decoder internals or model weights.
