<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import { getDeviceProfile, type DeviceProfile } from '../api/onboarding';
  import { listModelInstallations, type ModelCatalogInstallation } from '../api/runtime';
  import { catalogModel, transcriptionLanguageOptions, type AppConfig } from '../config';
  import { downloadTarget, type DownloadTarget } from '../modelDownloads';
  import {
    SETUP_SPEECH_MODELS,
    formatBytes,
    recommendedSpeechModel,
    remainingBytes,
    spaceNeeded,
    type SetupLanguage,
  } from '../firstRun';

  let {
    settings,
    language,
    started,
    preview = false,
  }: {
    settings: AppConfig;
    language: SetupLanguage;
    /** The download has already been started from this step. */
    started: boolean;
    preview?: boolean;
  } = $props();

  let device = $state<DeviceProfile | null>(null);
  let catalog = $state<ModelCatalogInstallation[]>([]);
  // svelte-ignore state_referenced_locally
  let choice = $state(recommendedSpeechModel(language));
  // svelte-ignore state_referenced_locally
  let tidy = $state(settings.cleanupEnabled);
  let picking = $state(false);
  let online = $state(typeof navigator === 'undefined' ? true : navigator.onLine);

  const supports = (id: string) =>
    transcriptionLanguageOptions(
      { ...settings.models, asr: id as AppConfig['models']['asr'], customAsr: null },
      'auto',
    ).some((option) => option.value === language);
  const nextSettings = $derived<AppConfig>({
    ...settings,
    language,
    cleanupEnabled: tidy,
    models: { ...settings.models, asr: choice as AppConfig['models']['asr'], customAsr: null },
  });
  const speech = $derived(downloadTarget(nextSettings, 'asr'));
  const writer = $derived(downloadTarget(nextSettings, 'rewriter'));
  const targets = $derived<DownloadTarget[]>(tidy ? [speech, writer] : [speech]);
  const bytes = $derived(remainingBytes(catalog, targets));
  const need = $derived(spaceNeeded(bytes));
  const lowDisk = $derived(device?.freeBytes != null && bytes > 0 && device.freeBytes < need);
  const recommended = $derived(recommendedSpeechModel(language));

  /** What the step would download, for the shell to start. */
  export function plan(): { settings: AppConfig; targets: DownloadTarget[]; bytes: number } {
    return { settings: nextSettings, targets, bytes };
  }
  /** Why the download cannot start yet, if anything. */
  export function blocker(): string | null {
    if (started) return null;
    if (lowDisk) return 'Free up space to continue.';
    if (bytes > 0 && !online) return 'Connect to the internet to download.';
    return null;
  }

  onMount(() => {
    const update = () => (online = navigator.onLine);
    addEventListener('online', update);
    addEventListener('offline', update);
    void (async () => {
      try {
        device = preview
          ? {
              os: 'macos',
              arch: 'aarch64',
              chip: 'Apple silicon',
              memoryBytes: 16e9,
              freeBytes: 214e9,
            }
          : await getDeviceProfile();
      } catch {
        device = null;
      }
      try {
        catalog = await listModelInstallations();
      } catch {
        catalog = [];
      }
    })();
    return () => {
      removeEventListener('online', update);
      removeEventListener('offline', update);
    };
  });
</script>

