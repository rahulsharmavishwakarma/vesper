# ADR-0011: Process long Qwen3-ASR and Cohere audio in bounded segments

- Status: Accepted
- Cohere ownership clause superseded by [ADR-0017](0017-resident-cohere-recognizer.md); Qwen-ASR ownership and the segmentation/completion rules remain in effect.
- Date: 2026-09-15
- Tracking: [GitHub issue #32](https://github.com/rahulsharmavishwakarma/transcribe/issues/32)
- Supersedes: ADR-0007 and ADR-0009; preserves their completion and resource protections.

## Context and approval

The application has no hosted-service time quota. ADR-0009 removed the general
duration setting, but retained ADR-0007's ten-second Qwen/Cohere qualification
guard. Removing only that number would send arbitrarily long input into a decoder
with bounded context and output. The user explicitly approved issue #32's internal
segmentation proposal and removal of the remaining restriction.

## Decision

Recording and file admission use zero for no duration cutoff for every current
speech model. Retain the old `maxRecordingSeconds` setting for compatibility but
ignore its value. Show no model-specific time restriction in the UI. Manual Stop,
elapsed time, cancellation, and the existing capture/import/decoded-buffer memory
protections remain in effect. This does not promise infinite memory or successful
recognition of every input.

For Qwen3-ASR and Cohere, retain one recognizer within each completed-audio job.
Decode contiguous, non-overlapping ranges of at most 160,000 samples at 16 kHz.
Balance remaining input before choosing a boundary, avoiding microscopic tails
such as a one-sample second part. Search up to one second before that target for
the lowest-energy 100 ms window, at 10 ms intervals; prefer the later point on
ties. Energy only chooses a cut: no quiet samples are removed and no endpoint or
speech-detection claim is made. Pause selection may increase the number of parts.

Create a fresh native stream for each range, set language options again, and
accept its PCM once. Retain the schema-v1 native completion contract: accept text
only after confirmed EOS without discarded input context; accept a native
silence/no-input report only with empty text. Reject missing, invalid, unsupported
or failed reports. Exact-zero ranges produce no words. Keep native sampling,
weights and context allowance unchanged. Qwen uses 512 for both `max_total_len`
and `max_new_tokens`: its existing total context bounds generation without an
independent 128-token output ceiling. This public setting does not enlarge the
preallocated KV cache, but can increase native work and cancellation delay.
Cohere retains its upstream output limits.

Append verified text in order with a space between nonempty parts. Never infer
word overlap or remove repeated phrases. Any failed or cancelled part returns an
error for the whole ASR job, without publishing its successful prefix. Check
cancellation before and after native work, between ranges and before returning.
Only a completed raw result enters the existing archive/publication/writing flow.
The original audio is archived by the existing capture/file path, independently
of ASR success. A failed writing pass retains the complete raw result.

Progress reports completed sample coverage through the existing bounded worker
and runtime status. Scope it to the operation, ignore it after cancellation or a
stage change, and show it in Dictate and Quick Capture. It is processing progress,
not a live or partial transcript. Record the segment policy and native identities
in inference provenance; persistent configuration and session schemas stay intact.

## Evidence and boundaries

This is frozen-weight application orchestration through existing public APIs.
There are no runtime-internal decoder/kernel changes, training/checkpoint changes,
new dependencies, live streaming, or cross-job Qwen/Cohere model residency.

The pinned runtime is Sherpa 1.13.8, source
`11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`, with the existing reporting patch.
Qwen artifacts use revision `68818b2313fe77bd06f6a7c5068ff3ef59d02b8a`; Cohere uses
`156a470cf08eefe706a0004f3c52d9ee567ca7a0`.
The [stream contract](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.8/sherpa-onnx/csrc/offline-stream.h)
allows one waveform acceptance per stream.
[Qwen's implementation](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.8/sherpa-onnx/csrc/offline-recognizer-qwen3-asr-impl.cc)
initializes generation state for each decode. Its [model implementation](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/offline-qwen3-asr-model.cc) allocates KV capacity independently of the output-token setting, and
[Cohere's decoder](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.8/sherpa-onnx/csrc/offline-cohere-transcribe-greedy-search-decoder.cc)
starts each decode from its initial cache and offset. Native context/output guards
remain necessary even for bounded parts.

No paper accuracy, latency or memory result is claimed for this algorithm. Public
API support is source evidence; completed local tests and benchmark scores are
reported in [validation](../validation.md#long-qwen3-asr-and-cohere-audio).
Performance outside the tested language/artifact/hardware/corpus envelope remains
unverified. The research map's training, endpointing and decoder techniques are
not implied by this application-level partitioning.

## Consequences and validation

Long inputs no longer fail solely because they exceed ten seconds. Native work
and its temporary state remain bounded per part, while total time grows with
input length. Every range has fresh frontend/decoder context, so words around a
cut can be misrecognized. Completion reporting proves native completion, not
word accuracy. Automatic language selection is independent per part.

Cancellation cannot interrupt a running native call; its wall-clock delay is not
the segment's audio duration. Full-audio storage and the optional writing context
remain bounded by the existing resource rules. No disk-backed capture or new
automatic endpointing is introduced.

Validate exact sample coverage and tails, quiet boundaries, genuine repetitions,
failure after success, cancellation at every boundary, operation-scoped progress,
and fresh-state A/B/A decoding. Run the real production and archive paths on
public natural and repeated English/French and Qwen Hindi audio above ten seconds.
Keep raw error scores and failed samples, measure latency and process peak RSS,
and check the browser controls and native build. Successful EOS alone cannot
satisfy the quality review.
