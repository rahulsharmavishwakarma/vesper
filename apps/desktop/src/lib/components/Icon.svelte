<script lang="ts">
  import { onMount } from 'svelte';
  // Ember icon set: 24px grid, 1.8px rounded stroke, drawn in the current text
  // colour. Each glyph carries one animated detail (class "a", "k1"/"k2",
  // "w" bars or "l" lines); whole-icon motion targets the svg root instead.
  const simplePaths = {
    chevron: 'm6 9 6 6 6-6',
    copy: 'M8 8h12v12H8zM16 8V4H4v12h4',
    search: 'M21 21l-5-5M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0Z',
    download: 'M12 3v13m-5-5 5 5 5-5M4 16v4a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-4',
    downloaded: 'm7 8 3 3 7-7M4 14v6h16v-6M4 16h16',
    warning: 'M12 3 2 21h20L12 3Zm0 6v5m0 3v1',
    unknown:
      'M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20Zm-3-14a3 3 0 1 1 5 2.2c-1.2.8-2 1.3-2 2.8m0 3v1',
    unavailable: 'M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20ZM5 5l14 14',
    stop: 'M6 6h12v12H6z',
    like: 'M7 10v11H3V10h4Zm0 10h10a2 2 0 0 0 2-1.6l1.4-7A2 2 0 0 0 18.4 9H13l1-5a2 2 0 0 0-2-2L7 10',
    dislike:
      'M7 14V3H3v11h4Zm0-10h10a2 2 0 0 1 2 1.6l1.4 7a2 2 0 0 1-2 2.4H13l1 5a2 2 0 0 1-2 2l-5-8',
    plus: 'M12 5v14M5 12h14',
    refresh: 'M20 12a8 8 0 1 1-2.5-5.8M20 4v5h-5',
    close: 'M6 6l12 12M18 6L6 18',
  };
  type IconName =
    | keyof typeof simplePaths
    | 'mic'
    | 'history'
    | 'models'
    | 'settings'
    | 'upload'
    | 'downloading'
    | 'arrow'
    | 'check'
    | 'spark'
    | 'book'
    | 'wave'
    | 'globe'
    | 'trash'
    | 'record';
  const waveBars = ['M4 10v4', 'M8 6v12', 'M12 3v18', 'M16 8v8', 'M20 11v2'];
  const wholeIconMotion = new Set(['refresh', 'plus', 'close']);
  const spring = 'cubic-bezier(.34,1.56,.64,1)';
  const ease = 'cubic-bezier(.22,1,.36,1)';

  let {
    name,
    active = false,
    motionKey,
  }: { name: IconName; active?: boolean; motionKey?: number } = $props();
  let element: SVGSVGElement | undefined = $state();
  let wasSelected = false;

  function framesFor(
    glyph: string,
    index: number,
    trigger: 'nav' | 'hover',
    repeated = false,
  ): { frames: Keyframe[]; options: KeyframeAnimationOptions; whole: boolean } {
    if (trigger === 'nav') {
      const duration =
        Number.parseFloat(
          getComputedStyle(element as Element).getPropertyValue('--duration-nav-icon'),
        ) || 620;
      const easing =
        getComputedStyle(element as Element)
          .getPropertyValue('--ease-nav-icon')
          .trim() || 'cubic-bezier(.35,0,.2,1)';
      if (glyph === 'settings') {
        const end = index === 0 ? 7 : -7;
        return {
          whole: false,
          frames: repeated
            ? [
                { transform: `translateX(${end}px)` },
                { transform: `translateX(${end / 2}px)`, offset: 0.4 },
                { transform: `translateX(${end}px)` },
              ]
            : [{ transform: 'translateX(0)' }, { transform: `translateX(${end}px)` }],
          options: { duration, easing },
        };
      }
      if (glyph === 'history') {
        return {
          whole: false,
          frames: [{ transform: 'rotate(-55deg)' }, { transform: 'rotate(0)' }],
          options: { duration, easing },
        };
      }
      if (glyph === 'mic') {
        return {
          whole: false,
          frames: [
            { transform: 'scaleY(1)', opacity: 1 },
            { transform: 'scaleY(.45)', opacity: 0.55, offset: 0.35 },
            { transform: 'scaleY(1)', opacity: 1 },
          ],
          options: { duration, easing },
        };
      }
      return {
        whole: false,
        frames: [
          { transform: 'scale(.72)', opacity: 0.4 },
          { transform: 'scale(1)', opacity: 1 },
        ],
        options: { duration, easing },
      };
    }
    switch (glyph) {
      case 'mic':
        return {
          whole: false,
          frames: [
            { transform: 'translateY(0)' },
            { transform: 'translateY(-2.5px)', offset: 0.4 },
            { transform: 'translateY(1px)', offset: 0.7 },
            { transform: 'translateY(0)' },
          ],
          options: { duration: 700, easing: ease },
        };
      case 'history':
        return {
          whole: false,
          frames: [{ transform: 'rotate(0)' }, { transform: 'rotate(360deg)' }],
          options: { duration: 600, easing: ease },
        };
      case 'models':
        return {
          whole: false,
          frames: [{ transform: 'scale(1)' }, { transform: 'scale(1.35)' }],
          options: { duration: 500, easing: ease, direction: 'alternate', iterations: 2 },
        };
      case 'settings': {
        const end = index === 0 ? 7 : -7;
        return {
          whole: false,
          frames: [{ transform: 'translateX(0)' }, { transform: `translateX(${end}px)` }],
          options: { duration: 450, easing: spring },
        };
      }
      case 'upload':
        return {
          whole: false,
          frames: [{ transform: 'translateY(0)' }, { transform: 'translateY(-2.5px)' }],
          options: { duration: 450, easing: spring, direction: 'alternate', iterations: 2 },
        };
      case 'refresh':
        return {
          whole: true,
          frames: [{ transform: 'rotate(0)' }, { transform: 'rotate(360deg)' }],
          options: { duration: 600, easing: ease },
        };
      case 'arrow':
        return {
          whole: false,
          frames: [{ transform: 'translateX(0)' }, { transform: 'translateX(3px)' }],
          options: { duration: 450, easing: spring, direction: 'alternate', iterations: 2 },
        };
      case 'check':
        return {
          whole: false,
          frames: [{ strokeDashoffset: 1 }, { strokeDashoffset: 0 }],
          options: { duration: 600, easing: ease },
        };
      case 'plus':
      case 'close':
        return {
          whole: true,
          frames: [{ transform: 'rotate(0)' }, { transform: 'rotate(90deg)' }],
          options: { duration: 450, easing: spring },
        };
      case 'spark':
        return {
          whole: false,
          frames: [{ transform: 'rotate(0) scale(1)' }, { transform: 'rotate(90deg) scale(1.15)' }],
          options: { duration: 450, easing: spring },
        };
      case 'book':
        return {
          whole: false,
          frames: [{ transform: 'scaleX(0)' }, { transform: 'scaleX(1)' }],
          options: { duration: 600, easing: ease },
        };
      case 'wave':
        return {
          whole: false,
          frames: [{ transform: 'scaleY(1)' }, { transform: 'scaleY(.35)' }],
          options: {
            duration: 350,
            easing: 'ease-in-out',
            direction: 'alternate',
            iterations: 4,
            delay: index * 60,
          },
        };
      case 'globe':
        return {
          whole: false,
          frames: [
            { transform: 'scaleX(1)' },
            { transform: 'scaleX(.1)', offset: 0.5 },
            { transform: 'scaleX(1)' },
          ],
          options: { duration: 700, easing: ease, iterations: 2 },
        };
      case 'trash':
        return {
          whole: false,
          frames: [
            { transform: 'rotate(0) translate(0,0)' },
            { transform: 'rotate(-14deg) translate(-1px,-2px)' },
          ],
          options: { duration: 450, easing: spring },
        };
      case 'record':
        return {
          whole: false,
          frames: [{ transform: 'scale(1)' }, { transform: 'scale(.7)' }],
          options: { duration: 450, easing: spring, direction: 'alternate', iterations: 2 },
        };
      default:
        return {
          whole: false,
          frames: [
            { transform: 'scale(.72)', opacity: 0.4 },
            { transform: 'scale(1)', opacity: 1 },
          ],
          options: { duration: 620, easing: 'cubic-bezier(.35,0,.2,1)' },
        };
    }
  }

  function play(trigger: 'nav' | 'hover', repeated = false): () => void {
    const node = element;
    if (!node) return () => {};
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)');
    if (reduced.matches || document.hidden) return () => {};
    const glyph = name;
    const animations: Animation[] = [];
    if (wholeIconMotion.has(glyph) && trigger === 'hover') {
      const { frames, options } = framesFor(glyph, 0, trigger, repeated);
      animations.push(node.animate(frames, options));
    } else {
      node.querySelectorAll<SVGElement>('.nav-detail').forEach((part, index) => {
        const { frames, options } = framesFor(glyph, index, trigger, repeated);
        animations.push(part.animate(frames, options));
      });
    }
    return () => animations.forEach((animation) => animation.cancel());
  }

  // A new key acknowledges a navigation click, including the selected tab.
  // Other uses of Icon stay static; model polling never restarts this motion.
  $effect(() => {
    const sequence = motionKey;
    const selected = active;
    const node = element;
    if (sequence === undefined || !node) return;
    const repeated = wasSelected;
    wasSelected = selected;
    if (!selected || sequence === 0) return;
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)');
    const cancel = play('nav', repeated);
    const visibility = () => {
      if (document.hidden) cancel();
    };
    const preference = () => {
      if (reduced.matches) cancel();
    };
    reduced.addEventListener('change', preference);
    document.addEventListener('visibilitychange', visibility);
    return () => {
      cancel();
      reduced.removeEventListener('change', preference);
      document.removeEventListener('visibilitychange', visibility);
    };
  });

  // Hovering the control replays the glyph's detail motion, mirroring the
  // design-system hover states. Glyph-only hover falls back to the svg itself.
  onMount(() => {
    const host = element?.closest('button, a, [role="button"]') ?? element;
    if (!host) return;
    const enter = () => play('hover');
    host.addEventListener('pointerenter', enter);
    return () => host.removeEventListener('pointerenter', enter);
  });
