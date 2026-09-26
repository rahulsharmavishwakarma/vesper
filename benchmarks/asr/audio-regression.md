# Hard audio frontend regression protocol

This extends the [initial screen](../../docs/validation/audio-frontends-2026-09-16.md) under [issue42](https://github.com/rahulsharmavishwakarma/transcribe/issues/42#issuecomment-5688135829). It tests frozen weights through public native APIs. No production lifetime, filtering, training, kernel or default changes are part of this experiment. Only Silero v6.2, DPDFNet4/8 and Uhm qualify as newly trained checkpoints in the requested preceding-year window. Ear is retained as an explicitly requested recent deployment of older Whisper weights.

## Frozen data and contrasts

[The frozen manifests](manifests/audio-regression-2026-09-16/frontend.json) contain 24 previously unused English and 12 previously unused Hindi FLEURS test recordings. Selection uses SHA256 ordering within published sex and duration strata: female/male × (2,8], (8,16], (16,25] seconds; four English and two Hindi per stratum. IDs already present in repository benchmark manifests/reports were excluded before inference. No speaker IDs are available; this is neither a speaker-disjoint nor checkpoint-training-disjoint claim. Two English recordings share a sentence: confidence intervals group those recordings together.

Four existing synthetic controls test negation/amount, date/time, spoken instructions and self-correction. Four generated non-speech controls and four five-second natural environmental noises are separate negative lanes. Synthetic controls never count toward natural-speech accuracy.

Ten versions of each of the40 speech sources are frozen:

- `clean`: original decoded samples, saved losslessly as float32 WAV.
- `matchedClean`: the same common headroom gain used for noisy mixtures, retained as a signal reference.
- `quiet30` / `quiet10`: original samples ×0.03 / ×0.01. Attenuation is not whispered speech.
- `natural10` / `natural0` / `naturalm5`: recorded environmental noise at +10/0/−5dB whole-waveform RMS SNR. Noise is assigned cyclically, with a hash-selected circular offset. Scale noise by `rms(speech)/(rms(noise)*10**(snr/20))`; then apply a common speech/noise gain if needed to keep every mixture peak≤0.95. Preserve the exact gain and measured SNR. This avoids rejecting only the most difficult mixtures. These are artificial mixtures of real sources, not naturally noisy dictation.
- `clip`: multiply original samples by6 and saturate at±0.25; record clipped-sample fraction.
- `reverb`: direct signal plus40/90/170ms echoes at0.4/0.25/0.1, divided by1.75; tail truncated. This is a diagnostic FIR, not a measured room response.
- `drop10`: zero a seeded selection of approximately10% of20ms packets. This is waveform loss injection, not an application queue measurement.

ASR runs original, quiet30, natural10 and natural0 English through **raw/DPDFNet4/DPDFNet8 → Cohere → Qwen**. The model order rotates across conditions. All three receive exactly the same PCM within a condition. Three natural English sources and all four synthetic sources need headroom attenuation in the noisy variants. Because ASR does not run `matchedClean`, clean-to-noisy differences combine noise and headroom effects; within-condition frontend comparisons remain paired. Hindi frontend/LID coverage is separate: the current Cohere adapter rejects Hindi, so no Hindi ASR qualification is implied. Every raw ASR failure and writer-fidelity rejection remains visible.

MS-SNSD files are pinned at `fe61c4ba0d9ac8dd7e23d719cc79f8947e1dc742`, with [individual hashes](manifests/audio-regression-2026-09-16/noise-assets.json). Its README identifies CC0/Freesound and CC-BY-SA3/DEMAND audio sources; the MIT code license is not an audio-license assertion. Only local audio copies are used. [Official dataset and mixing motivation](https://github.com/microsoft/MS-SNSD/tree/fe61c4ba0d9ac8dd7e23d719cc79f8947e1dc742).

## Natural filler evaluation

[AMI windows](manifests/audio-regression-2026-09-16/ami.json) add24 preselected conversation excerpts:6uh,6um,4mm-hmm backchannels,2hmm backchannels,3laughter and3ordinary-speech controls. Selection uses lexical and dialogue-act annotations, before any Uhm outputs. Fifteen windows have no other annotated speaker; nine backchannel/laughter windows retain overlap as separate diagnostics. Corpus references to uh/um do not exhaust every possible filler family.

The single ES2004a meeting has four speakers and is a local validation selection, not the official AMI test split. Approximate word intervals support target-overlap checks; they do not justify exact boundary-F1 or independent-session confidence intervals. A filler hit requires overlap with an annotated uh/um event. Backchannel-anchor overlap and any-span false alarms are reported separately.

AMI is CC-BY4 according to the [official download page](https://groups.inf.ed.ac.uk/ami/download/) and archive license. The [annotation guidelines](https://groups.inf.ed.ac.uk/ami/corpus/Guidelines/speech-transcription-manual.v1.2.pdf) distinguish hesitation fillers from acknowledgment sounds. The original audio host timed out; bytes were downloaded from a pinned mirror and checked against its Git LFS hashes. Original-to-mirror byte parity was not independently verified. Exact URLs, hashes, source intervals and annotations are in the manifest. Uhm raw and author-normalized input are separate fixed configurations; neither deletes audio.

## Accuracy, failures and runtime contracts

Use total edits / total reference words for WER, and the existing grapheme scorer for CER. Keep natural/synthetic/language/condition lanes separate. Bootstrap paired reference-sentence blocks 10,000 times with fixed seed 20260916; this groups repeated readings together and preserves pairing. Intervals are descriptive, unadjusted 95% percentile intervals, not speaker-independent or multiple-testing-corrected significance claims. Keep failed/missing cases visible alongside scored denominators. Report accepted writer results separately from fidelity rejections. Also report accepted cleaned scores with otherwise-retained raw scores over the complete scored corpus: selecting only successful rewrites would hide empty ASR outputs and rejected writing.

The statistical motivation is [Liu et al., blockwise ASR bootstrap](https://www.isca-archive.org/interspeech_2020/liu20c_interspeech.html). [Ochiai et al., enhancement distortions and ASR](https://arxiv.org/abs/2404.14860v1) motivates direct word/meaning checks; neither paper establishes a speedup or accuracy result for this checkpoint/backend. Existing model/runtime pins remain in [the research map](../../docs/research-streaming-inference-papers.md).

As a secondary signal diagnostic, calculate zero-mean scale-invariant SDR against the matched clean waveform, without searching for a favorable time shift. Undefined zero-energy or perfect residual cases remain null. [Le Roux et al., SI-SDR](https://arxiv.org/abs/1811.02508v1) motivates this metric; a better SI-SDR does not establish better words or preserved intent. Separate standalone frontend runs also check whether the exact same input/model produces the same output as during ASR qualification.

Contract tests cover1,63,64,159,160,255,256,319,320,511,512,513,639,640,641and15,999sample recordings. Compare completed-file,320sample chunks and a repeating variable schedule1,63,159,511,320,1600,17. Validate finite output, complete tail, coordinate bounds, sample count and paired signal differences. A fresh recording follows each stress sequence in the same process. Three-minute stitched clean/noisy/quiet frontend replay tests cumulative pacing and post-stream reset. First partial, word-emission, live-ASR endpoint, actual queue drops, cancellation during native calls, sustained thermals and other devices remain unmeasured.

RTF is frontend computation/input duration, excluding pacing sleeps, loads, file I/O and ASR. Per-call p95 is not per-utterance latency. ASR stage measurements include the existing adapter's full artifact verification and model loading; they are not warm-decoder timings. Peak RSS is the whole process. Timed model jobs run sequentially; unrelated system load is uncontrolled.

The VAD gating analysis is a counterfactual on paired completed-file outputs: replace the entire raw transcript with empty text if Silero returned no segment. This measures possible whole-clip suppression, not an implemented gated pipeline, segment trimming, or word-boundary recall.

An additional five-case Cohere diagnostic was selected **after** observing an empty quiet transcript. It is excluded from qualification accuracy. Run the same first English clip as original, original ×0.03, attenuated then divided by0.03, attenuated repeat, and original repeat. Fresh per-case recognizers preserve the production lifetime. Measure artifact verification, recognizer creation, waveform acceptance, decode and release separately; record the native completion reason and truncation flag. Restoring the transcript after restoring volume establishes amplitude sensitivity for that clip, not the cause or a validated automatic-gain policy. The OS file cache is not flushed.

## Reproduction

Use the pinned environment/artifacts in [the benchmark README](README.md). Obtain the two FLEURS test archives/TSVs and previous synthetic controls as documented there. Download the noise files at the exact URLs/hashes above into `.cache/benchmarks/downloads/ms-snsd/`, including `assets.json`; download the two AMI assets and frozen `candidate-windows.json` into `.cache/benchmarks/downloads/ami/`. Audio stays in the ignored cache.

```sh
.cache/audio-eval-venv/bin/python scripts/prepare-audio-regression.py \
  --root .cache/benchmarks/results/audio-regression-2026-09-16 \
  --prior-ids benchmarks/asr/manifests/audio-regression-2026-09-16/excluded-ids.json
.cache/audio-eval-venv/bin/python scripts/prepare-ami-regression.py \
  .cache/benchmarks/downloads/ami .cache/benchmarks/results/audio-regression-2026-09-16
source scripts/dev-env.sh
cargo build --release -p vesper-eval --example audio_frontend --locked
```

Place the official pinned Ear frontend golden JSON at `<result-root>/ear-golden.json`. Fill the environment JSON with the actual host details; do not inherit another host's attestation. Run `scripts/run-audio-regression.py <result-root> PHASE` sequentially for `asr`, `frontend`, `contracts`, `desert`, `paced`. The runner refuses to overwrite prior completed or partial runs; use a new result root for independent repetition. It freezes a native executable, records process exit codes and captures macOS `/usr/bin/time -l` RSS. On other operating systems replace the process measurement wrapper and report a different hardware envelope.

```sh
.cache/audio-eval-venv/bin/python scripts/summarize-audio-regression.py \
  .cache/benchmarks/results/audio-regression-2026-09-16 \
  .cache/benchmarks/results/audio-regression-2026-09-16/debug-summary.json
```

No threshold or model is tuned after viewing this validation set. Always-on promotion requires demonstrated paired benefit without clean/meaning regressions, valid runtime contracts and a suitable local latency/RAM envelope; otherwise preserve original audio and state the narrower conclusion.

To reproduce the separate amplitude/timing diagnostic, copy `manifests/audio-regression-2026-09-16/cohere-diagnostic.json` into the result root. Read its second sample as float32 with SoundFile, divide by `numpy.float32(0.03)`, and write the third sample's path as a 16 kHz `FLOAT` WAV. Verify the manifest's recorded SHA-256 before using it. Build `cargo build --release -p vesper-eval --example cohere_stage_probe --locked`; run that example with the five arguments `MANIFEST CORPUS MODELS ENV OUTPUT`, capturing `/usr/bin/time -l` into the output's `.log` sibling. Use `<result-root>/cohere-stage-probe.json` for automatic inclusion in the summary.

The isolated SHA-256 follow-up compares `sha2 = 0.10.9` with and without its `asm` feature. Its [official ARM dispatch](https://docs.rs/sha2/0.10.9/src/sha2/sha256.rs.html) requires that feature for hardware SHA support. After all timed model runs, execute `scripts/run-integrity-probe.py <result-root>` with the repository Rust environment active. It builds a separate nested Cargo workspace, rotates software/hardware order across three passes over the four Cohere artifacts, and checks padding/block boundaries plus mutation/truncation rejection. It does not change application dependencies, cache verified results or alter model lifetimes. File-read/hash timings are not whole-application latency.

Two further diagnostics follow observed boundary mismatches and are excluded from qualification: `silero-variable-diagnostic.json` repeats the same 4.62-second recording three times under the variable schedule, distinguishing packetization from reset state. `denoiser-startup-diagnostic.json` compares the existing 15,999-sample hard cut with that waveform prefixed by 320 exact zeros. For each DPDF model, run both `offline` and `chunks` modes on both samples; after the harness's existing delay compensation, discard the additional 320 samples only for the prefixed case before computing `signal_difference`. These frozen manifests and source hashes allow reproduction; neither diagnostic changes the main matrix or qualifies a production padding policy. `detectedChunks` is callback-dependent telemetry, not speech duration or a cross-schedule accuracy metric.
