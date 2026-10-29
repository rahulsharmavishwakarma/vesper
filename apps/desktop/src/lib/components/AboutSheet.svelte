<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import { getAppInfo, type AppInfo } from '../api/desktop';
  import { updateBackend, updateManager } from '../updatesTauri';
  import type { UpdateState } from '../updates';
  import { parseReleaseNotes, previewNotes } from '../releaseNotes';
  import Icon from './Icon.svelte';
  import BrandLogo from './BrandLogo.svelte';

  type PillIcon = 'check' | 'refresh' | 'download' | 'downloading' | 'unavailable' | 'warning';
  interface Pill {
    kind: 'ok' | 'busy' | 'primary' | 'danger' | 'static';
    icon: PillIcon;
    label: string;
    spin?: boolean;
    flash?: boolean;
    progress?: number | null;
    hoverIcon?: PillIcon;
    hoverLabel?: string;
    action?: () => void;
    title?: string;
  }

  let {
    fallbackFocus,
    inline = false,
    visible = false,
  }: { fallbackFocus?: () => void; inline?: boolean; visible?: boolean } = $props();
  const headingId = $derived(inline ? 'inline-about-heading' : 'about-heading');
  const prefId = $derived(inline ? 'inline-about' : 'about');
  $effect(() => {
    if (!inline) return;
    identityMotion = visible;
    if (visible)
      untrack(() => {
        if (!info && !loading) void load();
      });
  });

  let dialog: HTMLDialogElement | undefined = $state();
  let identityMotion = $state(false);
  let info = $state<AppInfo | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let updates = $state<UpdateState>(updateManager.state);
  // Check on first view so the panel shows a live result (unless automatic
  // checks are off); the quiet startup check usually settled this already.
  function ensureChecked() {
    if (updateBackend.enabled && updateManager.state.status === 'idle') {
      void updateManager.automaticCheck();
    }
  }
  $effect(() => {
    ensureChecked();
    return updateManager.subscribe((next) => (updates = next));
  });
  const updateProgress = $derived(
    updates.totalBytes && updates.totalBytes > 0
      ? Math.min(100, Math.round((updates.downloadedBytes / updates.totalBytes) * 100))
      : null,
  );
  const errorLabel = $derived(
    updates.errorPhase === 'download'
      ? 'Download failed'
      : updates.errorPhase === 'install'
        ? 'Install failed'
        : updates.errorPhase === 'restart'
          ? 'Restart failed'
          : 'Couldn’t check',
  );
  // What's new sits under the Install button it describes. A few items show at
  // first; the rest open in place, without a nested scroll box.
  const NOTES_PREVIEW = 4;
  const offered = $derived(
    updates.status === 'available' || updates.status === 'downloading' || updates.status === 'ready'
      ? updates.available
      : null,
  );
  const noteSections = $derived(offered ? parseReleaseNotes(offered.notes) : []);
  const noteCount = $derived(noteSections.reduce((total, entry) => total + entry.items.length, 0));
  let notesExpanded = $state(false);
  const shownNotes = $derived(
    notesExpanded ? noteSections : previewNotes(noteSections, NOTES_PREVIEW),
  );
  // One control carries both the update status and its next action.
  const pill = $derived.by((): Pill => {
    if (!updateBackend.enabled) {
      return {
        kind: 'static',
        icon: 'unavailable',
        label: 'Updates unavailable',
        title: 'Automatic updates are not available in this build.',
      };
    }
    switch (updates.status) {
      case 'current':
        return {
          kind: 'ok',
          icon: 'check',
          label: 'Up to date',
          flash: true,
          hoverIcon: 'refresh',
          hoverLabel: 'Check now',
          action: () => void updateManager.check(),
          title: updates.lastCheckedAt
            ? `Checked ${new Date(updates.lastCheckedAt).toLocaleString()}`
            : undefined,
        };
      case 'available':
        return {
          kind: 'primary',
          icon: 'download',
          label: updates.available ? `Install ${updates.available.version}` : 'Install update',
          action: () => void updateManager.downloadAndInstall(),
        };
      case 'downloading':
        return {
          kind: 'busy',
          icon: 'downloading',
          label: 'Downloading',
          progress: updateProgress,
        };
      case 'ready':
        return {
          kind: 'primary',
          icon: 'refresh',
          label: 'Restart to update',
          flash: true,
          action: () => void updateManager.restart(),
        };
      case 'error':
        return {
          kind: 'danger',
          icon: 'warning',
          label: errorLabel,
          hoverIcon: 'refresh',
          hoverLabel: 'Retry',
          action: () =>
            void (updates.errorPhase === 'restart'
              ? updateManager.restart()
              : updateManager.check()),
          title: updates.errorMessage ?? undefined,
        };
      case 'idle':
        if (!updates.autoCheck) {
          return {
            kind: 'ok',
            icon: 'refresh',
            label: 'Check for updates',
            action: () => void updateManager.check(),
          };
        }
        return { kind: 'busy', icon: 'refresh', label: 'Checking…', spin: true };
      default:
        return { kind: 'busy', icon: 'refresh', label: 'Checking…', spin: true };
    }
  });
  // A result that lands under a resting pointer must stay readable: the
  // hover swap (for example Check now) only arms once the pointer has left.
  function armHover(node: HTMLElement) {
    const timer = setTimeout(() => {
      if (node.matches(':hover')) {
        node.addEventListener(
          'pointerleave',
          () => {
            node.dataset.hover = '1';
          },
          { once: true },
        );
      } else {
        node.dataset.hover = '1';
      }
    }, 80);
    return {
      destroy() {
        clearTimeout(timer);
      },
    };
  }

  let generation = 0;
  let opener: HTMLElement | null = null;

  async function load() {
    const request = ++generation;
    loading = true;
    error = null;
    try {
      const result = await getAppInfo();
      if (request === generation) info = result;
    } catch {
      if (request === generation) error = 'Could not load app information.';
    } finally {
      if (request === generation) loading = false;
    }
  }
  export function open() {
    if (inline || dialog?.open) return;
    ensureChecked();
    opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    info = null;
    void load();
    dialog?.showModal();
    identityMotion = true;
  }
  function close() {
    identityMotion = false;
    generation += 1;
    dialog?.close();
  }
  onDestroy(() => {
    generation += 1;
  });
