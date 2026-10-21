<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import AudioIndicator from './AudioIndicator.svelte';
  import Icon from './Icon.svelte';
  import Select from './Select.svelte';
  import {
    microphoneAccess,
    openPrivacySettings,
    startMicrophoneCheck,
    stopMicrophoneCheck,
    type MicrophoneAccess,
  } from '../api/onboarding';
  import { listAudioDevices, type AudioDevice } from '../api/runtime';
  import { catalogModel } from '../config';
  import { waveformEnergy } from '../voiceWaveform';
  import { COHERE, SETUP_LANGUAGES, recommendedSpeechModel, type SetupLanguage } from '../firstRun';

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

  let heardMs = 0;
  let poll: ReturnType<typeof setInterval> | undefined;
  let restart: ReturnType<typeof setTimeout> | undefined;
  let disposed = false;

  $effect(() => onReady(access === 'allowed' && heard));

  const languageHint = $derived.by(() => {
    const model = recommendedSpeechModel(language);
    if (model === COHERE) return 'You can change this later on the Dictate page.';
    const name = catalogModel(model)?.name ?? 'a different speech model';
    return `${SETUP_LANGUAGES.find((option) => option.value === language)?.label ?? 'This language'} uses ${name}. Vesper picks it for you next.`;
  });

  /** About 0.6 s of clear speech counts as heard. Levels arrive every ~40 ms. */
  function hear(raw: number) {
    if (heard) return;
    heardMs = waveformEnergy(raw) > 0.22 ? heardMs + 40 : Math.max(0, heardMs - 40);
    if (heardMs >= 600) heard = true;
  }

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
  }

  async function stop() {
    clearTimeout(restart);
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
    <label class="field-label" for="setup-language"
      ><Icon name="globe" />Transcription language</label
    >
    <Select
      id="setup-language"
      value={language}
      options={SETUP_LANGUAGES}
      describedBy="setup-language-help"
      onChange={(value) => onLanguage(value)}
    />
    <p id="setup-language-help" class="field-hint">{languageHint}</p>
  </section>
  <section class="sec">
    {#if access === 'allowed'}<label class="field-label" for="setup-microphone"
        ><Icon name="mic" />Microphone</label
      >
    {:else}<p class="lab"><Icon name="mic" />Microphone</p>{/if}
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
      <Select
        id="setup-microphone"
        bind:value={deviceId}
        options={devices.length
          ? devices.map((device) => ({
              value: device.id,
              label: device.name,
              description: device.isDefault ? 'System default' : undefined,
            }))
          : [{ value: deviceId, label: 'Looking for microphones…' }]}
        disabled={!devices.length}
        onChange={() => void relisten()}
      />
      <div class="meter" class:heard>
        <AudioIndicator recording={listening} desktop onLevel={hear} />
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
  /* Matches label.field-label for the heading shown before access. */
  .lab {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 500;
    margin: 0 0 8px;
  }
  .lab :global(.icon) {
    width: 18px;
    height: 18px;
    flex: none;
    color: var(--color-accent);
  }
  .hint {
    font-size: 13.5px;
    color: var(--color-muted);
    margin: 0;
  }
  .sec :global(.select-trigger) {
    max-width: none;
  }
  .sec > .field-hint {
    margin-top: 8px;
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
  .meter {
    margin-top: 12px;
    display: flex;
    align-items: center;
    gap: 22px;
    min-height: 96px;
  }
  .meter > :global(.voice-indicator) {
    --voice-height: 96px;
    --voice-width: 440px;
    flex: 1;
    margin: 0;
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
    .meter > :global(.voice-indicator) {
      --voice-height: 64px;
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
