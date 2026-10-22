# Backend inference architecture

This page describes Vesper's custom on-device inference backend, as implemented
on `main` in October 2026. It starts with the whole backend and then gives one
section per component. Each diagram is a versioned Mermaid source, and each
label names real code. Paths that are proposed, experimental or not adopted are
labelled as such. Product behavior lives in [live transcription](live-transcription.md),
[offline ASR](offline-asr.md) and the [user guide](user-guide.md). Measurements
live in [inference performance](inference-performance.md). Decisions are recorded
in [ADRs](adrs/README.md).

Visual review: no Excalidraw MCP server was connected when these diagrams were
written ([#189](https://github.com/rahulsharmavishwakarma/vesper/issues/189)).
The Mermaid sources were rendered and checked with `@mermaid-js/mermaid-cli`
11.4.2. Excalidraw versions should be generated from these sources with the
official Mermaid-to-Excalidraw converter, and kept in sync with them.

## 1. High-level backend

One Tauri 2 desktop process owns capture, orchestration and persistence. All
model work runs on one inference worker thread, owned by `InferenceManager`.
Native runtimes are linked libraries, not services, so nothing leaves the
device after the models are installed.

```mermaid
flowchart TB
  subgraph UI["Svelte 5 UI (WebView)"]
    views[Dictate / History / Models / Settings]
  end
  subgraph Desktop["vesper-desktop (Tauri 2, Rust)"]
    commands[commands.rs: start / stop recording]
    runtime[runtime.rs WorkContext: process_capture]
    dictation[dictation.rs: paste via vesper-desktop-platform]
  end
  subgraph Audio["vesper-audio"]
    capture[CPAL capture, lock-free ring]
    decode[File decode, Symphonia]
    mixer[ChannelMixer + StreamingResampler]
  end
  subgraph Inference["vesper-inference (one worker thread)"]
    manager[InferenceManager + WorkerState]
    frontend[Audio frontend: Silero VAD, DPDFNet8]
    speech[Speech adapters]
    writer[Qwen3.5 writer]
    diar[Speaker diarization]
  end
  subgraph Native["Native runtimes (linked or bundled)"]
    sherpa[sherpa-onnx 1.13.8]
    nemo[NeMo-Speech.cpp C ABI via vesper-nemo]
    mlx[MLX Swift bridge via vesper-mlx]
    llama[llama.cpp via llama-cpp-2 0.1.159 + mtmd]
  end
  subgraph Store["Local storage"]
    models[(Model root: weights + runtimes)]
    data[(vesper-data: sessions.sqlite3 + audio)]
  end
  core[vesper-core: config, model ids, adapter registry, personalization]
  modelsCrate[vesper-models: catalog, install, sha256, selection]

  views <-->|Tauri IPC + events| commands
  commands --> capture
  commands --> runtime
  runtime --> manager
  runtime --> dictation
  runtime --> data
  capture --> mixer --> manager
  decode --> mixer
  manager --> frontend
  manager --> speech
  manager --> writer
  manager --> diar
  speech --> sherpa & nemo & mlx & llama
  writer --> llama
  diar --> nemo
  frontend --> sherpa
  modelsCrate --> models
  manager -. verify + resolve .-> modelsCrate
  core -. shared types .-> Desktop & Inference & modelsCrate
```

| Crate | Responsibility |
| --- | --- |
| `vesper-core` | Config, model ids, adapter registry (`AdapterKind`, `AdapterCapabilities`), personalization types; no workspace dependencies |
| `vesper-audio` | Capture, WAV storage, decoding, `ChannelMixer`, resampling, completed-audio segmentation |
| `vesper-models` | Pinned artifact catalog, install, SHA-256 verification, speech/backend selection |
| `vesper-inference` | Single worker, adapters, audio frontend, live previews, writer, diarization adapter |
| `vesper-nemo` / `vesper-mlx` | Safe wrappers over the NeMo-Speech.cpp C ABI and the MLX Swift bridge |
| `vesper-data` | Local SQLite sessions, transcripts, labels, learning (no inference) |
| `vesper-desktop` | Tauri app: commands, orchestration, paste, diagnostics |

## 2. Recording lifecycle

This sequence covers one dictation using the default buffered model (Cohere) with
the desktop's managed audio frontend. Native streams (Nemotron) skip the mailbox
and produce their final inside the stream.

```mermaid
sequenceDiagram
  autonumber
  participant UI
  participant Cmd as commands.rs
  participant Mic as vesper-audio capture
  participant Mgr as InferenceManager (caller side)
  participant FE as vesper-live-frontend thread
  participant W as inference worker
  participant RT as runtime.rs WorkContext
  participant DB as vesper-data
  UI->>Cmd: start recording
  Cmd->>Mgr: start_stream_personalized
  Mgr->>W: StartStream (resolve, load speech model)
  W-->>Mgr: started (buffered)
  Mgr->>FE: spawn live frontend (Silero + DPDFNet8)
  loop every 20 ms
    Mic->>Mgr: StreamingSession.push(chunk)
    Mgr->>FE: raw chunk (unbounded, never blocks capture)
    FE->>W: enhanced 16 kHz PCM -> preview mailbox, pause marks
    W-->>UI: StreamingTranscript (committed + provisional)
  end
  UI->>Cmd: stop
  Cmd->>Mgr: finish_live
  Mgr->>FE: Finish (flush backlog)
  FE-->>Mgr: enhanced recording -> EnhancedStore
  Cmd->>RT: process_capture_deferring_speakers
  RT->>W: Transcribe (reuses enhanced audio, no second denoise)
  W-->>RT: raw transcript
  RT->>DB: set_transcripts (raw + provenance)
  RT->>W: Rewrite (prefix-cached writer, time-limited)
  W-->>RT: polished text or raw with warning
  RT->>DB: complete_cleanup, personalization
  RT-->>Cmd: PipelineResult
  Cmd->>UI: paste final text (dictation.rs)
  Cmd->>RT: complete_deferred_speakers
  RT->>W: Diarize (original audio)
  RT->>DB: complete_speaker_timeline
  RT-->>UI: republish with timeline
```

## 3. Inference manager and worker

`InferenceManager` is the only owner of native model handles. Callers send
`Command` values over a bounded channel (capacity 2), and a single
`vesper-inference` thread executes them serially. That thread owns
`WorkerState`. Cancellation flags are checked around every native call, and
native decodes themselves cannot be interrupted mid-call.

```mermaid
flowchart LR
  subgraph Callers["Caller threads"]
    t[transcribe / rewrite / analyze_speakers]
    s[start_stream -> StreamingSession]
    sem[schedule semantic job]
  end
  q[[Command channel, capacity 2]]
  subgraph Worker["vesper-inference thread"]
    loop[worker_loop]
    subgraph State["WorkerState (resident handles)"]
      sp[speech: SpeechKey -> SpeechModel]
      rw[rewriter: RewriterKey -> TextModel]
      fe[frontend: FrontendCache]
      es[enhanced: EnhancedStore]
      fb[automatic_fallback: InitializationFallback]
    end
    ev[(evidence JSON per operation)]
  end
  semq[(SemanticQueue: one coalesced job)]
  t --> q
  s --> q
  sem --> semq
  q --> loop
  loop --> State
  loop --> ev
  semq -. "polled every 50 ms only while pending; never loads a model" .-> loop
  loop -->|reply channel| t
  loop -->|StreamingFinal| s
```

- **Residency:** `ensure_speech` reloads only when the `SpeechKey` changes or a
  model is marked unhealthy. It re-checks the key after load, in case files
  changed while loading. The writer is cached by `qwen::key`.
- **Failure isolation:** a native or identity failure drops that handle, and the
  next request reloads it with full verification.
- **Memory pressure** (`resources.rs`) means available memory is below
  max(5% of RAM, 256 MiB). Under pressure the idle writer is released before
  recording (recorded as `writerReleasedForMemory` in the operation's
  evidence), and buffered previews back off by 0.5–5 s. The writer then reloads
  when the final transcript is polished, which adds its load time before paste.
- **Zero-sample shortcut:** an exactly silent recording on MLX, Cohere, Whisper
  or Qwen-ASR returns empty text after verification, without loading weights.

## 4. Speech adapter registry and runtimes

The registry in `vesper-core/src/adapters.rs` maps each built-in model to an
`AdapterKind` with declared capabilities. `vesper-inference/src/adapters.rs`
turns a selection into a loaded `SpeechModel`. Backend selection lives in
`vesper-models/src/selection.rs` and `vesper-inference/src/backend.rs`.

```mermaid
flowchart LR
  subgraph Models["AsrModel (vesper-core)"]
    nem[Nemotron 3.5 streaming]
    par[Parakeet TDT v3]
    qwa[Qwen3-ASR 0.6B]
    coh[Cohere Transcribe - default]
    whi[Whisper Small / Turbo]
    lfm[LFM2.5 Audio]
    moo[Moonshine Base]
    gra[Granite Speech 5.0 TurboCTC]
    pho[Phonon-2]
    red[Parakeet Redux]
    cus[Custom NeMo transducer profile]
  end
  subgraph Kinds["AdapterKind"]
    k1[NemoRnnt: native_streaming, phrase hints]
    k2[SherpaTransducer]
    k3[QwenAsr]
    k4[CohereAsr]
    k5[Whisper]
    k6[LfmAudio]
    k7[MoonshineAsr]
    k8[GraniteCtc: Apple GPU only]
    k9[PackedTdt: Apple GPU only]
    k10[TernaryTdt: Apple GPU only]
    k11[NemoTdt: completed audio, buffered live]
  end
  subgraph Runtime["Runtime"]
    r1[NeMo-Speech.cpp: CPU / Metal / CUDA / Vulkan]
    r2[sherpa-onnx: CPU only]
    r3[MLX Swift: Metal; English except<br/>Granite and Redux languages]
    r4[llama.cpp mtmd: CPU / GPU]
  end
  nem --> k1 --> r1
  par --> k2
  cus --> k2
  qwa --> k3
  coh --> k4
  whi --> k5
  moo --> k7
  k2 & k3 & k4 & k5 & k7 --> r2
  gra --> k8 --> r3
  pho --> k9 --> r3
  red --> k10 --> r3
  par -. "MLX registration" .-> r3
  par -. "Automatic engine (default)" .-> k11 --> r1
  qwa -. "MLX registration, native live preview" .-> r3
  whi -. "WhisperSmall MLX registration" .-> r3
  lfm --> k6 --> r4
```

```mermaid
flowchart TB
  req[Requested backend: Auto / Cpu / Metal / Cuda / Vulkan / Rocm]
  req --> auto{Auto?}
  auto -->|no| exact[Use the explicit backend; error if unavailable]
  auto -->|yes| unified{Effective engine is the unified GGUF?<br/>Automatic for Parakeet v3}
  unified -->|yes| nemosel[NeMo-Speech.cpp package for Metal on Apple Silicon,<br/>CUDA if an NVIDIA GPU is visible, else CPU]
  nemosel --> nload[Load the one GGUF]
  nload -->|GPU load fails| nretry[Retry once on CPU with the same file]
  nload -->|ok| run
  nretry --> run
  unified -->|no| gpuonly{Apple-GPU-only model<br/>Granite, Phonon-2 or Redux, MLX build?}
  gpuonly -->|yes| gpusel[Select MLX / Metal artifacts,<br/>any declared language]
  gpusel --> gload[Load on Metal]
  gload -->|AcceleratorInitialization| gerr[Error: no CPU version;<br/>never another model]
  gload -->|ok| run
  gpuonly -->|no| mlxq{English, built-in, MLX-supported model,<br/>MLX runtime hashes + device probe OK?}
  mlxq -->|yes| mlxsel[Select MLX / Metal artifacts]
  mlxq -->|no| cpu[Select CPU / backend-less artifacts]
  mlxsel --> files{MLX files present?}
  files -->|no, CPU files present| cpu
  files -->|yes| load[Load on Metal]
  load -->|AcceleratorInitialization| fallback[InitializationFallback:<br/>reload once on CPU, remember failed key]
  load -->|ok| run[Resident model]
  cpu --> run
  fallback --> run
```

The llama.cpp paths (LFM and the writer) choose the GPU with the most free
memory, or CPU. Only under Auto do they retry a failed GPU initialization on
CPU. Nemotron, and Parakeet v3 under the Automatic engine, pick Metal on Apple
Silicon under Auto, CUDA when an NVIDIA GPU is visible, and CPU otherwise; both
also retry on CPU, with the same GGUF file
([ADR-0024](adrs/0024-unified-gguf-speech-engine.md)). Cancellation,
verification and decode errors never trigger a fallback.

## 5. Live pipeline

All live models share one `StreamingSession`. Nemotron, and MLX Qwen with the
Metal backend, run native streams on the worker. Every other model uses
buffered previews (ADR-0023). The completed-audio final stays authoritative.

```mermaid
flowchart TB
  mic[Capture chunks, native rate] --> sess[StreamingSession.push]
  sess -->|native model| nat[run_stream on worker:<br/>inline frontend session -> native stream push]
  nat -->|Nemotron final| fin1[StreamingFinal::Transcript]
  nat -->|MLX Qwen| keep[retain enhanced PCM -> EnhancedStore]
  sess -->|buffered model| lf[vesper-live-frontend thread:<br/>own Silero + DPDFNet8 session]
  lf -->|enhanced 16 kHz| mb[Preview mailbox]
  lf -->|Silero end of speech| pause[mark_pause]
  pause --> mb
  lf -->|whole enhanced recording| es[(EnhancedStore)]
  subgraph Mailbox["Mailbox (live_preview.rs)"]
    open[Open buffer]
    open -->|pause after >= 2 s, or quietest point 4-8 s| pend[Closed segments FIFO <= 2]
    pend -. oldest dropped when decoder lags .-> drop[previewSegmentsDropped]
  end
  mb --> open
  pend -->|decode once| com[committedText, append only]
  open -->|re-decode tail >= 2 s| prov[provisionalText, replaced]
  com & prov --> ui[StreamingTranscript events]
  keep --> es
  es -->|same operation, settings, length +-50 ms| final[Completed-audio final decode]
```

Not adopted: decoding only the tail at Stop, reusing committed segments
(Phase B). It failed the WER parity gate; see ADR-0023 and #155.

## 6. Audio frontend

Desktop recordings always enable Silero v6.2 voice activity detection and
DPDFNet8 denoising (`settings.rs` `managed_audio`). VAD is advisory. It chooses
live commit boundaries and records evidence, and it never deletes audio.

```mermaid
flowchart LR
  raw[Interleaved device PCM] --> cm[ChannelMixer:<br/>average channels within 30 dB of loudest]
  cm --> rs[StreamingResampler -> 16 kHz]
  rs --> vad[Silero v6.2, 1 thread:<br/>speech intervals, min silence 0.25 s]
  rs --> dn[DPDFNet8, 1 thread:<br/>~0.19x real time on M5 Pro CPU]
  dn --> out[Enhanced 16 kHz PCM]
  vad -. pause marks (live) .-> live[Preview mailbox]
  vad -. evidence .-> ev[(audioFrontend evidence)]
  out --> live
  out --> store[(EnhancedStore for the final)]
  out --> asr[Completed-audio ASR when no live session matched]
  dn -. failure .-> orig[Original audio fallback + warning]
```

- **Ownership:** the worker's `FrontendCache` serves imports and fallbacks, and
  the live frontend thread owns a per-recording copy. While a buffered
  recording runs, the worker releases its copy.
- **Reuse rule:** the final reuses the live-enhanced recording only for the
  same operation id, the same frontend settings and a length within 50 ms. A
  failed live session means the final uses the original audio with a warning,
  and the frontend is not run again.

## 7. Writer (Qwen3.5-0.8B)

The writer runs after the final ASR transcript, through llama.cpp. Weights stay
resident, and every pass gets a fresh context.

```mermaid
flowchart TB
  rawt[Raw transcript] --> mode{Profile automatic_writing?}
  mode -->|yes| aw[Grammar-constrained JSON: form, text, terms]
  mode -->|no| prompt[Non-thinking prompt:<br/>preset instruction + up to 16 spellings + 2 examples]
  prompt --> ctx[Fresh 4096-token context]
  ctx --> pc{Prefix cache:<br/>no personal examples, built-in template?}
  pc -->|hit| restore[Restore system-prefix state<br/>max 2 snapshots, 64 MiB]
  pc -->|miss| prefill1[Prefill prefix, then snapshot]
  restore --> prefill2[Prefill transcript part]
  prefill1 --> prefill2
  prefill2 --> gen[Greedy generation, one token per decode]
  gen --> lim{Limits}
  lim -->|EOG or formatting complete| fid[fidelity::validate:<br/>words, numbers, negation]
  lim -->|output tokens or 5 s + 50 ms/token| keepraw[Keep raw transcript + message]
  fid -->|pass| polished[Polished text]
  fid -->|fail; Minimal may retry once| keepraw
  aw --> fid
```

The prompt is always split at the prefix boundary, whether or not the cache is
used, so cached and uncached greedy outputs are identical. Measured on 52 public
transcripts: median CPU rewrite 412 → 201 ms. Not adopted: prompt-lookup
speculative decoding with llama.cpp recurrent rollback, which was slower end to
end (#141, `inference-performance.md`).

## 8. Speaker diarization

Diarization uses Nemotron-3-Diarization through a namespaced NeMo-Speech.cpp
runtime that ships in the app bundle. Live recordings run it after the text is
delivered. Imported files run it before the result is shown.

```mermaid
flowchart LR
  bundle[App bundle Resources/diarization-runtime/rev] -->|startup pre-check, non-fatal| inst[model root nemotron-diarization-runtime/rev]
  w[(Nemotron-3-Diarization q8_0 GGUF, 107 MB)] --> ver
  inst --> ver[verify: full sha256 of GGUF + 7 dylibs, ~38 ms]
  ver --> load[dlopen RTLD_LOCAL + Diarizer::load<br/>Metal; Auto retries on CPU]
  load --> run[diarize original 16 kHz audio<br/>~0.56 s per 45 s on Metal]
  run --> tl[SpeakerTimeline -> complete_speaker_timeline]
  load -. "per recording, ~14-55 ms; first-ever Metal shader compile ~7 s" .-> note[not resident by design]
```

## 9. Model installation and verification

```mermaid
flowchart TB
  cat[(models/artifacts.json + mlx-artifacts.json + diarization artifact)] --> sel[selected_exact: model, backend, platform]
  sel --> have{File present and hash ok?}
  have -->|yes| skip[Skip]
  have -->|no| hub{HF hub cache has it?}
  hub -->|yes| reuse[Copy from hub cache]
  hub -->|no| dl[Download to temp file:<br/>stream SHA-256 + size check]
  dl --> pub[Atomic publish into model root]
  reuse --> pub
  pub --> rt{Runtime archive?}
  rt -->|NeMo-Speech tar.gz / zip| ext[Extract to staging, check binary + C ABI lib,<br/>atomic swap into nemo-speech-runtime/0.1.0]
  rt -->|no| done[Ready]
  ext --> done
  done --> inspect[inspect: size only, for UI polling]
  done --> verify[verify_asr / verify_rewriter / verify_frontend:<br/>full hash before every native load]
```

The MLX runtime (`libVesperMLX.dylib`, `mlx.metallib`, tokenizer tool) is never
downloaded. It ships inside the app bundle, and its build provenance hashes are
checked at build time. Resident reuse checks file identity, meaning metadata
plus the recorded hash, and every new load re-verifies the full hash.

## 10. Threads, memory and failure policy

| Thread | Work | Native threads |
| --- | --- | --- |
| CPAL callback | Lock-free ring write only | — |
| `vesper-live-audio` | Forwards capture chunks to the session | — |
| `vesper-live-frontend` | Silero + DPDFNet8 per buffered recording | ONNX Runtime 1 each |
| `vesper-inference` | All model work, serially | sherpa-onnx and llama.cpp up to clamp(cores − 2, 1, 8); NeMo and MLX use their runtime defaults |
| Caller and Tauri threads | Orchestration, persistence, paste | — |

```mermaid
flowchart LR
  err[Native error] --> drop[Drop that resident handle]
  drop --> next[Next request: full verify + reload]
  cancel[Cancel flag] --> checks[Checked before and after native calls]
  checks --> discard[Discard partial output]
  press[Memory pressure] --> rel[Release idle writer before recording;<br/>back off previews]
  fe[Frontend failure] --> origa[Original audio + warning]
  wr[Writer failure / time limit / fidelity] --> rawk[Raw transcript kept]
  di[Diarization failure] --> textk[Transcript kept + warning]
```

Not yet qualified: a shared thread budget across runtimes, an Apple Neural
Engine path (proposal #143, not approved), and CUDA, Vulkan and ROCm on
non-Apple hardware beyond the documented smoke checks.
