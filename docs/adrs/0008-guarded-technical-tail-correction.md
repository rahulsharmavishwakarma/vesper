# ADR-0008: Guard technical tail correction before full cleanup

- Status: Accepted
- Date: 2026-09-15

## Context

The final cleanup already receives the complete raw ASR transcript, but the small Qwen model can overlook an obvious technical filename near the end of a longer dictation. Repeating recent words inside the same full prompt corrected a short synthetic case but did not correct the longer installed-model case. Running generative cleanup over live partials would add flicker, latency, and state-merging risks.

## Decision

Keep live partials and durable raw ASR unchanged. Formatting-only cleanup remains one full pass and cannot change normalized words.

For Context correction with the built-in Qwen model, transcripts longer than 24 words whose tail contains file/repository editing context receive one preliminary technical-only request containing the final 24 words. Reuse resident weights but create a fresh context. Extract only one nearby, technically notated candidate and apply it to the untouched raw tail; ignore all other generated edits. Preserve numbers and negation and reject ambiguous or multiple candidates. A built-in `README.txt` fallback may supply that candidate only under the same context and two-character-distance guards. Join an accepted tail to the untouched raw prefix, then run normal full-transcript cleanup in another fresh context. Reapply the exact accepted token if broad cleanup undoes it, and validate the final output against both the prepared text and original raw ASR.

Custom cleanup models retain the single full-transcript path until separately qualified. Do not call the cleanup model on live partial captions.

## Consequences

- A final technical term can receive focused context without truncating the complete cleanup input.
- Context correction can add one extra generation and therefore additional Stop latency; formatting-only latency is unchanged.
- No transcript or KV state crosses contexts, and a failed tail pass cannot erase or replace raw ASR.
- The narrow lexical guard reduces broad rewrites but cannot establish semantic correctness. Users must still review context-corrected output.
- Project-specific names still need a future explicit vocabulary/ASR-biasing design; this decision does not claim arbitrary error recovery.
