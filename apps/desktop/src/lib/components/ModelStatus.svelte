<script lang="ts">
  import type { DownloadOptionStatus } from '../modelDownloads';
  import Icon from './Icon.svelte';

  let { status, labeled = false }: { status: DownloadOptionStatus; labeled?: boolean } = $props();
  const glyph = $derived(
    (
      {
        downloaded: 'downloaded',
        missing: 'download',
        busy: 'refresh',
        warning: 'warning',
        unavailable: 'unavailable',
        unknown: 'unknown',
      } as const
    )[status.kind],
  );
  const showLabel = $derived(labeled || !['downloaded', 'missing'].includes(status.kind));
</script>

<span
  class="model-status"
  class:ready={status.tone === 'ready'}
  class:warning={status.tone === 'warning'}
  data-status={status.kind}
  title={status.label}
>
  <span
    class="status-glyph"
    class:busy={status.kind === 'busy'}
    role="img"
    aria-label={status.label}><Icon name={glyph} /></span
  >
  {#if showLabel}<span class="status-label" aria-hidden="true">{status.label}</span
    >{:else if status.detail}<span class="status-label" aria-hidden="true">{status.detail}</span
    >{/if}
</span>

<style>
  /* Hallmark · component: model status · existing Studio tokens · P5 H5 E4 S5 R5 V4 */
  .model-status {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    color: var(--color-muted);
    font-size: 11px;
    line-height: 1.4;
  }
  .ready {
    color: var(--color-accent);
  }
  .warning {
    color: var(--color-warning);
  }
  .status-glyph {
    display: inline-flex;
    flex-shrink: 0;
    width: 20px;
    height: 20px;
  }
  .status-glyph :global(svg) {
    width: 100%;
    height: 100%;
  }
  .status-label {
    overflow-wrap: anywhere;
  }
  .busy {
    animation: status-turn 1.6s linear infinite;
  }
  @keyframes status-turn {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .busy {
      animation: none;
    }
  }
</style>
