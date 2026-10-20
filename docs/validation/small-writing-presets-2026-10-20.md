# Small writing-model candidates — 20 October 2026

Question ([#236](https://github.com/rahulsharmavishwakarma/vesper/issues/236)):
can four smaller GGUF writers be offered as data-only `llama_cpp` presets
through the existing custom-writer path?

**Result: no. None was added to `models/presets.json`.**
- All four load and run in the bundled llama.cpp (`d8123504`).
- Through the production prompt and fidelity checks, each was rejected far more
  often than the built-in Qwen3.5-0.8B. A rejection keeps the raw transcript.
- Qwen3-0.6B never produced usable output, because its embedded template
  reasons first and every pass hit the output limit.
- These are local measurements of this application path, not general model
  quality results.

## Setup

- Application source: `origin/main` at `ac71849`, plus the test-only
  `VESPER_CUSTOM_WRITER_PROFILE` option of `dump_cleanup_outputs`. With it,
  the custom writer runs the same prompt, generation limits and fidelity checks
  as the app. The prefix cache does not apply to custom writers.
- Hardware: Apple M5 Pro, macOS 27.0.1, battery power. The release build was run
  with `--features mlx` on the CPU and Automatic (Metal) backends.
- Inputs: the 52 Cohere transcripts and verbatim references from the
  [polishing study](polishing-wer-2026-10-16.md): 24 clean FLEURS en-US clips,
  the same clips at 0 dB noise, and 4 synthetic controls. Raw WER is
  160/1268 = 12.62%.
- Profiles were generated with `scripts/huggingface-profile.mjs` from these
  pinned revisions. Sizes and SHA-256 matched the issue and the downloaded files.

| Candidate | Repository @ revision | File | Bytes | License |
| --- | --- | --- | ---: | --- |
| LFM2.5-230M Q8_0 | `LiquidAI/LFM2.5-230M-GGUF@03502067` | `LFM2.5-230M-Q8_0.gguf` | 246,598,496 | LFM Open License v1.0 |
| LFM2.5-350M Q8_0 | `LiquidAI/LFM2.5-350M-GGUF@657e078c` | `LFM2.5-350M-Q8_0.gguf` | 379,217,632 | LFM Open License v1.0 |
| Granite 4.0 350M Q8_0 | `ibm-granite/granite-4.0-350m-GGUF@b8208a86` | `granite-4.0-350m-Q8_0.gguf` | 378,138,016 | Apache-2.0 |
| Qwen3-0.6B Q8_0 | `Qwen/Qwen3-0.6B-GGUF@23749fef` | `Qwen3-0.6B-Q8_0.gguf` | 639,446,688 | Apache-2.0 |

## Results (Automatic backend)

"Rejected" counts passes the fidelity checks or the output limit refused. WER is
scored on the retained text: the polished output, or raw on rejection. CPU
results matched within one or two rejections.

| Writer | Minimal rejected | Minimal WER | Balanced rejected | Balanced WER | Median rewrite |
| --- | ---: | ---: | ---: | ---: | ---: |
| Qwen3.5-0.8B Q4_0 (built-in) | 1/52 | 12.78% | 5/52 | 13.09% | 168–191 ms |
| LFM2.5-230M | 8/52 | 12.62% | 31/52 | 12.78% | 78–97 ms |
| Granite 4.0 350M | 34/52 | 12.78% | 34/52 | 12.78% | 110–154 ms |
| LFM2.5-350M | 40/52 | 12.62% | 41/52 | 13.33% | 58–60 ms |
| Qwen3-0.6B | 52/52 | 12.62% | 52/52 | 12.62% | 380 ms |

Rejection reasons by candidate:
- **Granite and LFM2.5-230M:** mostly negation changes.
- **LFM2.5-350M:** mostly formatting checks under Minimal, and length growth
  under Balanced.
- **All three:** some changed numbers.
- **Qwen3-0.6B:** the output limit on every pass.

No candidate lowered WER, consistent with the 16 October finding that polishing
is not an accuracy step.

## Licensing note

LFM Open License v1.0 permits use and redistribution with the license and
notices preserved. It licenses commercial use only for entities below
US$10 million annual revenue. This matches the terms of the optional LFM2.5
Audio adapter.

## Limits and follow-up

- English read speech only, on one machine. Other languages declared on the
  cards were not tested.
- Qwen3-0.6B would need its non-thinking mode. Granting it by checking the model
  name in the generic path is ruled out. A profile-level template option would
  be a separate, approved change.
- Re-run when a candidate offers a faithful-editing instruction model, or if
  the fidelity checks or the generic prompt change.
