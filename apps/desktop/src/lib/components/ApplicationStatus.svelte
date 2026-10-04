<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import type { AppConfig } from '../config';
  import { getEngineDiagnostics, type AppEnvironment } from '../api/desktop';
  import type { ModelCatalogInstallation } from '../api/runtime';
  import { downloadTarget, matchingDownload } from '../modelDownloads';
  import {
    activityLabels,
    appendSample,
    createResourcePoller,
    durationLabel,
    memoryScale,
    resourceBars,
    validSample,
    type EngineDiagnostics,
    type ResourceSample,
  } from '../engineStatus';

  let {
    environment,
    visible,
    settings,
    catalog,
    activity,
    refreshing,
    statusError,
    onRefresh,
  }: {
    environment: AppEnvironment;
    visible: boolean;
    settings: AppConfig | null;
    catalog: ModelCatalogInstallation[];
    activity: { busy: boolean; stage: string } | null;
    refreshing: boolean;
    statusError: string | null;
    onRefresh: () => Promise<void>;
  } = $props();
  let pageVisible = $state(true);
  let history = $state<ResourceSample[]>([]);
  let diagnostics = $state<EngineDiagnostics | null>(null);
  let reading = $state<ResourceSample | null>(null);
  let readError = $state<string | null>(null);
  let poller = $state<ReturnType<typeof createResourcePoller<EngineDiagnostics>> | null>(null);
  let firstSample = true;
  let requestedRefresh = $state(false);
  let refreshRevision = $state(0);
  let animateResources = $state(false);
  let revealTimer: ReturnType<typeof setTimeout> | undefined;
  const refreshBusy = $derived(refreshing || requestedRefresh);
  const desktop = $derived(environment === 'desktop');
  const enabled = $derived(desktop && visible && pageVisible);
  const asr = $derived(settings ? downloadTarget(settings, 'asr') : null);
  const cleanup = $derived(settings ? downloadTarget(settings, 'rewriter') : null);
  const asrInstall = $derived(asr ? matchingDownload(catalog, asr) : undefined);
  const cleanupInstall = $derived(cleanup ? matchingDownload(catalog, cleanup) : undefined);
  const engineActivity = $derived(
    enabled && diagnostics && !readError ? diagnostics.activity : activity,
  );
  const ready = $derived(asrInstall?.ready && (!settings?.cleanupEnabled || cleanupInstall?.ready));
  const label = $derived(
    !desktop
      ? 'Desktop preview'
      : readError || statusError
        ? 'Status unavailable'
        : engineActivity?.busy ||
            engineActivity?.stage === 'error' ||
            engineActivity?.stage === 'cancelled'
          ? (activityLabels[engineActivity.stage] ?? 'Working')
          : !settings
            ? 'Loading settings'
            : ready
              ? 'Ready to listen'
              : 'Check model setup',
  );
  const currentReading = $derived(enabled && !readError ? reading : null);
  const ramScale = $derived(memoryScale(history));
  const timings = $derived(diagnostics?.timings);
  const active = $derived(
    timings?.outcome === 'running' && engineActivity?.busy ? timings.activeStage : null,
  );
  const rows = $derived([
    {
      key: 'transcribe',
      label: timings?.speechMode === 'finalization' ? 'Finalize speech' : 'Transcribe',
      value: timings?.transcribeMs,
    },
    { key: 'cleanup', label: 'Polish text', value: timings?.cleanupMs },
    { key: 'save', label: 'Save', value: timings?.saveMs },
  ]);
  async function refresh() {
    if (refreshBusy) return;
    requestedRefresh = enabled;
    poller?.refresh();
    await onRefresh();
  }
  onMount(() => {
    const visibility = () => {
      pageVisible = document.visibilityState === 'visible';
    };
    visibility();
    document.addEventListener('visibilitychange', visibility);
    poller = createResourcePoller(
      getEngineDiagnostics,
      (snapshot) => {
        let sample = validSample(snapshot.resources);
        if (firstSample) {
          sample = { ...sample, cpuPercent: null };
          firstSample = false;
        }
        diagnostics = snapshot;
        reading = sample;
        history = appendSample(history, sample);
        readError = null;
        if (requestedRefresh) {
          requestedRefresh = false;
          refreshRevision++;
          animateResources = true;
          clearTimeout(revealTimer);
          revealTimer = setTimeout(() => {
            animateResources = false;
          }, 550);
        }
      },
      (error) => {
        requestedRefresh = false;
        readError = error instanceof Error ? error.message : String(error);
        reading = null;
      },
    );
    return () => {
      clearTimeout(revealTimer);
      poller?.dispose();
      document.removeEventListener('visibilitychange', visibility);
    };
  });
  $effect(() => {
    if (enabled) {
      firstSample = true;
      history = [];
      reading = null;
      readError = null;
    } else {
      requestedRefresh = false;
      animateResources = false;
      clearTimeout(revealTimer);
    }
    poller?.setEnabled(enabled);
  });
