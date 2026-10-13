<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { CaptureDesign } from '../config';
  import { captureDesignOptions } from '../captureDesigns';
  import BrandLogo from './BrandLogo.svelte';

  type CapturePhase = 'idle' | 'recording' | 'processing';

  let {
    design,
    heading,
    status = '',
    elapsed,
    failed = false,
    attention = false,
    complete = false,
    detailVisible = false,
    phase = 'idle',
    recording = false,
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
    phase?: CapturePhase;
    recording?: boolean;
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
  // Spec states (vesper-floating-capture.html): rec covers the idle shell and
  // live recording; the ring, timer tone and wave motion key off `recording`.
  const state = $derived(complete ? 'done' : phase === 'processing' ? 'proc' : 'rec');
  const showDetail = $derived(
    (failed || attention || complete || detailVisible) && Boolean(status),
  );
  const capsuleMessage = $derived(state === 'proc' ? heading : status);
  const showCapsuleMessage = $derived(
    state === 'proc' || ((state === 'done' || failed || attention) && Boolean(status)),
  );
  const width = $derived(
    captureDesignOptions.find((option) => option.value === design)?.width ?? 428,
  );
</script>

{#snippet brandMark()}
  <button
    type="button"
    class="capture-bird"
    onclick={onOpen}
    aria-label="Open Vesper"
    title="Open Vesper"><BrandLogo mark decorative /></button
  >
{/snippet}
{#snippet stateMorph()}
  <span class="capture-morph" aria-hidden="true">
    {#if state === 'proc'}<span class="morph-dots"><b></b><b></b><b></b></span>
    {:else if state === 'done'}<svg class="morph-check" viewBox="0 0 24 24"
        ><path d="M5 12.5l4.5 4.5L19 7" /></svg
      >
    {:else}<span class="morph-live"></span>{/if}
  </span>
{/snippet}
{#snippet timer()}
  <span class="capture-time" aria-label="Recording duration">{elapsed}</span>
{/snippet}
{#snippet statusHeading()}
  <span
    class="capture-status"
    class:warning={failed || attention}
    aria-live="polite"
    title={status || heading}>{heading}</span
  >
{/snippet}

<div
  class="layout-wrap"
  class:failed
  class:attention
  class:complete
  style:--capture-width={`${width}px`}
  data-capture-design={design}
  data-state={state}
  data-live={recording}
>
  {#if design === 'capsule'}
    <div
      class="glass capture capsule"
      class:movable={Boolean(onDrag)}
      class:dragging
      use:dragSurface
    >
      {@render brandMark()}
      <div class="capsule-wave capture-wave">{@render waveform()}</div>
      {#if showCapsuleMessage}<div class="capsule-message">
          {@render stateMorph()}<b class="capsule-label">{capsuleMessage}</b>
        </div>{/if}
      {@render timer()}
      <div class="quick-actions">{@render actions(false)}</div>
    </div>
  {:else if design === 'island'}
    <div
      class="glass capture island"
      class:movable={Boolean(onDrag)}
      class:dragging
      use:dragSurface
    >
      <div class="island-top">
        {@render brandMark()}
        <div class="island-wave capture-wave">{@render waveform()}</div>
        {@render timer()}
        <div class="quick-actions">{@render actions(false)}</div>
      </div>
      {#if showDetail}<div class="island-expander">
          <div class="island-status">{@render stateMorph()}{@render statusHeading()}</div>
          <p class="capture-detail" title={status}>{status}</p>
        </div>{/if}
    </div>
  {:else}
    <div class="split-wrap">
      <div
        class="glass capture split-card"
        class:movable={Boolean(onDrag)}
        class:dragging
        use:dragSurface
      >
        <div class="split-head">
          {@render brandMark()}<b class="split-name">vesper</b>
          {@render timer()}
        </div>
        <div class="split-wave capture-wave">{@render waveform()}</div>
        <div class="split-foot">{@render stateMorph()}{@render statusHeading()}</div>
        {#if showDetail}<div class="split-expander">
            <p class="capture-detail" title={status}>{status}</p>
          </div>{/if}
      </div>
      <div class="glass split-actions quick-actions">{@render actions(false)}</div>
    </div>
  {/if}
</div>

<style>
  .layout-wrap {
    width: min(var(--capture-width), 100%);
    color: var(--color-ink);
    font-family: var(--font-body);
  }
  /* Glass blends the overlay with the desktop behind it; the native window is
   * transparent, so the blur reads as frosted glass rather than a solid card. */
  .glass {
    background: var(--color-glass);
    border: 1px solid var(--color-glass-line);
    -webkit-backdrop-filter: blur(28px) saturate(1.6);
    backdrop-filter: blur(28px) saturate(1.6);
    /* The native window leaves a 12px gutter around the card; the shadow is
     * sized to fade inside it. A larger blur clips at the window edge and
     * paints a hard dark band beside the pill. */
    box-shadow:
      0 6px 16px var(--color-glass-shadow),
      inset 0 1px 0 rgb(255 255 255 / 8%);
  }
  .movable {
    cursor: grab;
    user-select: none;
  }
  .movable.dragging {
    cursor: grabbing;
  }
  .capture-bird {
    display: grid;
    flex: none;
    place-items: center;
    width: 32px;
    height: 32px;
    min-height: 0;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: var(--color-transparent);
  }
  /* The lockup sizes itself from container query units, which collapse inside
   * a shrink-to-fit button and let the wave slide under the visible bird, so
   * the overlay pins the mark geometry instead. */
  .capture-bird :global(.brand-logo) {
    width: 29px;
    container-type: normal;
    font-size: 30px;
  }
  .capture-bird :global(.brand-logo svg) {
    height: 22px;
  }
  .capture-bird:hover:enabled {
    background: var(--color-transparent);
  }
  .capture-time {
    flex: none;
    color: var(--color-glass-muted);
    font-size: 14px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  [data-live='true'] .capture-time {
    color: var(--color-ink);
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
  .capture-status.warning {
    color: var(--color-error);
  }
  .capture-detail {
    margin: 0;
    color: var(--color-glass-muted);
    font-size: 14.5px;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }
  /* Status morph: pulsing live dot while recording, bouncing dots while the
   * transcript is prepared, drawn check when it lands. */
  .capture-morph {
    display: inline-grid;
    flex: none;
    place-items: center;
    width: 18px;
    height: 18px;
  }
  .morph-live {
    display: block;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--color-accent);
    animation: morph-breathe 1.4s ease-in-out infinite;
  }
  .morph-dots {
    display: inline-flex;
    gap: 3px;
  }
  .morph-dots b {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: var(--color-accent);
    animation: morph-breathe 1s ease-in-out infinite;
  }
  .morph-dots b:nth-child(2) {
    animation-delay: 0.12s;
  }
  .morph-dots b:nth-child(3) {
    animation-delay: 0.24s;
  }
  .morph-check {
    width: 18px;
    height: 18px;
    fill: none;
    stroke: var(--color-accent);
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
    color: var(--color-accent);
  }
  @keyframes morph-breathe {
    0%,
    100% {
      transform: translateY(0);
      opacity: 0.4;
    }
    50% {
      transform: translateY(-4px);
      opacity: 1;
    }
  }
  @keyframes capture-ring {
    from {
      transform: scale(1);
      opacity: 0.6;
    }
    to {
      transform: scale(1.5);
      opacity: 0;
    }
  }
  /* Circular glass actions shared by all three variants. */
  .quick-actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 12px;
  }
  .quick-actions :global(button) {
    position: relative;
    display: grid;
    flex: none;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: var(--color-transparent);
    color: var(--color-glass-muted);
    transition:
      background-color var(--duration-hover) var(--ease-out),
      color var(--duration-hover) var(--ease-out),
      transform 0.3s var(--ease-spring);
  }
  .quick-actions :global(button:hover:enabled) {
    background: var(--color-glass-wash);
    color: var(--color-ink);
  }
  .quick-actions :global(button:active:enabled) {
    transform: scale(0.92);
  }
  .quick-actions :global(button:focus-visible),
  .capture-bird:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }
  .quick-actions :global(button:disabled) {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .quick-actions :global(svg) {
    width: 18px;
    height: 18px;
  }
  .quick-actions :global(.record-toggle) {
    background: var(--color-accent);
    color: var(--color-on-accent);
  }
  .quick-actions :global(.record-toggle:hover:enabled) {
    background: var(--color-accent);
    color: var(--color-on-accent);
    transform: scale(1.06);
  }
  [data-live='true'] .quick-actions :global(.record-toggle::after) {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: 50%;
    border: 2px solid var(--color-accent);
    animation: capture-ring 1.6s var(--ease) infinite;
    pointer-events: none;
  }
  /* Mock contract: the confirm button stays accent-filled in every state;
   * failure is signalled by the heading and detail text, never a dark disc. */
  /* Capsule: one slim pill. */
  .capsule {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 58px;
    min-width: 400px;
    padding: 0 9px 0 20px;
    border-radius: 999px;
  }
  .capsule-wave {
    flex: 1;
    min-width: 0;
    height: 24px;
  }
  .capsule-message {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  [data-state='rec'] .capsule-wave {
    flex: 1;
  }
  [data-state='proc'] .capsule-wave,
  [data-state='done'] .capsule-wave {
    display: none;
  }
  [data-state='proc'] .capsule-message,
  [data-state='done'] .capsule-message {
    display: flex;
  }
  .capsule-label {
    min-width: 0;
    font-size: 14.5px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Island: compact pill that grows downward with the transcript. */
  .island {
    width: 400px;
    padding: 9px 9px 9px 20px;
    border-radius: 32px;
  }
  .island-top {
    display: flex;
    align-items: center;
    gap: 12px;
    /* The 44px action targets need a 44px row; the spec draws 40px buttons. */
    min-height: 44px;
  }
  .island-wave {
    flex: 1;
    min-width: 0;
    height: 22px;
  }
  .island-expander {
    display: grid;
    grid-template-rows: 1fr;
  }
  .island-status {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 20px 6px 0;
  }
  .island-status .capture-status {
    color: var(--color-glass-muted);
    font-size: 14.5px;
    line-height: 1.45;
  }
  .island-expander .capture-detail {
    padding: 0 20px 8px 0;
  }
  /* Split: glass card plus a separate action pill. */
  .split-wrap {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .split-actions {
    padding: 8px;
    border-radius: 999px;
  }
  .split-actions.quick-actions {
    flex-direction: column;
    gap: 6px;
  }
  .split-card {
    width: 350px;
    padding: 16px 20px;
    border-radius: 26px;
  }
  .split-head {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 12px;
  }
  .split-name {
    flex: 1;
    font-size: 16px;
    font-weight: 800;
    letter-spacing: -0.03em;
  }
  .split-head .capture-time {
    color: var(--color-glass-muted);
  }
  [data-live='true'] .split-head .capture-time {
    color: var(--color-ink);
  }
  .split-wave {
    height: 34px;
    margin-bottom: 12px;
  }
  .split-foot {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .split-foot .capture-status {
    color: var(--color-glass-muted);
    font-size: 14.5px;
    line-height: 1.45;
  }
  .split-expander {
    display: grid;
    grid-template-rows: 1fr;
  }
  .split-expander .capture-detail {
    padding-top: 8px;
  }
  .capsule-wave :global(svg),
  .capsule-wave :global(.capture-wave-inner),
  .island-wave :global(svg),
  .split-wave :global(svg) {
    width: 100%;
    height: 100%;
  }
  @media (prefers-reduced-motion: reduce) {
    .morph-live,
    .morph-dots b,
    [data-live='true'] .quick-actions :global(.record-toggle::after) {
      animation: none;
    }
    .quick-actions :global(button),
    .quick-actions :global(button:active:enabled) {
      transition: none;
      transform: none;
    }
    .quick-actions :global(.record-toggle:hover:enabled) {
      transform: none;
    }
  }
  @media (max-width: 520px) {
    .capsule {
      min-width: 0;
      width: 100%;
    }
    .split-card,
    .island {
      width: min(350px, 100%);
    }
  }
</style>
