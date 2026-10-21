# ASR quantization notebooks (research)

Colab notebooks for [#271](https://github.com/rahulsharmavishwakarma/vesper/issues/271): why the Parakeet TDT 0.6B v3 4-bit
weight-only encoder is not smaller or faster than the old INT8, and whether a graph rewrite or a different quantizer helps.
They are development research tools. They do not touch the application, its catalog or its defaults, and they are not part of
CI or the desktop runtime.

| Notebook | Question | Output |
| --- | --- | --- |
| [01-wo4-graph-diagnosis](01-wo4-graph-diagnosis.ipynb) | Which operators hold the bytes and the milliseconds, and does `accuracy_level` matter? | `01-wo4-graph-diagnosis.json` |
| [02-conv-to-matmul-int4](02-conv-to-matmul-int4.ipynb) | Does rewriting pointwise convolutions as `MatMul` before 4-bit quantization shrink the file, and what do RTN and HQQ change? | `02-conv-to-matmul-int4.json`, variant encoders |
| [03-wer-fleurs-en](03-wer-fleurs-en.ipynb) | What is the word error rate of each variant on public FLEURS English? | `03-wer-fleurs-en.json` |

Results from the first full run are in [the 22 October 2026 record](../../docs/validation/parakeet-conv-rewrite-2026-10-22.md).

## Running

1. Open a notebook in Google Colab (File > Open notebook > GitHub, or upload it). A **CPU runtime is enough**; none of these
   notebooks use a GPU. A High-RAM runtime (8 vCPUs, 51 GB) is much faster for notebooks 02 and 03.
2. Run all cells. Notebook 01 is independent. Run **02 before 03 in the same runtime**: 03 reads the variants 02 writes to
   `/content/work/variants/`. To split sessions, copy that folder to Drive using the last cell of 02.
3. Each notebook saves a JSON result with its environment (CPU model, RAM, library versions, source revisions) and offers it
   as a download. Send those files back to attach their numbers to #271.

Colab recycles a runtime it considers idle. Work started only from a shell (for example over SSH) did not count as activity in our runs and was lost after about an hour, so run the cells in the notebook or through `colab exec`, and download results as they appear. Notebook 03 also writes `03-wer-fleurs-en.partial.json` after each variant.

Observed times: 01 took about 8 minutes on a standard 2-vCPU runtime. On a High-RAM 8-vCPU runtime, 02 took about 8 minutes and 03 about 19 minutes; on 2 vCPUs, 03 took over an hour.

## What the results can and cannot say

- Sources are pinned: the official sherpa-onnx FP32 export `csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3@1a468a35`, the
  app's current 4-bit build `rahulsharmavishwakarma/vesper-parakeet-tdt-0.6b-v3-sherpa-onnx-wo4@8e178c8a`, and FLEURS at
  revision `70bb2e84` with file hashes checked. Public data only; no private audio or transcripts are used or needed.
- Colab CPU timings are **not comparable** with the desktop numbers in the validation records. Compare variants only against
  each other within one run, and record that the machine is a shared virtual CPU.
- Notebook 01 and 02 time random 10-second inputs, so they measure speed, not accuracy. Notebook 02's closeness to FP32 on
  random features is only a sanity check. Accuracy comes from notebook 03.
- Notebook 03 covers English read speech only (100 clips). It does not test noise, German, Hindi, long audio or the app path.
  Its decoder and joiner for new variants stay FP32, unlike the current app build, which quantizes them too.
- GPTQ is not included: it needs calibration features computed exactly as the model expects (128-bin log-mel), which need a
  verified extractor first. HQQ is data-free and is included.
- A result here is research evidence only. Shipping a different encoder needs a pinned artifact, license and provenance
  records, and the held-out qualification described in [`benchmarks/asr/README.md`](../../benchmarks/asr/README.md).

## Dependencies

Everything installs through uv from the locked project `benchmarks/asr/python/onnx-quantize`. The first cell of each notebook
writes a hash-checked requirements file, exported with `uv export --locked` (notebook 03 uses the `evaluation` group, which
adds `sherpa-onnx`, `sherpa-onnx-core` and `soundfile`; notebook 02 uses the `hqq` group, which adds CPU PyTorch because ONNX Runtime's
HQQ quantizer imports it, installed from PyTorch's CPU index with the hashes still enforced), and installs it with `uv pip install --require-hashes`. Colab already
ships uv; the cell installs it only if missing. Nothing is added to the repository's other Python projects. The project
supports Python 3.12 and 3.13 so that Colab's current runtime works.

Do not edit the embedded requirements by hand. After changing the project's dependencies, regenerate them:

```sh
uv export --locked --project benchmarks/asr/python/onnx-quantize --no-emit-project --no-header
uv export --locked --project benchmarks/asr/python/onnx-quantize --group evaluation --no-emit-project --no-header
uv export --locked --project benchmarks/asr/python/onnx-quantize --group hqq --no-emit-project --no-header
```

The `evaluation` and `hqq` groups are not covered by CI's dependency audit, which checks only the default group. The pip wheel of
sherpa-onnx may bundle a different ONNX Runtime than the application's build, so a variant that loads in the app could fail to
load there (for example if `MatMulNBits` is unavailable); notebook 03 prints load failures instead of hiding them.

## Running from the command line

With Google's `colab` CLI (`uv tool install google-colab-cli`) a connected agent or you can run the same code from a terminal:
`colab ssh` gives a shell on the runtime, and `colab exec` runs a script. Create a virtual environment with
`uv venv --python 3.13` and install the exported requirements into it, then run each notebook's code cells in order. Over
SSH, a runtime allows one connection at a time.
