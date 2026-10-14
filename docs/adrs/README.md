# Architecture Decision Records

This directory records decisions that are expensive to rediscover or easy to accidentally reverse. The documents describe the current implementation; research notes and feature wish lists are not ADRs.

## Index

| ADR | Status | Decision |
| --- | --- | --- |
| [0001](0001-local-first-native-desktop.md) | Accepted | Run the product as a local-first native desktop pipeline |
| [0002](0002-split-persistence-by-data-shape.md) | Accepted | Store settings, sessions, audio/models, and transient state separately |
| [0003](0003-require-runtime-specific-model-adapters.md) | Accepted | Require a real runtime adapter and pinned artifacts for model support |
| [0004](0004-persist-raw-asr-before-cleanup.md) | Accepted | Persist and publish raw ASR before optional cleanup |
| [0005](0005-rust-orchestration-with-native-accelerators.md) | Accepted | Keep orchestration in Rust and use native C/C++ runtimes for accelerated kernels |
| [0006](0006-resident-nemotron-streaming-worker.md) | Accepted | Retain Nemotron/Qwen weights while isolating per-recording stream/context state |
| [0007](0007-report-native-asr-completion.md) | Superseded by ADR-0011 | Require native completion reports for bounded Qwen3-ASR/Cohere decoding |
| [0008](0008-guarded-technical-tail-correction.md) | Retired 2026-10-14 | Guard one technical tail correction before full Qwen cleanup (removed; depended on a fixture fallback) |
| [0009](0009-no-general-transcription-time-quota.md) | Superseded by ADR-0011 | Remove general transcription time quotas while retaining adapter and resource checks |
| [0010](0010-evidence-gated-asr-recommendations.md) | Accepted | Qualify and recommend ASR models only through compatible local evidence |
| [0011](0011-segment-long-generative-asr.md) | Accepted | Remove the remaining whole-audio cutoff through bounded Qwen/Cohere segments with completion checks |
| [0012](0012-adapter-registry-and-reviewed-personalization.md) | Accepted | Register inference contracts, reviewed local preferences, and an opt-in complete-state prefix cache |
| [0013](0013-lfm-audio-native-adapter.md) | Accepted | Retain LFM audio backbone/projector weights with fresh completed-audio segment state |
| [0014](0014-native-dictation-and-final-insertion.md) | Accepted | Native hold/toggle control, nonactivating macOS recorder, and final insertion with Copy fallback |
| [0015](0015-model-independent-local-memory.md) | Superseded by ADR-0016 | Keep reviewed correction, context, intent and pronunciation-confusion memory on device across model changes |
| [0016](0016-automatic-local-spelling-learning.md) | Accepted | Add optional automatic spelling learning from ordinary local edits with separate evidence and reversible activation |
| [0017](0017-resident-cohere-recognizer.md) | Accepted | Retain selected Cohere sessions with verified load, observed-file invalidation and fresh segment streams |
| [0018](0018-shared-whisper-adapter.md) | Accepted | Share Whisper checkpoint configuration and selected Sherpa ownership; gate product selection on qualification |
| [0019](0019-shared-audio-frontend.md) | Accepted | Share advisory Silero and opt-in DPDFNet processing across live and completed recordings |
| [0020](0020-optional-mlx-speech-runtime.md) | Selection/build policy superseded by ADR-0021 | Package optional MLX speech execution with verified artifacts and bounded selected-model residency |
| [0021](0021-automatic-accelerator-artifacts.md) | Accepted | Select compatible accelerated artifacts automatically and include MLX in native Apple Silicon builds |
| [0022](0022-local-speaker-timelines.md) | Accepted | Add experimental local speaker timelines after ASR with isolated native packaging and anonymous activity |

## Convention

ADRs are numbered, immutable decision records with a title, status, date, context, decision, and consequences. A changed decision gets a new ADR that marks the old one superseded; do not rewrite history. Use `Proposed`, `Accepted`, `Deprecated`, or `Superseded by ADR-NNNN` as the status.

The current operational/storage truth is [Application internals](../internals.md). Broader component relationships are in [Architecture](../architecture.md).
