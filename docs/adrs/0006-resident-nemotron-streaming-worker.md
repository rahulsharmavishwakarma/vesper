# ADR-0006: Resident Nemotron streaming worker with isolated request state

- Status: Accepted
- Date: 2026-09-15

## Context

The default Nemotron adapter launched the NeMo-Speech.cpp CLI for every completed recording. That repeatedly loaded weights, prevented partial results, and could not use the checkpoint's cache-aware native stream lifecycle. Qwen cleanup likewise rebuilt its model for every transcript. Research on stateful Conformer/RNN-T inference supports preserving model-owned caches within an utterance and avoiding overlapping audio recomputation, but does not justify application-managed tensor caches, automatic endpointing, a new quantization, or custom kernels.

The approved proposal is recorded in GitHub issues #2 and #3. Its scope is the frozen-weight application/runtime path exposed by NeMo-Speech.cpp 0.1.0 and llama.cpp 0.1.156.

## Decision

Use one bounded Rust inference worker as the sole owner of at most one Nemotron recognizer and one Qwen model. Cache identity includes the selected artifact path and relevant device option. Unload both handles before saved model/backend changes or installed artifact replacement, and destroy them at application exit.

Call NeMo-Speech.cpp through its versioned C ABI. Isolate dynamic loading, opaque handles, result copying, and all unsafe code in `transcribe-nemo`; keep model-specific C++ CPU/Metal/CUDA/Vulkan kernels and RNNT caches inside the upstream runtime. Every recording creates a fresh native stream, sends bounded non-overlapping mono chunks, polls native results, signals finish, drains the tail, and closes the stream. Imported files use the resident recognizer's offline call.

The UI event contains an operation ID, monotonic sequence, committed/provisional text, processed-audio time, emission time, and final flag. Only the active operation and newer sequence may update visible partial text. Partials are transient; only the final raw transcript is persisted and passed to cleanup.

Retain Qwen weights but create a fresh llama.cpp context per final transcript. Do not run cleanup on mutable partials or reuse transcript KV state. Keep manual Stop authoritative. Use bounded queues outside the real-time audio callback. In `Auto`, retry CPU when native accelerator initialization fails; an explicitly requested accelerator remains an error.

## Consequences

- Repeated dictations amortize artifact verification and model loading while isolating ASR stream and cleanup context state.
- Microphone transcription can display native partials and finalization includes every captured tail frame.
- The audio callback remains lock-free. If a fixed live queue saturates, capture stays responsive, the partial stream is discarded, and final ASR falls back to the complete archived capture instead of silently dropping audio.
- Resident weights increase idle memory until a selection change or application exit; an idle eviction policy is deferred until memory pressure is measured.
- Selected-provider metadata is not yet proof of the device that executed every operator when `Auto` falls back. Runtime-observed provider provenance and visible fallback reporting remain follow-up work.
- Automatic endpointing, right-context tuning, prompt/KV caching, lower-bit Nemotron artifacts, custom CUDA kernels, Kyutai streaming, and new model adapters require separate evidence, approval, and validation.
- Acceptance still requires live-device latency/quality measurements and the documented 100-cycle finish/cancel/reset gate; the initial public-fixture smoke proves only basic lifecycle correctness.
