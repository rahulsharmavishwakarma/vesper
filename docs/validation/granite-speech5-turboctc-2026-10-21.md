# Granite Speech 5.0 TurboCTC adapter — 21 October 2026

Question ([#234](https://github.com/rahulsharmavishwakarma/vesper/issues/234)):
can IBM's Granite Speech 5.0 TurboCTC run in Vesper through an adapter built
around its own architecture? If so, how does it compare with Parakeet v3?

**Result: added as an optional Apple-GPU-only model.** It is never a default.

Through the application path it matches or beats Parakeet v3 on every lane
measured. It runs at about half of Parakeet's per-clip latency and uses about a
third of its GPU memory. IBM publishes no CPU export, so the model is offered
only on Apple Silicon builds that include the MLX bridge.

| Lane (Metal, app path) | Parakeet v3 | Granite | Granite − v3 (95% CI) |
| --- | --- | --- | --- |
| FLEURS en-US held-out, 94 clips | 5.25% | 5.30% | [−0.77, +0.91] |
| Natural clean, 24 | 6.25% | 5.92% | [−2.15, +1.47] |
| Natural +10 dB SNR, 24 | 6.74% | 6.25% | [−2.00, +0.88] |
| Natural 0 dB SNR, 24 | 9.21% | 8.06% | [−3.00, +0.62] |
| Quiet (−30 dB), 24 | 6.91% | 5.59% | [−2.72, +0.00] |
| Joined FLEURS long-form, 6 × ~95 s | 10.50% | 6.57% | [−7.55, −0.26] |
| FLEURS de-DE, 50 clips | 6.62%¹ | 6.44% | [−1.82, +1.46] |

¹ Parakeet's German MLX path is not offered (Metal is English-only for the
shared models), so the German baseline is its 4-bit CPU export through the same
evaluator.

| Resource (FLEURS en-US, Metal) | Parakeet v3 MLX | Granite |
| --- | --- | --- |
| Median / p95 ASR time per clip | 48 / 71 ms | 22 / 32 ms |
| First request after load (kernel compilation) | 1.19 s | 1.09 s |
| Model load, cold file cache | 218 ms | 679 ms |
| Peak MLX allocator memory | 3.80 GB | 1.37 GB |
| Download | ~2.5 GB | ~948 MB |

Except for long-form, every confidence interval includes or touches zero: on
these sets Granite is as accurate as Parakeet v3, not proven better. Long-form
(six recordings) favours Granite. These are local measurements on read speech.
They come from one M5 Pro on battery and are not general claims.

## Model and evidence classes

- **Official model** ([card](https://huggingface.co/ibm-granite/granite-speech-5.0-470m-turboctc/blob/7171e21e921e39d62fdc42016e58d8fa10ad01c8/README.md),
  Apache-2.0).
  - Revision `7171e21e921e39d62fdc42016e58d8fa10ad01c8` is IBM's multilingual
    update of 21 October 2026. It covers English, French, German, Spanish,
    Portuguese, Italian and Japanese, with punctuation and capitalization. The
    earlier English-only release is `947f59a`.
  - About 470M parameters. Weights are BF16 (946,311,576 bytes).
  - Training: about 153,000 hours with CTC; greedy non-autoregressive decoding.
  - The card states an English Open ASR average of 5.51%. That is a publisher
    claim, not reproduced here.
- **Reference implementation:** transformers v5.16.1 `models/granite_speech5`,
  cross-checked against mlx-audio 0.6.1 `granite_speech5_ctc`.
- **Local measurements:** the tables above.
- **Unverified:** the five non-measured declared languages (French, Spanish,
  Portuguese, Italian, Japanese) rely on IBM's claim. Vesper offers them because
  `models/languages.json` records vendor-declared languages, but no local WER
  exists for them.

Files load unmodified from IBM's repository; nothing is converted or re-hosted.

| File | Bytes | SHA-256 |
| --- | --- | --- |
| `config.json` | 921 | `aabd64a2da736a1a1c21237c896b71628f6dd7364f1f5ec8a77f5e04b3ef1011` |
| `model.safetensors` | 946,311,576 | `5b569c3744c42a9172ac976881539926b66176d03fbaaa3c51b5d2960067f6fc` |
| `tokenizer.json` | 1,699,784 | `a1c3ca763ce0690faf0df3456365358547acfdf766122c9e4a69be8e8708042e` |
| `tokenizer_config.json` | 223 | `2f61ece3468bf7ca7a1b6dd3fd5b809814315bea9c298feea035fda4931b9f02` |

## Adapter design

The model runs in Vesper's own Swift module
(`native/mlx/Sources/VesperMLX/GraniteSpeech5CTC.swift`) inside the pinned MLX
bridge. The upstream MLX-Audio-Swift package is not forked or patched for it.

1. **Frontend**, matching IBM's feature extractor:
   - periodic Hann(400) centred in a 512-point FFT, 10 ms hop, reflect padding;
   - 80 HTK mel bins without area normalization, built in float64;
   - `log10(max(power, 1e-10))`, then a dynamic floor at the maximum − 8, then
     `/4 + 1`;
   - replicate-padded deltas, and two stacked frames (320 features).
2. **Encoder:** 16 macaron Conformer blocks.
   - Attention is local to 128-frame blocks, with Shaw relative-position logits
     (distances clipped to ±128). Padded keys of the last block are masked.
   - Layers 0 and 1 use a stride-2 depthwise convolution; the residual averages
     frame pairs.
   - BatchNorm is evaluated in float32.
   - After layer 8, self-conditioning adds `out_mid(softmax(out(h)))`.
3. **Decoding:** greedy CTC with blank 0, using the tied `encoder.out` head.
   - The detokenizer reads `tokenizer.json`'s BPE vocabulary: `▁` becomes a
     space, `<0xNN>` pieces are byte fallback, and the 255 special tokens are
     skipped. The result is trimmed.
   - Every part reports `audio_consumed`.
4. **Parts of at most 30 s.** Attention is block-local (1.28 s of audio per
   block), so a 30 s part loses no context the model would otherwise use.
5. **Input leveling** — a Vesper adaptation, described in the next section.
6. **Apple GPU only.**
   - Automatic and Metal both run the model, for all seven declared languages.
   - A GPU initialization failure reports that the model has no CPU version. It
     never switches to another model.
   - Builds without the MLX bridge reject the selection.

## Input leveling

IBM's log-mel features have no per-utterance normalization. Quiet input
therefore shifts every feature away from the training distribution. Parakeet,
by contrast, normalizes each feature per utterance.

Measured through the app before leveling, the −30 dB lane scored **20.89%**
against Parakeet's 6.91%.

The adapter now applies a boost-only gain before the frontend:

- The target is −23 dBFS RMS, the EBU R128 programme level.
- The gain is capped at +40 dB, and at a 0.9 sample peak so it never clips.
- Input that is already louder is unchanged. Exact digital silence is unchanged.

The target was chosen among two candidates (peak 0.5 and RMS −23 dBFS) on the
English and noise lanes. German was then used as the check. These figures come
from the reference implementation on identical float input:

| Lane | No leveling | Leveled |
| --- | --- | --- |
| FLEURS en-US held-out | 5.95% | 5.25% |
| FLEURS de-DE (check set) | 6.26% | 6.17% |
| Natural 0 dB | 8.88% | 8.06% |
| Quiet (−30 dB) | 20.89% | 5.59% |

## Parity with the reference

- **Synthetic speech** (macOS `say`, 11.2 s): the Swift model matches mlx-audio
  frame for frame (≥99% argmax agreement) and produces identical text.
- **Frontend:** against a float64 NumPy evaluation of the published extractor,
  the Swift frontend is within 5.2e-5 with no mean bias. mlx-audio's MLX
  frontend has a −4e-5 mean bias and errs by up to 1.6e-4.
- **Full English set without leveling:** the app path and the reference score
  the same 5.95% on identical float input. 89 of 94 transcripts are identical;
  the other five differ by a near-tie token.
- **Earlier Python-only figures** (5.55% English; 44.4% and 57.7% on the quiet
  lane) used `load_audio`, which quantizes to 16 bits. On −30 dB speech that
  quantization alone degrades both models. Those figures are superseded by the
  app-path results above.

`native/mlx/Tests/VesperMLXTests/GraniteSpeech5Tests.swift` checks:

- the frontend against the float64 values;
- the detokenizer's byte fallback and special-token handling;
- leveling;
- whole-model argmax and text parity, when `VESPER_GRANITE_PARITY` names a
  reference fixture.

## Environment

- Application branch `feature/granite-speech5-ctc` at the commits recorded in #234.
- MLX-Audio-Swift `d302a5c` with the Vesper bridge.
- MLX allocator defaults from the bridge.
- Apple M5 Pro, 18 logical CPUs, 64 GiB; macOS 27.0.1; on battery.

The evaluator is `vesper-eval run … --model ibm-granite/granite-speech-5.0-470m-turboctc --language en|de`
with a Metal environment file and no audio frontend.

The noise lanes are the 2026-09-16 audio-regression corpus. The joined
long-form set concatenates consecutive held-out FLEURS en-US clips, with 0.5 s
of silence between them, into six recordings of 90.7–101.4 s. That makes text
cross 30 s part boundaries.

The supplied 180 s continuous fixture has no reference transcript. It decoded
in seven parts without failure: 0.30 s for Granite and 0.78 s for Parakeet.

## Limitations and deferred work

- **No CPU or non-Apple GPU path.** IBM publishes no ONNX or GGUF export.
  transcribe.cpp and an ONNX conversion remain options if a portable path is
  needed.
- **No local measurement** for French, Spanish, Portuguese, Italian or Japanese.
- **No automatic language detection is declared.** The model ignores the
  selected language; IBM documents no language-identification guarantee.
- **No native streaming.** Live drafts use the shared buffered preview.
- **Single machine.** Older or lower-memory Apple Silicon remains to be measured
  (#137).
