<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import {
    microphoneAccess,
    openPrivacySettings,
    startMicrophoneCheck,
    stopMicrophoneCheck,
    type MicrophoneAccess,
  } from '../api/onboarding';
  import { getRuntimeStatus, listAudioDevices, type AudioDevice } from '../api/runtime';
  import { ReedMotion, followLevel, waveformEnergy } from '../voiceWaveform';
  import { SETUP_LANGUAGES, type SetupLanguage } from '../firstRun';

  let {
    language,
    preview = false,
    onLanguage,
    onReady,
  }: {
    language: SetupLanguage;
    /** Browser preview: the microphone is simulated. */
    preview?: boolean;
    onLanguage: (language: SetupLanguage) => void;
    onReady: (ready: boolean) => void;
  } = $props();

  let access = $state<MicrophoneAccess | 'checking'>('checking');
  let devices = $state<AudioDevice[]>([]);
  let deviceId = $state('');
  let heard = $state(false);
  let listening = $state(false);
  let error = $state<string | null>(null);
  let path = $state('');

  const reed = new ReedMotion(true);
  let level = 0;
  let raw = 0;
  let sampler: ReturnType<typeof setInterval> | undefined;
  let heardMs = 0;
  let frame = 0;
  let poll: ReturnType<typeof setInterval> | undefined;
  let restart: ReturnType<typeof setTimeout> | undefined;
  let disposed = false;

  $effect(() => onReady(access === 'allowed' && heard));

  async function readAccess(request = false) {
    if (preview) return (access = 'allowed');
    try {
      access = await microphoneAccess(request);
    } catch {
      access = 'allowed';
    }
    return access;
  }

  async function allow() {
    error = null;
    await readAccess(true);
    // macOS shows its prompt; follow it until the person answers.
    clearInterval(poll);
    poll = setInterval(async () => {
      const next = await readAccess();
      if (next !== 'asking' && next !== 'not_asked') {
        clearInterval(poll);
        if (next === 'allowed') void listen();
      }
    }, 500);
  }

  async function watchSettings() {
    // After a denial, notice the moment the person turns access on.
    clearInterval(poll);
    poll = setInterval(async () => {
      if ((await readAccess()) === 'allowed') {
        clearInterval(poll);
        void listen();
      }
    }, 1500);
  }

  async function loadDevices() {
    if (preview) {
      devices = [{ id: 'preview', name: 'Built-in microphone', isDefault: true }];
      deviceId = 'preview';
      return;
    }
    try {
      devices = await listAudioDevices();
      deviceId = devices.find((device) => device.isDefault)?.id ?? devices[0]?.id ?? '';
    } catch {
      devices = [];
    }
  }

  async function listen() {
    if (disposed || listening) return;
    error = null;
    await loadDevices();
    try {
      if (!preview) await startMicrophoneCheck(deviceId || null);
      listening = true;
    } catch (cause) {
      error = `Vesper could not open the microphone. ${String(cause)}`;
      return;
    }
    // The check stops itself after 30 seconds; start a fresh one before then.
    clearTimeout(restart);
    restart = setTimeout(() => void relisten(), 25_000);
    let last = performance.now();
    const started = last;
    // Read the level about 14 times a second; draw every frame from the latest.
    clearInterval(sampler);
    if (!preview)
      sampler = setInterval(async () => {
        try {
          raw = (await getRuntimeStatus()).audioLevel;
        } catch {
          raw = 0;
        }
      }, 70);
    const draw = (now: number) => {
      if (disposed || !listening) return;
      const elapsed = now - last;
      last = now;
      if (preview) {
        const t = (now - started) / 1000;
        raw = t > 1 ? 0.05 * Math.max(0, Math.sin(t * 1.3 - 0.6)) : 0;
      }
      level = followLevel(level, waveformEnergy(raw), elapsed);
      path = reed.path(Math.max(0.04, level), 3.4, elapsed);
      if (!heard) {
        heardMs = level > 0.22 ? heardMs + elapsed : Math.max(0, heardMs - elapsed);
        if (heardMs > 600) heard = true;
      }
      frame = requestAnimationFrame(draw);
    };
    frame = requestAnimationFrame(draw);
  }

  async function stop() {
    cancelAnimationFrame(frame);
    clearTimeout(restart);
    clearInterval(sampler);
    if (listening && !preview) await stopMicrophoneCheck().catch(() => {});
    listening = false;
  }

  async function relisten() {
    await stop();
    void listen();
  }

  onMount(async () => {
    const state = await readAccess();
    if (state === 'allowed') void listen();
  });
  onDestroy(() => {
    disposed = true;
    clearInterval(poll);
    void stop();
  });
</script>

