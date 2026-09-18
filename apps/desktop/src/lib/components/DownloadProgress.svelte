<script lang="ts">
  import type { InstallProgress } from '../api/runtime';
  import { downloadBytes, downloadPercent } from '../modelDownloads';
  let { progress, cancelling = false }: { progress: InstallProgress | null; cancelling?: boolean } =
    $props();
  const percent = $derived(downloadPercent(progress));
  const verifying = $derived(progress?.phase === 'verifying' && !cancelling);
  const phase = $derived(
    cancelling
      ? 'Cancelling…'
      : progress
        ? {
            checking: 'Checking existing files…',
            downloading: 'Downloading…',
            verifying: 'Verifying download…',
            installed: 'Finishing setup…',
          }[progress.phase]
        : 'Preparing download…',
  );
</script>

<div class="download-meter">
  <div class="meter-heading">
    <span role="status">{phase}</span><strong
      >{percent === null ? 'Please wait' : `${percent}%`}</strong
    >
  </div>
  <div
    class="meter-track"
    class:verifying
    class:cancelling
    role="progressbar"
    aria-label="Model download progress"
    aria-valuemin="0"
    aria-valuemax="100"
    aria-valuenow={percent ?? undefined}
    aria-valuetext={`${phase}${percent === null ? '' : ` ${percent}% of model files`}`}
  >
    <span
      class:indeterminate={percent === null}
      style:--download-fraction={percent === null ? 0.35 : percent / 100}
    ></span>
  </div>
  {#if progress}
    <div class="meter-detail">
      <span class="meter-file"
        >File {progress.fileIndex} of {progress.fileCount} · {progress.fileName}</span
      ><span>{downloadBytes(progress.completedBytes)} / {downloadBytes(progress.totalBytes)}</span>
    </div>
  {/if}
</div>

<style>
  .download-meter {
    width: 100%;
    min-width: 0;
    padding-block: var(--space-3);
  }
  .meter-heading,
  .meter-detail {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-3);
    font-size: 12px;
  }
  .meter-heading {
    color: var(--color-ink);
    margin-bottom: var(--space-2);
  }
  .meter-heading strong {
    color: var(--color-accent);
    font: 12px var(--font-mono);
    white-space: nowrap;
  }
  .meter-track {
    height: 5px;
    border-radius: var(--radius-pill);
    background: var(--color-line);
    overflow: hidden;
  }
  .meter-track span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--color-accent);
    transform: scaleX(var(--download-fraction));
    transform-origin: left;
    transition: transform 180ms var(--ease);
  }
  .meter-track .indeterminate {
    animation: download-wait 1.3s var(--ease) infinite alternate;
  }
  .meter-track.verifying span {
    animation: download-verify 1.2s var(--ease-in-out) infinite;
  }
  .meter-track.cancelling span {
    animation: none;
    opacity: 0.5;
  }
  .meter-detail {
    margin-top: var(--space-2);
    color: var(--color-muted);
    font-size: 11px;
    flex-wrap: wrap;
  }
  .meter-file {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .meter-detail > span:last-child {
    white-space: nowrap;
    font-family: var(--font-mono);
  }
  @keyframes download-wait {
    to {
      opacity: 0.35;
      transform: translateX(65%) scaleX(var(--download-fraction));
    }
  }
  @keyframes download-verify {
    50% {
      opacity: 0.45;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .meter-track span {
      animation: none;
      transition: none;
    }
  }
</style>
