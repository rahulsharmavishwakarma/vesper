<script lang="ts">
  import { onMount } from 'svelte';
  import artwork from '../../assets/vesper.svg?raw';

  let {
    animate = false,
    decorative = false,
    large = false,
  }: { animate?: boolean; decorative?: boolean; large?: boolean } = $props();

  let reducedMotion = $state(false);
  let interrupted = $state(false);
  $effect(() => {
    if (!animate) interrupted = false;
    else if (reducedMotion) interrupted = true;
  });
  onMount(() => {
    const preference = matchMedia('(prefers-reduced-motion: reduce)');
    const update = () => {
      reducedMotion = preference.matches;
    };
    update();
    preference.addEventListener('change', update);
    return () => preference.removeEventListener('change', update);
  });
</script>

<span
  class="brand-logo"
  class:animate={animate && !interrupted}
  class:large
  aria-hidden={decorative ? 'true' : undefined}
>
  <!-- Only a trusted, bundled SVG is accepted; no caller-supplied markup. -->
  <!-- eslint-disable-next-line svelte/no-at-html-tags -->
  {@html artwork}
</span>

<style>
  /* Non-interactive SVG; the host retains its existing button states. */
  .brand-logo {
    --duration-logo-call: 480ms;
    --ease-logo-call: cubic-bezier(0.16, 1, 0.3, 1);
    display: block;
    width: 160px;
    max-width: 100%;
    flex: none;
  }
  .large {
    width: 224px;
  }
  .brand-logo :global(svg) {
    display: block;
    width: 100%;
    height: auto;
  }
  .brand-logo :global(.vesper-mark) {
    color: var(--color-accent);
  }
  .brand-logo :global(.vesper-wordmark) {
    color: var(--color-ink);
  }
  .animate :global(.vesper-call-1) {
    animation: first-call var(--duration-logo-call) var(--ease-logo-call) 120ms both;
  }
  .animate :global(.vesper-call-2) {
    animation: second-call var(--duration-logo-call) var(--ease-logo-call) 360ms both;
  }
  @keyframes first-call {
    from {
      opacity: 0.18;
      transform: translate(-3px, 2px);
    }
    to {
      opacity: 1;
      transform: translate(0, 0);
    }
  }
  @keyframes second-call {
    from {
      opacity: 0.18;
      transform: translateX(-3px);
    }
    to {
      opacity: 1;
      transform: translateX(0);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .brand-logo :global(.vesper-call-1),
    .brand-logo :global(.vesper-call-2) {
      animation: none;
    }
  }
</style>
