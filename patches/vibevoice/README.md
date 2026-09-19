# Pinned VibeVoice compatibility patch

This patch adapts only the isolated Streaming-1.5B research probe to
Transformers 5.10.1. It does not register a desktop model, alter its weights,
change decoder algorithms or qualify a new application runtime.

Publisher source: `1541f590c7099820f10ea012f48d2399282df69f`.
Checkpoint: `4262d23d8a539a6530cf64fbd0b1751ef9a30853`.
Both remain pinned in `benchmarks/asr/vibevoice-streaming-artifacts.json`.

`scripts/vibevoice_compat.py` copies checksum-verified publisher files into a
fresh temporary directory, applies this checksum-verified patch and checks
every output. It leaves the original publisher source and model files intact.
The probe records the patch and source hashes alongside each result.

The patch addresses five API differences:

- The Qwen2 fast tokenizer is exposed through the unified Qwen2 tokenizer class.
- Transformers 5 uses target-to-source tied-weight mappings and loader-aware
  tying. The outer configuration reflects the existing decoder tying setting.
- Parameter initialization uses guarded `nn.init` functions, so the new loader
  cannot reinitialize weights already loaded from the checkpoint.
- The acoustic sampler restores its configured, non-persistent `fix_std` buffer
  after the new loader materializes meta storage. Parameter checks alone cannot
  detect this buffer being left uninitialized.
- The custom acoustic tokenizer config is registered by its class identity,
  even though Transformers now contains a different class with the same name.

The baseline tokenizer fixture contains synthetic multilingual/whitespace/
punctuation controls, chat-template results, token metadata and a vocabulary
digest from Transformers 4.57.6. Run `scripts/check-vibevoice-tokenizer.py` in the
locked probe environment before accepting changes. The same check exercises
small synthetic tied/untied checkpoint round trips
and verifies that publisher and built-in acoustic configs dispatch separately.
Both the round trips and the actual probe assert the acoustic sampling buffer.
Each actual probe load also
compares every tensor with the checkpoint and checks tied-parameter identity,
missing/unexpected weights and cache progression. A clean package audit alone
does not establish model compatibility or renew historical benchmark results.

The compatibility scope is processor `from_pretrained` and the publisher's
`init_streaming_state` / `streaming_generate_step` API. TTS classes, alternate
checkpoints and direct tokenizer construction from separate vocab/merge files
are outside this qualification. Remove this patch only after a pinned upstream
revision passes the same checks and held-out plans.
