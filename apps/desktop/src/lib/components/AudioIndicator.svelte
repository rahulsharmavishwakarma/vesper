<script lang="ts">
  import { onMount } from 'svelte';
  import { getAudioFeatures } from '../api/runtime';
  import { followSignal, pitchCycles, reedPath } from '../voiceWaveform';
  import { PitchDisplay } from '../pitchDisplay';

  let {
    recording,
    processing = false,
    complete = false,
    failed = false,
    desktop,
    visible = true,
    compact = false,
    fit = false,
    onSignal,
  }: {
    recording: boolean;
    processing?: boolean;
    complete?: boolean;
    failed?: boolean;
    desktop: boolean;
    visible?: boolean;
    compact?: boolean;
    fit?: boolean;
    onSignal?: () => void;
  } = $props();
  let rawLevel = $state(0);
  let pitchHz = $state<number | null>(null);
  let unavailable = $state(false);
  let pageVisible = $state(true);
  let reducedMotion = $state(false);
  let trace = $state('');
  let container: HTMLDivElement | undefined = $state();
  let fittedStroke = $state(3);
  $effect(() => {
    if (!fit || !container) return;
    const observer = new ResizeObserver(([entry]) => {
      fittedStroke = Math.min(3, (entry.contentRect.width * 3) / 320);
    });
    observer.observe(container);
    return () => observer.disconnect();
  });
  onMount(() => {
    const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
    const updateMotion = () => {
      reducedMotion = preference.matches;
    };
    updateMotion();
    const updateVisibility = () => {
      pageVisible = document.visibilityState !== 'hidden';
    };
    updateVisibility();
    preference.addEventListener('change', updateMotion);
    document.addEventListener('visibilitychange', updateVisibility);
    return () => {
      preference.removeEventListener('change', updateMotion);
      document.removeEventListener('visibilitychange', updateVisibility);
    };
  });
  const energy = $derived(
    recording && Number.isFinite(rawLevel)
      ? Math.max(0, Math.min(1, (20 * Math.log10(Math.max(rawLevel, 0.00001)) + 60) / 48))
      : 0,
  );

  $effect(() => {
    const enabled = recording && desktop && visible && pageVisible;
    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let staleTimer: ReturnType<typeof setTimeout> | undefined;
    const pitchDisplay = new PitchDisplay();
    rawLevel = 0;
    pitchHz = null;
    unavailable = false;
    function clearSignal() {
      rawLevel = 0;
      pitchHz = null;
      pitchDisplay.reset();
    }
    async function sample() {
      let delay: number;
      const requestedAt = performance.now();
      try {
        const value = await getAudioFeatures();
        if (disposed) return;
        const now = performance.now();
        const roundtrip = now - requestedAt;
        // Single-flight cadence includes bridge time instead of adding it to
        // every 40 ms interval. Never catch up by queuing multiple requests.
        delay = Math.max(0, 40 - roundtrip);
        unavailable = !Number.isFinite(value.level);
        if (staleTimer) clearTimeout(staleTimer);
        if (unavailable || roundtrip >= 200) {
          // A late reply must not revive audio that has already expired.
          clearSignal();
          delay = 40;
        } else {
          rawLevel = Math.max(0, Math.min(1, value.level));
          pitchHz = pitchDisplay.update(rawLevel > 0.001 ? value.pitchHz : null, now);
          if (rawLevel >= 0.008) onSignal?.();
          staleTimer = setTimeout(clearSignal, 200 - roundtrip);
        }
      } catch {
        if (disposed) return;
        clearSignal();
        unavailable = true;
        delay = 250;
      }
      if (!disposed)
        timer = setTimeout(() => {
          void sample();
        }, delay);
    }
    if (enabled) void sample();
    return () => {
      disposed = true;
      if (timer) clearTimeout(timer);
      if (staleTimer) clearTimeout(staleTimer);
    };
  });

  $effect(() => {
    const active = (recording || processing) && visible && pageVisible && !failed && !unavailable;
    const quietMotion = reducedMotion;
    let frame: number | undefined;
    trace = reedPath(quietMotion && active ? 0.2 : 0, 3, 0, compact);
    if (active && !quietMotion) {
      let last = performance.now();
      let phase = 0;
      let amplitude = 0;
      let cycles = 3;
      function draw(now: number) {
        const elapsed = Math.min(now - last, 50);
        last = now;
        const target = recording ? energy : 0.35;
        // Fast attack, softer release. Pitch validity is conditioned separately
        // so consonants do not delay the response to measured loudness.
        amplitude = followSignal(amplitude, target, elapsed, target > amplitude ? 20 : 70);
        const targetCycles = recording ? pitchCycles(pitchHz) : 3;
        cycles = followSignal(cycles, targetCycles, elapsed, 35);
        if (amplitude < 0.002 && target === 0) amplitude = 0;
        // Recording shape changes only with measured input. A clock-driven
        // travelling wave is reserved for indeterminate transcription progress.
        if (!recording && amplitude > 0) phase = (phase + elapsed * 0.002) % (Math.PI * 2);
        trace = reedPath(amplitude, cycles, phase, compact);
        frame = requestAnimationFrame(draw);
      }
      frame = requestAnimationFrame(draw);
    }
    return () => {
      if (frame !== undefined) cancelAnimationFrame(frame);
    };
  });
