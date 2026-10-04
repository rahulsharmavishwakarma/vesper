<script lang="ts">
  import { onMount } from 'svelte';
  import artwork from '../../assets/ember-bird.svg?raw';

  let {
    animate = false,
    decorative = false,
    large = false,
    mark = false,
  }: { animate?: boolean; decorative?: boolean; large?: boolean; mark?: boolean } = $props();
  // The Ember lockup pairs the bird mark with a Bricolage Grotesque wordmark;
  // collapsed rails show the bird alone. Only a trusted, bundled SVG is accepted.
  let replay = $state(0);
  let wasAnimating = false;

  let reducedMotion = $state(false);
  $effect(() => {
    // A fresh About opening replays the idle call motion; otherwise the
    // ambient chirp and blink continue undisturbed.
    if (animate && !wasAnimating && !reducedMotion) replay += 1;
    wasAnimating = animate;
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

<span class="brand-logo" class:large aria-hidden={decorative ? 'true' : undefined}>
  <span class="ember-lockup">
    {#key replay}
      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
      {@html artwork}
    {/key}
    {#if !mark}<b class="ember-word">vesper</b>{/if}
  </span>
</span>

<style>
  /* Ember lockup: bird mark plus Bricolage wordmark, sized by container width
   * so the sidebar (160px), compact header (120px) and About (224px) all
   * scale from the same lockup. Idle motion touches the call dashes and the
   * eye-lid highlight only; the bird body and wordmark never animate. */
  .brand-logo {
    display: block;
    width: 160px;
    max-width: 100%;
    container-type: inline-size;
    flex: none;
  }
  .large {
    width: 224px;
  }
  .ember-lockup {
    display: inline-flex;
    align-items: center;
    gap: 0.3em;
    white-space: nowrap;
    font-size: 30px;
    font-size: 19cqw;
  }
  .brand-logo :global(svg) {
    display: block;
    height: 1.15em;
    width: auto;
    overflow: visible;
    color: var(--color-accent);
  }
  .brand-logo :global(.bd),
  .brand-logo :global(.dsh),
  .brand-logo :global(.lid) {
    fill: currentColor;
  }
  .ember-word {
    font-family: var(--font-display);
    font-weight: 800;
    letter-spacing: -0.035em;
    line-height: 1;
    color: var(--color-ink);
  }
  .brand-logo :global(.dsh) {
    transform-box: fill-box;
    transform-origin: left center;
    animation: ember-chirp 2.6s ease-in-out infinite;
  }
  .brand-logo :global(.d1) {
    animation-delay: 0.35s;
  }
  .brand-logo :global(.lid) {
    transform-box: fill-box;
    transform-origin: center;
    transform: scaleY(0);
    animation: ember-blink 4.2s ease-in-out infinite;
  }
  .brand-logo :global(svg) {
    transition: transform 0.5s var(--ease-spring);
  }
  .brand-logo:hover :global(svg) {
    transform: rotate(-8deg) translateY(-1px);
  }
  @keyframes ember-chirp {
    0%,
    55%,
    100% {
      opacity: 1;
      transform: scale(1);
    }
    20% {
      opacity: 0.12;
      transform: scale(0.7);
    }
    38% {
      opacity: 1;
      transform: scale(1.08);
    }
  }
  @keyframes ember-blink {
    0%,
    90%,
    100% {
      transform: scaleY(0);
    }
    94%,
    96% {
      transform: scaleY(1);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .brand-logo :global(.dsh),
    .brand-logo :global(.lid) {
      animation: none;
    }
    .brand-logo :global(svg) {
      transition: none;
    }
    .brand-logo:hover :global(svg) {
      transform: none;
    }
  }
</style>
