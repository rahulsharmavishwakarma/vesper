# ADR-0003: Require runtime-specific model adapters

- Status: Accepted
- Date: 2026-09-14

## Context

Speech repositories differ in preprocessing, tokenizer/vocabulary, decoding, streaming state, language controls, licenses, artifact layouts, and native runtime support. Listing a model name or downloading its weights does not make it executable.

## Decision

Call a model "supported" only when the repository contains:

1. a typed catalog entry and configuration compatibility rules;
2. an architecture-specific inference adapter;
3. a pinned artifact/runtime manifest with revision, size, SHA-256, and license information;
4. installation/readiness reporting in the desktop UI;
5. pre-inference integrity checks and adapter tests.

Nemotron is the default multilingual ASR, Kyutai STT is a selectable English/French adapter, and Parakeet is the offline fallback. Qwen3.5-0.8B is the cleanup adapter. Generic Hugging Face profiles are accepted only for already implemented sherpa-onnx transducer or llama.cpp contracts; importing data never executes repository code.

## Consequences

- Qwen3-ASR, Voxtral, Liquid, VibeVoice, Cohere Transcribe, Whisper, and Moshi remain research/integration candidates until each satisfies the support contract.
- The download UI can accurately separate artifact presence, native runtime readiness, platform support, and selection.
- Adding a genuinely new architecture takes more work than adding a label, but avoids broken or misleading downloads.
- Model installation does not silently change the user's selected pipeline.