</script>

{#snippet pillBody(item: Pill)}
  <span class="ico rest" class:spin={item.spin}><Icon name={item.icon} /></span>
  <span class="lab-wrap">
    <span class="lab rest" role="status"
      >{item.label}{#if item.progress != null}<span class="pct">{item.progress}%</span>{/if}</span
    >
    {#if item.hoverLabel}<span class="lab hov" aria-hidden="true">{item.hoverLabel}</span>{/if}
  </span>
  {#if item.hoverIcon}<span class="ico hov" aria-hidden="true"><Icon name={item.hoverIcon} /></span
    >{/if}
{/snippet}

{#snippet aboutContent()}
  <header>
    <h2 id={headingId} class:visually-hidden={inline}>About Vesper</h2>
    {#if !inline}<button
        type="button"
        class="quiet icon-button close"
        aria-label="Close About"
        onclick={close}><Icon name="close" /></button
      >
    {/if}
  </header>
  <div class="top">
    <div class="identity">
      <h3><BrandLogo animate={identityMotion} large /></h3>
      {#if info}
        <p class="version"><Icon name="history" />Version <b>{info.version}</b></p>
      {/if}
    </div>
    {#if info}
      <div class="update" role="group" aria-label="Software updates">
        {#key `${updateBackend.enabled}:${updates.status}:${updates.errorPhase}:${updates.autoCheck}`}
          {#if pill.kind === 'static'}
            <span class="pill" data-kind="static" title={pill.title}>{@render pillBody(pill)}</span>
          {:else}
            <button
              type="button"
              class="pill"
              data-kind={pill.kind}
              data-flash={pill.flash ? '' : undefined}
              data-progress={pill.progress != null ? '' : undefined}
              style:--pr={pill.progress != null ? pill.progress / 100 : undefined}
              disabled={pill.kind === 'busy'}
              title={pill.title}
              onclick={pill.action}
              use:armHover>{@render pillBody(pill)}</button
            >
          {/if}
        {/key}
      </div>
    {/if}
  </div>
  {#if info && offered && noteCount}
    <section class="whats-new" aria-labelledby="{prefId}-whats-new">
      <h4 id="{prefId}-whats-new">What’s new in {offered.version}</h4>
      {#each shownNotes as entry, index (index)}
        {#if entry.title}<h5>{entry.title}</h5>{/if}
        <ul>
          {#each entry.items as item, itemIndex (itemIndex)}<li>{item}</li>{/each}
        </ul>
      {/each}
      {#if noteCount > NOTES_PREVIEW}
        <button
          type="button"
          class="quiet notes-toggle"
          aria-expanded={notesExpanded}
          onclick={() => (notesExpanded = !notesExpanded)}
          >{notesExpanded ? 'Show fewer' : `Show all ${noteCount} changes`}</button
        >
      {/if}
    </section>
  {/if}
  {#if info && updateBackend.enabled}
    <div class="switches">
      <div class="setting-toggle">
        <label class="checkbox-field" for="{prefId}-auto-check"
          ><input
            id="{prefId}-auto-check"
            type="checkbox"
            checked={updates.autoCheck}
            onchange={(event) =>
              updateManager.setPreferences({ autoCheck: event.currentTarget.checked })}
          /><span><Icon name="refresh" />Check for updates automatically</span></label
        >
      </div>
      <div class="setting-toggle">
        <label class="checkbox-field" for="{prefId}-auto-install"
          ><input
            id="{prefId}-auto-install"
            type="checkbox"
            checked={updates.autoInstall}
            disabled={!updates.autoCheck}
            onchange={(event) =>
              updateManager.setPreferences({ autoInstall: event.currentTarget.checked })}
          /><span><Icon name="download" />Download and install automatically</span></label
        >
      </div>
      {#if !info.version.includes('-nightly.')}
        <div class="setting-toggle">
          <label class="checkbox-field" for="{prefId}-prereleases"
            ><input
              id="{prefId}-prereleases"
              type="checkbox"
              checked={updates.includePrereleases}
              aria-describedby="{prefId}-prereleases-note"
              onchange={(event) =>
                updateManager.setPreferences({ includePrereleases: event.currentTarget.checked })}
            /><span><Icon name="spark" />Include beta and pre-release builds</span></label
          >
          <p class="note" id="{prefId}-prereleases-note">
            Off shows only the latest stable release. On also offers release candidates, which may
            be less stable.
          </p>
        </div>
      {/if}
    </div>
  {/if}
  {#if loading}
    <p class="metadata-loading" role="status">Loading app information…</p>
  {:else if error}
    <p class="error" role="alert">{error}</p>
    <button type="button" onclick={load}>Try again</button>
  {:else if info}
    {#if (updates.status === 'available' || updates.status === 'ready') && updates.blockedReason}
      <p class="update-note" role="status">{updates.blockedReason}</p>
    {/if}
  {/if}
{/snippet}
{#if inline}
  <section class="about-inline" aria-labelledby={headingId}>{@render aboutContent()}</section>
{:else}
  <dialog
    bind:this={dialog}
    class="about-sheet"
    aria-labelledby={headingId}
    oncancel={() => {
      identityMotion = false;
      generation += 1;
    }}
    onclose={() => {
      identityMotion = false;
      if (opener?.isConnected && opener !== document.body && opener.getClientRects().length) {
        opener.focus({ preventScroll: true });
        if (document.activeElement === opener) return;
      }
      fallbackFocus?.();
    }}
    onpointerdown={(event) => {
      if (event.target !== dialog || !dialog) return;
      const bounds = dialog.getBoundingClientRect();
      if (
        event.clientX < bounds.left ||
        event.clientX > bounds.right ||
        event.clientY < bounds.top ||
        event.clientY > bounds.bottom
      )
        close();
    }}
  >
    {@render aboutContent()}
  </dialog>
{/if}

<style>
  .about-inline {
    min-width: 0;
  }
  .about-inline header {
    margin: 0;
  }
  .about-sheet {
    width: min(520px, calc(100% - 32px));
    max-height: calc(100dvh - 32px);
    overflow-y: auto;
    margin: auto;
    padding: var(--space-6);
    color: var(--color-ink);
    background: var(--color-raised);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-lg);
  }
  .about-sheet::backdrop {
    background: var(--color-overlay);
  }
  .about-sheet[open] {
    animation: about-enter 180ms var(--ease-out) both;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin-bottom: var(--space-6);
  }
  header h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 500;
  }
  .top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4) var(--space-6);
  }
  .identity h3 {
    margin: 0;
    font-size: 28px;
    letter-spacing: -0.05em;
    line-height: 1.2;
  }
  .version {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    /* Lines up with the bird's visible edge, not the lockup's padding. */
    margin: var(--space-3) 0 0 4px;
    font-size: 13px;
    color: var(--color-muted);
    font-variant-numeric: tabular-nums;
  }
  .version b {
    font-weight: 500;
    color: var(--color-ink);
  }
  .version :global(.icon) {
    width: 16px;
    height: 16px;
    color: var(--color-accent);
  }
  .switches {
    margin-top: var(--space-4);
    border-top: 1px solid var(--color-line);
  }
  .switches :global(.setting-toggle .checkbox-field) {
    flex-direction: row;
    width: 100%;
  }
  .switches :global(.setting-toggle .checkbox-field input[type='checkbox']) {
    order: 3;
    margin-left: auto;
  }
  .switches :global(.setting-toggle .checkbox-field > span) {
    order: 1;
  }
  .switches :global(.checkbox-field span) {
    display: inline-flex;
    align-items: center;
    gap: var(--space-3);
    transition: opacity var(--duration-state) var(--ease-out);
  }
  .switches :global(.checkbox-field .icon) {
    width: 18px;
    height: 18px;
    color: var(--color-accent);
  }
  .switches :global(input:disabled ~ span) {
    opacity: 0.45;
  }
  .metadata-loading {
    margin-top: var(--space-6);
    font-size: 13px;
    color: var(--color-muted);
  }
  .update-note {
    margin: var(--space-4) 0 0;
    font-size: 13px;
    color: var(--color-muted);
  }
  .whats-new {
    margin-top: var(--space-4);
    padding-top: var(--space-4);
    border-top: 1px solid var(--color-line);
  }
  .whats-new h4 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--color-ink);
  }
  .whats-new h5 {
    margin: var(--space-3) 0 var(--space-1);
    font-size: 12px;
    font-weight: 600;
    color: var(--color-muted);
  }
  .whats-new h4 + ul {
    margin-top: var(--space-2);
  }
  .whats-new ul {
    max-width: 68ch;
    margin: 0;
    padding-left: var(--space-4);
    font-size: 13px;
    line-height: 1.5;
    color: var(--color-muted);
  }
  .whats-new li + li {
    margin-top: var(--space-1);
  }
  .whats-new li::marker {
    color: var(--color-accent);
  }
  .notes-toggle {
    min-height: 0;
    margin: var(--space-2) 0 0 calc(-1 * var(--space-2));
    padding: var(--space-1) var(--space-2);
    font-size: 13px;
    color: var(--color-accent);
  }
  .notes-toggle:is(:hover, :focus-visible):enabled {
    color: var(--color-accent-hover);
  }

  /* The update pill: one control for status and the next action. */
  .pill {
    --pr: 0;
    position: relative;
    display: inline-grid;
    grid-template-columns: 18px auto;
    align-items: center;
    column-gap: var(--space-2);
    min-height: 44px;
    padding: 0 var(--space-4) 0 var(--space-3);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-pill);
    background: var(--color-bg);
    color: var(--color-ink);
    font-size: 14px;
    max-width: 100%;
    white-space: nowrap;
    overflow: hidden;
    animation: pill-in 420ms var(--ease-spring) backwards;
  }
  .pill:disabled {
    opacity: 1;
  }
  .ico {
    grid-area: 1 / 1;
    position: relative;
    z-index: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    color: var(--color-muted);
    transition:
      opacity 160ms var(--ease-out),
      transform 220ms var(--ease-out);
  }
  .ico :global(.icon) {
    width: 18px;
    height: 18px;
  }
  .lab-wrap {
    min-width: 0;
    grid-column: 2;
    position: relative;
    z-index: 1;
    display: inline-grid;
  }
  .lab {
    grid-area: 1 / 1;
    transition:
      opacity 160ms var(--ease-out),
      transform 220ms var(--ease-out);
  }
  .hov {
    opacity: 0;
    transform: translateY(6px);
  }
  .ico.hov {
    transform: rotate(-90deg) scale(0.6);
  }
  .pill:global([data-hover='1']):hover:enabled .rest,
  .pill:global([data-hover='1']):focus-visible .rest {
    opacity: 0;
    transform: translateY(-6px);
  }
  .pill:global([data-hover='1']):hover:enabled .ico.rest,
  .pill:global([data-hover='1']):focus-visible .ico.rest {
    transform: rotate(90deg) scale(0.6);
  }
  .pill:global([data-hover='1']):hover:enabled .hov,
  .pill:global([data-hover='1']):focus-visible .hov {
    opacity: 1;
    transform: none;
  }
  .pill:global([data-hover='1']):hover:enabled {
    background: var(--color-raised);
    border-color: var(--color-control-edge);
  }
  .pill[data-kind='ok'] .ico.rest,
  .pill[data-kind='busy'] .ico.rest {
    color: var(--color-accent);
  }
  .pill[data-kind='primary'] {
    background: var(--color-accent);
    border-color: var(--color-accent);
    color: var(--color-on-accent);
    font-weight: 600;
  }
  .pill[data-kind='primary'] .ico {
    color: var(--color-on-accent);
  }
  .pill[data-kind='primary']:hover:enabled {
    background: var(--color-accent-hover);
    border-color: var(--color-accent-hover);
  }
  .pill[data-kind='danger'] {
    border-color: color-mix(in srgb, var(--color-danger) 50%, var(--color-line));
    background: var(--color-error-tint);
  }
  .pill[data-kind='danger'] .ico.rest {
    color: var(--color-danger);
  }
  .pill[data-kind='busy'] {
    cursor: progress;
    border-color: color-mix(in srgb, var(--color-accent) 40%, var(--color-line));
  }
  .pill[data-kind='static'] {
    cursor: default;
    color: var(--color-muted);
  }
  .spin {
    animation: pill-spin 1s linear infinite;
  }
  .pct {
    display: inline-block;
    min-width: 3.2ch;
    margin-left: var(--space-2);
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--color-muted);
  }
  /* Download progress: a GPU-scaled fill with a soft leading edge. */
  .pill[data-progress]::after {
    content: '';
    position: absolute;
    inset: 0;
    z-index: 0;
    background: linear-gradient(
      90deg,
      color-mix(in srgb, var(--color-accent) 10%, var(--color-tint)),
      color-mix(in srgb, var(--color-accent) 26%, var(--color-tint))
    );
    transform: scaleX(var(--pr));
    transform-origin: left center;
    box-shadow: 3px 0 10px -2px color-mix(in srgb, var(--color-accent) 55%, transparent);
    transition: transform 140ms linear;
    will-change: transform;
  }
  .pill[data-flash]::before {
    content: '';
    position: absolute;
    inset: -1px;
    border-radius: inherit;
    border: 1.5px solid var(--color-accent);
    opacity: 0;
    pointer-events: none;
    animation: pill-flash 700ms var(--ease-out) 1;
  }
  .pill :global(.arr) {
    transform-box: fill-box;
    transform-origin: center;
    animation: pill-arrow 1.3s cubic-bezier(0.45, 0, 0.25, 1) infinite;
  }
  @keyframes pill-in {
    from {
      transform: scale(0.94);
      opacity: 0.6;
    }
  }
  @keyframes pill-spin {
    to {
      transform: rotate(360deg);
    }
  }
  @keyframes pill-flash {
    0% {
      opacity: 0.7;
      transform: scale(1);
    }
    100% {
      opacity: 0;
      transform: scale(1.08, 1.35);
    }
  }
  @keyframes pill-arrow {
    0% {
      transform: translateY(-5px);
      opacity: 0;
    }
    35% {
      transform: translateY(0);
      opacity: 1;
    }
    75% {
      transform: translateY(0);
      opacity: 1;
    }
    100% {
      transform: translateY(3px);
      opacity: 0;
    }
  }
  @keyframes about-enter {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .about-sheet[open],
    .pill,
    .spin,
    .pill[data-flash]::before,
    .pill :global(.arr) {
      animation: none;
    }
    .pill::after,
    .ico,
    .lab {
      transition: none;
    }
  }
</style>
