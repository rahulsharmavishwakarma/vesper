<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import AudioIndicator from './AudioIndicator.svelte';
  import Icon from './Icon.svelte';
  import {
    getLatestResult,
    getRuntimeStatus,
    startRecording,
    stopRecording,
    type PipelineResult,
    type StreamingTranscript,
  } from '../api/runtime';
  import { recordingShortcuts, type AppConfig } from '../config';
  import { liveTranscriptFromStatus, mergeLiveTranscript } from '../liveTranscript';
  import { shortcutLabel } from '../recording-shortcuts';

  let {
    settings,
    install,
    percent,
    preview = false,
    onRetry,
    onDone,
  }: {
    settings: AppConfig;
    install: 'idle' | 'running' | 'done' | 'error';
    percent: number;
    /** Browser preview: no native events. */
    preview?: boolean;
    onRetry: () => void;
    onDone: (done: boolean) => void;
  } = $props();

  let text = $state('');
  let phase = $state<'idle' | 'listening' | 'transcribing'>('idle');
  /** A quick click latches recording on; a held press records until release. */
  let latched = $state(false);
  /** The global shortcut started this capture, not the button. */
  let external = $state(false);
  let error = $state<string | null>(null);
  let live = $state<StreamingTranscript | null>(null);
  let box: HTMLTextAreaElement | undefined = $state();
  let pressing = false;
  let pressedAt = 0;
  let starting: Promise<void> | null = null;
  let elapsed = $state(0);
  let clock: ReturnType<typeof setInterval> | undefined;
  let poll: ReturnType<typeof setTimeout> | undefined;
  let unlisten: (() => void) | undefined;
  let disposed = false;
  let operation: string | null = null;
  const HOLD_MS = 350;

  const ready = $derived(install === 'done');
  const hold = $derived(shortcutLabel(recordingShortcuts(settings).hold));
  const timer = $derived(
    `${String(Math.floor(elapsed / 60)).padStart(2, '0')}:${String(elapsed % 60).padStart(2, '0')}`,
  );
  const status = $derived(
    phase === 'listening'
      ? 'Listening…'
      : phase === 'transcribing'
        ? 'Writing it down…'
        : text.trim()
          ? 'It works'
          : 'Ready to try',
  );
  const liveWords = $derived(live ? `${live.committedText}${live.provisionalText}`.trim() : '');

  // Any text that lands in the box counts: the button, or the global shortcut.
  $effect(() => onDone(text.trim().length > 1));

  const wordsOf = (result: PipelineResult) =>
    (
      result.personalizedTranscript ??
      result.cleanedTranscript ??
      result.rawTranscript ??
      ''
    ).trim();
  function append(words: string) {
    if (!words || text.trim().endsWith(words)) return;
    text = text.trim() ? `${text.trim()} ${words}` : words;
  }

  function startClock() {
    elapsed = 0;
    clearInterval(clock);
    clock = setInterval(() => (elapsed += 1), 1000);
  }

  async function start() {
    if (!ready || phase !== 'idle') return;
    error = null;
    live = null;
    starting = (async () => {
      try {
        operation = await startRecording(null);
        phase = 'listening';
        startClock();
      } catch (cause) {
        error = `Vesper could not start listening. ${String(cause)}`;
      }
    })();
    await starting;
    starting = null;
  }

  async function stop() {
    if (starting) await starting;
    if (phase !== 'listening' || external) return;
    latched = false;
    clearInterval(clock);
    phase = 'transcribing';
    try {
      const words = wordsOf(await stopRecording());
      if (!words) error = 'Vesper did not hear any words. Try again and speak a little louder.';
      append(words);
    } catch (cause) {
      error = `Vesper could not finish that recording. ${String(cause)}`;
    }
    phase = 'idle';
    live = null;
    box?.focus();
  }

  // Press: start on the way down. Release: a short press latches recording
  // on (click to start, click again to stop); a longer press is push-to-talk.
  function press() {
    if (pressing) return;
    pressing = true;
    if (phase === 'listening' && !external) return;
    pressedAt = performance.now();
    latched = false;
    void start();
  }
  function release() {
    if (!pressing) return;
    pressing = false;
    if (phase === 'listening' && pressedAt === 0) return void stop();
    if (performance.now() - pressedAt < HOLD_MS) {
      latched = true;
      pressedAt = 0;
      return;
    }
    pressedAt = 0;
    void stop();
  }

  // Follow the recorder like Dictate does: live words while speaking, and a
  // capture started by the global shortcut shows the same states.
  async function follow() {
    if (disposed) return;
    try {
      const runtime = await getRuntimeStatus();
      if (disposed) return;
      const recording = runtime.busy && runtime.stage === 'recording';
      if (recording && phase === 'idle') {
        external = true;
        operation = runtime.operationId;
        phase = 'listening';
        startClock();
      }
      if (external && phase !== 'idle' && !runtime.busy) {
        clearInterval(clock);
        external = false;
        phase = 'idle';
        live = null;
        // Typing into apps pastes into the focused box; otherwise take the result.
        setTimeout(async () => {
          try {
            const latest = await getLatestResult();
            if (latest) append(wordsOf(latest));
          } catch {
            // The paste, if any, already landed.
          }
        }, 600);
      } else if (external && phase === 'listening' && !recording && runtime.busy) {
        clearInterval(clock);
        phase = 'transcribing';
      }
      if (runtime.busy && runtime.operationId === operation)
        live = liveTranscriptFromStatus(live, runtime);
    } catch {
      // Status is best-effort; the button path still finishes.
    }
    poll = setTimeout(follow, phase === 'idle' ? 400 : 150);
  }

  onMount(() => {
    void follow();
    if (!preview)
      void listen<StreamingTranscript>('transcription-partial', (event) => {
        live = mergeLiveTranscript(live, event.payload, operation, null);
      })
        .then((dispose) => {
          if (disposed) dispose();
          else unlisten = dispose;
        })
        .catch(() => {});
  });
  onDestroy(() => {
    disposed = true;
    clearInterval(clock);
    clearTimeout(poll);
    unlisten?.();
  });
