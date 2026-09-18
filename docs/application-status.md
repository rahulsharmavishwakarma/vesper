# Local engine

Screenshots mentioned below are local validation captures excluded from source
control. The written results and reproducible test fixtures remain in the repository.

Open **Models** to see the always-open **Local engine** panel with the saved model choices, installation state,
current activity, desktop-process resources, and the current or last operation's
stage durations. Unsaved model selections do not change this panel. Use the model
controls below it to check files or install a model. Cleanup can be switched off;
the Dictate result's **Copy text** button copies cleanup output when available,
otherwise the raw transcript.

## What the readings mean

- **CPU activity** is this desktop backend process's share of total logical CPU
  capacity: `100 × Δprocess CPU milliseconds / Δmonotonic wall milliseconds /
  logical CPU count`. One fully busy core on an 18-core logical machine is about
  5.6%. The initial reading is unavailable until an interval has been measured.
- **Memory in use** is current resident RAM (RSS), displayed in GiB. It includes
  shared models, runtime, and allocator overhead. These are process readings, not
  individual model allocations. WebView, native helper processes, GPU
  memory, and other applications are excluded.
- **Installed** means the configured model files/runtime passed the existing
  installation check. It does not mean the model is resident. Backend preferences
  are labelled as preferences; the panel does not claim an observed accelerator.
- **Transcribe** wraps the existing ASR call, including its model verification and
  load work. A live recording instead shows **Finalize speech**, measuring the
  remaining wait after stop. If live recognition fails and the completed recording
  is retried, **Transcribe** includes both waits and identifies that fallback.
- **Clean up** wraps the existing rewrite call, including model loading. **Save**
  sums actual audio, transcript, provenance, and event writes. With collection off,
  unperformed writes remain unavailable. Durations exclude microphone time, file
  decoding outside those calls, and UI delay; they are not decoder benchmarks.

An operation can finish while retaining raw text and a cleanup warning. Its existing
Dictate warnings remain the source of that detail. Cancelled and failed operations
show partial durations; unstarted stages show a dash. Timings retain only the current
or last operation in memory and reset on the next operation or app exit.

## Sampling and implementation

The frontend keeps at most 60 samples covering 60 seconds. It requests a reading
approximately once per second while Models and the document are visible.
Navigation, visibility changes, and unmount stop future requests; an
already running native request can finish but its obsolete response is ignored.
Only one request and one timer are allowed. A stalled request is reported after
three seconds, including when the panel is reopened while an old request is pending.
Graphs break across missing readings rather than inventing zeros.

The native command runs on a blocking worker outside inference. It keeps one lazy
sampler, caches calls within 900 ms, and resets the CPU baseline after a long gap,
counter reset, or failed reading. No resource history, transcript, or raw process
metadata is written by the monitor. Timing guards use monotonic clocks and operation
IDs so older stage results cannot replace a newer operation.

The dependency is pinned to `sysinfo = 0.36.1`, with default features disabled and
only `system` enabled. This release declares Rust 1.75 compatibility, fitting the
project's declared 1.85 minimum. The sampler selects the current PID and requests
CPU/memory, with Linux task enumeration disabled. The library internally enumerates
OS process identifiers/metadata and, on macOS, transiently reads the current
process's argument/environment buffer. Those fields are not retained by the chosen
refresh configuration or exposed by our command; its output contains numeric
resource values, application activity, and stage timings only.

Primary source revision: sysinfo
[`fc31b411eea7b9983176399dc5be162786dec95b`](https://github.com/GuillaumeGomez/sysinfo/tree/fc31b411eea7b9983176399dc5be162786dec95b).
See the [manifest](https://github.com/GuillaumeGomez/sysinfo/blob/fc31b411eea7b9983176399dc5be162786dec95b/Cargo.toml),
[common API](https://github.com/GuillaumeGomez/sysinfo/blob/fc31b411eea7b9983176399dc5be162786dec95b/src/common/system.rs),
and [macOS process implementation](https://github.com/GuillaumeGomez/sysinfo/blob/fc31b411eea7b9983176399dc5be162786dec95b/src/unix/apple/macos/process.rs).
CPU is derived from accumulated time because this version's macOS `cpu_usage()`
can retain its previous value when the CPU delta is zero.

## Local validation

On macOS, six samples from the isolated native test process took 148–727 µs each
(median 512 µs), excluding IPC and rendering. After the initial allocation,
RSS differed from the immediately subsequent `ps` reading by at most 32 KiB.
The first CPU result was null; subsequent idle readings were 0–0.0056% of 18 logical
CPUs. `ps` showed an idle decaying CPU average. Its `%cpu` has a different time
window, so this is an idle consistency check, not interval equivalence under load.
See [raw observations](validation/application-status-resources.json).

To reproduce the explicit sampler observation, run:

```sh
source scripts/dev-env.sh
cargo test --locked -p transcribe-desktop resources::tests::measure_process_resources -- --ignored --exact --nocapture
```

During the six-second run, compare the printed PID with
`ps -p PID -o rss=,%cpu=,time=`; macOS RSS there is in KiB. This test loads no models
and opens no microphone. Linux/Windows behavior has not been executed locally;
denied or unsupported readings show unavailable values.

The installed public-fixture Nemotron → Qwen pipeline passed with nonempty cleanup,
unchanged archived audio, preserved provenance, and all three positive stage times.
One observed run reported 8.69 s transcribe, 7.49 s cleanup, and 40 ms save, including
loading and verification. This single debug run is not a model performance claim.

Browser tests use synthetic Tauri IPC and cover pause/resume, errors/retry,
first readings, missing models, cleanup off, recording/transcription/cleanup,
cancelled/failed durations, and disposal. Layout checks passed at 320, 375, 414,
768, and 1100 px. Local engine has a static heading and stays open; only
The explanatory About these readings disclosure can collapse. Download status and removal are integrated into the speech/writing model cards below this panel. Reduced motion
disables the working animation. Text contrast against the panel measured 6.75:1
for muted labels, 10.44:1 for model status, and 14.95:1 for readings.
The screenshot is explicitly a synthetic UI
fixture and is not telemetry or a performance measurement.
