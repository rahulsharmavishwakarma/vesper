# Hugging Face model profiles

This developer reference describes the pinned profiles behind the curated
Models menus and previously saved custom selections. There is no user-facing
JSON import section. Hugging Face hosts many unrelated architectures; a repository
being on the Hub does not make its files compatible with a particular inference
engine. Profiles describe files for the adapters the application actually runs.

| Purpose            | Native adapter                             | Accepted files                                                                                                                 |
| ------------------ | ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------ |
| Speech recognition | sherpa-onnx 1.13.8 offline NeMo transducer | Exactly one encoder ONNX, decoder ONNX, joiner ONNX and tokens TXT, exported for sherpa-onnx; 16 kHz audio and 80 feature bins |
| Transcript cleanup | llama-cpp-2 0.1.159 / bundled llama.cpp    | One complete, unsplit GGUF instruction model whose architecture and embedded chat template are supported by that build         |

Raw Transformers `.safetensors`, PyTorch checkpoints, `.nemo` bundles, arbitrary
ONNX graphs, split GGUF sets, models with external ONNX tensor files, and other
ASR families need another adapter or a compatible conversion. The application
does not execute repository Python, `trust_remote_code`, or custom scripts.
Adding these adapters later does not require changing the recording or dataset
format. Python is not required for this runtime.

## Select a profile

The Models menus offer [pinned presets](model-presets.md) for speech and writing.
Choose a model; settings save automatically. Use **Download** (shown as **Save &
download** when the selection has unsaved edits) to install it, or **Verify
files** to repair an installed one. Download controls and file status appear
with each model selector, and only the selected model's files are fetched.

