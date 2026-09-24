# ADR-0021: Automatic runtime and compatible artifact selection

Status: Accepted
Date: 2026-09-25
Related: #35; supersedes ADR-0020's explicit-only selection and optional-build policy

## Context and approval

Choosing a GPU at decode time is insufficient when its runtime requires a different export. Downloads, readiness and execution previously selected these paths independently. The user approved the [concrete proposal](https://github.com/rahulsharmavishwakarma/transcribe/issues/35#issuecomment-5819624222); [approval recorded](https://github.com/rahulsharmavishwakarma/transcribe/issues/35#issuecomment-5820334472).

## Decision

Resolve preferred downloads and available speech execution through `transcribe-models::selection`. Keep the saved preference unchanged and apply the effective override only to speech, preserving the writer's independent selection. Automatic prefers the existing English Parakeet/Qwen MLX exports after verifying the compiled application runtime and probing a real Metal device. Whisper Small follows this policy only in evaluation; its picker gate remains. Cohere, Turbo, custom profiles, unsupported languages and native streaming retain their supported paths. No selected-model identity or application mode is silently changed.

The installer prepares the preferred export. Until it exists, an installed CPU export remains usable. Readiness checks file presence/size; full artifact hashes and directory allowlists remain mandatory before native use. During Automatic initialization, only a typed native model-load failure permits one same-model CPU retry with already installed files. Cancellation, verification and transcription failures do not. The worker retains one failed GPU identity/reason to avoid repeated failing loads; selection changes, changed identities or explicit unload reset it. At most one speech model remains resident. Inference performs no downloads or conversions.

Inventory/removal includes all declared weight exports for the model card, retaining shared runtimes. Per-request evidence distinguishes saved, preferred and effective backend and observed provider/artifacts. Preferred metadata is not execution attestation.

The normal desktop launcher builds/packages MLX for native Apple Silicon targeting aarch64 macOS and preserves app/DMG release targets. Other targets remain feature-off; an explicit portable build switch remains available. Required Swift/Metal tools fail visibly when unavailable. Runtime hashes remain bound to the application; distribution signing qualification is unchanged.

## Research and evidence boundaries

1. Frozen-weight public-API changes: device discovery, artifact selection, bounded initialization fallback and packaging. Existing residency and fresh request state are reused.
2. Runtime-internal work: no new decoder, kernel, quantization format or CUDA graph. ADR-0020's existing completion/frontend patch remains unchanged.
3. Training/checkpoint changes: none.

**Paper result:** [Whisper](https://arxiv.org/abs/2212.04356) and [TDT](https://arxiv.org/abs/2304.06795) describe different model/decoder designs; neither establishes an automatic-device speedup. See the [canonical research map](../research-streaming-inference-papers.md).

**Official capability:** [MLX Swift Device API](https://github.com/ml-explore/mlx-swift/blob/dc43e62d7055353c7f99fa071a4e71d29dfddc44/Source/MLX/Device.swift) scopes GPU execution; [Apple Metal](https://developer.apple.com/documentation/metal/performing-calculations-on-a-gpu) requires an available device. [MLX-Audio-Swift Parakeet](https://github.com/Blaizzy/mlx-audio-swift/blob/d302a5c6080d2bb97bae38c7418f82abb76013b6/Sources/MLXAudioSTT/Models/Parakeet/ParakeetModel.swift) loads runtime-specific artifacts, not Sherpa ONNX graphs. Pins remain MLX Swift 0.31.4 at `dc43e62d7055353c7f99fa071a4e71d29dfddc44`, MLX-Audio-Swift 0.1.3 at `d302a5c6080d2bb97bae38c7418f82abb76013b6`, and the transitive lock in `native/mlx/Package.resolved`. [Tauri CLI](https://v2.tauri.app/reference/cli/) documents target/config merging and the Cargo separator; the installed CLI is 2.11.4.

**Local measurement:** prior explicit-Metal and CPU comparisons remain dated in [inference performance](../inference-performance.md). The new Automatic comparison is recorded separately there. Different CPU/GPU exports, precision and residency prevent interpreting old comparisons as isolated GPU speedups.

**Unverified hypothesis:** generalization to other Apple devices, languages, singing, CUDA, sustained thermals/power and distribution signing remains unqualified. This changes runtime preference for tested profiles, not the default model or a SOTA claim.

## Consequences

Normal Apple Silicon packages grow by the existing native runtime, and optimized model files can be larger. CPU alternatives cost disk space and are not downloaded twice automatically. Cold loading remains distinct from resident latency. Missing CPU files after a GPU initialization failure require explicit preparation. Corrupt/missing feature-on runtime resources fail instead of loading developer files. The user-facing path remains model selection, Save and Download.
