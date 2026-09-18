# ADR-0018: Share the Whisper runtime adapter and selected Sherpa ownership

- Status: Accepted
- Date: 2026-09-18
- Tracking and specific user approval: [issue #45](https://github.com/rahulsharmavishwakarma/transcribe/issues/45). The user approved resident reuse, native completion/no-speech reporting and the token-cap correction explicitly.
- Extends ADR-0017 to Whisper; preserves ADR-0011 segmentation/completion and ADR-0010 qualification gates.

## Context

Small and Turbo are checkpoints of the same Whisper encoder/decoder runtime contract. Separate checkpoint adapters would duplicate installation, recording state, segmentation and cancellation. The pinned Sherpa decoder also imposed an audio-duration token heuristic, hid the distinction between EOS and exhaustion, and silently truncated oversized native input.

## Decision

Register one `Whisper` architecture adapter for pinned multilingual Small and Turbo INT8 ONNX exports. Checkpoint differences select artifact paths and metadata. Reuse the existing installer, verified artifact identity, worker, writing stage and shared segmented-audio implementation. Cohere and Whisper share `resident_sherpa.rs`; their native configurations and result policies remain architecture-specific. There is no general plugin framework or new dependency.

The selected worker retains at most one speech model. Identity includes model variant, artifact descriptors and observed file stamps, language, backend/threads, exact native build and Whisper padding. Verify hashes before load; invalidate on observed changes, native failures, model/language switch or unload. Each segment creates a fresh stream and decode state. Recording text, generated KV and encoder features are never reused across recordings. Direct convenience calls retain per-job ownership.

Whisper receives all samples in contiguous, non-overlapping segments of at most 29 seconds; the existing boundary search and balanced final segment remain shared. Other adapters retain their existing bounds. No whole-recording duration quota is introduced. Exact-zero handling and cancellation retain the common contract. Native execution is synchronous: cancellation suppresses output after native return, without promising immediate interruption.

The native patch reports actual EOS, error, token/context exhaustion and input truncation through the strict existing completion v1 schema. Confidence and detected language use separate stream options. Clear reporting before each decode; reject invalid/nonfinite logits and oversized input instead of returning a prefix. Remove the six-tokens-per-second heuristic, retaining half-text-context (224 text tokens for these artifacts, with observed EOS required) and fixed KV bounds. EOS at the final allowed prediction remains successful completion.

After successful completion, skip text only when no-speech probability exceeds 0.6 **and** average log probability is at most −1. This follows the official policy's confidence override, without claiming numerical equivalence to OpenAI's different suppression/fallback decoder. Auto language is accepted only when the native result identifies English or French. Hindi, code-switching, singing and native Whisper streaming remain unqualified; live streaming requests are rejected before loading weights.

The native adapter and pinned installer are available to evaluation. Product selection and recommendation stay disabled until the release gates pass. The current fixed-threshold noise regression fails; implementation approval is not model qualification. Cohere plus Qwen remains the default. Kyutai remains removed.

## Evidence and implementation categories

**Paper result:** [Whisper](https://arxiv.org/abs/2212.04356) studies large-scale weak supervision and describes confidence-assisted long-form heuristics. Its results do not predict this INT8 CPU backend's WER or latency.

**Official capability:** the pinned [OpenAI decoder](https://github.com/openai/whisper/blob/86098128c0b4f24f0e2aa2994de830614b474227/whisper/decoding.py) computes the SOT no-speech probability and defaults to half-context output. The [transcription implementation](https://github.com/openai/whisper/blob/86098128c0b4f24f0e2aa2994de830614b474227/whisper/transcribe.py) combines no-speech and log probability. Sherpa's [exporter](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/scripts/whisper/export-onnx.py) and [model](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/offline-whisper-model.cc) establish the available logits, token metadata and initial state.

1. Frozen-weight public-API changes: pinned installation, shared resident ownership, fresh streams, segmentation, cancellation and measurements.
2. Runtime-internal changes: completion/confidence/language reporting, strict input/error handling and corrected token allowance. Kernels, attention implementation, KV format and quantized artifacts are unchanged.
3. Training/checkpoint changes: none. No distillation, fine-tuning or streaming-training technique is implemented by this adapter.

**Local measurement:** the [adapter regression](../validation/whisper-adapter-2026-09-18.md) records the executed application path, failures and qualification limits. The earlier development probe is separate evidence. **Hypothesis:** session reuse can reduce repeated setup and a larger token allowance may recover truncated output; neither establishes SOTA latency, Hindi qualification or noise robustness.

Pins: Sherpa 1.13.8 revision `11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`, local build `247e90de15e1772d0122836444a0fc6b1f92f85b32555f1eb3cabaa1c4cbbf69`, patch SHA-256 `b6bfad9330296d2c56309e3dd10d2848820d89cd18e376f90686da2b6595237a`. Small export revision `8f3c18b358db4d1f2fc1eae49d75cd20989e4309`; Turbo `2ca6ff69fc878651b770880507669577ac41c2ff`. Exact artifact hashes are in `models/artifacts.json` and the dated result JSON.

## Consequences

Shared ownership removes duplicated lifecycle logic while preserving incompatible native decoder contracts. Resident weights consume idle RAM, larger output allowance may increase ambiguous-audio latency, and segmented offline inference provides no live partials. The fixed confidence policy is insufficient on the current environmental noise cases. Threshold tuning on those observed cases would contaminate evaluation; any future policy needs separate development and held-out recordings plus quiet-speech false-rejection checks. A new VAD subsystem, native backend or live-streaming algorithm requires a separate bounded proposal and approval.
