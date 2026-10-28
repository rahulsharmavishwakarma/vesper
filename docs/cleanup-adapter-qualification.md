# Local cleanup adapter compatibility

The optional `training/cleanup` environment and native `adapter_probe` example
test a restricted Qwen cleanup adapter. **The cross-runtime compatibility gate
fails.** These tools do not train a candidate, register a model, change the active
writer or enable promotion. The [approved cleanup-first design](self-improvement.md#proposed-cleanup-first-implementation)
requires this proof before product integration. Application vocabulary and
project learning remain independent of it.

## Evidence and exact boundary

[LoRA](https://arxiv.org/abs/2106.09685) motivates low-rank adaptation; its paper
results do not establish this app's quality, resource requirements or latency.
The conversion uses official runtime APIs and an application-side tensor bridge.
No production decoder/kernel change is included. Fitting adapters would be a
training/checkpoint change, still gated here.

| Component               | Inspected/tested identity                                                                                                                                     |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Public base source      | `Qwen/Qwen3.5-0.8B@2fc06364715b967f1860aea9cf38778875588b17`                                                                                                  |
| MLX-LM                  | `0.31.3`, source `ed1fca4cef15a824c5f1702c80f70b4cffc8e4dd`                                                                                                   |
| MLX / MLX Metal         | `0.32.2`                                                                                                                                                      |
| Native binding          | `llama-cpp-2 0.1.156` (27 September experiment; the application now pins 0.1.159 / llama.cpp `d8123504`)                                                      |
| Native source/converter | `llama.cpp e79e4bf660e19f2ad851e06c6913f7a8c5852621`                                                                                                          |
| Converter environment   | Python 3.12, Torch 2.13.0, Transformers 5.17.0, NumPy 2.5.3, safetensors 0.8.0, protobuf 7.36.2, sentencepiece 0.2.2; full lock in `training/cleanup/uv.lock` |
| Native fixture          | Apple M5 Pro, macOS 27.0 (26A428), CPU or Metal, 4 CPU threads, context 512, batch/microbatch 128                                                             |

Only final decoder layer 23's `mlp.down_proj` is accepted. The checkpoint has
24 layers, input width 3584 and output width 1024. MLX stores `A[in,rank]` and
`B[rank,out]` with update `scale × xAB`. PEFT stores their transposes and uses
`alpha/rank`; the bridge therefore writes `alpha = rank × scale`, with native
adapter scale 1.0. It rejects missing pairs, extra tensors, wrong shapes,
nonfinite/non-floating values, unsupported configuration and oversized input.
It does not support DoRA, arbitrary modules, fused bases or other checkpoints.

The [MLX exporter](https://github.com/ml-explore/mlx-lm/blob/ed1fca4cef15a824c5f1702c80f70b4cffc8e4dd/mlx_lm/fuse.py)
does not directly export Qwen3.5 GGUF. The maintained bridge instead supplies
the [official PEFT converter](https://github.com/ggml-org/llama.cpp/blob/e79e4bf660e19f2ad851e06c6913f7a8c5852621/convert_lora_to_gguf.py).
Successful conversion is not qualification: every bridge manifest explicitly
records `qualified: false`, and is not an activation credential.

## Reproduction

Use an Apple-silicon Mac. This environment is separate from contributor tools and
is never started by dictation. Do not point `UV_PROJECT_ENVIRONMENT` at the root
contributor environment when running it.

```sh
uv sync --locked --project training/cleanup
make test-cleanup-bridge
mkdir -p .cache/training-proof
```

Download only the exact public source revision's config, tokenizer files and
single safetensors shard into `.cache/training-proof/source`. Check the source
weight and config hashes below; the comparator rejects mismatches. Obtain the
official llama.cpp source at the exact revision above into
`.cache/training-proof/llama` (for example a detached checkout). No remote custom
model code is loaded. The tools never read the application dataset.

Create deterministic synthetic adapters in new directories:

```sh
uv run --locked --project training/cleanup python - <<'PY'
import json
from pathlib import Path
import numpy as np
from safetensors.numpy import save_file
prefix = "language_model.model.layers.23.mlp.down_proj"
rng = np.random.default_rng(1701)
a = rng.normal(0, 0.01, (3584, 8)).astype(np.float32)
b = rng.normal(0, 0.01, (8, 1024)).astype(np.float32)
for name, right in [("zero", np.zeros_like(b)), ("nonzero", b)]:
    root = Path(".cache/training-proof") / f"mlx-{name}"
    root.mkdir(exist_ok=False)
    save_file({f"{prefix}.lora_a": a, f"{prefix}.lora_b": right}, root / "adapters.safetensors")
    (root / "adapter_config.json").write_text(json.dumps({
        "fine_tune_type": "lora", "num_layers": 1,
        "lora_parameters": {"rank": 8, "scale": 2.5, "dropout": 0.0, "keys": ["mlp.down_proj"]}
    }) + "\n")
PY
uv run --locked --project training/cleanup python training/cleanup/adapter_bridge.py .cache/training-proof/mlx-zero .cache/training-proof/peft-zero
uv run --locked --project training/cleanup python training/cleanup/adapter_bridge.py .cache/training-proof/mlx-nonzero .cache/training-proof/peft-nonzero
uv run --locked --project training/cleanup python .cache/training-proof/llama/convert_lora_to_gguf.py --base .cache/training-proof/source --outtype f32 --outfile .cache/training-proof/zero.gguf .cache/training-proof/peft-zero
uv run --locked --project training/cleanup python .cache/training-proof/llama/convert_lora_to_gguf.py --base .cache/training-proof/source --outtype f32 --outfile .cache/training-proof/nonzero.gguf .cache/training-proof/peft-nonzero
uv run --locked --project training/cleanup python .cache/training-proof/llama/convert_hf_to_gguf.py .cache/training-proof/source --outtype f32 --outfile .cache/training-proof/qwen-f32.gguf
```

Run with [the normal native build prerequisites](development.md). Use a new output
directory for each attempt; do not overwrite failed evidence. The native controls
test adapter off, zero weights, scale zero, nonzero weights, repeat and removal
with fresh KV state. Three authored prompts cover two English inputs and one
Hindi input. These are numerical inputs, not a quality corpus.

```sh
. scripts/dev-env.sh
cargo run --locked -p vesper-inference --example adapter_probe -- .cache/training-proof/qwen-f32.gguf .cache/training-proof/zero.gguf .cache/training-proof/nonzero.gguf cpu-f32-cache .cache/training-proof/f32-cpu
uv run --locked --project training/cleanup python training/cleanup/compare_mlx.py --source .cache/training-proof/source --adapter .cache/training-proof/mlx-nonzero --native .cache/training-proof/f32-cpu --output .cache/training-proof/mlx-native-parity.json
```

The comparator is expected to exit nonzero for the pinned, currently failing
envelope. It compares full next-token logits using the native prompt token IDs.
Both bases cast original source tensors to float32 **before** Qwen norm-weight
sanitation. It also compares adapter-on minus adapter-off deltas. `cpu` and
`metal` probe modes use the native default F16 KV cache; `cpu-f32-cache` isolates
cache precision for the F32 reference. CPU explicitly selects no GPU devices.
To test deployed Q4 controls, replace the base argument with the installed pinned
`Qwen3.5-0.8B-Q4_0.gguf`, use `cpu` or `metal`, and new output directories.
Q8, CUDA and other hardware are not qualified by these Q4 runs.

Model/adapter hashes, probe-source hash, backend and settings are emitted with
the native report. The comparator records source/adapter/report/source-code
hashes and package versions. Keep full reports, generated weights and logits in
ignored local storage. Commit code, locks and canonical evidence summaries;
do not check in generated benchmark manifests or model files.

## Measured result — 27 September 2026

Application baseline `0bf4c59` plus the `feature/automatic-personalization` proof
changes; original artifacts/reports are retained, and the current probe adds
source/artifact hash recording. Native zero, scale-zero, repeat and removal
maximum errors were **0.0** on all three inputs for Q4 CPU, Q4 Metal and F32 CPU.
Nonzero effects were measurable in every case. The predefined native control
limit is max absolute error `0.0005`, with nonzero effect greater than `0.00001`.
The converter produced exactly the intended two adapter tensors with exact F32
payloads. An initial CPU probe that set only GPU-layer count failed to create a
context because default devices still selected Metal inside the sandbox; the
correct explicit empty-device configuration matches the production CPU path.

Cross-runtime limits fixed before the first comparison: baseline max absolute
error ≤0.02, baseline RMS ≤0.002, adapter-delta relative RMS ≤0.1 and cosine ≥0.99.

| Run                                                | Baseline max absolute range | Baseline RMS range | Result |
| -------------------------------------------------- | --------------------------- | ------------------ | ------ |
| Locked MLX-LM 0.31.3 vs native F32                 | 0.03565–0.04840             | 0.00681–0.01151    | Fail   |
| Native F32 KV cache control                        | Same as above               | Same as above      | Fail   |
| Official normalization correction, diagnostic only | 0.00980–0.01221             | 0.00215–0.00282    | Fail   |
| Native normalization formula, diagnostic only      | 0.00905–0.01595             | 0.00204–0.00374    | Fail   |
| Official formula with reference GDN operations     | 0.00947–0.01277             | 0.00218–0.00285    | Fail   |

The locked run's adapter-delta cosine was 0.999809–0.999989, relative RMS
0.00475–0.01957. Delta agreement supports the narrower hypothesis of a base-runtime
discrepancy; it does not clear the failed baseline gate.

There is an identified normalization difference. For head width `D=128`, old MLX
normalizes by `sqrt(sum(x²) + D×1e-6)`. The
[official MLX correction](https://github.com/ml-explore/mlx-lm/commit/a63e24c389382619eb6d9af656e3b46024be217a)
uses `sqrt(sum(x²) + 1e-6)`, while the
[pinned native implementation](https://github.com/ggml-org/llama.cpp/blob/e79e4bf660e19f2ad851e06c6913f7a8c5852621/ggml/src/ggml-cpu/ops.cpp#L4204)
clamps the square root to `1e-6`. The three diagnostic variants used isolated
in-memory overrides of the locked MLX implementation; they are **not** a validated
dependency upgrade or a production patch. The official correction improves
agreement but does not account for the remaining discrepancy. Locks, production
runtime and original acceptance thresholds remain unchanged.

| Artifact             | SHA-256                                                            |
| -------------------- | ------------------------------------------------------------------ |
| Source config        | `b90b86f35c8e6925ef74ee04d0e758f0a845c83a42089ad82bbaa948de9b4204` |
| Source safetensors   | `04b1c301231dd422b8860db31311ab2721511346a32cb1e079c4c4e5f1fe4696` |
| Converted F32 base   | `0db234f4b33f0624cbab12f259d9769ccf1146e2807380eb085aa5aed4d55ed8` |
| Deployed Q4 base     | `57d1997790d1744fba5b40a7317df71ea5e2acee28c47e78f0cce39c0703f8cf` |
| Zero GGUF adapter    | `e1fe2d30f09a26dbea4eee6449550b988f064f0f3fb60782643545370a6b0be1` |
| Nonzero GGUF adapter | `2a2f8b09fa6df53098d813c16e5f33a6cffd5df16675469d0bd5fba84ea99ce7` |

These short synthetic controls establish neither writing quality nor CPU/GPU
speedup. Several diagnostics shared the host with builds; power and sustained
thermal behavior were not controlled. No private audio, personal training data,
held-out quality test, trained candidate, automatic job, activation or rollback
was exercised. Missing Q8 and language/duration/resource coverage also prevents
general qualification. The next diagnostic is matched layerwise activations at
the first divergence, using the native standalone evaluation callback; it is not
permission to change production math. Training/promotion remains blocked in
[#5](https://github.com/rahulsharmavishwakarma/vesper/issues/5).
