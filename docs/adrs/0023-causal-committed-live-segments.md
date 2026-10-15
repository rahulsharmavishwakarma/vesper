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
- Closed audio is decoded once instead of on every pass of a growing window.
- Segment-local decoding loses cross-segment context; committed draft wording can
  differ from the final, which still replaces the draft.
- Stop still decodes the whole recording.

## Deferred, requiring measurement before adoption

- Phase B: reuse committed segment text at Stop and decode only the open tail.
  This changes final segmentation and must pass WER/CER parity against the
  current completed-audio final on the held-out manifests before it can become
  the default.
- Closing segments early at Silero-detected pauses when the optional frontend is
  enabled.
- LocalAgreement-style stabilisation of the provisional tail.