<div class="card">
  <section class="sec this-mac" aria-label="This Mac">
    <span class="hd">This Mac</span>
    {#if !device}<span class="item"><Icon name="refresh" />Checking…</span>
    {:else}
      <span class="item"><Icon name="models" />{device.chip ?? 'This Mac'}</span>
      {#if device.memoryBytes}<span class="item"
          ><Icon name="spark" />{Math.round(device.memoryBytes / 1e9)} GB
          <span class="muted">memory</span></span
        >{/if}
      {#if device.freeBytes != null}<span class="item"
          ><Icon name="downloaded" />{formatBytes(device.freeBytes)}
          <span class="muted">free</span></span
        >{/if}
    {/if}
  </section>
  <section class="sec">
    <div class="rec-h">
      <b>Recommended for this Mac</b>
      <button
        type="button"
        class="quiet"
        aria-expanded={picking}
        disabled={started}
        onclick={() => (picking = !picking)}>{picking ? 'Done' : 'Choose different'}</button
      >
    </div>
    <div class="row">
      <Icon name="mic" />
      <div>
        <b>{speech.name}</b><span
          >Speech model · {SETUP_SPEECH_MODELS.find((m) => m.id === choice)?.reason ?? ''}</span
        >
      </div>
      <span class="size">{formatBytes(remainingBytes(catalog, [speech]))}</span>
    </div>
    <div class="row">
      <Icon name="spark" />
      <div>
        <b>Tidy my text · {writer.name}</b><span
          >Writing model · Adds punctuation and tidies the text</span
        >
      </div>
      <span class="size-switch">
        {#if tidy}<span class="size">{formatBytes(remainingBytes(catalog, [writer]))}</span>{/if}
        <label class="switch"
          ><input
            type="checkbox"
            bind:checked={tidy}
            disabled={started}
            aria-label="Tidy my text"
          /></label
        >
      </span>
    </div>
    {#if picking}
      <div class="picker" role="radiogroup" aria-label="Speech model">
        {#each SETUP_SPEECH_MODELS as model (model.id)}
          {@const ok = supports(model.id)}
          <button
            type="button"
            class="opt"
            role="radio"
            aria-checked={choice === model.id}
            aria-disabled={!ok}
            onclick={() => {
              if (ok) choice = model.id;
            }}
          >
            <span class="rd"></span>
            <span
              ><b>{catalogModel(model.id)?.name ?? model.id}</b><small
                >{ok ? model.reason : `Doesn’t support this language`}{model.id === recommended
                  ? ' · Recommended'
                  : ''}</small
              ></span
            >
            <span class="size"
              >{formatBytes(
                remainingBytes(catalog, [{ modelId: model.id, quantization: null }]),
              )}</span
            >
          </button>
        {/each}
      </div>
    {/if}
    <p class="total">
      {#if bytes === 0}<span>Already on this Mac. Nothing to download.</span>
      {:else}<span>Download <b>{formatBytes(bytes)}</b></span>
        {#if device?.freeBytes != null}<span
            >Needs <b>{formatBytes(need)}</b> free · you have
            <b>{formatBytes(device.freeBytes)}</b></span
          >{/if}
      {/if}
    </p>
    {#if lowDisk && !started}
      <div class="note bad" role="alert">
        <Icon name="warning" />
        <div>
          <b>Not enough free space.</b> This setup needs {formatBytes(need)} free. Free up space, or turn
          off Tidy my text and choose a lighter speech model.
        </div>
      </div>
    {:else if !online && bytes > 0 && !started}
      <div class="note warn">
        <Icon name="warning" />
        <div>
          <b>You’re offline.</b> Models download once, so Vesper needs a connection for this step. Connect
          and it continues.
        </div>
      </div>
    {:else if started}
      <div class="note">
        <Icon name="downloading" />
        <div><b>Installing in the background.</b> Keep going. Progress is at the bottom left.</div>
      </div>
    {/if}
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
  .this-mac {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 22px;
    padding: 16px 26px;
  }
  .hd {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-muted);
  }
  .item {
    display: inline-flex;
    align-items: center;
    gap: 9px;
  }
  .item :global(svg),
  .row > :global(svg),
  .note > :global(svg) {
    width: 18px;
    height: 18px;
    color: var(--color-accent);
    flex: none;
  }
  .muted {
    color: var(--color-muted);
  }
  .rec-h {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 6px;
  }
  .rec-h b {
    font: 700 22px/1.2 var(--font-display);
    letter-spacing: -0.02em;
  }
  .rec-h button {
    margin-left: auto;
  }
  .row {
    display: grid;
    grid-template-columns: 22px 1fr auto;
    gap: 14px;
    align-items: center;
    padding: 11px 0;
    border-top: 1px solid var(--color-line);
  }
  .row b {
    display: block;
    font-weight: 600;
    font-size: 16px;
  }
  .row span,
  .opt small {
    font-size: 13px;
    color: var(--color-muted);
  }
  .size {
    font-variant-numeric: tabular-nums;
    color: var(--color-muted);
    font-size: 14px;
  }
  .size-switch {
    display: inline-flex;
    align-items: center;
    gap: 14px;
  }
  .picker {
    margin-top: 12px;
    border: 1px solid var(--color-line);
    border-radius: 16px;
    overflow: hidden;
    background: var(--color-bg);
  }
  .opt {
    all: unset;
    box-sizing: border-box;
    width: 100%;
    cursor: pointer;
    display: grid;
    grid-template-columns: 20px 1fr auto;
    gap: 12px;
    align-items: center;
    padding: 12px 16px;
    border-top: 1px solid var(--color-line);
  }
  .opt:first-child {
    border-top: 0;
  }
  .opt:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: -2px;
  }
  .opt[aria-disabled='true'] {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .opt b {
    display: block;
    font-weight: 500;
  }
  .rd {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 1.5px solid var(--color-control-edge);
    display: grid;
    place-items: center;
  }
  .opt[aria-checked='true'] .rd {
    border-color: var(--color-accent);
  }
  .opt[aria-checked='true'] .rd::after {
    content: '';
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--color-accent);
  }
  .total {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 12px;
    margin: 0;
    padding-top: 14px;
    border-top: 1px solid var(--color-line);
    font-size: 14px;
    color: var(--color-muted);
  }
  .total b {
    color: var(--color-ink);
  }
  .note {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    margin-top: 16px;
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
  .note.bad {
    border-color: var(--color-error);
    background: var(--color-error-tint);
    color: var(--color-ink);
  }
  .note.bad > :global(svg) {
    color: var(--color-error);
  }
  .note.warn {
    border-color: var(--color-warning);
  }
  .note.warn > :global(svg) {
    color: var(--color-warning);
  }
  @container first-run (max-width: 760px) {
    .sec {
      padding: 18px;
    }
    .rec-h {
      flex-wrap: wrap;
    }
    .rec-h button {
      margin-left: 0;
    }
  }
</style>
