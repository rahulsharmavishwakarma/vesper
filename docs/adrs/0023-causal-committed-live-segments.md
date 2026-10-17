# ADR-0023: Commit causal segments in buffered live previews

Status: Accepted (phase A implemented)
Date: 16 October 2026
Tracking: [issue #139](https://github.com/rahulsharmavishwakarma/vesper/issues/139) (owner approval recorded 16 October 2026)

## Context

Only Nemotron and English/Metal Qwen have native live sessions. Every other
speech adapter previewed by re-decoding hard-cut eight-second windows: the
first draft of each window waited two seconds, words spanning a window edge were
split, drafts were replaced wholesale with no stable part, a lagging decoder left
permanent gaps, and the growing window was decoded repeatedly. The completed-audio
final already decodes independent segments of at most ten seconds, but its
balanced boundaries depend on the total recording length, so live decoding cannot
reproduce them exactly.

## Decision

Phase A changes only the provisional live display; the completed-audio final is
unchanged and remains authoritative.

- Received 16 kHz preview audio accumulates in one open buffer. When it reaches
  eight seconds, it is closed at the centre of the quietest 100 ms window between
  four seconds and the received end, preferring the later point on ties. The
  boundary depends only on audio already received. Energy only selects the
  boundary; no audio is removed and Silero VAD is not used, so ADR-0019's
  advisory VAD contract is unchanged.
- Closed segments wait in a FIFO of at most two. Each is decoded exactly once and
  its text is appended to `committedText`, which only grows. When the decoder
  falls behind, the oldest waiting segment is dropped and counted; the recorder
  still retains its audio for the final.
- The open tail of at least two seconds is re-decoded on the existing adaptive
  interval as `provisionalText`. Committing a segment clears the stale tail until
  the next tail pass.
- Preview memory is bounded to three eight-second buffers (about 1.5 MiB of PCM).

## Consequences

- Committed draft text is stable and no longer cut at fixed eight-second edges.
- Closed audio is decoded once, but repeated tail passes still dominate preview
  compute, so phase A does not reduce total preview decoding (see below).
- Segment-local decoding loses cross-segment context; committed draft wording can
  differ from the final, which still replaces the draft.
- Stop still decodes the whole recording.

## Measurement — 16 October 2026

`installed_live_long_recording_draft_coverage` streamed the public 23.04 s
`repeated-boundaries.wav` diagnostic (FLEURS clip 13297690853135603331 repeated
three times) in real time on an Apple M5 Pro, once with the previous window policy
and once with phase A. Single runs; few-percent differences are within noise.

| Path              | Final words in last draft (old → new) | Preview decode (old → new) | Stop to final (old → new) |
| ----------------- | ------------------------------------- | -------------------------: | ------------------------: |
| Cohere CPU        | 41/42 → 42/42                         |              6.9 s → 7.3 s |           1.15 s → 1.24 s |
| Qwen3-ASR CPU     | 42/42 → 42/42                         |              7.4 s → 7.5 s |           1.17 s → 1.35 s |
| Parakeet v3 CPU   | 42/42 → 42/42                         |              3.3 s → 3.4 s |           0.40 s → 0.45 s |
| Parakeet v3 Metal | 41/42 → 41/42                         |            1.75 s → 1.81 s |           0.12 s → 0.13 s |

Both policies produced drafts on every path with no skipped windows, dropped
segments or preview failures; this fast machine never fell behind, so the lagging
case is not exercised. Phase A's measured effect is stable append-only committed
text without fixed eight-second cuts, not lower compute or Stop latency. Those
require phase B and shorter pause-closed segments.

## Deferred, requiring measurement before adoption

- Phase B: reuse committed segment text at Stop and decode only the open tail.
  This changes final segmentation and must pass WER/CER parity against the
  current completed-audio final on the held-out manifests before it can become
  the default.
- Closing segments early at Silero-detected pauses when the optional frontend is
  enabled.
- LocalAgreement-style stabilisation of the provisional tail.
