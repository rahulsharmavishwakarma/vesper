# Rust dependency upgrade — 19 October 2026

This record covers the 19 updates from the closed Dependabot group #190 and the
code they required ([#196](https://github.com/rahulsharmavishwakarma/vesper/issues/196)).
Compiling #190 as-is broke six crates. The comment that closed #190 named three.

| Upgrade | Code change |
| --- | --- |
| symphonia 0.5 → 0.6 | `decode.rs`: `probe()` returns the reader; audio tracks via `TrackType::Audio`; `make_audio_decoder`; `next_packet()` returns `None` at end; gapless moved to `AudioDecoderOptions` (kept on) |
| rubato 0.16 → 5.0 | `pcm.rs`: `FftFixedIn(1024, 2)` becomes `Fft::new_custom(…, 1024, 2, 1, BlackmanHarris2, FixedSync::Input)` with `audioadapter` slices |
| cpal 0.16 → 0.18 | `capture.rs`: `Device::name` becomes `description().name()` (same CoreAudio name on macOS, so saved microphone selections still match); `SampleRate` is a `u32`; stream configs are `Copy` |
| sha1 0.10 → 0.11 | `hub_cache.rs`: the `digest` 0.11 trait in scope; explicit hex formatting |
| rusqlite 0.37 → 0.40 | `semantic.rs`: the `usize` parameter becomes `i64` |
| llama-cpp-2 0.1.156 → 0.1.159 (llama.cpp `e79e4bf` → `d8123504`) | Tokenization and token metadata moved to `LlamaVocab`. The `ModelText` adapter in `llama_runtime.rs` keeps the previous semantics (`add_special` only for `AddBos::Always`, `parse_special = true`, streaming UTF-8 pieces). `eval_chunks` needs `&mut`, so the LFM projector is held in a `RefCell` |

## Equivalence

All measurements were taken locally on an Apple M5 Pro and compared back to back
with `main` (`d96fa9f`) on the same public audio.

- **Decoding and resampling** of a 63 s FLEURS clip as 16/22.05/44.1/48 kHz WAV
  (mono and stereo), AAC and FLAC, plus the streaming resampler fed 20 ms chunks:
  - Every output has the same length.
  - 16 kHz input is bit-identical.
  - Elsewhere the largest sample difference is 7.5e-9, against an RMS of 1.1e-3
    (about −100 dB). The new FFT path rounds differently.
- **ASR** (Cohere, paced 48 kHz live capture): 3 of 4 finals are identical to the
  earlier `main` run. On a 108 s recording with 0 dB noise, 2 of 215 words
  changed: one comma, and one nonsense word that was already misrecognized.
  Live-frontend reuse still matches the completed path on 4 of 4. Live adapter
  parity and Parakeet long-audio tests pass.
- **Writer** (`dump_cleanup_outputs`, 416 rewrites): 410 are identical. The 6
  that changed are all Balanced-preset near-ties already known to be sensitive
  to rounding. Median rewrite time is 2–5% faster on Metal and 5–10% slower on
  CPU (for example 224 → 247 ms, CPU, Minimal, cached).

Not covered: a physical microphone with cpal 0.18 (owner check), and non-macOS
capture names (cpal 0.18 changed Linux/PipeWire naming).
