# ADR-0022: Local speaker timelines after ASR

Status: Accepted
Date: 2026-09-25
Related: [#42 approval](https://github.com/rahulsharmavishwakarma/transcribe/issues/42#issuecomment-5821925540)

## Context

The user approved integrating the GA Nemotron 3 Diarization candidate into the
ASR pipeline after the [CPU/Metal sample test](../validation/nemotron-diarization-2026-09-25.md).
Speaker activity supplies information absent from plain ASR text. It does not
correct words, separate voices acoustically, identify the owner, or establish
recursive improvement. The measured native output overruns one clip by 10 ms.

## Decision

Add an experimental `speakerDiarization` setting, default false for existing and
new configurations. The desktop saves and publishes raw ASR first, then submits
the original completed 16 kHz mono recording to the existing bounded inference
worker. Run a fresh diarization model/stream for each job; existing ASR model
ownership and residency remain unchanged. Use the public streaming state machine
over completed input in non-overlapping 320 ms packets, then finish exactly once.
This is post-recording analysis, including after final live ASR; it emits no live
speaker updates. No new service or inference-time download is introduced.

Validate finite, ordered intervals and one-based speaker IDs 1–8. Clip only a
maximum 10 ms native end-frame overrun (plus floating-point tolerance), record
the count and reject larger errors. Preserve simultaneous activity as overlapping
intervals. Bound native result allocation to 100,000 segments. Cancellation is
checked around native loading, each synchronous push, finish and result retrieval;
the upstream standalone API does not interrupt a native call in progress.

Persist a typed optional `speakerTimeline` on the local session and return it on
the latest result even when audio collection is off. Missing/invalid artifacts,
native errors or invalid intervals preserve the raw transcription and produce a
warning. Cancellation suppresses final delivery but does not erase saved raw text.
Dataset JSONL export includes the timeline as machine predictions, never as a
human-approved ASR target. Display anonymous activity in latest transcription and
History; render at most 200 rows while exporting the complete timeline. Word-level
speaker assignment awaits reliable ASR word timestamps. Speaker numbers do not
change personalization or identify the owner across recordings.

## Native packaging and evidence

Weights: NVIDIA GA revision `f667ed73aee57d40cc39428eb768b4fd87a0a29e`, 107 MB
Q8_0 GGUF. Native source: `97a15afa5caa9bce5baaa86c1184103877af4101`; ggml
`c03b4e2bcece5134827881af90242086daf75be5` with the pinned upstream patch series.
The exact [standalone C header](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/97a15afa5caa9bce5baaa86c1184103877af4101/include/nemo_speech/diar.h)
defines ownership and segmentation. Existing ASR C signatures are unchanged,
but that does not imply compatible C++/ggml binary identities.

Only the August v0.1.0 runtime archive is published at review time. Therefore the
experimental native bundle is locally source-built for Apple silicon macOS,
separate from the existing ASR runtime. The packaging script renames all seven
NeMo/ggml dylibs, rewrites their install IDs/dependencies and removes build RPATHs,
then signs them ad hoc. It retains upstream notices and records each generated
hash in a bounded local manifest. This is local build provenance, not a publisher
signature. Full file hashes, the exact component allowlist and source revision
are checked before each use; descendant symlinks are rejected. Load only the
isolated C ABI handle with local symbol scope and keep it alive through teardown.
See [native setup](../development.md#experimental-speaker-runtime).

The experiment uses CPU/Accelerate or Metal with CPU fallback operations.
Automatic can retry CPU after verified native initialization fails, retaining
the failed Metal reason in the timeline and execution evidence; explicit
Metal errors preserve plain ASR. Per-operator placement is unprofiled. Timings
include verification and loading and are recorded separately under execution
evidence. No language, accuracy, thermal or live latency promotion follows from
the initial smoke results.

## Research boundary and validation

1. Frozen-weight/public-API work: local artifacts, safe bindings, bounded worker
   requests, fresh recording state, speaker results, validation and measurement.
2. Runtime-internal decoder/kernel changes: none. Renamed Mach-O linkage is a
   packaging isolation measure; it does not implement a new inference method.
3. Training/checkpoint changes: none. Speaker-conditioned multi-stream ASR and
   learned owner extraction remain separate proposals.

[Streaming Sortformer](https://arxiv.org/abs/2507.18446v1) motivates the cache
method. NVIDIA's [release article](https://huggingface.co/blog/nvidia/nemotron-diarization)
and pinned implementation establish capabilities. The dated local report records
observations. Better word accuracy, owner isolation and robust new-domain speaker
attribution remain hypotheses, not implemented benefits.

Validation must cover malformed/tail intervals, overlap, legacy session/config
migration, export label isolation, failure/cancellation recovery, actual public
audio through the native worker and full ASR/persistence path, and both native
library load orders with unchanged before/after ASR output. Broader speaker and
language qualification, standard DER and sustained app measurements remain
necessary before changing the experimental default or publishing a runtime.
