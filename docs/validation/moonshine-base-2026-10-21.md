# Moonshine Base (English) adapter — 21 October 2026

Question ([#235](https://github.com/rahulsharmavishwakarma/vesper/issues/235)):
can Moonshine Base be offered as a small, fast English CPU model through a
dedicated adapter built around its architecture and its reference runtime?

**Result: added as an optional English-only CPU model.** It is never a default.

Held-out WER is **10.11%** on 94 FLEURS en-US clips, at a median of 86 ms per
clip on CPU. The download is about 141 MB.

| Model            | Held-out WER | Download |
| ---------------- | ------------ | -------- |
| Moonshine Base   | 10.11%       | ~141 MB  |
| Parakeet v3, CPU | 5.95%        | ~730 MB  |
| Cohere           | 5.85%        | ~2.9 GB  |

Moonshine's own published Open ASR average is about 10%, so the adapter reaches
the model's expected quality. These are local measurements on read English
speech.

## Sources that shaped the adapter

- **Moonshine v1 paper** ([arXiv 2410.15608](https://arxiv.org/abs/2410.15608)).
  - Raw 16 kHz audio passes through convolution strides 64·3·2, a 384-sample
    hop at ~41.7 Hz, into a RoPE encoder-decoder.
  - Training segments run from 4 to 30 s.
  - Errors rise on longer clips through hallucination.
  - Decoding uses a per-second token limit to stop repetition.
- **Reference runtime**
  ([moonshine-ai/moonshine@234f60f](https://github.com/moonshine-ai/moonshine/tree/234f60faa0eb388b01cdf7e60aca232af37aefda)):
  - Silero VAD segments of up to 15 s (`vad_max_segment_duration`), with no
    overlap.
  - Greedy decoding with `max_len = ceil(seconds × 6.5)` (`max_tokens_per_second`).
- **sherpa-onnx:**
  - The v2 export is `encoder_model.ort` plus `decoder_model_merged.ort`.
  - The pinned v1.13.8 fed the decoder's `encoder_attention_mask` at frame
    length, which breaks every decode above 384 encoder frames (~9.2 s). This
    is [issue #3975](https://github.com/k2-fsa/sherpa-onnx/issues/3975), fixed
    upstream in [3a82dc5](https://github.com/k2-fsa/sherpa-onnx/commit/3a82dc5cae).

## Adapter design

1. **Native patch** (`patches/sherpa-onnx/decode-report.patch`):
   - The Moonshine v2 recognizer reports `vesper.decode_report.v1`:
     - `eos`;
     - `duration_token_limit`;
     - `silence_detected` / `no_input`;
     - `error` for a non-finite or invalid-shape logit, an unknown token or any
       exception.
   - The #3976 mask fix is backported.
   - The token budget is the reference runtime's 6.5 tokens/s, not sherpa's 15.
2. **Parts of at most 15 s** (`MOONSHINE_MAX_SEGMENT_SECONDS`), matching the
   reference runtime, so most dictation reaches the model as one part. Live
   committed segments stay at most 8 s.
3. **Empty-result retry.**
   - The trigger is a non-silent part of at least 2 s that decodes to nothing
     at EOS.
   - The part is decoded once more as two halves, split at the quietest 100 ms
     within 1.5 s of the middle.
   - On the 9–15 s held-out clips, 3 of 44 came back empty at EOS before this
     rule.
4. **Shared contract.** Completed-audio finals use the shared completion gate,
   resident weights and buffered live drafts of the other CPU adapters. Hitting
   the token budget is incomplete and never pasted.

Files are unchanged copies of the official sherpa package
`sherpa-onnx-moonshine-base-en-quantized-2026-02-27`. They are hosted at
[`rahulsharmavishwakarma/vesper-moonshine-base-en-sherpa-onnx@577269bb`](https://huggingface.co/rahulsharmavishwakarma/vesper-moonshine-base-en-sherpa-onnx)
(MIT, with provenance).

## Results

Apple M5 Pro, macOS 27.0.1, battery power. The production installer and
`vesper-eval` built-in path were used, on CPU in English.

| 94 held-out FLEURS clips | Generic 9 s parts | 15 s parts + mask fix | + empty-result retry |
| ------------------------ | ----------------: | --------------------: | -------------------: |
| ≤ 9 s (46)               |             6.69% |                 6.69% |                6.69% |
| 9–15 s (44)              |            16.68% |                15.80% |               11.32% |
| > 15 s (4)               |            18.98% |                21.90% |               21.90% |
| All                      |            12.66% |                12.41% |           **10.11%** |

On the held-out regression lanes:

- Non-speech controls: 0/8 false transcripts.
- Synthetic controls: 15–33% WER.
- −30 dB quiet lane: 62%. One clip hit the token budget and was rejected
  (fail-closed), not pasted.

## Limitations

- English only. The non-English Moonshine models use the Moonshine Community
  License (non-commercial, or commercial use below US$1M revenue) and are not
  offered.
- Accuracy on continuous speech longer than about 15 s remains weaker.
- Parts are cut by energy, not by the reference runtime's VAD. Reusing frontend
  speech intervals for cuts is a possible follow-up.
- Streaming Moonshine (`moonshine-streaming-*`) needs the reference incremental
  encoder and is not supported by sherpa's offline recognizer.
