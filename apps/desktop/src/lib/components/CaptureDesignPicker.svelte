<script lang="ts">
  import { onMount } from 'svelte';
  import type { CaptureDesign } from '../config';
  import { captureDesignOptions } from '../captureDesigns';
  import { pitchCycles, reedPath } from '../voiceWaveform';
  import Select from './Select.svelte';
  import CaptureLayout from './CaptureLayout.svelte';
  import CaptureGlyph from './CaptureGlyph.svelte';
  import Icon from './Icon.svelte';

  let {
    value = $bindable<CaptureDesign>('split'),
    disabled = false,
    visible = true,
  }: { value?: CaptureDesign; disabled?: boolean; visible?: boolean } = $props();
  const duration = 9_000;
  let container: HTMLDivElement | undefined = $state();
  let wave: SVGSVGElement | undefined = $state();
  let fittedStroke = $state(3);
  let inView = $state(false);
  let pageVisible = $state(false);
  let reducedMotion = $state(false);
  let paused = $state(false);
  let elapsedMs = $state(0);
  const finished = $derived(elapsedMs >= duration);
  const stage = $derived(
    elapsedMs < 6_000 ? 'listening' : elapsedMs < 8_200 ? 'processing' : 'complete',
  );
  const heading = $derived(
    stage === 'listening'
      ? 'Listening'
      : stage === 'processing'
        ? 'Transcribing'
        : 'Transcript ready',
  );
  const status = $derived(
    stage === 'processing' ? 'Putting your words together…' : 'A thought, put into words.',
  );
  const elapsed = $derived(`0:0${Math.floor(Math.min(elapsedMs, 6_000) / 1_000)}`);
  const sample = $derived.by(() => {
    const t = elapsedMs / 1_000;
    // Explicitly simulated phrasing, quiet breath and pitch variation. The real
    // recording path still uses microphone telemetry; this preview has no IPC.
    const quiet = t > 2.4 && t < 3.1;
    const strength =
      stage === 'complete'
        ? 0
        : stage === 'processing'
          ? 0.3
          : quiet
            ? 0
            : Math.min(1, t / 0.25) * (0.25 + 0.45 * Math.sin(t * 2.3) ** 2);
    const cycles = stage === 'listening' ? pitchCycles(160 + 75 * Math.sin(t * 0.9)) : 3;
    return reedPath(strength, cycles, stage === 'processing' ? t * 2 : 0, false);
  });

  $effect(() => {
    // Reset the preview when the selected design changes.
    void value;
    elapsedMs = 0;
    paused = false;
  });
  $effect(() => {
    if (!wave) return;
    const observer = new ResizeObserver(([entry]) => {
      fittedStroke = Math.min(3, (entry.contentRect.width * 3) / 320);
    });
    observer.observe(wave);
    return () => observer.disconnect();
  });
  onMount(() => {
    const preference = matchMedia('(prefers-reduced-motion: reduce)');
    const motion = () => {
      reducedMotion = preference.matches;
    };
    const visibility = () => {
      pageVisible = !document.hidden;
    };
    motion();
    visibility();
    const observer = new IntersectionObserver(
      ([entry]) => {
        inView = entry.isIntersecting && entry.intersectionRatio > 0;
      },
      { threshold: [0, 0.01] },
    );
    if (container) observer.observe(container);
    preference.addEventListener('change', motion);
    document.addEventListener('visibilitychange', visibility);
    return () => {
      observer.disconnect();
      preference.removeEventListener('change', motion);
      document.removeEventListener('visibilitychange', visibility);
    };
  });
  $effect(() => {
    if (!visible || disabled || !inView || !pageVisible || reducedMotion || paused || finished)
      return;
    let frame: number;
    let last = performance.now();
    const draw = (now: number) => {
      // One bounded 30 fps loop, running only while this preview is visible.
      if (now - last >= 1_000 / 30) {
        elapsedMs = Math.min(duration, elapsedMs + Math.min(now - last, 100));
        last = now;
      }
      if (elapsedMs < duration) frame = requestAnimationFrame(draw);
    };
    frame = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(frame);
  });
  function toggleMotion() {
    if (finished) {
      elapsedMs = 0;
      paused = false;
    } else paused = !paused;
  }
</script>

