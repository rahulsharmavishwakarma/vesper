<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import Select from './Select.svelte';
  import { getDeviceProfile, type DeviceProfile } from '../api/onboarding';
  import { listModelInstallations, type ModelCatalogInstallation } from '../api/runtime';
  import type { AppConfig, Language } from '../config';
  import {
    downloadTarget,
    type DownloadOptionStatus,
    type DownloadTarget,
  } from '../modelDownloads';
  import { modelOptions, selectModel, selectedModelValue } from '../modelPresets';
  import {
    candidateGauges,
    decorateSpeechOptions,
    decorateWriterOptions,
    languageLabel,
    rankSpeechModels,
    type Device,
  } from '../recommendations';
  import { formatBytes, remainingBytes, spaceNeeded } from '../firstRun';

  let {
    settings,
    language,
    started,
    preview = false,
  }: {
    settings: AppConfig;
    language: Language;
    /** The download has already been started from this step. */
    started: boolean;
    preview?: boolean;
  } = $props();

  let device = $state<DeviceProfile | null>(null);
  let catalog = $state<ModelCatalogInstallation[]>([]);
  let online = $state(typeof navigator === 'undefined' ? true : navigator.onLine);
  // svelte-ignore state_referenced_locally
  let tidy = $state(settings.cleanupEnabled);
  /** Null follows the recommendation, which can change once this Mac is read. */
  let speechChoice = $state<string | null>(null);
  let writerChoice = $state<string | null>(null);
  let entered = $state(false);
  let shown = $state({ memory: 0, free: 0 });

  const name = $derived(languageLabel(language));
  const mac = $derived<Device | null>(
    device ? { os: device.os, arch: device.arch, memoryBytes: device.memoryBytes } : null,
  );
  const ranked = $derived(rankSpeechModels(language, mac, settings.computeBackend));
  const speech = $derived(speechChoice ?? ranked[0]?.id ?? settings.models.asr);
  const current = $derived(ranked.find((candidate) => candidate.id === speech));
  // svelte-ignore state_referenced_locally
  const savedWriter = selectedModelValue(settings.models, 'rewriter');

  const writerOptions = $derived(
    decorateWriterOptions(
      modelOptions(settings.models, 'rewriter').map((option) => ({
        ...option,
        status: statusFor('rewriter', option.value),
      })),
      language,
    ),
  );

  // The ranked list puts the best writer for this language first.
  const writerRecommendation = $derived(
    writerOptions.find((option) => !option.disabled)?.value ?? savedWriter,
  );
  const writer = $derived(writerChoice ?? writerRecommendation);
  const nextSettings = $derived.by<AppConfig>(() => ({
    ...settings,
    language,
    cleanupEnabled: tidy,
    models: selectModel(selectModel(settings.models, 'asr', speech), 'rewriter', writer),
  }));
  const speechTarget = $derived(downloadTarget(nextSettings, 'asr'));
  const writerTarget = $derived(downloadTarget(nextSettings, 'rewriter'));
  const targets = $derived<DownloadTarget[]>(tidy ? [speechTarget, writerTarget] : [speechTarget]);
  // A preset writer or speech profile is not in the installed-model list until
  // it is saved, so its size comes from the profile's own file list.
  function bytesFor(target: DownloadTarget): number {
    const listed = catalog.some(
      (item) => item.modelId === target.modelId && item.quantization === target.quantization,
    );
    if (listed) return remainingBytes(catalog, [target]);
    const profile =
      target.role === 'speech_recognition'
        ? nextSettings.models.customAsr
        : nextSettings.models.customRewriter;
    return profile?.id === target.modelId
      ? profile.files.reduce((sum, file) => sum + file.sizeBytes, 0)
      : 0;
  }
  const bytes = $derived(targets.reduce((sum, target) => sum + bytesFor(target), 0));
  const need = $derived(spaceNeeded(bytes));
  const lowDisk = $derived(device?.freeBytes != null && bytes > 0 && device.freeBytes < need);
  const ram = $derived(device?.memoryBytes ? Math.round(device.memoryBytes / 1e9) : null);

  function installStatus(target: DownloadTarget): DownloadOptionStatus {
    const entry = catalog.find(
      (item) => item.modelId === target.modelId && item.quantization === target.quantization,
    );
    return entry?.artifactsInstalled
      ? { label: 'Downloaded', tone: 'ready', kind: 'downloaded' }
      : { label: 'Not downloaded', tone: 'muted', kind: 'missing' };
  }
  // Status reads the saved settings so the list does not depend on its own choice.
  const statusFor = (role: 'asr' | 'rewriter', value: string) =>
    installStatus(
      downloadTarget({ ...settings, models: selectModel(settings.models, role, value) }, role),
    );

  // The same options, order, tags and gauges as the Models page picker.
  const speechOptions = $derived(
    decorateSpeechOptions(
      modelOptions(settings.models, 'asr').map((option) => ({
        ...option,
        disabled:
          option.value.startsWith('preset:') &&
          !option.languages.some((entry) => entry.value === language),
        status: statusFor('asr', option.value),
      })),
      language,
      mac,
      settings.computeBackend,
    ),
  );
  const pill = $derived.by(() => {
    if (!current) return { label: 'Choose a speech model', tone: 'quiet' };
    if (current.local && !current.local.passed)
      return { label: `Failed Vesper’s ${name} test`, tone: 'bad' };
    if (current.state === 'recommended') return { label: `Best for ${name}`, tone: 'accent' };
    if (current.state === 'published')
      return { label: `Published result for ${name}`, tone: 'quiet' };
    return { label: `No published result for ${name} yet`, tone: 'quiet' };
  });
  const gauges = $derived(current ? candidateGauges(current, language, mac) : []);
  const evidence = $derived.by(() => {
    if (!current) return '';
    const figure = current.published;
    const parts = [
      figure
        ? figure.source === 'leaderboard'
          ? `Accuracy from the Open ASR Leaderboard (${figure.set})`
          : `Accuracy is the model maker’s own figure (${figure.set})`
        : `No accuracy figure is published for ${name}`,
    ];
    if (current.local?.ms && current.run)
      parts.push(`speed and memory measured by Vesper on ${current.run.hardware}`);
    if (current.local && !current.local.passed && current.local.why)
      parts.push(`in Vesper’s own test ${current.local.why}`);
    return `${parts.join('; ')}.`;
  });
  const writerOption = $derived(writerOptions.find((option) => option.value === writer));
  const writerIsRecommended = $derived(writer === writerRecommendation);

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

  // The spec tiles count up once, when the step opens.
  function countUp(memory: number, free: number) {
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) {
      shown = { memory, free };
      return;
    }
    const start = performance.now();
    const step = (now: number) => {
      const p = Math.min(1, (now - start) / 900);
      const eased = 1 - (1 - p) ** 3;
      shown = { memory: Math.round(memory * eased), free: Math.round(free * eased) };
      if (p < 1) requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  }

  onMount(() => {
    const update = () => (online = navigator.onLine);
    addEventListener('online', update);
    addEventListener('offline', update);
    requestAnimationFrame(() => (entered = true));
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
      countUp(
        device?.memoryBytes ? Math.round(device.memoryBytes / 1e9) : 0,
        device?.freeBytes ? Math.round(device.freeBytes / 1e9) : 0,
      );
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

<div class="models" class:entered>
  <div class="mac" aria-label="This Mac">
    <div class="spec" style="--i:0">
      <span class="glyph"><Icon name="models" /></span>
      <b>{device ? (device.chip ?? 'This Mac') : 'Checking…'}</b><small>Chip</small>
    </div>
    <div class="spec" style="--i:1">
      <span class="glyph"><Icon name="memory" /></span>
      <b>{device?.memoryBytes ? `${shown.memory} GB` : '—'}</b><small>Memory</small>
    </div>
    <div class="spec" style="--i:2">
      <span class="glyph"><Icon name="disk" /></span>
      <b>{device?.freeBytes != null ? `${shown.free} GB` : '—'}</b><small
        >Free{#if device?.freeBytes != null && bytes > 0}<span class="after"
            >· {formatBytes(Math.max(0, device.freeBytes - bytes))} after setup</span
          >{/if}</small
      >
      {#if device?.freeBytes != null}<span class="disk" aria-hidden="true"
          ><i style:width={`${Math.min(100, Math.max(1.5, (bytes / device.freeBytes) * 600))}%`}
          ></i></span
        >{/if}
    </div>
  </div>

  <div class="card">
    <section class="sec">
      <div class="for">
        <span class="pill {pill.tone}">{pill.label}</span>
        <span class="from">You speak <b>{name}</b> · chosen on the previous step</span>
      </div>
      <Select
        id="setup-speech-model"
        identity
        label="Speech model"
        value={speech}
        options={speechOptions}
        languageFilters={[{ value: language, label: name }]}
        initialLanguageFilter={language}
        disabled={started}
        onChange={(value) => (speechChoice = value)}
      />
      {#key speech}
        <div class="metrics">
          {#each gauges as gauge (gauge.kind)}
            <div class="metric" class:none={gauge.score === null} title={gauge.title}>
              <b
                >{gauge.score === null
                  ? 'Not measured'
                  : gauge.kind === 'accuracy'
                    ? `${gauge.text} right`
                    : gauge.text}</b
              >
              <span
                >{gauge.kind === 'accuracy'
                  ? `accuracy in ${name}`
                  : gauge.kind === 'speed'
                    ? 'to write a recording'
                    : ram
                      ? `memory of your ${ram} GB`
                      : 'memory'}</span
              >
              {#if gauge.score !== null}<span
                  class="meter"
                  class:low={gauge.tone === 'low'}
                  class:bad={gauge.tone === 'bad'}><i style:--w={`${gauge.score}%`}></i></span
                >{/if}
            </div>
          {/each}
        </div>
        <p class="evidence"><Icon name="target" /><span>{evidence}</span></p>
      {/key}
    </section>

    <section class="sec writer">
      <div class="writer-head">
        <Icon name="spark" />
        <div>
          <b>Tidy my text</b><span
            >Adds punctuation and tidies the words, using a writing model.</span
          >
        </div>
        <input
          type="checkbox"
          class="switch"
          role="switch"
          bind:checked={tidy}
          disabled={started}
          aria-label="Tidy my text"
        />
      </div>
      {#if tidy}
        <div class="for">
          <span class="pill {writerIsRecommended ? 'accent' : 'quiet'}"
            >{writerIsRecommended ? `Best writing model for ${name}` : 'Your choice'}</span
          >
        </div>
        <Select
          id="setup-writer-model"
          identity
          label="Writing model"
          value={writer}
          options={writerOptions}
          languageFilters={[{ value: language, label: name }]}
          initialLanguageFilter={language}
          disabled={started}
          onChange={(value) => (writerChoice = value)}
        />
        <p class="evidence">
          <Icon name="shield" /><span
            >{writerOption?.evidence ?? ''}
            {formatBytes(bytesFor(writerTarget))} to download.</span
          >
        </p>
      {/if}
    </section>

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
  </div>
</div>

<style>
  .models {
    display: grid;
    gap: 14px;
  }
  .mac {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
  }
  .spec {
    display: grid;
    grid-template-columns: 40px minmax(0, 1fr);
    gap: 2px 14px;
    align-items: center;
    padding: 16px 18px;
    border: 1px solid var(--color-line);
    border-radius: 18px;
    background: var(--color-raised);
    opacity: 0;
    transform: translateY(6px);
    transition:
      opacity 420ms var(--ease),
      transform 420ms var(--ease);
    transition-delay: calc(var(--i) * 90ms);
  }
  .entered .spec {
    opacity: 1;
    transform: none;
  }
  .glyph {
    grid-row: span 2;
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: 12px;
    background: var(--color-tint);
    color: var(--color-accent);
  }
  .glyph :global(.icon) {
    width: 22px;
    height: 22px;
  }
  .spec b {
    font: 700 21px/1.15 var(--font-display);
    letter-spacing: -0.02em;
    font-variant-numeric: tabular-nums;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .spec small {
    color: var(--color-muted);
    font-size: 12.5px;
  }
  .after {
    margin-left: 0.3em;
  }
  .disk {
    grid-column: 1 / -1;
    height: 4px;
    margin-top: 10px;
    border-radius: 99px;
    background: var(--color-line);
    overflow: hidden;
  }
  .disk i {
    display: block;
    height: 100%;
    background: var(--color-accent);
    transform-origin: left;
    transform: scaleX(0);
    transition: transform 900ms var(--ease) 380ms;
  }
  .entered .disk i {
    transform: none;
  }
  .card {
    border: 1px solid var(--color-line);
    border-radius: var(--radius-lg);
    background: var(--color-raised);
  }
  .sec {
    padding: 20px 26px 22px;
  }
  .sec + .sec {
    border-top: 1px solid var(--color-line);
  }
  .for {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px 12px;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 4px 11px 4px 9px;
    border-radius: 999px;
    font-size: 12.5px;
    font-weight: 600;
  }
  .pill::before {
    content: '';
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: currentColor;
  }
  .pill.accent {
    background: var(--color-tint);
    color: var(--color-accent);
  }
  .pill.quiet {
    border: 1px solid var(--color-control-edge);
    color: var(--color-muted);
  }
  .pill.bad {
    background: var(--color-error-tint);
    color: var(--color-error);
  }
  .from {
    color: var(--color-muted);
    font-size: 12.5px;
  }
  .from b {
    color: var(--color-ink);
    font-weight: 500;
  }
  .sec :global(.select-trigger) {
    max-width: none;
    margin-top: 6px;
  }
  .metrics {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 14px;
    margin-top: 18px;
    animation: swap 420ms var(--ease);
  }
  @keyframes swap {
    from {
      opacity: 0;
      transform: translateY(6px);
      filter: blur(4px);
    }
  }
  .metric b {
    display: block;
    font: 700 22px/1.15 var(--font-display);
    letter-spacing: -0.02em;
    font-variant-numeric: tabular-nums;
  }
  .metric.none b {
    color: var(--color-muted);
    font-size: 16px;
    padding: 4px 0 2px;
  }
  .metric span {
    color: var(--color-muted);
    font-size: 12.5px;
  }
  .meter {
    display: block;
    height: 4px;
    margin-top: 10px;
    border-radius: 99px;
    background: var(--color-line);
    overflow: hidden;
  }
  .meter i {
    display: block;
    height: 100%;
    width: var(--w);
    background: var(--color-accent);
    animation: fill 700ms var(--ease);
  }
  .meter.low i {
    background: var(--color-warning);
  }
  .meter.bad i {
    background: var(--color-error);
  }
  @keyframes fill {
    from {
      width: 0;
    }
  }
  .evidence {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin: 14px 0 0;
    color: var(--color-muted);
    font-size: 12.5px;
  }
  .evidence :global(.icon) {
    flex: none;
    width: 15px;
    height: 15px;
    margin-top: 2px;
    color: var(--color-accent);
  }
  .writer-head {
    display: grid;
    grid-template-columns: 22px 1fr auto;
    gap: 14px;
    align-items: center;
  }
  .writer-head > :global(.icon) {
    width: 20px;
    height: 20px;
    color: var(--color-accent);
  }
  .writer-head b {
    display: block;
    font-weight: 600;
  }
  .writer-head span {
    color: var(--color-muted);
    font-size: 13px;
  }
  .writer .for {
    margin-top: 16px;
  }
  .total {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 12px;
    margin: 0;
    padding: 14px 26px;
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
    margin: 0 26px 20px;
    padding: 14px 16px;
    border: 1px solid var(--color-line);
    border-radius: 14px;
    background: var(--color-bg);
    font-size: 14px;
    color: var(--color-muted);
  }
  .note > :global(svg) {
    width: 18px;
    height: 18px;
    flex: none;
    color: var(--color-accent);
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
    .mac,
    .metrics {
      grid-template-columns: 1fr;
    }
    .sec {
      padding: 18px;
    }
    .total {
      padding: 14px 18px;
    }
    .note {
      margin: 0 18px 18px;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .spec,
    .disk i {
      transition: none;
    }
    .metrics,
    .meter i {
      animation: none;
    }
  }
</style>
