<script lang="ts">
  import Icon from './Icon.svelte';
  import ModelStatus from './ModelStatus.svelte';
  import { MODEL_ROLE_LABELS } from '../config';
  import DownloadProgress from './DownloadProgress.svelte';
  import {
    downloadBytes,
    downloadKey,
    matchingDownload,
    type DownloadTarget,
    type ModelDownloadState,
    type DownloadOptionStatus,
  } from '../modelDownloads';
  import type { ModelCatalogInstallation } from '../api/runtime';
  let {
    target,
    state,
    desktop,
    dirty,
    disabled,
    knownSelection,
    downloadDisabled = false,
    fallbackBytes = 0,
    onDownload,
    onRemove,
  }: {
    target: DownloadTarget;
    state: ModelDownloadState;
    desktop: boolean;
    dirty: boolean;
    disabled: boolean;
    knownSelection: boolean;
    downloadDisabled?: boolean;
    fallbackBytes?: number;
    onDownload: () => void;
    onRemove: (model: ModelCatalogInstallation) => void;
  } = $props();
  const entry = $derived(knownSelection ? matchingDownload(state.catalog, target) : undefined);
  const installing = $derived(state.installingKey === downloadKey(target));
  const bytes = $derived(
    entry?.files.reduce((total, file) => total + file.sizeBytes, 0) ?? fallbackBytes,
  );
  const packageLabel = $derived(
    target.quantization
      ? target.quantization === 'q4_0'
        ? 'Q4 weights'
        : 'Q8 weights'
      : 'Model package',
  );
  const sizeLabel = $derived(
    bytes > 0
      ? `${(bytes / (bytes >= 1024 ** 3 ? 1024 ** 3 : 1024 ** 2)).toLocaleString(undefined, { maximumFractionDigits: 2 })} ${bytes >= 1024 ** 3 ? 'GiB' : 'MiB'} · `
      : '',
  );
  const installed = $derived(entry?.artifactsInstalled === true);
  // A re-check (after any download, verify or remove) keeps every row's last
  // known status; only a row with nothing known yet says it is checking.
  const checkingUnknown = $derived(state.checking && !entry);
  const unsupported = $derived(entry?.supported === false && desktop);
  const status = $derived(
    !desktop
      ? 'Desktop app required'
      : installing
        ? 'Downloading and verifying…'
        : !knownSelection
          ? 'Save selection to check files'
          : checkingUnknown
            ? 'Checking files…'
            : installed
              ? entry?.runtimeReady
                ? 'Ready on this device'
                : 'Files downloaded · setup incomplete'
              : entry?.ready
                ? 'Ready · update available'
                : entry?.localFileCount
                  ? 'Incomplete download'
                  : unsupported
                    ? 'Unavailable on this device'
                    : entry
                      ? 'Not downloaded'
                      : 'Status unavailable',
  );
  const statusKind = $derived<DownloadOptionStatus['kind']>(
    !desktop
      ? 'unavailable'
      : installing
        ? 'busy'
        : !knownSelection
          ? 'unknown'
          : checkingUnknown
            ? 'busy'
            : installed
              ? entry?.runtimeReady
                ? 'downloaded'
                : 'warning'
              : entry?.ready
                ? 'downloaded'
                : entry?.localFileCount
                  ? 'warning'
                  : unsupported
                    ? 'unavailable'
                    : entry
                      ? 'missing'
                      : 'unknown',
  );
  const label = $derived(
    installing
      ? 'Downloading…'
      : dirty
        ? installed
          ? 'Save & verify'
          : 'Save & download'
        : installed
          ? 'Verify files'
          : 'Download',
  );
  const localFiles = $derived(entry && (entry.localFileCount > 0 || entry.removalError));
</script>

<div
  class="model-download"
  aria-label={`${target.role === 'audio_frontend' ? 'Audio processing model' : MODEL_ROLE_LABELS[target.role === 'speech_recognition' ? 'asr' : 'rewriter']} download`}
>
  <div class="model-download-row">
    <div class="model-download-copy">
      <ModelStatus
        status={{
          label: status,
          kind: statusKind,
          tone:
            statusKind === 'downloaded' ? 'ready' : statusKind === 'warning' ? 'warning' : 'muted',
        }}
      /><span class="note"
        >{localFiles
          ? `${downloadBytes(entry!.downloadedBytes)} on device · ${packageLabel}`
          : sizeLabel + packageLabel}</span
      >
    </div>
    <div class="model-download-actions">
      {#if localFiles && entry}
        <button
          type="button"
          class="quiet model-remove"
          onclick={() => onRemove(entry!)}
          disabled={!desktop ||
            disabled ||
            state.checking ||
            state.installing ||
            Boolean(entry.removalError)}
          aria-label={`Remove ${target.name}${target.quantization ? ` ${target.quantization.slice(0, 2).toUpperCase()}` : ''}`}
          ><Icon name="trash" />Remove</button
        >
      {/if}
      <button
        type="button"
        class:primary={!installed}
        onclick={onDownload}
        disabled={!desktop ||
          disabled ||
          downloadDisabled ||
          installing ||
          unsupported ||
          (!dirty && (state.checking || !entry))}
        aria-label={`${label} ${target.name}${target.quantization ? ` ${target.quantization}` : ''}`}
      >
        {#if installing}<span class="spinning" aria-hidden="true"></span>{:else}<Icon
            name={installed ? 'refresh' : 'download'}
          />{/if}{label}
      </button>
    </div>
  </div>
  {#if entry?.removalError}<p class="note">{entry.removalError}</p>{/if}
  {#if dirty}<p class="note">
      {installed ? 'Verifying' : 'Downloading'} also saves your pending settings.
    </p>
  {:else if entry && desktop && (!entry.runtimeReady || !entry.supported)}<p class="note">
      {entry.message}
    </p>{/if}
  {#if installing}<DownloadProgress progress={state.details} cancelling={state.cancelling} />{/if}
</div>

<style>
  .model-download-copy {
    display: flex;
    align-items: center;
    flex-direction: row;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .model-download-copy .note {
    margin: 0;
  }
  .model-download-actions {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .model-remove:hover:enabled {
    color: var(--color-error);
  }
  .model-remove :global(.icon) {
    width: 16px;
    height: 16px;
    transition: transform 160ms var(--ease);
  }
  .model-remove:hover:enabled :global(.icon) {
    transform: translateY(-1px);
  }
  @media (prefers-reduced-motion: reduce) {
    .model-remove :global(.icon) {
      transition: none;
    }
  }
</style>
