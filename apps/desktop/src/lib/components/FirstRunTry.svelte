<script lang="ts">
  import { onDestroy } from 'svelte';
  import { getRuntimeStatus, startRecording, stopRecording } from '../api/runtime';
  import { recordingShortcuts, type AppConfig } from '../config';
  import { shortcutLabel } from '../recording-shortcuts';
  import { ReedMotion, followLevel, waveformEnergy } from '../voiceWaveform';

  let {
    settings,
    install,
    percent,
    onRetry,
    onDone,
  }: {
    settings: AppConfig;
    install: 'idle' | 'running' | 'done' | 'error';
    percent: number;
    onRetry: () => void;
    onDone: (done: boolean) => void;
  } = $props();

  let text = $state('');
  let phase = $state<'idle' | 'listening' | 'transcribing'>('idle');
  let error = $state<string | null>(null);
  let path = $state('');
  let box: HTMLTextAreaElement | undefined = $state();
  const reed = new ReedMotion(false);
  let level = 0;
  let raw = 0;
  let frame = 0;
  let sampler: ReturnType<typeof setInterval> | undefined;
  let holding = false;
  const ready = $derived(install === 'done');
  const hold = $derived(shortcutLabel(recordingShortcuts(settings).hold));

  // Any text that lands in the box counts: the button below, or the global
  // shortcut pasting into it when typing into apps is on.
  $effect(() => onDone(text.trim().length > 1));

  function animate() {
    let last = performance.now();
    const draw = (now: number) => {
      const elapsed = now - last;
      last = now;
      level = followLevel(level, phase === 'listening' ? waveformEnergy(raw) : 0, elapsed);
      path = reed.path(Math.max(0.03, level), 3.4, elapsed);
      if (phase !== 'idle' || level > 0.01) frame = requestAnimationFrame(draw);
    };
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(draw);
  }

  async function begin() {
    if (!ready || holding || phase === 'transcribing') return;
    holding = true;
    error = null;
    try {
      await startRecording(null);
    } catch (cause) {
      holding = false;
      error = `Vesper could not start listening. ${String(cause)}`;
      return;
    }
    phase = 'listening';
    clearInterval(sampler);
    sampler = setInterval(async () => {
      try {
        raw = (await getRuntimeStatus()).audioLevel;
      } catch {
        raw = 0;
      }
    }, 70);
    animate();
    // A release that arrived while starting still finishes the recording.
    if (!holding) void end();
  }

  async function end() {
    if (phase !== 'listening') {
      holding = false;
      return;
    }
    holding = false;
    clearInterval(sampler);
    raw = 0;
    phase = 'transcribing';
    try {
      const result = await stopRecording();
      const words = (
        result.personalizedTranscript ??
        result.cleanedTranscript ??
        result.rawTranscript ??
        ''
      ).trim();
      if (!words) error = 'Vesper did not hear any words. Hold the button and try again.';
      else text = text.trim() ? `${text.trim()} ${words}` : words;
    } catch (cause) {
      error = `Vesper could not finish that recording. ${String(cause)}`;
    }
    phase = 'idle';
    box?.focus();
  }

  onDestroy(() => {
    cancelAnimationFrame(frame);
    clearInterval(sampler);
  });
</script>

<div class="card">
  <section class="sec">
    <textarea
      bind:this={box}
      bind:value={text}
      rows="3"
      placeholder="Your words will appear here…"
      aria-label="Practice text"
      disabled={!ready}></textarea>
    <div class="live">
      <svg viewBox="0 0 320 100" preserveAspectRatio="none" aria-hidden="true"
        ><path d={path} /></svg
      >
      <p class="state" role="status">
        {#if phase === 'listening'}<b>Listening</b>{:else if phase === 'transcribing'}<b
            >Transcribing…</b
          >{:else if text.trim()}<b>It works</b>{:else}Waiting for you{/if}
      </p>
    </div>
    <div class="hold-row">
      <button
        type="button"
        class="hold"
        class:on={phase === 'listening'}
        disabled={!ready || phase === 'transcribing'}
        onpointerdown={(event) => {
          event.currentTarget.setPointerCapture(event.pointerId);
          void begin();
        }}
        onpointerup={() => void end()}
        onpointercancel={() => void end()}
        onkeydown={(event) => {
          if ((event.key === ' ' || event.key === 'Enter') && !event.repeat) {
            event.preventDefault();
            void begin();
          }
        }}
        onkeyup={(event) => {
          if (event.key === ' ' || event.key === 'Enter') {
            event.preventDefault();
            void end();
          }
        }}>Hold to talk</button
      >
      <span>or click the box, then hold <kbd>{hold}</kbd></span>
    </div>
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
    border: 1px solid var(--color-line);
    border-radius: var(--radius-lg);
    background: var(--color-raised);
    overflow: hidden;
  }
  .sec {
    padding: 22px 26px;
  }
  textarea {
    width: 100%;
    min-height: 120px;
    resize: none;
    font-size: 20px;
    line-height: 1.45;
    padding: 18px 20px;
    border-radius: 16px;
  }
  .live {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-top: 16px;
    min-height: 60px;
  }
  .live svg {
    flex: 1;
    min-width: 0;
    height: 60px;
    color: var(--color-accent);
    stroke: currentColor;
    stroke-width: 3;
    stroke-linecap: round;
    fill: none;
  }
  .state {
    min-width: 128px;
    margin: 0;
    font-size: 14px;
    color: var(--color-muted);
  }
  .state b {
    color: var(--color-ink);
    font-weight: 600;
  }
  .hold-row {
    display: flex;
    align-items: center;
    gap: 18px;
    flex-wrap: wrap;
    margin-top: 18px;
    color: var(--color-muted);
    font-size: 14px;
  }
  .hold {
    min-height: 52px;
    padding: 10px 24px;
    border-radius: 14px;
    user-select: none;
    touch-action: none;
  }
  .hold.on,
  .hold.on:hover:enabled {
    background: var(--color-accent);
    border-color: var(--color-accent);
    color: var(--color-on-accent);
  }
  kbd {
    font: 600 13px var(--font-body);
    padding: 3px 9px;
    border-radius: 8px;
    border: 1px solid var(--color-control-edge);
    border-bottom-width: 2px;
    color: var(--color-ink);
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
    .sec {
      padding: 18px;
    }
    .hold-row {
      flex-direction: column;
      align-items: stretch;
    }
  }
</style>
