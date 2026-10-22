# Benchmark dataset: getting the audio for each row

This dataset ships text plus Vesper's own generated noise clips. Every other row points at audio in its source dataset
through `source_id` and `sample_id`. This page explains how to fetch that audio, for reproducing results or training.

## Quick map

| `source_id` | Where the audio is | How `sample_id` maps |
| --- | --- | --- |
| `fleurs-<locale>-test`, `fleurs-<locale>-dev` | [`google/fleurs`](https://huggingface.co/datasets/google/fleurs) at revision `70bb2e84b976b7e960aa89f1c648e09c59f894dd`, config `<locale>` (for example `en_us`), split `test` or `validation` | `<locale>-<N>`, `<locale>-test-<N>` or `<locale>-dev-<N>` → the FLEURS audio file `<N>.wav` (the second column of `<split>.tsv`, and the file name in the row's `path`) |
| `google-fleurs-test` | built from FLEURS clips (see composites below) | composite IDs such as `fr_fr-baseline` and `en_us-repeat3` |
| `ami-es2004a` | [AMI Meeting Corpus](https://groups.inf.ed.ac.uk/ami/download/), meeting ES2004a, mixed headset (CC BY 4.0) | `ami-backchannel-<AMI words id>`: a short window around that word segment in the AMI annotations |
| `ms-snsd-noise-test` | [MS-SNSD `noise_test`](https://github.com/microsoft/MS-SNSD/tree/fe61c4ba0d9ac8dd7e23d719cc79f8947e1dc742/noise_test) at `fe61c4ba` | `noise-natural-0` to `noise-natural-3`, four fixed test noises. The collection mixes Freesound CC0 and DEMAND CC BY-SA 3.0 recordings. |
| `audio-frontend-synthetic-noise` | **included:** `audio/synthetic-noise/<sample_id>.wav` | `noise-silence`, `noise-white`, `noise-hum`, `noise-tone`: 5 s, 16 kHz PCM16, deterministic (seed 17) |
| `svarah` | [`ai4bharat/Svarah`](https://huggingface.co/datasets/ai4bharat/Svarah) at `ebbf7777`; gated, so request access | `svarah-<Svarah id>`: the Svarah utterance or chunk ID |
| `openslr-104-mucs-hi-en-test` | [OpenSLR 104](https://www.openslr.org/104/) `Hindi-English_test.tar.gz` (sha256 `93e358b3…`), CC BY-SA 4.0 | `<speaker>_<utterance>_<segment>`: the archive's utterance ID; `<utterance>-full` is the concatenated utterance |
| `synthetic-dictation-controls` | **not distributed** | Vesper's scripted sentences (`reference_normalized` holds the text). The audio was spoken by a macOS system voice (`say -v Samantha -r 175`, 16 kHz PCM16, macOS 26.6.2), and Apple's licence keeps it out. |

## Condition suffixes (`--<condition>`)

A `sample_id` like `en_us-11974497390699501411--quiet30` is a processed copy of the FLEURS clip before the `--`. These
copies are deterministic and are rebuilt by Vesper's scripts from the source clip plus an MS-SNSD noise file. See
[`scripts/prepare-audio-regression.py`](https://github.com/rahulsharmavishwakarma/vesper/blob/main/scripts/prepare-audio-regression.py)
and `benchmarks/asr/README.md` in the Vesper repository.

| Suffix | Meaning |
| --- | --- |
| `clean` | the original clip |
| `matchedClean` | the original at the common headroom gain used for the noisy mixes |
| `quiet30` | amplitude × 0.03 (about −30 dB) |
| `quiet10` | amplitude × 0.01 (−40 dB) |
| `natural10`, `natural0`, `naturalm5` | mixed with an MS-SNSD noise file at 10, 0 or −5 dB whole-clip RMS SNR, then the common headroom gain |
| `clip` | amplitude × 6, hard-clipped at ±0.25 |
| `reverb` | direct sound plus echoes at 40, 90 and 170 ms (gains 0.4, 0.25, 0.1), divided by 1.75 |
| `drop10` | 10% of 20 ms blocks zeroed (seeded) |
| `integration-*`, `followup-*` | the same conditions from later benchmark rounds |

Run-level details are in `data/reports.jsonl`, joined on `report_id`.

## Composite and diagnostic IDs

These are not single source clips. They are long-form or boundary diagnostics built from FLEURS clips by Vesper's
benchmark scripts:
- `fr_fr-baseline`, `*-repeat3`, `*-shifted-repeat3`, `*-natural-long`;
- `synthetic-threefold-boundaries`, `whisper-final-reset`, `cohere-diagnostic-restored`.

Treat them as text-only unless you rebuild them from the Vesper repository.

## Example: join with FLEURS

```python
from datasets import load_dataset

rows = load_dataset("rahulsharmavishwakarma/vesper-data-asr-outputs", split="train")
fleurs = load_dataset("google/fleurs", "en_us", split="test",
                      revision="70bb2e84b976b7e960aa89f1c648e09c59f894dd")
by_file = {r["path"].rsplit("/", 1)[-1].removesuffix(".wav"): i for i, r in enumerate(fleurs)}

def fleurs_audio(row):
    if not row["source_id"].startswith("fleurs-en_us") or "--" in row["sample_id"]:
        return None  # other locales: load their config; conditions: rebuild with Vesper's scripts
    number = row["sample_id"].rsplit("-", 1)[-1]
    index = by_file.get(number)
    return None if index is None else fleurs[index]["audio"]
```

## Licences when you use the audio

- FLEURS and AMI: CC BY 4.0, with attribution.
- MUCS: CC BY-SA 4.0, share-alike.
- Svarah: its own access terms.
- MS-SNSD: per-file Freesound CC0 or DEMAND CC BY-SA 3.0.
- Vesper's generated noise and text: CC BY 4.0, as this dataset.