</script>

<section class="engine-status" aria-labelledby="engine-heading" hidden={!visible}>
  <header class="engine-header">
    <h2 id="engine-heading">Local engine</h2>
    <span
      class="engine-state"
      class:working={engineActivity?.busy}
      class:has-error={Boolean(readError || statusError || engineActivity?.stage === 'error')}
      ><span class="engine-dot" aria-hidden="true"></span>{label}</span
    ><button
      class="quiet icon-button"
      type="button"
      onclick={refresh}
      disabled={refreshBusy}
      aria-label={refreshBusy ? 'Checking application status' : 'Refresh application status'}
      title={refreshBusy ? 'Checking application status' : 'Refresh application status'}
      ><span class:spinning={refreshBusy}><Icon name="refresh" /></span></button
    >
  </header>
  <div class="engine-body">
    <div class="engine-resources">
      {#each [{ key: 'cpuPercent' as const, name: 'CPU activity', value: currentReading?.cpuPercent?.toFixed(1) ?? '—', unit: '% capacity', scale: 100, scaleLabel: '100%' }, { key: 'memoryBytes' as const, name: 'Memory in use', value: currentReading?.memoryBytes != null ? (currentReading.memoryBytes / 2 ** 30).toFixed(2) : '—', unit: 'GiB', scale: ramScale, scaleLabel: `${(ramScale / 2 ** 30).toFixed(1)} GiB` }] as metric (metric.key)}
        <section class="engine-resource" aria-label={metric.name}>
          <div class="resource-heading">
            <h3>{metric.name}</h3>
            <span class="resource-value">{metric.value}<small>{metric.unit}</small></span>
          </div>
          <div class="resource-graph" class:stale={Boolean(readError)}>
            <svg viewBox="0 0 400 84" preserveAspectRatio="none" aria-hidden="true">
              <path class="resource-grid" d="M0 8H400M0 42H400M0 76H400" />
              {#key refreshRevision}<g class="resource-bars" class:reveal={animateResources}>
                  {#each resourceBars(history, metric.key, metric.scale) as bar, index (bar.time)}<rect
                      x={bar.x}
                      y={bar.y}
                      width="4"
                      height={bar.height}
                      rx="1.5"
                      style:--bar-delay={`${Math.min(index * 8, 120)}ms`}
                    />{/each}
                </g>{/key}
            </svg>
            {#if !history.some((point) => point[metric.key] !== null)}<p>
                {!desktop
                  ? 'Available in the desktop app'
                  : currentReading?.status === 'unavailable'
                    ? 'Unavailable on this system'
                    : readError
                      ? 'Waiting for a fresh reading'
                      : 'Measuring…'}
              </p>{/if}
            <span class="resource-scale">{metric.scaleLabel}</span>
          </div>
          <div class="resource-axis"><span>60s ago</span><span>Now</span></div>
        </section>
      {/each}
    </div>
    <p class="resource-scope">
      Main speech engine usage, including loaded models. App window and helper processes excluded.
    </p>
    <div class="engine-operation">
      <div class="operation-heading">
        <h3>
          {timings?.outcome === 'running' && engineActivity?.busy
            ? 'Current operation'
            : 'Last operation'}
        </h3>
        <span
          >{timings
            ? {
                running:
                  engineActivity?.stage === 'recording'
                    ? 'Recording · timings begin after stop'
                    : 'In progress',
                completed: 'Processing finished',
                cancelled: 'Cancelled · partial timings',
                error: 'Failed · partial timings',
              }[timings.outcome]
            : 'No recent activity'}</span
        >
      </div>
      {#if timings}<p class="operation-model">
          {timings.asrModel}{timings.cleanupModel
            ? ` → ${timings.cleanupModel}`
            : ' · writing model off'}{timings.speechMode === 'fallback'
            ? ' · completed-recording fallback'
            : ''}
        </p>{/if}
      <div class="engine-steps">
        {#each rows as row (row.key)}<div class="engine-step" class:current={active === row.key}>
            <span>{row.label}</span><span class="step-duration"
              >{row.key === 'cleanup' && timings && timings.cleanupModel === null
                ? 'Off'
                : durationLabel(row.value)}</span
            >
          </div>{/each}
      </div>
    </div>
    {#if readError || statusError}<p class="engine-error" role="status">
        {readError ?? statusError} Use Refresh to retry.
      </p>
    {:else if desktop && !ready}<p class="engine-note">
        {!settings
          ? 'Load your settings to check model readiness.'
          : 'Check your selected models below.'}
      </p>{/if}
    <details class="engine-definitions">
      <summary>About these readings</summary>
      <p>
        CPU is the main speech engine’s share of total CPU capacity. Memory covers its loaded models
        and other working memory, excluding GPU memory. Readings pause when this panel or window is
        hidden.
      </p>
      <p>
        Times cover transcription, writing and saving, including model loading. Recording time is
        excluded. For live speech, Finalize speech measures the wait after Stop. A fallback includes
        the retry.
      </p>
    </details>
  </div>
</section>

<style>
  .engine-status {
    margin: 0 0 var(--space-6);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-lg);
    background: var(--color-raised);
  }
  .engine-header {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    padding: var(--space-5) var(--space-6);
  }
  .engine-header h2 {
    font-size: 17px;
    font-weight: 600;
    letter-spacing: -0.03em;
    margin: 0;
  }
  .engine-state {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    color: var(--color-accent);
    font-size: 11px;
    margin-left: auto;
  }
  .engine-state.has-error {
    color: var(--color-error);
  }
  .engine-dot {
    width: 6px;
    height: 6px;
    flex-shrink: 0;
    border-radius: var(--radius-pill);
    background: currentColor;
  }
  .working .engine-dot {
    animation: engine-pulse 1.5s var(--ease) infinite;
  }
  .engine-body {
    padding: 0 var(--space-6);
  }
  .resource-heading,
  .operation-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }
  .engine-header button {
    font-size: 12px;
  }
  .engine-resources {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: var(--space-6);
    border-top: 1px solid var(--color-line);
    padding-top: var(--space-6);
  }
  .engine-resource + .engine-resource {
    border-left: 1px solid var(--color-line);
    padding-left: var(--space-6);
  }
  .resource-heading h3,
  .operation-heading h3 {
    font-size: 12px;
    font-weight: 400;
    margin: 0;
  }
  .resource-heading h3 {
    color: var(--color-muted);
  }
  .resource-value {
    font: 24px var(--font-mono);
    letter-spacing: -0.06em;
    white-space: nowrap;
  }
  .resource-value small {
    font: 10px var(--font-body);
    letter-spacing: 0;
    color: var(--color-muted);
    margin-left: var(--space-2);
  }
  .resource-graph {
    height: 96px;
    position: relative;
    margin: var(--space-3) 0 var(--space-2);
  }
  .resource-graph svg {
    width: 100%;
    height: 100%;
  }
  .resource-grid {
    fill: none;
    stroke: var(--color-line);
    stroke-dasharray: 2 6;
    vector-effect: non-scaling-stroke;
  }
  .resource-bars {
    fill: var(--color-accent);
  }
  .resource-bars.reveal rect {
    transform-box: fill-box;
    transform-origin: center bottom;
    animation: resource-rise 380ms var(--ease-slide) both;
    animation-delay: var(--bar-delay);
  }
  .resource-graph.stale .resource-bars {
    opacity: 0.35;
  }
  .resource-graph p {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    text-align: center;
    font-size: 11px;
    color: var(--color-muted);
    margin: 0;
  }
  .resource-scale {
    position: absolute;
    right: 0;
    top: 0;
    font: 9px var(--font-mono);
    color: var(--color-muted);
    background: var(--color-raised);
    padding-left: var(--space-1);
  }
  .resource-axis {
    display: flex;
    justify-content: space-between;
    color: var(--color-muted);
    font: 10px var(--font-mono);
  }
  .resource-scope {
    margin: var(--space-5) 0 var(--space-6);
    color: var(--color-muted);
    font-size: 11px;
  }
  .engine-operation {
    padding: var(--space-5) 0;
    border-top: 1px solid var(--color-line);
  }
  .operation-heading span {
    color: var(--color-muted);
    font-size: 11px;
  }
  .operation-model {
    font-size: 11px;
    color: var(--color-muted);
    overflow-wrap: anywhere;
    margin: var(--space-2) 0 0;
  }
  .engine-steps {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: var(--space-5);
    margin-top: var(--space-5);
  }
  .engine-step {
    display: flex;
    justify-content: space-between;
    gap: var(--space-2);
    border-top: 2px solid var(--color-line);
    padding-top: var(--space-3);
    color: var(--color-muted);
    font-size: 11px;
  }
  .engine-step.current {
    border-color: var(--color-accent);
    color: var(--color-accent);
  }
  .step-duration {
    font-family: var(--font-mono);
    white-space: nowrap;
  }
  .engine-error,
  .engine-note {
    font-size: 12px;
    padding: var(--space-3);
    background: var(--color-raised);
    border-radius: var(--radius-sm);
    margin: 0 0 var(--space-4);
    overflow-wrap: anywhere;
  }
  .engine-error {
    color: var(--color-error);
  }
  .engine-note {
    color: var(--color-muted);
  }
  .engine-definitions {
    margin: 0;
    border-top: 1px solid var(--color-line);
  }
  .engine-definitions summary {
    color: var(--color-muted);
    font-size: 11px;
    cursor: pointer;
  }
  .engine-definitions p {
    font-size: 11px;
    color: var(--color-muted);
    max-width: 75ch;
  }
  .spinning {
    display: inline-flex;
    animation: engine-spin 1s linear infinite;
  }
  @keyframes engine-pulse {
    50% {
      opacity: 0.3;
    }
  }
  @keyframes engine-spin {
    to {
      transform: rotate(360deg);
    }
  }
  @keyframes resource-rise {
    from {
      transform: scaleY(0.05);
      opacity: 0.3;
    }
    to {
      transform: scaleY(1);
      opacity: 1;
    }
  }
  @media (max-width: 768px) {
    .operation-heading {
      align-items: flex-start;
      flex-direction: column;
      gap: var(--space-2);
    }
  }
  @media (max-width: 480px) {
    .engine-header {
      padding: var(--space-4);
      gap: var(--space-2);
      flex-wrap: wrap;
    }
    .engine-header h2 {
      font-size: 15px;
    }
    .engine-state {
      font-size: 10px;
    }
    .engine-body {
      padding-inline: var(--space-4);
    }
    .engine-resources {
      grid-template-columns: minmax(0, 1fr);
    }
    .engine-resource + .engine-resource {
      padding: var(--space-5) 0 0;
      border-left: 0;
      border-top: 1px solid var(--color-line);
    }
    .engine-steps {
      gap: var(--space-3);
    }
    .engine-step {
      flex-direction: column;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .working .engine-dot,
    .spinning,
    .resource-bars.reveal rect {
      animation: none;
    }
  }
</style>
