<script lang="ts">
  const paths = {
    mic: 'M12 15a3 3 0 0 0 3-3V5a3 3 0 0 0-6 0v7a3 3 0 0 0 3 3Zm-7-4v1a7 7 0 0 0 14 0v-1M12 19v3m-4 0h8',
    history: 'M3 11a9 9 0 1 1 2.5 7M3 4v7h7m2-4v5l3 2',
    models: 'M9 3v3M15 3v3M9 18v3M15 18v3M3 9h3M3 15h3M18 9h3M18 15h3',
    settings: 'M4 7h16M4 17h16M9 4v6m6 4v6',
    chevron: 'm6 9 6 6 6-6',
    arrow: 'M5 12h14m-5-5 5 5-5 5',
    upload: 'M12 16V3m-5 5 5-5 5 5M4 16v4a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-4',
    refresh: 'M20 11a8 8 0 1 0-2.3 6.7M20 5v6h-6',
    stop: 'M6 6h12v12H6z',
    check: 'm5 12 4 4L19 6',
    copy: 'M8 8h12v12H8zM16 8V4H4v12h4',
    close: 'M6 6l12 12M18 6 6 18',
    search: 'M21 21l-5-5M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0Z',
    download: 'M12 3v13m-5-5 5 5 5-5M4 16v4a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-4',
    downloaded: 'm7 8 3 3 7-7M4 14v6h16v-6M4 16h16',
    warning: 'M12 3 2 21h20L12 3Zm0 6v5m0 3v1',
    unknown:
      'M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20Zm-3-14a3 3 0 1 1 5 2.2c-1.2.8-2 1.3-2 2.8m0 3v1',
    unavailable: 'M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20ZM5 5l14 14',
    trash: 'M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 11v5m4-5v5',
    like: 'M7 10v11H3V10h4Zm0 10h10a2 2 0 0 0 2-1.6l1.4-7A2 2 0 0 0 18.4 9H13l1-5a2 2 0 0 0-2-2L7 10',
    dislike:
      'M7 14V3H3v11h4Zm0-10h10a2 2 0 0 1 2 1.6l1.4 7a2 2 0 0 1-2 2.4H13l1 5a2 2 0 0 1-2 2l-5-8',
  };
  let {
    name,
    active = false,
    motionKey,
  }: { name: keyof typeof paths; active?: boolean; motionKey?: number } = $props();
  let element: SVGSVGElement | undefined = $state();
  let wasSelected = false;

  // A new key acknowledges a navigation click, including the selected tab.
  // Other uses of Icon stay static; model polling never restarts this motion.
  $effect(() => {
    const sequence = motionKey;
    const selected = active;
    const glyph = name;
    const node = element;
    if (sequence === undefined || !node) return;
    const repeated = wasSelected;
    wasSelected = selected;
    if (!selected || sequence === 0) return;
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)');
    const animations: Animation[] = [];
    const cancel = () => animations.forEach((animation) => animation.cancel());
    const visibility = () => {
      if (document.hidden) cancel();
    };
    const preference = () => {
      if (reduced.matches) cancel();
    };
    if (!reduced.matches && !document.hidden) {
      const style = getComputedStyle(node);
      const duration = Number.parseFloat(style.getPropertyValue('--duration-nav-icon')) || 620;
      const easing = style.getPropertyValue('--ease-nav-icon').trim() || 'cubic-bezier(.35,0,.2,1)';
      node.querySelectorAll<SVGElement>('.nav-detail').forEach((part, index) => {
        let frames: Keyframe[];
        if (glyph === 'settings') {
          const end = index === 0 ? 6 : -6;
          frames = repeated
            ? [
                { transform: `translateX(${end}px)` },
                { transform: `translateX(${end / 2}px)`, offset: 0.4 },
                { transform: `translateX(${end}px)` },
              ]
            : [{ transform: 'translateX(0)' }, { transform: `translateX(${end}px)` }];
        } else if (glyph === 'history') {
          frames = [{ transform: 'rotate(-55deg)' }, { transform: 'rotate(0)' }];
        } else if (glyph === 'mic') {
          frames = [
            { transform: 'scaleY(1)', opacity: 1 },
            { transform: 'scaleY(.45)', opacity: 0.55, offset: 0.35 },
            { transform: 'scaleY(1)', opacity: 1 },
          ];
        } else {
          frames = [
            { transform: 'scale(.72)', opacity: 0.4 },
            { transform: 'scale(1)', opacity: 1 },
          ];
        }
        animations.push(part.animate(frames, { duration, easing }));
      });
    }
    reduced.addEventListener('change', preference);
    document.addEventListener('visibilitychange', visibility);
    return () => {
      cancel();
      reduced.removeEventListener('change', preference);
      document.removeEventListener('visibilitychange', visibility);
    };
  });
</script>

<svg
  bind:this={element}
  class="icon"
  class:active
  class:has-motion={motionKey !== undefined}
  data-icon={name}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.6"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#if name === 'mic'}<rect class="icon-tone" x="9" y="3" width="6" height="12" rx="3" />
  {:else if name === 'models'}<rect class="icon-tone" x="6" y="6" width="12" height="12" rx="3" />
  {:else if name === 'history'}<circle class="icon-tone" cx="12" cy="12" r="5" />
  {:else if name === 'like' || name === 'dislike'}<path class="icon-tone" d={paths[name]} />{/if}
  {#if name === 'settings'}
    <path d="M3 7h18M3 17h18" />
    <rect class="icon-knob nav-detail first" x="7" y="4" width="4" height="6" rx="1.5" />
    <rect class="icon-knob nav-detail second" x="14" y="14" width="4" height="6" rx="1.5" />
  {:else if name === 'models'}
    <rect x="6" y="6" width="12" height="12" rx="3" /><path d={paths.models} />
    <rect class="nav-detail" x="9.5" y="9.5" width="5" height="5" rx="1" />
  {:else if name === 'mic'}
    <rect x="9" y="3" width="6" height="12" rx="3" /><path
      d="M5 10v2a7 7 0 0 0 14 0v-2M12 19v3M9 22h6"
    />
    <path class="nav-detail" d="M12 6v6" />
  {:else if name === 'history'}
    <path d="M4 10a8 8 0 1 1 .7 6M4 4v6h6" /><path class="nav-detail clock-hand" d="M12 7v5l3 2" />
  {:else}<path d={paths[name]} pathLength="1" />{/if}
</svg>

<style>
  /* Approved A / Instrument. Only a navigation activation starts detail motion. */
  .icon-tone {
    fill: currentColor;
    stroke: none;
    opacity: 0;
    transition: opacity var(--duration-state) var(--ease-out);
  }
  .active .icon-tone {
    opacity: 0.2;
  }
  .active {
    stroke-width: 1.9;
  }
  .nav-detail {
    transform-box: fill-box;
    transform-origin: center;
  }
  .clock-hand {
    transform-box: view-box;
    transform-origin: 12px 12px;
  }
  .icon-knob {
    fill: var(--color-panel);
    transition: transform var(--duration-state) var(--ease-out);
  }
  .has-motion .icon-knob {
    transition: none;
  }
  .active .first {
    transform: translateX(6px);
  }
  .active .second {
    transform: translateX(-6px);
  }
  @media (prefers-reduced-motion: reduce) {
    .icon-tone,
    .icon-knob {
      transition: none;
    }
  }
</style>
