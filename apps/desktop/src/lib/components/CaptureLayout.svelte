<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { CaptureDesign } from '../config';
  import { captureDesignOptions } from '../captureDesigns';

  let {
    design,
    heading,
    status = '',
    elapsed,
    failed = false,
    attention = false,
    complete = false,
    detailVisible = false,
    waveform,
    actions,
    onOpen,
    onDrag,
    dragging = false,
  }: {
    design: CaptureDesign;
    heading: string;
    status?: string;
    elapsed: string;
    failed?: boolean;
    attention?: boolean;
    complete?: boolean;
    detailVisible?: boolean;
    waveform: Snippet;
    actions: Snippet<[boolean]>;
    onOpen?: () => void;
    onDrag?: () => void;
    dragging?: boolean;
  } = $props();
  // A window drag surface is not a button. Interactive descendants retain
  // their usual clicks, and the inert Settings preview has no drag callback.
  function dragSurface(node: HTMLElement) {
    const down = (event: MouseEvent) => {
      if (
        !onDrag ||
        event.button !== 0 ||
        !(event.target instanceof Element) ||
        event.target.closest('button, a, input, textarea, select, [role="button"]')
      )
        return;
      event.preventDefault();
      onDrag();
    };
    node.addEventListener('mousedown', down);
    return { destroy: () => node.removeEventListener('mousedown', down) };
  }
  const showDetail = $derived(
    (failed || attention || complete || detailVisible) && Boolean(status),
  );
  const width = $derived(
    captureDesignOptions.find((option) => option.value === design)?.width ?? 372,
  );
</script>

