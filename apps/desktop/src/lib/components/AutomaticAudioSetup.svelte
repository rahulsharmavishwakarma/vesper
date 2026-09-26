<script lang="ts">
  import ModelStatus from './ModelStatus.svelte';
  import {
    AUTOMATIC_AUDIO_MODELS,
    downloadKey,
    matchingDownload,
    type ModelDownloadState,
    type DownloadOptionStatus,
  } from '../modelDownloads';
  let {
    state,
    desktop,
  }: {
    state: ModelDownloadState;
    desktop: boolean;
  } = $props();

  function status(modelId: string, name: string): DownloadOptionStatus {
    const target = { modelId, name, role: 'audio_frontend' as const, quantization: null };
    const entry = matchingDownload(state.catalog, target);
    if (!desktop) return { label: 'Desktop app required', kind: 'unavailable', tone: 'muted' };
    if (state.installingKey === downloadKey(target))
      return { label: 'Downloading and verifying…', kind: 'busy', tone: 'muted' };
    if (state.checking) return { label: 'Checking files…', kind: 'busy', tone: 'muted' };
    if (!entry) return { label: 'Status unavailable', kind: 'unknown', tone: 'muted' };
    if (!entry.supported || !entry.selected)
      return { label: 'Unavailable on this device', kind: 'unavailable', tone: 'muted' };
    if (!entry.runtimeReady)
      return { label: 'Awaiting device support', kind: 'unavailable', tone: 'muted' };
    if (entry.artifactsInstalled)
      return { label: 'Ready on this device', kind: 'downloaded', tone: 'ready' };
    return state.audioSetupPaused && !state.installing
      ? { label: 'Will retry automatically', kind: 'warning', tone: 'warning' }
      : { label: 'Will download automatically', kind: 'missing', tone: 'muted' };
  }
</script>

<p class="note">
  Noise reduction and speech detection are set up automatically after your speech and writing models
  are ready. Speaker timelines are added on supported devices. Processing stays on device.
</p>
<div class="automatic-audio-status" aria-label="Automatic audio setup" aria-live="polite">
  {#each AUTOMATIC_AUDIO_MODELS as { modelId, name } (modelId)}
    <div class="audio-status-row">
      <span>{name}</span><ModelStatus status={status(modelId, name)} />
    </div>
  {/each}
</div>
<p class="note">
  Noise reduction can reduce transcription accuracy, especially for quiet speech. If audio
  processing is unavailable, transcription uses the original audio.
</p>

<style>
  .automatic-audio-status {
    display: grid;
    gap: var(--space-3);
    margin-block: var(--space-4);
  }
  .audio-status-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }
</style>
