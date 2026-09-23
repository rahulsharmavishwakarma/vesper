# ADR-0020: Optional MLX speech execution on Apple Silicon

Status: Accepted
Date: 2026-09-23
Related: #35, #33, #45; ADR-0012, ADR-0017, ADR-0018

## Context

Sherpa's current CPU package cannot run its dynamic INT8 exports on Metal. MLX requires separate converted artifacts and architecture-specific preprocessing and decoding. The existing worker already owns one selected speech model and one writer; introducing another inference service would duplicate lifecycle management.

The user explicitly approved the [concrete proposal](https://github.com/rahulsharmavishwakarma/transcribe/issues/35#issuecomment-5783631246), recorded in the [approval comment](https://github.com/rahulsharmavishwakarma/transcribe/issues/35#issuecomment-5788299497). This resolves the earlier automatic-review rejection; that rejected patch was never applied.

## Decision

Add an optional `mlx` feature to the existing application and evaluation crates. Reuse architecture registrations with `MLX-Audio-Swift` as a distinct runtime and Metal as the backend. Only explicit Metal selects it. CPU and Automatic continue using their established speech runtimes. Limit initial availability to English Parakeet TDT v3, Qwen3-ASR 0.6B and Whisper Small. Whisper's desktop picker remains gated by ADR-0018. Cohere and Whisper Turbo's isolated quality regressions exclude those GPU artifacts from the application catalog.

The same worker retains at most one selected speech model, including MLX Qwen/Parakeet, and the existing writer. Every native segment starts fresh decoding state. Reuse requires the same model, language, model artifact identities and packaged runtime identities. Hashes are verified before each load; ordinary file replacement invalidates residency. Errors discard the recording; cancellation joins native work before reuse or unload. Native encoder/kernel calls cannot be preempted.

Keep the canonical balanced, non-overlapping energy-boundary splitter. Maximum native segments are 10 seconds for Qwen, 29 for Whisper and 30 for Parakeet. Report complete sample delivery independently of text completeness. No native live-streaming capability is added. Recorded and imported audio share the completed-audio application path and its optional frontend/writer stages.

Package the locally built Swift library, Metal kernels and tokenizer preparation tool as application resources. Build-time hashes bind them to the Rust application. Download model data only from immutable, reviewed URLs in `models/mlx-artifacts.json`; reject unlisted model-directory entries before loading because upstream enumerates weight shards. Qwen's tokenizer preparation runs only at explicit installation, in temporary storage using verified inputs; installation atomically replaces the output only after its pinned hash passes. Inference never downloads files or creates tokenizers. A bundle missing its resources never falls back to a developer path.

## Research and evidence

The paper-to-engineering map remains [canonical](../research-streaming-inference-papers.md). Exact pins: MLX-Audio-Swift `d302a5c6080d2bb97bae38c7418f82abb76013b6`, MLX Swift 0.31.4, MLX Swift LM 3.31.4; complete transitive pins are in `native/mlx/Package.resolved`. The narrow source patch and reproducible build are documented in `patches/mlx-audio-swift/README.md`.

1. Frozen-weight public-API work: artifact verification, shared selected-model residency, fresh request state, bounded audio delivery, packaging, cancellation and measurement.
2. Runtime-internal work: the existing isolated patch reports completion/confidence and corrects Qwen's frontend and integer audio-token geometry. This integration adds no attention kernel, quantization scheme or CUDA graph implementation.
3. Training/checkpoint work: none. TDT is a property of the downloaded Parakeet checkpoint, not an application switch.

**Paper result:** [Whisper](https://arxiv.org/abs/2212.04356) and [TDT](https://arxiv.org/abs/2304.06795) motivate their respective model designs; their published results are not application latency or accuracy measurements.

**Official capability:** [pinned MLX Swift](https://github.com/ml-explore/mlx-swift/tree/dc43e62d7055353c7f99fa071a4e71d29dfddc44), the [Qwen processor](https://github.com/QwenLM/Qwen3-ASR/blob/7c6daf77a2421100f5fb066495372c00129d39ff/qwen_asr/core/transformers_backend/processing_qwen3_asr.py) and [OpenAI's decoder](https://github.com/openai/whisper/blob/v20250625/whisper/decoding.py) define the relevant runtime/frontend contracts.

**Local measurement:** the isolated 84-case English screens and their errors, artifact hashes and hardware envelopes are in [inference performance](../inference-performance.md). Application-path validation is recorded separately there and in #35. CPU and MLX use different exports, precision and sometimes ownership, so their differences are not GPU-only speedups.

**Unverified hypothesis:** broader multilingual quality, code-switching, singing, sustained thermals and cross-device latency remain unqualified. No default-model or SOTA promotion follows from this integration.

## Consequences

Optional builds require Apple Silicon, macOS 14 or newer, full Xcode/Metal tooling and Swift dependencies. GPU files are separate from CPU files and may be substantially larger. Cold shader compilation and allocator caching affect latency and memory. Runtime source and binary identity checks require rebuilding Rust after rebuilding the native package. Distribution signing must preserve the bound native identities; the local optional bundle is not a notarized release. CUDA packaging/export qualification remains separate.
