# Vesper documentation

Start with the guide that matches what you want to do. The
[project README](../README.md) gives the overview and download links.

## Using Vesper

- [User guide](user-guide.md): installation, first launch, dictation, history,
  models, updates, privacy and troubleshooting
- [Language support and recommendations](language-support-and-recommendations.md):
  which speech model to use for each language
- [Models and presets](model-presets.md): every speech and writing model, and what
  it does while you speak
- [Accelerators](accelerators.md): which models use the Apple GPU or Neural Engine
- [Personal vocabulary and learning](personalization/README.md): what Vesper
  remembers and how to control it
- [Licenses and model terms](licenses-and-model-terms.md)

## How it works

- [Architecture](architecture.md): components, data flow and module boundaries
- [Inference architecture](inference-architecture.md): the inference worker,
  speech adapters, runtimes, writer and failure policy
- [Application internals](internals.md): storage, settings, model files and
  failure handling
- [Live transcription](live-transcription.md) and
  [completed-audio adapters](offline-asr.md)
- [Local engine](application-status.md) and [LFM2.5 Audio](lfm-audio.md)
- [Continuous improvement](self-improvement.md)
- [Architecture Decision Records](adrs/README.md)

## Contributing

- [Contributing guide](../CONTRIBUTING.md)
- [Development guide](development.md): setup, build, test and tooling policy
- [Project workflow](project-workflow.md): issues, pull requests, labels and the
  project board
- [CI and releases](ci-and-releases.md): checks, security audits, release and
  update pipeline
- [Manual validation](validation.md) and the
  [device checklist](validation/native-device-checklist.md)
- [UI design](ui-design.md) and [motion design](motion-design.md)

## Models, benchmarks and research

- [Hugging Face model profiles](huggingface-models.md)
- [Inference performance](inference-performance.md)
- [Benchmark guide](../benchmarks/asr/README.md),
  [benchmark dataset](benchmark-dataset.md) and
  [getting its audio](benchmark-dataset-referencing.md)
- [Cleanup adapter qualification](cleanup-adapter-qualification.md)
- Research:
  [streaming inference papers](research-streaming-inference-papers.md),
  [audio quality](research-audio-quality.md),
  [automatic personalization](research-automatic-personalization.md),
  [quantization](research-quantization.md) and the
  [September 2026 RSI review](research-rsi-2026-09.md)
- [Validation reports](validation/): dated measurements behind shipped models and
  defaults
