# ADR-0016: Learn scoped spellings from ordinary local edits

Status: Accepted

Date: 2026-09-16

Approval: [issue #34](https://github.com/rahulsharmavishwakarma/transcribe/issues/34#issuecomment-5688739690), following the [concrete phase-A proposal](https://github.com/rahulsharmavishwakarma/transcribe/issues/34#issuecomment-5688658223). The user approved implementation and requested detailed per-component documentation.

## Context

ADR-0015 requires a separate confirmation before remembering each reviewed correction. That remains useful for explicit supervision but does not supply the requested low-effort experience. Ordinary edits can provide spelling evidence without verifying an entire audio transcript. Usage and retries are weaker signals and need separate treatment.

## Decision

Extend the existing application-owned SQLite profile to schema 3 and add a bounded local feedback table. Provide editable final drafts with result/revision guards, automatic scoped spelling admission, usage-based ranking, provisional retry relationships, undo/suspension and logical deletion with permission epochs. Keep raw ASR, machine cleanup, personalized text, ordinary drafts and reviewed targets distinct. Automatic evidence never fabricates a human review or enters supervised targets implicitly.

Amended 2026-09-18 by the [explicit user requirement](https://github.com/rahulsharmavishwakarma/transcribe/issues/34#issuecomment-5721621186): self-improvement is mandatory automatic desktop behavior running in the background. This supersedes the original off-by-default choice. Desktop profile reads persist learning enabled for new and existing profiles through existing revision/epoch guards, and desktop saves cannot restore the retired opt-in gate. The UI explains local learning and retains undo, entry controls and forgetting. Existing event-driven workers perform the bounded work; no scheduler or new model owner is introduced. No per-entry Remember action is required for eligible automatic admissions. Audio retention remains separately controlled from automatic minimal learning evidence; no cloud inference or external-document observer is introduced.

The exact component contracts and resource/admission limits are in the [implementation guide](../personalization/README.md). This decision supersedes ADR-0015's requirement that every memory activation follow a separate review/remember action. Its manual workflow, inference ownership, native limits, deterministic application guards, raw fallback and training deferral remain applicable.

## Evidence and inference boundary

The [research synthesis](../research-automatic-personalization.md) distinguishes paper results, official capability, local measurement and unverified hypotheses. PROSE, GEC-RAG, contextual vocabulary retrieval, ReSpect and dialogue-repair analysis inform the design; the implementation is not a reproduction of their methods or published metrics. Its lexical retry rules do not implement a trained feedback decoder.

Category 1: new application records and UI use existing frozen-weight text/hint interfaces. There are no category-2 runtime-internal decoder/kernel changes or category-3 training/checkpoint changes. Phase B structured semantic analysis needs a separate approved application API/scheduling proposal.

Runtime/model pins remain unchanged: NeMo-Speech.cpp v0.1.0 `4f9676226f667d14608487df744f375db87127f8`, Nemotron model `ea30d66debe3740a08b573244286791d423d6b3e`; sherpa-onnx Rust 1.13.8/native `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`, Cohere model `156a470cf08eefe706a0004f3c52d9ee567ca7a0`; llama-cpp-2 0.1.156/native `e79e4bf660e19f2ad851e06c6913f7a8c5852621`, Qwen GGUF `8fea620810c4afa23dd6443f999a48574c1611a3`. Exact public API links and artifact hashes remain in [ADR-0015](0015-model-independent-local-memory.md#evidence-and-boundaries).

## Consequences

Eligible normal edits can improve later scoped text without model-specific memory or an extra confirmation. Conservative rules miss many valid corrections, cannot infer unknown spelling from repeated ASR guesses, and do not diagnose why a user retried. Existing external-editor corrections remain invisible. Profile revisions may change with usage/admission, so stale inspector saves must refresh.

Behavioral tests and chronological replay qualify bounded implementation properties. The public one-off correction replay does not show automatic accuracy gains. The automatic product requirement does not qualify long-term user effort, real microphone conditions or semantic phase B; these remain separate evaluation work in issue #34. Staging and commits remain unauthorized.