Previously saved custom profiles remain selectable and use the same validation
and runtime paths. New supported choices should be added through the
[catalog maintenance process](model-presets.md#maintaining-the-catalog).

The persisted fields are `models.customAsr` and `models.customRewriter`, each
either a profile object or `null`. Older settings without these fields receive
the current Cohere Transcribe and Qwen defaults. Cleanup being disabled omits its
weights from selected-pipeline installation and verification; an individual
catalog download remains explicit and independent.

The following desktop commands remain available for developer compatibility;
they are not a profile-import interface in the application:

```ts
await invoke('import_model_profile', { profileJson: JSON.stringify(profile) });
await invoke('clear_model_profile', { role: 'rewriter' }); // or 'asr'
// Both commands return the complete saved AppConfig. No download occurs.
await invoke('list_model_installations');
await invoke('install_model', { modelId: 'Qwen/Qwen3.5-0.8B', quantization: 'q4_0' });
await invoke('install_models'); // optional: install the complete selected pipeline
```

The regular `save_settings` command supports both custom fields and validates
them before replacing the saved preferences. Model changes are blocked while
recording, inference or installation is active. Invalid changes leave saved
preferences intact.

## A small cleanup candidate

This example uses a public 105 MB SmolLM2 instruction GGUF. Its repository,
revision, byte size and hash were checked against Hub metadata. It is a
compatibility candidate, not a recommendation based on cleanup quality tests;
the application has not benchmarked this model on dictation corrections.

```json
{
  "id": "smollm2-135m-q4",
  "name": "SmolLM2 135M Instruct Q4",
  "repository": "bartowski/SmolLM2-135M-Instruct-GGUF",
  "revision": "09816acd5d99df7be770d85ea30822623dab342c",
  "runtime": "llama_cpp",
  "license": "Apache-2.0",
  "languages": ["en"],
  "files": [
    {
      "role": "weights",
      "path": "SmolLM2-135M-Instruct-Q4_K_M.gguf",
      "sha256": "2e8040ceae7815abe0dcb3540b9995eaa1fa0d2ca9e797d0a635ae4433c68c2d",
      "sizeBytes": 105454432
    }
  ]
}
```

See the [pinned GGUF file](https://huggingface.co/bartowski/SmolLM2-135M-Instruct-GGUF/blob/09816acd5d99df7be770d85ea30822623dab342c/SmolLM2-135M-Instruct-Q4_K_M.gguf)
and the [original instruction model](https://huggingface.co/HuggingFaceTB/SmolLM2-135M-Instruct).

For a speech profile use `runtime: "sherpa_onnx_transducer"`,
`modelType: "nemo_transducer"`, the languages supported by the model, and four
file entries with roles `encoder`, `decoder`, `joiner`, and `tokens`. Their
filenames can differ from the built-in Parakeet filenames. A valid role set is
necessary but does not prove a compatible export: use a documented
[sherpa-onnx NeMo transducer export](https://k2-fsa.github.io/sherpa/onnx/pretrained_models/offline-transducer/nemo-transducer-models.html).
Explicit English/Hindi settings must appear in the selected speech profile's
`languages`; automatic language mode delegates to the model. Declaring a
language does not add language support to the weights.

## Integrity, storage and access

Profiles contain a repository ID (`owner/name`), a full immutable lowercase
40-character Git commit, exact file paths, byte sizes and SHA-256 hashes. Branch
names such as `main`, user-supplied URLs, absolute paths and path traversal are
rejected. Downloads start at `https://huggingface.co/OWNER/REPO/resolve/COMMIT/FILE`;
normal Hugging Face CDN redirects are followed. Files are staged and verified
before replacement; inference verifies their size and SHA-256 again. Storage is
namespaced under `huggingface/OWNER/REPO/COMMIT/FILE` within the selected model
directory. Managed files can be hard links to verified shared HF cache files,
with a copy fallback across filesystems. Descendant symbolic links in the managed
directory remain rejected. See the [cache contract](internals.md#hugging-face-cache-boundary)
for discovery, atomic publication, cancellation and removal.

For new profiles inspect the [Hub API](https://huggingface.co/docs/hub/api):
`GET /api/models/OWNER/REPO/revision/COMMIT?blobs=true` returns pinned file
metadata. Large LFS files expose `lfs.sha256`; a small file's `blobId` is a Git
object SHA-1 and **must not** be used as its SHA-256. Download small files to
calculate their SHA-256 when it is absent. The profile records the license for
provenance; read the repository's actual terms before using its weights.

The built-in downloader currently supports public repositories without
credentials. Gated/private repositories need their files downloaded separately
after obtaining access. Exact pinned files in the standard HF cache are reused;
files manually placed at the managed paths are also verified. Tokens are not
stored in profile JSON. A missing local file,
authorization failure, unsupported architecture or invalid template is reported
as an error; it does not silently change the selected model.

## Shared-cache validation

Run the isolated cache/installer suite with:

```sh
. ./scripts/dev-env.sh
cargo test --locked -p vesper-models
# Explicit network check: pinned public files, temporary cache and app directories.
cargo test --locked -p vesper-models public_hf_cache_smoke -- --ignored --nocapture
```

The default suite uses local HTTP fixtures and temporary directories. It checks
zero HTTP requests for verified snapshots, plain-file and blob-link layouts,
environment precedence, revision/export mismatches, incomplete/corrupt files,
Git and LFS ETags, atomic repair without mutating other shared installations,
concurrent installs sharing one transfer, cancellable lock waits, failed
publication and copy fallback. A subprocess exercises the public
catalog installer, ready status, inventory, removal and offline reinstall using
its configured HF cache. No user model files are edited or removed.

The explicit public smoke downloads pinned tokenizer metadata and a small ONNX
artifact, then installs each into a second directory with the same cache and a
local request counter. It verifies a complete cache hit makes no HTTP request.
On 2026-09-27, the macOS standard suite passed 41 tests and the MLX feature suite
passed 44 (installed-model tokenizer and network smoke cases are explicit opt-ins).
The public smoke passed for the pinned Qwen tokenizer config (12,487 bytes) and
Cohere encoder ONNX (3,090,822 bytes), with zero requests on their second installs.
The strict model-crate Clippy and repository fast checks also passed. Removal UI
checks used synthetic data; a microphone-animation assertion in the broader browser
suite failed once and passed on an isolated rerun, while both responsive suites
passed initially. This qualifies storage and installation, not model inference or
transcription quality. Cross-filesystem fallback is forced in the fixture;
Windows and network filesystems require their own platform qualification.

The local-main integration on 2026-09-27 also passed the 44-test MLX model
suite and all six browser suites, including the existing appearance preferences,
Settings drafts and narrow layouts in both themes. Cache installation and
removal retain their existing model controls within that layout.

## Cleanup and training data

Built-in Qwen retains its existing tested prompt with thinking disabled. Custom
GGUF profiles use their embedded chat template through llama.cpp's supported
template engine; this does not implement arbitrary Jinja or guarantee support
for every model's optional template arguments. Tokenization honors model special
token metadata while avoiding a duplicated leading BOS token. Missing or
unsupported templates fail explicitly. Reasoning-only models are unsuitable
for this bounded editing task; generated reasoning/control markers, empty or
truncated responses and detected fidelity failures cause the raw transcript to
be retained.

Cleanup keeps the existing greedy sampling, maximum 2,048 output tokens and
context capped at the smaller of 4,096 and the model's trained context. A very
small context model may reject a long transcript before generation. Current
adapters use the existing CPU execution path. Import success validates profile
structure, not model quality, hardware suitability or native load compatibility.

Saved sessions and exports include the selected profile, repository commit,
files/hashes, runtime versions and inference settings. Model output remains a
prediction; it does not become human-verified training data. Switching profiles
does not trigger training or automatic promotion of a new model.

For architecture and template details, consult the
[llama.cpp model documentation](https://github.com/ggml-org/llama.cpp/blob/master/docs/models.md)
and [Rust chat template API](https://docs.rs/llama-cpp-2/0.1.159/llama_cpp_2/model/struct.LlamaModel.html#method.apply_chat_template).

## Generate a profile without Python

Use the repository's supported Node version (22.13+ or 24+) to obtain pinned file metadata for a catalog candidate:

The Makefile shortcut for a writing model is `make model-profile HF_REPO=owner/repository HF_WEIGHTS=model.gguf HF_LANGUAGES=en`. Both speech and writing profile generation require declared languages (`--languages` in the script). Use the exact checkpoint's official evidence, not its runtime or family name. It writes `models/custom-profile.json`; use `HF_OUTPUT` or `HF_REVISION` to override the output path or source revision. Review the generated profile using the catalog maintenance process before adding it to `models/presets.json`. Generation alone does not add a menu entry. The script form below also supports speech bundles and additional metadata.

The application matches base language codes case-insensitively (`en-US` can
serve the English selection); a declaration is compatibility metadata, not a
quality benchmark. Legacy writer profiles without languages remain readable for
repair, but cannot be saved with polishing enabled or used to rewrite text.
Reselect a catalog preset with current metadata, update the reviewed custom
profile, or disable polishing. Custom writers require an explicit language;
automatic speech detection does not establish compatibility with an unknown
output language. See [writing language compatibility](language-support-and-recommendations.md#writing-language-compatibility).

```sh
node scripts/huggingface-profile.mjs \
  --repo bartowski/SmolLM2-135M-Instruct-GGUF \
  --runtime llama_cpp \
  --weights SmolLM2-135M-Instruct-Q4_K_M.gguf \
  --languages en \
  --out smollm-profile.json
```

For a speech export, specify its four files and the languages it supports:

```sh
node scripts/huggingface-profile.mjs \
  --repo csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8 \
  --revision 2bda32ec70b097a55adaa07d9a7173915b43cc78 \
  --runtime sherpa_onnx_transducer \
  --encoder encoder.int8.onnx \
  --decoder decoder.int8.onnx \
  --joiner joiner.int8.onnx \
  --tokens tokens.txt \
  --languages en \
  --license 'CC-BY-4.0 (NVIDIA original weights)' \
  --out parakeet-profile.json
```

The default revision is `main`; the utility resolves it to an immutable commit
and reads that commit's file metadata. `--name` sets the display name. License
metadata comes from the pinned model card unless `--license` is supplied; missing
license metadata requires an explicit value. `--out` writes or replaces a regular
file; omit it to print JSON to stdout. Diagnostics go to stderr.

Large weights are not downloaded. Files without an LFS SHA-256 are downloaded
only when their declared size is at most 1 MiB, with the response size bounded
and checked before hashing. This covers small token vocabularies without
mistaking Git object IDs for SHA-256. The utility supports public repositories
without tokens, and reports inaccessible repositories, missing files and
invalid metadata as errors. Generated metadata does not establish native
compatibility or cleanup quality. Validate the candidate's runtime contract
before adding it to the catalog; downloading remains an explicit user action.

Run `node scripts/huggingface-profile.mjs --help` for options, and
`node --test scripts/huggingface-profile.test.mjs` for the offline checks.
