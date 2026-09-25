# LFM2.5 Audio speech recognition

Choose **Liquid LFM2.5 Audio 1.5B** in **Models → Speech model**, select **English**, and use **Save & download**. Automatic uses an available supported accelerator; CPU remains an explicit override. The CPU and GPU paths use the same pinned Q4 backbone and audio projector, totaling 915,262,016 bytes (about 915 MB); RAM use is higher. Cohere → Qwen remains the default. See [accelerator support and evidence](accelerators.md) for build/device restrictions and the locally tested Metal path.

LFM transcribes completed microphone recordings and imported audio files. The application processes long recordings in bounded 10-second segments with fresh state and keeps weights resident between requests. There is no whole-recording time quota. Live partial captions, automatic language detection, Hindi, French, Japanese and speech synthesis are not enabled for this checkpoint. Select Nemotron for the existing native streaming path.

Writing cleanup uses the selected writing model and the existing fidelity guard. It can preserve recognition mistakes in names or numbers; it cannot infer what was actually spoken. The raw transcript remains available if cleanup fails. The [CPU screening report](../benchmarks/asr/2026-09-15-lfm.md) includes observed errors, latency variance, memory, and unsuccessful longer-segment trials.

## Install from the command line

```sh
. ./scripts/dev-env.sh
cargo run --locked -p transcribe-models --example install -- \
  models/downloads --asr-model LiquidAI/LFM2.5-Audio-1.5B --backend cpu
```

This uses the same pinned download and SHA-256 verification flow as the application. Both GGUF files are required. Changing Qwen's Q4/Q8 preference does not change the LFM speech artifacts.

## Reproduce native validation

Use the public FLEURS data prepared by the [ASR benchmark instructions](../benchmarks/asr/README.md), then run the generic evaluator with `--model LiquidAI/LFM2.5-Audio-1.5B --language en`. Add `--cleanup` for the two-stage pipeline. Record the actual hardware/environment instead of reusing another machine's envelope.

The explicit installed-model contract test performs no recording or download. Set `TRANSCRIBE_SMOKE_MODELS` to the installed model directory, `TRANSCRIBE_SMOKE_AUDIO` to the public English baseline, and `TRANSCRIBE_LFM_AUDIO_B` to a different public English clip. Use absolute paths because Cargo tests run from the crate directory. Install the default Qwen Q4 writing model as well.

```sh
cargo test --locked -p transcribe-inference \
  installed_lfm_reset_cancel_limits_and_two_stage_pipeline \
  -- --ignored --nocapture --test-threads=1
```

This exercises 100 repeated isolated requests with process RSS checkpoints, cancellation, silence, output limits, unload/reload, rejected streaming, and actual LFM → Qwen execution. Set `TRANSCRIBE_LFM_TEST_BACKEND=metal` to repeat the same contract on an available Metal device; the default test lane is explicit CPU. Ordinary unit tests also cover missing artifacts and installation changes. The safe runtime cannot interrupt an audio-encoder call immediately; it discards the result when cancellation is observed afterward.

The [accepted architecture](adrs/0013-lfm-audio-native-adapter.md) records the paper/API boundary and exact runtime revisions. The [official checkpoint documentation](https://docs.liquid.ai/lfm/models/lfm25-audio-1.5b) describes a broader audio model; this adapter implements ASR only. Preserve the checkpoint's LFM Open License and third-party notices when distributing its weights.