<div class="card">
  <section class="sec">
    <p class="lab"><Icon name="globe" />Transcription language</p>
    <div class="langs" role="radiogroup" aria-label="Transcription language">
      {#each SETUP_LANGUAGES as option (option.value)}
        <button
          type="button"
          class="lang"
          role="radio"
          aria-checked={language === option.value}
          onclick={() => onLanguage(option.value)}
          >{#if language === option.value}<Icon name="check" />{/if}{option.label}</button
        >
      {/each}
    </div>
    <p class="hint">
      {language === 'hi'
        ? 'Hindi needs a different speech model. Vesper picks it for you next.'
        : 'You can add more languages later in Models.'}
    </p>
  </section>
  <section class="sec">
    <p class="lab"><Icon name="mic" />Microphone</p>
    {#if access === 'checking'}
      <p class="hint">Checking microphone access…</p>
    {:else if access === 'not_asked' || access === 'asking'}
      <div class="note">
        <Icon name="mic" />
        <div>
          <b>Vesper needs your microphone.</b> macOS will ask once. Choose <b>Allow</b>.
          <div class="acts">
            <button type="button" class="primary" onclick={allow} disabled={access === 'asking'}
              >{access === 'asking' ? 'Waiting for macOS…' : 'Allow microphone'}</button
            >
          </div>
        </div>
      </div>
    {:else if access === 'denied' || access === 'restricted'}
      <div class="note bad" role="alert">
        <Icon name="warning" />
        <div>
          <b>Microphone is blocked.</b>
          {access === 'restricted'
            ? 'This Mac restricts microphone access. Ask whoever manages it to allow Vesper.'
            : 'Turn Vesper on under System Settings → Privacy & Security → Microphone. Vesper notices when you do.'}
          {#if access === 'denied'}<div class="acts">
              <button
                type="button"
                class="primary"
                onclick={() => {
                  void openPrivacySettings('microphone').catch(() => {});
                  void watchSettings();
                }}>Open System Settings</button
              >
            </div>{/if}
        </div>
      </div>
    {:else}
      {#if devices.length > 1}
        <select class="device" bind:value={deviceId} onchange={relisten} aria-label="Microphone">
          {#each devices as device (device.id)}<option value={device.id}>{device.name}</option
            >{/each}
        </select>
      {:else if devices[0]}<p class="device-name">{devices[0].name}</p>{/if}
      <div class="meter" class:heard>
        <svg viewBox="0 0 320 100" preserveAspectRatio="none" aria-hidden="true"
          ><path d={path} /></svg
        >
        <p class="cap" role="status">
          <b>{heard ? 'Heard you' : 'Say a few words'}</b>
          <span>{heard ? 'Your microphone works.' : 'Try “testing, one two three”.'}</span>
        </p>
      </div>
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </section>
</div>

<style>
  .card {
    border: 1px solid var(--color-line);
    border-radius: var(--radius-lg);
    background: var(--color-raised);
    overflow: hidden;
  }
  .sec {
    padding: 22px 26px;
  }
  .sec + .sec {
    border-top: 1px solid var(--color-line);
  }
  .lab {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    margin: 0 0 10px;
  }
  .lab :global(svg) {
    width: 16px;
    height: 16px;
    color: var(--color-accent);
  }
  .langs {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .lang {
    min-height: 40px;
    padding: 8px 16px;
    border-radius: var(--radius-pill);
    border: 1px solid var(--color-control-edge);
    background: transparent;
  }
  .lang[aria-checked='true'] {
    background: var(--color-tint);
    border-color: var(--color-accent);
    color: var(--color-accent-light);
  }
  .lang :global(svg) {
    width: 14px;
    height: 14px;
  }
  .hint {
    font-size: 13px;
    color: var(--color-muted);
    margin: 10px 0 0;
  }
  .note {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    padding: 14px 16px;
    border: 1px solid var(--color-line);
    border-radius: 14px;
    background: var(--color-bg);
    font-size: 14px;
    color: var(--color-muted);
  }
  .note b {
    color: var(--color-ink);
  }
  .note > :global(svg) {
    width: 18px;
    height: 18px;
    flex: none;
    margin-top: 2px;
    color: var(--color-accent);
  }
  .note.bad {
    border-color: var(--color-error);
    background: var(--color-error-tint);
    color: var(--color-ink);
  }
  .note.bad > :global(svg) {
    color: var(--color-error);
  }
  .acts {
    margin-top: 10px;
  }
  .device {
    width: 100%;
    margin-bottom: 12px;
  }
  .device-name {
    margin: 0 0 12px;
  }
  .meter {
    display: flex;
    align-items: center;
    gap: 22px;
    min-height: 96px;
  }
  .meter svg {
    flex: 1;
    min-width: 0;
    height: 84px;
    color: var(--color-accent);
    stroke: currentColor;
    stroke-width: 3;
    stroke-linecap: round;
    fill: none;
  }
  .cap {
    flex: none;
    width: 190px;
    margin: 0;
    font-size: 13px;
    color: var(--color-muted);
  }
  .cap b {
    display: block;
    font: 700 20px/1.2 var(--font-display);
    letter-spacing: -0.02em;
    color: var(--color-ink);
    margin-bottom: 2px;
  }
  .meter.heard .cap b {
    color: var(--color-accent);
  }
  .error {
    margin: 10px 0 0;
    color: var(--color-error);
    font-size: 13px;
  }
  @container first-run (max-width: 760px) {
    .sec {
      padding: 18px;
    }
    .meter {
      flex-direction: column;
      align-items: stretch;
      gap: 8px;
    }
    .cap {
      width: auto;
    }
  }
</style>