{#snippet brand()}
  <button
    type="button"
    class="capture-brand"
    onclick={onOpen}
    aria-label="Open Vesper"
    title="Open Vesper">vesper</button
  >
{/snippet}
{#snippet state()}
  <span
    class="capture-status"
    class:warning={failed || attention}
    aria-live="polite"
    title={status || heading}>{heading}</span
  >
{/snippet}
{#snippet timer()}
  <span class="capture-time" aria-label="Recording duration">{elapsed}</span>
{/snippet}
{#snippet wave()}
  <div class="capture-wave">{@render waveform()}</div>
{/snippet}

<div
  class="layout-wrap"
  class:failed
  class:attention
  class:complete
  style:--capture-width={`${width}px`}
  data-capture-design={design}
>
  <div
    class="capture capture-{design}"
    class:movable={Boolean(onDrag)}
    class:dragging
    use:dragSurface
  >
    {#if design === 'ribbon'}
      <div class="ribbon-identity">{@render brand()}{@render state()}</div>
      <div class="ribbon-signal">{@render wave()}</div>
      {@render timer()}
      <div class="quick-actions">{@render actions(false)}</div>
    {:else if design === 'underline'}
      <div class="capture-heading">{@render brand()}{@render state()}{@render timer()}</div>
      <div class="underline-copy">
        <p class="caption-text" title={status}>{status || heading}</p>
        <div class="quick-actions">{@render actions(false)}</div>
      </div>
      {@render wave()}
    {:else if design === 'studio'}
      <div class="capture-heading">{@render brand()}{@render timer()}</div>
      {@render wave()}
      <div class="studio-footer">
        {@render state()}
        <div class="quick-actions">{@render actions(true)}</div>
      </div>
    {:else if design === 'margin'}
      <div class="margin-peek">
        {@render brand()}{@render state()}
        <p title={status}>{status}</p>
      </div>
      <span class="margin-marker" aria-hidden="true"></span>
      <div class="margin-signal">{@render wave()}</div>
      {@render timer()}
      <div class="quick-actions">{@render actions(false)}</div>
    {:else if design === 'split'}
      <div class="split-signal-pod">
        <div class="capture-heading">{@render brand()}{@render timer()}</div>
        {@render wave()}
        <div class="split-feedback">
          {@render state()}
          {#if showDetail}<p
              class="split-detail"
              class:warning={failed || attention}
              title={status}
            >
              {status}
            </p>{/if}
        </div>
      </div>
      <div class="split-action-pod quick-actions">{@render actions(false)}</div>
    {:else}
      <div class="counter-heading">{@render brand()}{@render state()}</div>
      <div class="counter-clock">{@render timer()}</div>
      {@render wave()}
      <div class="quick-actions">{@render actions(false)}</div>
    {/if}
  </div>
  {#if showDetail && design !== 'underline' && design !== 'margin' && design !== 'split'}
    <p class="capture-note" class:warning={failed || attention} title={status}>{status}</p>
  {/if}
</div>

<style>
  /* Hallmark · component: capture layouts · graphite/sage · modern-minimal
   * P5 H5 E4 S5 R5 V5. Shared eight-state controls; inherited type and tokens. */
  .layout-wrap {
    --capture-shadow: color-mix(in srgb, var(--color-bg) 60%, var(--color-transparent));
    width: min(var(--capture-width), 100%);
    color: var(--color-ink);
  }
  .capture {
    position: relative;
    min-width: 0;
    width: 100%;
    background: var(--color-panel);
    border: 1px solid var(--color-line);
    border-radius: 20px;
    box-shadow: 0 4px 12px var(--capture-shadow);
  }
  .movable {
    cursor: grab;
    user-select: none;
  }
  .movable.dragging {
    cursor: grabbing;
  }
  .split-feedback {
    min-width: 0;
  }
  .split-detail {
    margin: 4px 0 0;
    font-size: 11px;
    line-height: 16px;
    color: var(--color-muted);
    overflow-wrap: anywhere;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
  }
  .capture-brand {
    position: relative;
    min-height: 24px;
    padding: 0;
    border: 0;
    background: var(--color-transparent);
    border-radius: var(--radius-sm);
    font-size: 13px;
    font-weight: 500;
    letter-spacing: -0.45px;
    white-space: nowrap;
    text-align: left;
  }
  .capture-brand::after {
    content: '';
    position: absolute;
    inset: -10px 0;
  }
  .capture-brand:hover:enabled {
    color: var(--color-accent);
    background: var(--color-transparent);
  }
  .capture-status {
    display: block;
    min-width: 0;
    color: var(--color-muted);
    font-size: 11px;
    line-height: 16px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .capture-time {
    color: var(--color-muted);
    font-family: var(--font-mono);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .capture-wave {
    width: 100%;
    height: 40px;
    min-width: 0;
  }
  .capture-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    min-width: 0;
  }
  .quick-actions {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    gap: 4px;
  }
  .quick-actions :global(button) {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
    width: 44px;
    height: 44px;
    min-height: 44px;
    min-width: 44px;
    padding: 0;
    border: 1px solid var(--color-transparent);
    border-radius: 12px;
    background: var(--color-transparent);
    color: var(--color-muted);
    transition:
      background-color var(--duration-hover) var(--ease-out),
      transform var(--duration-press) var(--ease-out);
  }
  .quick-actions :global(button:hover:enabled) {
    background: var(--color-raised);
    color: var(--color-ink);
  }
  .quick-actions :global(button:active:enabled) {
    transform: scale(0.985);
  }
  .quick-actions :global(button:focus-visible),
  .capture-brand:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }
  .quick-actions :global(button:disabled) {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .quick-actions :global(.record-toggle) {
    background: var(--color-tint);
    background-image: linear-gradient(180deg, var(--color-sheen), var(--color-transparent));
    color: var(--color-accent);
  }
  .quick-actions :global(.record-toggle:hover:enabled) {
    background: var(--color-line);
    color: var(--color-accent);
  }
  .failed .quick-actions :global(.record-toggle) {
    color: var(--color-error);
    background: var(--color-error-tint);
  }
  .complete .quick-actions :global(.record-toggle) {
    color: var(--color-accent);
  }
  .quick-actions :global(svg) {
    width: 18px;
    height: 18px;
  }
  .capture-ribbon {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 64px;
    padding: 8px 8px 8px 14px;
  }
  .ribbon-identity {
    flex: 0 1 96px;
    min-width: 74px;
  }
  .ribbon-signal {
    flex: 1;
    min-width: 16px;
  }
  .capture-underline {
    padding: 12px 14px 8px;
    border-radius: 16px;
  }
  .capture-underline .capture-time {
    margin-left: auto;
  }
  .underline-copy {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 60px;
  }
  .caption-text {
    flex: 1;
    min-width: 0;
    font-size: 15px;
    line-height: 21px;
    margin: 6px 0;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
    overflow-wrap: anywhere;
  }
  .capture-studio {
    padding: 10px 12px 8px;
  }
  .capture-studio .capture-wave {
    height: 56px;
    padding: 0 12px;
  }
  .studio-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    border-top: 1px solid var(--color-line);
    padding-top: 4px;
  }
  .studio-footer .quick-actions :global(.record-toggle) {
    width: auto;
    min-width: 80px;
    padding: 0 10px;
    font-size: 12px;
  }
  .capture-margin {
    width: 64px;
    margin-left: auto;
    display: flex;
    align-items: center;
    flex-direction: column;
    padding: 8px;
    gap: 8px;
    border-radius: 22px;
  }
  .margin-marker {
    width: 4px;
    height: 4px;
    margin: 8px 0 0;
    border-radius: 50%;
    background: var(--color-accent);
  }
  .margin-signal {
    width: 28px;
    height: 80px;
    position: relative;
  }
  .margin-signal .capture-wave {
    width: 80px;
    height: 28px;
    position: absolute;
    top: 26px;
    left: -26px;
    transform: rotate(90deg);
  }
  .capture-margin .quick-actions,
  .split-action-pod {
    flex-direction: column;
  }
  .margin-peek {
    position: absolute;
    right: calc(100% + 12px);
    top: 8px;
    width: 172px;
    padding: 10px 12px;
    border-radius: 16px;
    border: 1px solid var(--color-line);
    background: var(--color-panel);
    opacity: 0;
    visibility: hidden;
    transform: translateX(4px);
    transition:
      opacity 160ms var(--ease),
      transform 160ms var(--ease),
      visibility 160ms;
  }
  .margin-peek p {
    margin: 8px 0 0;
    color: var(--color-muted);
    font-size: 11px;
    line-height: 16px;
    overflow-wrap: anywhere;
    display: -webkit-box;
    -webkit-line-clamp: 7;
    line-clamp: 7;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .capture-margin:hover .margin-peek,
  .capture-margin:focus-within .margin-peek,
  .failed .margin-peek,
  .attention .margin-peek,
  .complete .margin-peek {
    opacity: 1;
    visibility: visible;
    transform: none;
  }
  .capture-split {
    display: flex;
    align-items: center;
    gap: 12px;
    background: var(--color-transparent);
    border: 0;
    box-shadow: none;
  }
  .split-signal-pod {
    flex: 1;
    min-width: 0;
    padding: 10px 16px;
    background: var(--color-panel);
    border: 1px solid var(--color-line);
    border-radius: 16px;
    box-shadow: 0 4px 12px var(--capture-shadow);
  }
  .split-action-pod {
    padding: 8px;
    background: var(--color-panel);
    border: 1px solid var(--color-line);
    border-radius: 20px;
    box-shadow: 0 4px 12px var(--capture-shadow);
  }
  .capture-counter {
    padding: 14px 16px 10px;
    border-radius: 24px;
  }
  .counter-clock {
    margin: 14px 0 4px;
  }
  .counter-clock .capture-time {
    color: var(--color-ink);
    font-size: 38px;
    letter-spacing: -2px;
    line-height: 44px;
  }
  .capture-counter .quick-actions {
    justify-content: space-between;
    margin-top: 10px;
  }
  .capture-note {
    margin: 8px 4px 0;
    padding: 6px 10px;
    border-radius: 10px;
    background: var(--color-panel);
    color: var(--color-muted);
    font-size: 11px;
    line-height: 16px;
    overflow-wrap: anywhere;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .warning {
    color: var(--color-error);
  }
  @media (prefers-reduced-motion: reduce) {
    .capture-brand,
    .margin-peek,
    .quick-actions :global(button) {
      transition: none;
    }
    .quick-actions :global(button:active:enabled) {
      transform: none;
    }
  }
</style>