</script>

<div class="card studio-recorder" class:is-recording={phase === 'listening'}>
  <div class="dictate-body">
    <div class="dictate-controls">
      <div>
        <div class="recorder-heading">
          <Icon name="mic" />
          <h2 class="recorder-label"><span role="status">{status}</span></h2>
        </div>
        <p class="dictate-hint">
          Click Record to start and again to stop, or hold it while you speak. In any app, hold
          <kbd>{hold}</kbd>.
        </p>
      </div>
      <div class="capture-actions">
        <button
          type="button"
          class="primary record-button"
          class:capturing={phase === 'listening'}
          disabled={!ready || phase === 'transcribing' || external}
          onpointerdown={(event) => {
            event.currentTarget.setPointerCapture(event.pointerId);
            press();
          }}
          onpointerup={release}
          onpointercancel={release}
          onkeydown={(event) => {
            if ((event.key === ' ' || event.key === 'Enter') && !event.repeat) {
              event.preventDefault();
              press();
            }
          }}
          onkeyup={(event) => {
            if (event.key === ' ' || event.key === 'Enter') {
              event.preventDefault();
              release();
            }
          }}
          ><Icon name="record" />{phase === 'listening'
            ? external
              ? 'Recording from your shortcut'
              : latched
                ? 'Stop and transcribe'
                : 'Release to finish'
            : phase === 'transcribing'
              ? 'Finishing…'
              : 'Record'}</button
        >
      </div>
    </div>
    <div class="dictate-stage">
      <div class="stage-header">
        <span
          class="recording-timer"
          class:live={phase === 'listening'}
          aria-label="Recording duration">{timer}</span
        >
        <span>Practice</span>
      </div>
      <div class="recorder-signal">
        <AudioIndicator
          recording={phase === 'listening'}
          processing={phase === 'transcribing'}
          complete={phase === 'idle' && Boolean(text.trim())}
          desktop
        />
      </div>
    </div>
  </div>
  <section class="practice">
    {#if liveWords && phase !== 'idle'}<p class="live-words" aria-live="polite">
        <span class="label"
          >{live?.isFinal || phase === 'transcribing' ? 'Finalizing' : 'Live'}</span
        ><span>{live?.committedText}</span><span class="provisional">{live?.provisionalText}</span>
      </p>{/if}
    <textarea
      bind:this={box}
      bind:value={text}
      rows="3"
      placeholder="Your words will appear here…"
      aria-label="Practice text"
      disabled={!ready}></textarea>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </section>
  {#if !ready}
    <div class="lock">
      {#if install === 'error'}
        <b>The download stopped.</b>
        <p>Check your connection, then try again. The practice opens when your models are ready.</p>
        <button type="button" class="primary" onclick={onRetry}>Try again</button>
      {:else}
        <b>Finishing your models</b>
        <div
          class="bar"
          role="progressbar"
          aria-label="Model download"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={percent}
        >
          <span style:transform={`scaleX(${percent / 100})`}></span>
        </div>
        <p>{percent}% · The practice opens as soon as they are ready.</p>
      {/if}
    </div>
  {/if}
</div>
<p class="hint">
  {settings.dictationOutput === 'paste'
    ? 'In everyday use the text goes into the app you’re in.'
    : 'Your words appear in Vesper, ready to copy.'}
</p>

<style>
  .card {
    position: relative;
  }
  .practice {
    padding: 20px 28px 24px;
    border-top: 1px solid var(--color-line);
  }
  textarea {
    width: 100%;
    min-height: 110px;
    resize: none;
    font-size: 18px;
    line-height: 1.5;
    padding: 16px 18px;
    border-radius: 14px;
    border: 1px solid var(--color-line);
    background: var(--color-bg);
    transition: border-color var(--duration-state) var(--ease-out);
  }
  textarea:focus-visible {
    outline: none;
    border-color: var(--color-accent);
  }
  kbd {
    font: 600 12.5px var(--font-body);
    padding: 2px 8px;
    border-radius: 8px;
    border: 1px solid var(--color-control-edge);
    border-bottom-width: 2px;
    color: var(--color-ink);
    white-space: nowrap;
  }
  /* The waveform sits in the middle of its column, under the timer. */
  .recorder-signal {
    align-items: center;
  }
  .live-words {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0 6px;
    margin: 0 0 12px;
    font-size: 18px;
    line-height: 1.5;
    color: var(--color-ink);
    animation: live-in 220ms var(--ease);
  }
  .live-words .label {
    margin-right: 4px;
    font-size: 12px;
    font-weight: 600;
    color: var(--color-accent);
  }
  .live-words .provisional {
    color: var(--color-muted);
  }
  @keyframes live-in {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .live-words {
      animation: none;
    }
  }
  .record-button {
    user-select: none;
    touch-action: none;
  }
  .error {
    margin: 12px 0 0;
    color: var(--color-error);
    font-size: 13px;
  }
  .lock {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 20px;
    text-align: center;
    background: color-mix(in srgb, var(--color-bg) 84%, transparent);
    backdrop-filter: blur(3px);
  }
  .lock b {
    font: 700 20px var(--font-display);
    letter-spacing: -0.02em;
  }
  .lock p {
    margin: 0;
    color: var(--color-muted);
    font-size: 14px;
  }
  .bar {
    width: min(320px, 80%);
    height: 5px;
    border-radius: 99px;
    background: var(--color-line);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--color-accent);
    transform-origin: left;
    transition: transform 180ms var(--ease);
  }
  .hint {
    margin: 14px 0 0;
    font-size: 13px;
    color: var(--color-muted);
  }
  @container first-run (max-width: 760px) {
    .dictate-body {
      grid-template-columns: 1fr;
      padding: 18px;
    }
    .practice {
      padding: 18px;
    }
  }
</style>
