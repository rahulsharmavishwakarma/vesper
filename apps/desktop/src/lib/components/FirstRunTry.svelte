<script lang="ts">
  import { onDestroy } from 'svelte';
  import AudioIndicator from './AudioIndicator.svelte';
  import Icon from './Icon.svelte';
  import { startRecording, stopRecording } from '../api/runtime';
  import { recordingShortcuts, type AppConfig } from '../config';
  import { shortcutLabel } from '../recording-shortcuts';

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
  let box: HTMLTextAreaElement | undefined = $state();
  let holding = false;
  let elapsed = $state(0);
  let clock: ReturnType<typeof setInterval> | undefined;
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
  onDestroy(() => clearInterval(clock));
  const ready = $derived(install === 'done');
  const hold = $derived(shortcutLabel(recordingShortcuts(settings).hold));

  // Any text that lands in the box counts: the button below, or the global
  // shortcut pasting into it when typing into apps is on.
  $effect(() => onDone(text.trim().length > 1));

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
    elapsed = 0;
    clearInterval(clock);
    clock = setInterval(() => (elapsed += 1), 1000);
    // A release that arrived while starting still finishes the recording.
    if (!holding) void end();
  }

  async function end() {
    if (phase !== 'listening') {
      holding = false;
      return;
    }
    holding = false;
    clearInterval(clock);
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
          Hold Record and say the sentence, or click the box below and hold <kbd>{hold}</kbd>.
        </p>
      </div>
      <div class="capture-actions">
        <button
          type="button"
          class="primary record-button"
          class:capturing={phase === 'listening'}
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
          }}
          ><Icon name="record" />{phase === 'listening'
            ? 'Release to finish'
            : phase === 'transcribing'
              ? 'Finishing…'
              : 'Hold to record'}</button
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
        <span>Practice · nothing is saved</span>
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