<div class="capture-design-field">
  <label for="capture-design">Capture design</label>
  <Select
    id="capture-design"
    bind:value
    options={captureDesignOptions}
    {disabled}
    describedBy="capture-design-help"
  />
  <div
    bind:this={container}
    class="design-preview"
    aria-hidden="true"
    inert
    data-preview-stage={stage}
  >
    {#key value}
      <CaptureLayout
        design={value}
        {heading}
        {status}
        {elapsed}
        complete={stage === 'complete'}
        phase={stage === 'listening' ? 'recording' : stage === 'processing' ? 'processing' : 'idle'}
        recording={stage === 'listening'}
        {waveform}
        {actions}
      />
    {/key}
  </div>
  <div class="preview-controls">
    <p id="capture-design-help" class="note">
      Simulated preview · microphone off.{#if reducedMotion}
        Animation is off with Reduce Motion.{/if}
    </p>
    <button
      type="button"
      class="quiet preview-motion"
      disabled={disabled || reducedMotion}
      onclick={toggleMotion}
      aria-label={reducedMotion
        ? 'Preview animation disabled by Reduce Motion'
        : finished
          ? 'Replay preview'
          : paused
            ? 'Resume preview'
            : 'Pause preview'}
    >
      {#if finished}<Icon name="refresh" />{:else}<svg
          viewBox="0 0 20 20"
          fill="none"
          aria-hidden="true"
          ><path
            d={paused ? 'm7 4 8 6-8 6Z' : 'M7 4v12M13 4v12'}
            stroke="currentColor"
            stroke-width="1.6"
            stroke-linecap="round"
            stroke-linejoin="round"
          /></svg
        >{/if}
      {reducedMotion ? 'Motion reduced' : finished ? 'Replay' : paused ? 'Resume' : 'Pause'}
    </button>
  </div>
  <span class="visually-hidden" role="status"
    >{visible && inView
      ? `Preview: ${heading}${paused || reducedMotion ? ', paused' : ''}`
      : ''}</span
  >
</div>

{#snippet waveform()}
  <svg
    bind:this={wave}
    class="preview-wave"
    viewBox="0 0 320 100"
    preserveAspectRatio="none"
    aria-hidden="true"><path d={sample} style:stroke-width={fittedStroke} /></svg
  >
{/snippet}
{#snippet actions(labeled: boolean)}
  <button type="button" tabindex="-1" aria-label="Dismiss preview"><Icon name="close" /></button>
  <button
    type="button"
    class="record-toggle"
    tabindex="-1"
    aria-label={stage === 'listening'
      ? 'Stop preview'
      : stage === 'processing'
        ? 'Processing preview'
        : 'Completed preview'}
    >{#if stage === 'complete'}<Icon name="check" />{:else}<CaptureGlyph
        recording={stage === 'listening'}
      />{/if}{#if labeled}<span
        >{stage === 'listening' ? 'Stop' : stage === 'processing' ? 'Working' : 'Ready'}</span
      >{/if}</button
  >
{/snippet}

<style>
  .capture-design-field {
    margin: var(--space-6) 0;
  }
  .design-preview {
    display: flex;
    justify-content: center;
    align-items: center;
    min-height: 280px;
    padding: 16px 0;
    margin-top: var(--space-3);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    overflow: visible;
  }
  .design-preview :global(.layout-wrap) {
    animation: design-enter 180ms var(--ease) both;
  }
  @keyframes design-enter {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .design-preview :global(.layout-wrap) {
      animation: none;
    }
  }
  .preview-wave {
    display: block;
    width: 100%;
    height: 100%;
    fill: none;
    color: var(--color-accent);
  }
  .preview-wave path {
    stroke: currentColor;
    stroke-linecap: round;
    vector-effect: non-scaling-stroke;
  }
  .preview-controls {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin-top: var(--space-2);
  }
  .preview-controls .note {
    margin: 0;
  }
  .preview-motion {
    flex-shrink: 0;
    min-width: 100px;
    min-height: 44px;
    gap: var(--space-2);
  }
  .preview-motion :global(svg) {
    width: 16px;
    height: 16px;
  }
  @media (max-width: 560px) {
    .preview-controls {
      align-items: flex-start;
      flex-direction: column;
      gap: var(--space-1);
    }
  }
</style>
