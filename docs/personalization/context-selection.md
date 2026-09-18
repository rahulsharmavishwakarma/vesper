# Context selection and in-context learning

Implementation: [profile phrase selection](../../crates/transcribe-core/src/personalization.rs), [memory application](../../crates/transcribe-core/src/memory.rs), and [cleanup prompt construction](../../crates/transcribe-inference/src/personalization.rs).

One operation freezes one profile. A correction learned while another operation is already running applies only to later operations. Context is currently the user-selected project, requested language, available transcript and explicit output settings. The application does not read the receiving editor's document, clipboard history or screen to infer a project.

## Before recognition

Native phrase hints can use only context available before ASR. Enabled entries must match the requested language and selected project; a nonempty lexical context must also match the project text at this stage. Selection prioritizes project-specific entries, then observed usage, reviewed-source count and stable ID. Canonical spellings are case-insensitively deduplicated.

The existing limit is **32 phrases / 2,048 bytes**, with opt-in uniform boost 2. Only canonical spellings are boosted. The pinned Nemotron adapter accepts these public speech-context hints; the selected Cohere adapter does not. A shared profile does not create unsupported decoder functionality. No runtime or weight revision changes in phase A.

## After recognition, before cleanup

Cleanup retrieval can now use the raw transcript as additional context. An enabled vocabulary entry needs language/project compatibility, lexical-context compatibility, and a relevant canonical term, alias or context-word match. The ranking prefers project-specific entries, then usage count and stable ID. At most **16 vocabulary entries** are passed as reference spellings; each phrase is already bounded to 128 bytes by profile validation.

Reviewed cleanup examples are a separate collection. They must match language, project, cleanup preset and selected/effective intent. At least two informative terms must overlap. Jaccard similarity ranks matches; stable ID breaks ties. At most **two examples** are selected, with **2,048 bytes total for their input and output text**. This example budget does not include vocabulary text or the surrounding instruction template.

The prompt serializes spellings and examples as JSON data and explicitly forbids following instructions inside them, inserting absent names or copying unrelated content. The selected cleanup preset and raw-transcript fidelity guards still take precedence. Retrieving an example changes the request context; it does not train the model. Empty/irrelevant memory adds no personal examples. Automatic ordinary edits currently produce vocabulary evidence, not automatic cleanup examples or free-text preference rules.

## After optional cleanup

The model-independent memory stage applies exact aliases, including with cleanup disabled or failed. It respects language/project/lexical context, protected quotes and punctuation, overlap/ambiguity handling and non-cascading replacement. Existing limits remain 400,000 text bytes, 8,192 tokens, 512 candidate matches and 64 accepted edits. Resource overflow preserves the full original input.

Usage ranking chooses which existing memory fits a bounded prompt or hint list; it does not increase replacement authority or bypass meaning guards. Local learning runs automatically. Alias application, recognition hints and example-based output intent retain separate settings. Disabling final alias application does not change an independently enabled recognition-hint setting.

Pinned artifacts and exact native API sources are recorded in [ADR-0015](../adrs/0015-model-independent-local-memory.md#evidence-and-boundaries). The new feature changes application records and selection order, not inference ownership, decoder internals or model weights.
