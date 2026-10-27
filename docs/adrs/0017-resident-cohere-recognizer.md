# ADR-0017: Retain the selected Cohere recognizer in the inference worker

- Status: Accepted
- Date: 2026-09-17
- Tracking and specific user approval: [issue #2](https://github.com/rahulsharmavishwakarma/transcribe/issues/2#issuecomment-5718735696)
- Supersedes the Cohere per-job ownership clause of ADR-0011; retains its segmentation and completion requirements. Qwen-ASR and Parakeet ownership are unchanged.

## Context

The default Cohere→Qwen pipeline repeatedly verified 2.89 GB of speech artifacts
and created ONNX sessions for each recording. Separate local CPU profiling
measured about 1.04 s verification, 1.75 s construction and .29 s decoding for a
short recording after the hardware-SHA improvement. Removing small copies and
sampler calls did not materially reduce pipeline latency. The user accepted the
specific proposal to retain Cohere, with increased idle memory and paired
qualification. This approval does not extend to other per-job ASR adapters.

## Decision

The existing bounded `InferenceManager` owns at most one selected ASR handle
(Cohere, Nemotron or LFM) alongside its Qwen writer. Cohere loads lazily on the
first non-silent valid request. A cold exact-zero recording verifies artifacts
and completes without constructing native sessions or caching verification
success. Invalid-rate and pre-cancelled inputs cannot trigger a model load.

Full size/SHA verification precedes every Cohere native load. Its typed identity
includes the selected artifact descriptors, resolved root/file paths, observed
file length/modification time, Unix device/inode/change time, exact Sherpa
revision/build/patch/binding, INT8 precision, CPU provider, requested backend,
thread count, language and fixed constructor settings. Constructor schema 1
pins the remaining frontend defaults. Check identity before reuse, around load,
and after transcription. Missing or changed observed files invalidate the handle;
changed-during-request input discards the result. File stamps detect ordinary
changes, not adversarial edits between observations with preserved metadata.
There is no separate persistent or reusable verification-success cache.

Every contiguous segment gets a new `OfflineStream`, its language option, and
exactly one waveform acceptance. Retain ADR-0011's segmentation, zero safeguard,
completion report, no-overlap/no-deduplication rules and final cancellation
checks. Never reuse generated tokens, encoder features or generated KV state
between segments or recordings. Cohere remains completed-audio ASR.

Native calls remain synchronous. Cancellation discards text after return; native
completion health is inspected even when cancellation suppresses its result.
Unknown native/identity failures discard the Cohere handle on its owner thread.
Safe cancelled calls with successful native completion can retain weights.
Cancellation during verification retains its cancellation error classification.
Request metrics reset before early cancellation; load timings are lifetime
measurements identified by `residentLoadOrdinal`, not repeated per-request costs.

Existing settings/profile/install changes and explicit unload release resident
handles. Switching ASR drops the old selected handle before replacement;
shutdown joins the worker and releases its models. There is no prewarming,
background service, idle timer, additional model manager, cross-recording prefix
reuse, model training, or decoder/kernel change. Direct convenience
`transcribe`/`run` calls still have per-call ownership; applications obtain
residency through `InferenceManager`.

## Research and official implementation

This is category 1 frozen-weight/public-API orchestration under the
[paper-to-engineering map](../research-streaming-inference-papers.md).
[Stateful Conformer](https://arxiv.org/abs/2312.17279v3) grounds preserving native
state for compatible streaming models; it does not predict Cohere setup savings.
[FlashAttention](https://arxiv.org/abs/2205.14135) concerns attention memory traffic,
and [Prompt Cache](https://arxiv.org/abs/2311.04934) reusable prompt state. Neither
was implemented by retaining offline ONNX sessions. Category 2 native
kernel/decoder work and category 3 checkpoint/training work are outside this
decision. Expected performance savings were a hypothesis until local measurement.

The pinned Sherpa 1.13.8 revision is
`11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf`, local build
`48de319e110df610b6d53933f47f5669f3cf18b837c5974d2c40d21668586bf5`, patch
`cc53fa27fc98252436a12dafcc04c458be2ff98d6024d51a7a639b960df6bdfc`.
Cohere export revision: `156a470cf08eefe706a0004f3c52d9ee567ca7a0`.

The official [recognizer](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/offline-recognizer-cohere-transcribe-impl.h)
creates independent streams. The [model](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/offline-cohere-transcribe-model.cc)
retains encoder/decoder sessions and initial zero self-KV buffers, returned as
views. The [decoder](https://github.com/k2-fsa/sherpa-onnx/blob/11afbd009a7f8c08f4bcf2fc1b265d0df4670fbf/sherpa-onnx/csrc/offline-cohere-transcribe-greedy-search-decoder.cc)
starts offset zero and builds generated state locally. Initial zero KV is not
newly allocated per recording. Source capability does not substitute for
state-isolation or fault-recovery tests.

Byte-verified source SHA-256 values:

| Cohere source             | Upstream                                                           | Local patched build                                                |
| ------------------------- | ------------------------------------------------------------------ | ------------------------------------------------------------------ |
| Recognizer implementation | `06dbee9a2ac4bedf9426ac463b4dd03f7209ac86490c77e06d3a1b165a13c50a` | `54a0f39e564c6ca7a21a7e37cfda2719e8b4603588dab35aeed21f1629513038` |
| Model implementation      | `21e4325f177367e325bc37cdba71ae87f019b93a455acdd45b8c3940a0048ad0` | Same                                                               |
| Greedy decoder            | `981ecc91f445a4b2b0072219693c11bda1b6904f4eb6eb502661de4c5da481c0` | `4f02b80b05d13c3db895bb3377e50cdb55a6bb0475bd8108370ff1cc35212e58` |

## Consequences and qualification

Warm requests should avoid verification and session construction. Selected
Cohere consumes substantial idle RAM; model file size is not peak process memory
and this decision does not establish suitability for small edge devices.

Qualification uses the actual manager and frozen per-job/resident binaries on
the same pinned audio: English two-stage dictation, French, >10-second segments,
quiet/noise, silence and reset cases. Preserve every completion/fidelity failure.
The lifecycle test includes 100 mixed operations, A→B→A, writer calls between
ASR requests, cold/timed/between-segment/final cancellation, English→French→English,
model switch, unload/shutdown and copied-artifact corruption/removal/replacement.
Measure RSS after warmup and unloading; the predefined growth ceiling is 128 MiB
from cycle 10 to the maximum later checkpoint. Arbitrary ORT execution exceptions,
other devices, microphone/UI timing and sustained energy/thermal behavior need
separate qualification. See the [dated validation](../validation/cohere-residency-2026-09-17.md).
