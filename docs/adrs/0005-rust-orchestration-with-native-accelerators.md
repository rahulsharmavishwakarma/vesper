# ADR-0005: Rust orchestration with native accelerators

- Status: Accepted
- Date: 2026-09-14

## Context

The application needs memory-safe lifecycle and storage code while also using mature CPU vectorization and GPU backends. CUDA kernel ecosystems and several selected inference engines are C/C++-native. Reimplementing complete tensor runtimes in the application would be costly and difficult to validate.

## Decision

Keep application orchestration, configuration, cancellation, storage, and adapter boundaries in Rust. Use pinned native engines behind those boundaries: NeMo-Speech.cpp for Nemotron, sherpa-onnx for Parakeet-compatible transducers, llama.cpp bindings for Qwen cleanup, and Rust/Candle for Kyutai.

Support acceleration through explicit build/runtime profiles: portable CPU, Apple Metal, NVIDIA CUDA, Vulkan where the packaged speech runtime supports it, and ROCm for compatible cleanup builds. The AMD profile may use Vulkan for Nemotron because NeMo-Speech.cpp does not publish a ROCm backend. MLX is not treated as a generic substitute for Metal/Candle or NeMo-Speech.cpp.

Use C++/CUDA for new NVIDIA-specific kernels only after profiling identifies a hot path that the selected upstream engine cannot solve. Keep a stable Rust-facing interface and a CPU reference/fallback where practical.

## Consequences

- Rust remains sufficient for product code; C++ and CUDA are scoped to native runtime integration and measured kernels.
- Accelerator builds need matching toolchains, packaged binaries, CI coverage, and device-level tests.
- Backend availability is both model- and platform-specific; the UI must reject invalid combinations instead of silently falling back.
- Kernel work must include correctness comparisons, latency/throughput measurements, memory bounds, cancellation behavior, and a documented fallback.
