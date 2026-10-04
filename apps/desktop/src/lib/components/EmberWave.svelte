<script lang="ts">
  import { onMount } from 'svelte';
  import { getAudioFeatures } from '../api/runtime';

  // Ember live waveform: 56 bars that sit flat while idle and dance in the
  // accent colour from measured microphone level while recording.
  const BARS = 56;
  const flat = () => Array<number>(BARS).fill(0);
  let {
    recording,
    desktop,
    visible = true,
  }: { recording: boolean; desktop: boolean; visible?: boolean } = $props();
  let levels = $state<number[]>(flat());
  let reducedMotion = $state(false);
  onMount(() => {
    const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
    const update = () => {
      reducedMotion = preference.matches;
    };
    update();
    preference.addEventListener('change', update);
    return () => preference.removeEventListener('change', update);
  });

  $effect(() => {
    const live = recording && desktop && visible && !reducedMotion;
    if (!live) {
      levels = flat();
      return;
    }
    let disposed = false;
    const timer = window.setInterval(() => void sample().catch(() => undefined), 110);
    async function sample() {
      if (disposed || document.hidden) return;
      let level = 0;
      try {
        const value = await getAudioFeatures();
        if (disposed || !Number.isFinite(value.level)) return;
        level = Math.max(0, Math.min(1, value.level));
      } catch {
        return;
      }
      const now = performance.now();
      levels = levels.map((_, index) => {
        const envelope = Math.sin(((index + 0.5) / BARS) * Math.PI);
        const wobble = 0.75 + 0.25 * Math.sin(now / 300 + index * 0.7);
        return Math.min(1, level * 5 * (0.3 + 0.7 * envelope) * wobble);
      });
    }
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  });
</script>

<div
  class="ember-wave"
  class:live={recording}
  role="img"
  aria-label={recording ? 'Recording level' : 'Microphone idle'}
>
  {#each levels as level, index (index)}<i style:height={`${3 + Math.round(level * 49)}px`}
    ></i>{/each}
</div>

<style>
  .ember-wave {
    display: flex;
    align-items: center;
    gap: 3px;
    height: 56px;
    overflow: clip;
  }
  .ember-wave i {
    display: block;
    width: 3px;
    min-height: 3px;
    flex: none;
    border-radius: 2px;
    background: var(--color-muted);
    opacity: 0.5;
  }
  .ember-wave.live i {
    background: var(--color-accent);
    opacity: 1;
  }
</style>
