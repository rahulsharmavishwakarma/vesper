# ADR-0009: No general transcription time quota

- Status: Superseded by [ADR-0011](0011-segment-long-generative-asr.md)
- Date: 2026-09-15
- Tracking: [GitHub issue #28](https://github.com/rahulsharmavishwakarma/transcribe/issues/28)

## Context

Transcription runs locally. The former 120-second default and configurable
600-second ceiling were application policy, not a hosted-service quota. The
user requested removing that policy from the product, UI, and documentation.
Local execution still depends on available resources and the actual model's
input, output, and context capabilities.

## Decision

Do not impose a general wall-clock or audio-duration quota on recording or file
transcription. Manual Stop remains authoritative. Remove the maximum recording
length control and show “No fixed time limit” where no adapter cutoff applies.
Keep the elapsed timer, cancellation, progress, and explicit error reporting.

Retain `maxRecordingSeconds` in saved configurations for backward compatibility,
but ignore it when determining runtime admission. New configurations use zero.
The effective runtime limit is zero for no duration cutoff, or a positive
adapter restriction. Old saved values must never reinstate a general timer.

Preserve the existing capture worker, bounded streaming queues, artifact
verification, completion reporting, and resource checks. Recording keeps its
512 MiB audio buffer ceiling; reaching it stops capture with an explicit warning
and preserves captured audio. File imports and decoded buffers keep their
512 MiB protections. These are resource bounds, not promises of unlimited memory.

ADR-0007 remains in force: the current Qwen3-ASR and Cohere adapters are qualified
for clips up to ten seconds. The UI must name this adapter limitation, rather
than suggest it is a general product quota. Longer audio for these adapters
requires separately approved segmentation/decoding work and qualification.

## Consequences

- Nemotron, Kyutai, Parakeet, and compatible transducer profiles no longer reject
  audio merely because it crosses the old application duration setting.
- This change does not prove model accuracy, successful long-form inference,
  or constant memory consumption for arbitrary recordings. Writing-model context
  limits remain; a failed writing pass retains the raw transcript.
- Capture still owns a bounded in-memory buffer. With no duration cutoff it
  reserves capacity up to the existing memory ceiling; allocation failures remain
  explicit. Disk-backed capture and segmented inference are separate architectural
  work requiring approval, not implied by this policy change.
- Runtime guards and UI wording share the effective adapter limit. Tests must
  cover old settings, audio beyond the former ceiling, model-specific rejection,
  and capture preservation at resource exhaustion.