</script>

<svg
  bind:this={element}
  class="icon"
  class:active
  data-icon={name}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#if name === 'mic'}
    <rect class="a nav-detail" x="9" y="3" width="6" height="11" rx="3" /><path
      d="M5 11a7 7 0 0 0 14 0M12 18v3"
    />
  {:else if name === 'history'}
    <path d="M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5" /><path class="a vb nav-detail" d="M12 8v4l3 2" />
  {:else if name === 'models'}
    <rect x="6" y="6" width="12" height="12" rx="2" /><rect
      class="a nav-detail"
      x="10"
      y="10"
      width="4"
      height="4"
      rx="1"
    /><path d="M9 2v4M15 2v4M9 18v4M15 18v4M2 9h4M2 15h4M18 9h4M18 15h4" />
  {:else if name === 'settings'}
    <path d="M4 7h16M4 17h16" /><circle
      class="k1 nav-detail"
      cx="8"
      cy="7"
      r="2.4"
      fill="currentColor"
    /><circle class="k2 nav-detail" cx="16" cy="17" r="2.4" fill="currentColor" />
  {:else if name === 'upload'}
    <path d="M5 15v4h14v-4" /><g class="a nav-detail"><path d="M12 15V4M8 8l4-4 4 4" /></g>
  {:else if name === 'downloading'}
    <path d="M4 16v4a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-4" /><path
      class="arr"
      d="M12 3v13m-5-5 5 5 5-5"
    />
  {:else if name === 'arrow'}
    <g class="a nav-detail"><path d="M5 12h14M13 6l6 6-6 6" /></g>
  {:else if name === 'check'}
    <path
      class="a nav-detail"
      pathLength="1"
      style="stroke-dasharray: 1;"
      d="M5 12.5l4.5 4.5L19 7"
    />
  {:else if name === 'spark'}
    <path class="a nav-detail" d="M11 3l1.8 5.2L18 10l-5.2 1.8L11 17l-1.8-5.2L4 10l5.2-1.8z" /><path
      d="M19 16v4M17 18h4"
    />
  {:else if name === 'book'}
    <path d="M4 5.5A2.5 2.5 0 0 1 6.5 3H20v16H6.5A2.5 2.5 0 0 0 4 21.5z" /><path
      class="a l nav-detail"
      d="M8 8h8M8 12h5"
    />
  {:else if name === 'wave'}
    {#each waveBars as bar, index (index)}<path class="w nav-detail" d={bar} />{/each}
  {:else if name === 'globe'}
    <circle cx="12" cy="12" r="9" /><path d="M3 12h18" /><ellipse
      class="a nav-detail"
      cx="12"
      cy="12"
      rx="4.5"
      ry="9"
    />
  {:else if name === 'trash'}
    <g class="a nav-detail"><path d="M4 7h16M10 7V4h4v3" /></g><path
      d="M6 7l1 13h10l1-13M10 11v5M14 11v5"
    />
  {:else if name === 'record'}
    <circle class="a nav-detail" cx="12" cy="12" r="5.5" fill="currentColor" stroke="none" /><circle
      cx="12"
      cy="12"
      r="9.5"
    />
  {:else}<path d={simplePaths[name]} pathLength="1" />{/if}
</svg>

<style>
  /* Ember icon grid. Motion arrives through the Web Animations API so hover
   * and navigation activation share one reduced-motion-aware path. */
  .icon {
    transform-box: fill-box;
    transform-origin: center;
  }
  .nav-detail {
    transform-box: fill-box;
    transform-origin: center;
  }
  .vb {
    transform-box: view-box;
    transform-origin: 12px 12px;
  }
  .l {
    transform-box: fill-box;
    transform-origin: 0 50%;
  }
  .active .k1 {
    transform: translateX(7px);
  }
  .active .k2 {
    transform: translateX(-7px);
  }
  @media (prefers-reduced-motion: reduce) {
    .active .k1,
    .active .k2 {
      transform: none;
    }
  }
</style>
