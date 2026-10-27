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
  boundary; no audio is removed. Since 19 October 2026, when the live frontend
  runs, a Silero end-of-speech point closes the buffer earlier once it holds two
  seconds (100 ms into the silence). VAD only chooses boundaries for drafts; it
  deletes no audio and does not affect the final, so ADR-0019's advisory VAD
  contract is unchanged.
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

## Phase B parity gate (16 October 2026): not met

Phase B would reuse committed segment text at Stop and decode only what remains.
The ignored `installed_segmented_final_parity` test measures that offline. It
concatenates public FLEURS en-US clips into three 63–118 s recordings per lane
(608 reference words per lane, clean and 0 dB environmental noise), decodes them
with resident CPU recognizers on an Apple M5 Pro, and compares candidate causal
boundary policies with today's completed-audio final. Today's final already
splits audio into balanced parts of at most 10 s, but it places boundaries
using the total length, which is unknown during recording.

| WER (clean / 0 dB) |   Current final | Causal 4–8 s (phase A) | Fixed 10 s, last 20 s re-split | Pause-first 4–10 s |
| ------------------ | --------------: | ---------------------: | -----------------------------: | -----------------: |
| Cohere CPU         |  9.05% / 14.31% |        12.83% / 16.12% |                11.02% / 14.14% |     8.06% / 15.62% |
| Qwen3-ASR CPU      | 10.03% / 15.62% |        11.18% / 17.76% |                11.84% / 19.24% |    11.18% / 18.09% |

No causal policy matches the current final on both lanes for both models. Every
policy worsens Qwen3-ASR by 1.2–3.6 points. Fixing boundaries live would cut
Stop-to-final decoding on these recordings from a median of about 5.6 s to
about 1.1 s, but it does not pass the accuracy gate. The completed-audio final
therefore stays authoritative. Six recordings are too few to rank policies
reliably; a larger natural-dictation corpus is needed before trying again.
Parakeet's whole-recording final lost words on these recordings, a separate bug
fixed in [#153](https://github.com/rahulsharmavishwakarma/vesper/issues/153), so
its rows are not used as a parity reference.

## Deferred, requiring measurement before adoption

- Phase B: retry only with a policy that passes the gate above on a larger
  natural-dictation corpus. Candidates are overlapping parts with a word-level
  merge, or per-model enablement where parity holds.
- LocalAgreement-style stabilisation of the provisional tail.