</script>

<div
  bind:this={container}
  class="voice-indicator"
  class:fit
  style:--fitted-stroke={fittedStroke}
  class:compact
  class:listening={recording}
  class:thinking={processing && !recording}
  class:complete
  class:failed={failed || unavailable}
  style:--energy={energy}
  role="img"
  aria-label={unavailable
    ? 'Microphone level unavailable'
    : failed
      ? 'Recording needs attention'
      : recording
        ? energy > 0
          ? 'Microphone signal detected'
          : 'Listening for microphone input'
        : processing
          ? 'Processing recording'
          : complete
            ? 'Transcription complete'
            : 'Microphone idle'}
>
  <svg
    class="voice-waveform"
    viewBox="0 0 320 100"
    preserveAspectRatio={fit ? 'none' : 'xMidYMid meet'}
    fill="none"
    aria-hidden="true"
  >
    <path class="wave-trace" d={trace} />
  </svg>
</div>

<style>
  /* W02 Reed · Hallmark: P5 H5 E5 S5 R5 V4. Measured volume and pitch. */
  .voice-indicator {
    display: grid;
    place-items: center;
    width: 100%;
    min-width: 0;
    height: var(--voice-height, 140px);
    margin: var(--space-2) auto;
    color: var(--color-muted);
  }
  .voice-waveform {
    width: min(var(--voice-width, 320px), 100%);
    height: auto;
    overflow: visible;
  }
  .voice-waveform path {
    stroke: currentColor;
    stroke-linecap: round;
    vector-effect: non-scaling-stroke;
  }
  .wave-trace {
    stroke-width: 3;
    opacity: 0.35;
    transition: opacity 100ms linear;
  }
  .listening,
  .thinking,
  .complete {
    color: var(--color-accent);
  }
  .listening .wave-trace {
    opacity: calc(0.45 + var(--energy) * 0.55);
  }
  .thinking .wave-trace {
    opacity: 0.7;
  }
  .failed {
    color: var(--color-error);
  }
  .compact {
    height: 46px;
    margin: 0;
  }
  .compact .voice-waveform {
    width: min(148px, 100%);
    max-height: 46px;
  }
  .compact .wave-trace {
    stroke-width: 1.6;
  }
  .fit {
    display: block;
    height: 100%;
    margin: 0;
  }
  .fit .voice-waveform {
    width: 100%;
    height: 100%;
  }
  .fit .wave-trace {
    stroke-width: var(--fitted-stroke);
  }
  @media (prefers-reduced-motion: reduce) {
    .voice-waveform path {
      transition: none;
    }
  }
</style>
