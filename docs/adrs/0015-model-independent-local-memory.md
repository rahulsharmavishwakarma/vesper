# ADR-0015: Keep continuous personalization in local application memory

Status: Superseded by ADR-0016 for the activation contract; its manual workflow and inference boundaries are preserved. See [ADR-0016](0016-automatic-local-spelling-learning.md).

Date: 2026-09-16

Approval: [#34, local memory scope](https://github.com/rahulsharmavishwakarma/transcribe/issues/34#issuecomment-5686786110). The user approved all six application-level capabilities, required on-device operation without cloud APIs, and deferred training until their own model publication. This extends ADR-0012's profile and context selection; it does not change its inference ownership or prefix-cache decisions.

## Context

A vocabulary tied to one decoder cannot provide a consistent personalization experience across the available speech models. The selected Cohere adapter has no native contextual phrase support. Reviewed corrections should improve later relevant dictation without weight updates, external services, or treating model guesses as supervision.

## Decision

Extend the existing SQLite profile to schema 2. Vocabulary entries own canonical spelling, up to eight observed aliases, language, optional project/context, a pronunciation-confusion label, reviewed source references, enabled state and a store-authored learning timestamp. Model identity is absent from memory ownership. Schema 1 loads with empty aliases; existing preferences do not silently acquire replacement rules. Optimistic revision checks remain mandatory.

After an explicit verbatim or cleanup review, offer at most one bounded contiguous correction per target kind, deduplicated across those targets. Saving a review does not activate memory. A separate remember command validates the latest suggestion and supporting review. Source-backed aliases must each match a saved review; manually authored edits clear source links. Repeated compatible evidence merges into one entry. Common numbers, dates, negation, uncertainty and repair terms are excluded from automatic suggestions, as are pure insertions/deletions. These lexical guards are conservative heuristics, not complete multilingual semantic analysis.

Each operation freezes one profile. Native phrase hints use only enabled canonical spellings in compatible language/project/context, selected from information available before recognition. The existing Nemotron limit remains 32 phrases/2048 bytes, with opt-in uniform boost 2. Unsupported adapters receive no hints. Cleanup retrieves scoped spellings/aliases and up to two examples within its existing prompt budget and fidelity guard.

A deterministic final correction stage runs after optional cleanup, including when cleanup is disabled or fails. It matches explicit aliases as token spans, preserving Unicode combining marks and internal identifier punctuation. It does not fuzzy-match, cross sentence punctuation/newlines, edit single/double-quoted spans, cascade replacements or resolve overlapping conflicts. It leaves the complete input unchanged on resource-budget overflow: 400,000 bytes, 8,192 tokens, 512 candidate matches or 64 accepted edits. The pronunciation label records the user's interpretation of a recurring error; it adds no acoustic classifier.

Store personalized text separately from immutable raw ASR, immutable model cleanup and human revisions. Evidence records profile revision, applied entry IDs/counts, ambiguity, budget decisions and any output-form suggestion. Final Copy/insertion prefers personalized output. Per-recording undo rejects that extra result and restores the machine fallback. Unsaved results also support undo in runtime state; stale result IDs cannot alter a newer result. Pausing/deleting saved memory affects new operations. Historical applied IDs can refer to an edited or removed entry; the inspector uses the current entry, not a historical profile snapshot.

Output form remains explicit by default. An optional suggestion requires at least two distinct remembered sessions to agree among the top three lexically relevant examples, within language/project scope. Retrieval uses at least two shared informative terms and Jaccard overlap of at least one third. Automatic use requires the user's opt-in and falls back to the selected form on disagreement. These are unqualified engineering thresholds, not confidence probabilities. Preferences guide presentation only; they do not add facts or execute commands.

The local learning report inspects at most 512 recent sessions. Actual uses/undos and reviewed coverage are distinct from paired text replay. Replay compares saved raw text and current eligible aliases against verbatim reviews, excludes source/duplicate audio, requires source reviews and store-authored learning times to precede the evaluated recording, and preserves both improvements and regressions. It groups by model/language plus a hash of stored static provenance and settings, excluding per-operation execution events. It never reruns ASR or qualifies a checkpoint. Matching hashes do not prove identical unstored hardware/corpus conditions; overlapping/near-duplicate audio and holdout selection still need the native qualification protocol.

The review queue reserves up to four observed-error cases, with language/day diversity, plus one otherwise-unflagged audit case. Reasons come from explicit rejection, an undone edit, conflicting memory or a recurring alias. The audit is deterministic, not random; queue statistics do not establish population accuracy. No numeric ASR confidence is manufactured.

All these operations use existing local Rust, SQLite and the selected installed native model. No new dependency, service, embedding model, cloud inference API, automatic training or promotion is introduced.

## Evidence and boundaries

The [recent research synthesis](../research-personal-dictation.md) uses a 2024-09-16–2026-09-16 window and separates historical background. Primary evidence includes [GEC-RAG (2025)](https://arxiv.org/html/2501.10734v1), [ASR correction-interface confidence study (2025)](https://arxiv.org/abs/2503.15124), [Ontology Memory-Augmented ASR Correction (2026)](https://arxiv.org/html/2606.13464v2), and [MemConflict (2026)](https://arxiv.org/html/2605.20926v1). These motivate external correction memory, a raw fallback, scoped conflicts and cautious review selection. This implementation is not a reproduction of those systems and inherits none of their quality or performance claims.

Official runtime boundary, unchanged:

- NeMo-Speech.cpp v0.1.0 `4f9676226f667d14608487df744f375db87127f8`, [public ABI](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/4f9676226f667d14608487df744f375db87127f8/include/nemo_speech/asr.h); Nemotron revision `ea30d66debe3740a08b573244286791d423d6b3e`, Q8 SHA-256 `3fc991d3badad7277c11030a7519832cddaf2057aafed6d4b25147e953a070b1`.
- sherpa-onnx Rust 1.13.8, native `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`; Cohere ONNX revision `156a470cf08eefe706a0004f3c52d9ee567ca7a0`. [Official hotword guidance](https://k2-fsa.github.io/sherpa/onnx/hotwords/index.html) is architecture-specific, not a generic Cohere option.
- `llama-cpp-2` 0.1.156, native `e79e4bf660e19f2ad851e06c6913f7a8c5852621`; Qwen GGUF revision `8fea620810c4afa23dd6443f999a48574c1611a3`, Q4 SHA-256 `57d1997790d1744fba5b40a7317df71ea5e2acee28c47e78f0cce39c0703f8cf`.

Evidence classes: papers provide results in their own experimental settings; official runtime docs establish supported interfaces; synthetic local tests establish application behavior; fewer personal errors, accurate intent suggestions and useful review selection remain unverified hypotheses requiring later-session evaluation. The [ASR protocol](../../benchmarks/asr/README.md) remains authoritative for model qualification.

This is category 1 frozen-weight application/public-API work. New decoder graphs, acoustic-state access, kernels or arbitrary adapter loading are category 2 and excluded. Accent classifiers, acoustic adaptation, LoRA, distillation and model training/promotion are category 3 and deferred under #5. Downloaded weights do not gain these techniques through configuration.

## Consequences

Memory survives model changes, and confirmed exact corrections can work without cleanup. Native hints and generative preference adherence remain model-dependent. A different ASR model can stop making an old error or expose new errors, so users can inspect, scope, pause, remove and undo memory.

Conservative matching misses unfamiliar variants, multiple distant edits and some valid identifiers/names containing protected terms. Explicit context and review are a deliberate burden until false-substitution and user-effort evidence supports broader automation. Large transcripts retain their original result rather than receiving partial memory edits.

The report is an explanatory text comparison, not a locked prospective holdout, calibrated confidence display, acoustic-quality claim or automatic promotion gate. Collection must be enabled to retain future reviewed evidence; disabling it still permits existing local memory to personalize unsaved results.
