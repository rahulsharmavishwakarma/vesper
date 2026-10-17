<script lang="ts">
  import { onDestroy, tick, untrack } from 'svelte';
  import { getAppInfo, type AppInfo } from '../api/desktop';
  import { updateBackend, updateManager } from '../updatesTauri';
  import type { UpdateState } from '../updates';
  import CopyGlyph from './CopyGlyph.svelte';
  import Icon from './Icon.svelte';
  import BrandLogo from './BrandLogo.svelte';

  let {
    fallbackFocus,
    inline = false,
    visible = false,
  }: { fallbackFocus?: () => void; inline?: boolean; visible?: boolean } = $props();
  const headingId = $derived(inline ? 'inline-about-heading' : 'about-heading');
  const updatesId = $derived(inline ? 'inline-about-updates-heading' : 'about-updates-heading');
  const copyFieldId = $derived(inline ? 'inline-about-copy-fallback' : 'about-copy-fallback');
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
  let copied = $state(false);
  let copying = $state(false);
  let copyError = $state(false);
  let updates = $state<UpdateState>(updateManager.state);
  // Check on first view so the panel always shows a live result; the quiet
  // startup check usually already settled this before the user arrives.
  function ensureChecked() {
    if (updateBackend.enabled && updateManager.state.status === 'idle') {
      void updateManager.check();
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
  const manualDownload = $derived(
    updates.available
      ? `https://github.com/rahulsharmavishwakarma/vesper/releases/download/v${updates.available.version}/vesper-macos-arm64.dmg`
      : 'https://github.com/rahulsharmavishwakarma/vesper/releases/latest/download/vesper-macos-arm64.dmg',
  );

  let fallback: HTMLTextAreaElement | undefined = $state();
  let opener: HTMLElement | null = null;
  let generation = 0;
  let copyTimer: ReturnType<typeof setTimeout> | undefined;
  const buildLabel = $derived(
    info?.buildType === 'browser-preview'
      ? 'Browser preview'
      : info?.buildType === 'development'
        ? 'Development build'
        : 'Release build',
  );
  const platform = $derived(
    !info
      ? ''
      : info.os === 'browser'
        ? 'Web browser'
        : `${({ macos: 'macOS', windows: 'Windows', linux: 'Linux' } as Record<string, string>)[info.os] ?? info.os} · ${info.os === 'macos' && info.arch === 'aarch64' ? 'Apple silicon' : info.arch}`,
  );
  const copyText = $derived(
    info
      ? `${info.name}\nVersion: ${info.version}\nBuild: ${buildLabel}\nPlatform: ${platform}`
      : '',
  );

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
    copied = false;
    copyError = false;
    copying = false;
    info = null;
    void load();
    dialog?.showModal();
    identityMotion = true;
  }
  function close() {
    identityMotion = false;
    generation += 1;
    clearTimeout(copyTimer);
    dialog?.close();
  }
  async function copy() {
    if (!info || copying) return;
    const request = generation;
    copying = true;
    copyError = false;
    try {
      await navigator.clipboard.writeText(copyText);
      if (request !== generation) return;
      copied = true;
      clearTimeout(copyTimer);
      copyTimer = setTimeout(() => {
        copied = false;
      }, 2000);
    } catch {
      if (request !== generation) return;
      copyError = true;
      await tick();
      fallback?.focus();
      fallback?.select();
    } finally {
      if (request === generation) copying = false;
    }
  }
  onDestroy(() => {
    generation += 1;
    clearTimeout(copyTimer);
  });
</script>

{#snippet aboutContent()}
  <header>
    <h2 id={headingId}>About Vesper</h2>
    {#if !inline}<button
        type="button"
        class="quiet icon-button close"
        aria-label="Close About"
        onclick={close}><Icon name="close" /></button
      >
    {/if}
  </header>
  <div class="identity">
    <div>
      <h3><BrandLogo animate={identityMotion} large /></h3>
      <p>On-device voice to text</p>
    </div>
  </div>
  {#if loading}
    <p class="metadata-loading" role="status">Loading app information…</p>
  {:else if error}
    <p class="error" role="alert">{error}</p>
    <button type="button" onclick={load}>Try again</button>
  {:else if info}
    <div class="app-info">
      <div class="metadata">
        <p><span class="version">Version {info.version}</span> · {buildLabel}</p>
        <p class="platform">{platform}</p>
      </div>
      <button
        type="button"
        class="quiet icon-button copy"
        onclick={copy}
        disabled={copying}
        aria-busy={copying}
        aria-label={copying ? 'Copying app info' : copied ? 'Copied' : 'Copy app info'}
        title={copying ? 'Copying…' : copied ? 'Copied' : 'Copy app info'}
      >
        <CopyGlyph {copied} />
      </button>
    </div>
    {#if copyError}
      <p class="note" role="status">
        Copy is unavailable. Select the app information below to copy it.
      </p>
      <label class="visually-hidden" for={copyFieldId}>App information</label>
      <textarea bind:this={fallback} id={copyFieldId} readonly value={copyText} rows="4"></textarea>
    {:else}
      <span class="visually-hidden" role="status">{copied ? 'App information copied.' : ''}</span>
    {/if}
    <section class="updates" aria-labelledby={updatesId}>
      <h3 id={updatesId}>Software updates</h3>
      {#if info && info.version.includes('-')}
        <p class="channel">Beta channel</p>
      {/if}
      {#if !updateBackend.enabled}
        <p>Automatic updates are not available in this build.</p>
      {:else if updates.status === 'checking' || updates.status === 'idle'}
        <p role="status">Checking for updates…</p>
      {:else if updates.status === 'current'}
        <p role="status">
          Vesper is up to date{updates.lastCheckedAt
            ? ` (checked ${new Date(updates.lastCheckedAt).toLocaleDateString()})`
            : ''}.
        </p>
      {:else if updates.status === 'available' && updates.available}
        <p role="status">Version {updates.available.version} is available.</p>
        {#if updates.available.notes}<p class="notes">{updates.available.notes}</p>{/if}
        {#if updates.blockedReason}<p role="status">{updates.blockedReason}</p>{/if}
        <p>
          <button type="button" onclick={() => void updateManager.downloadAndInstall()}
            >Download and install</button
          >
        </p>
      {:else if updates.status === 'downloading'}
        <p role="status">
          Downloading update…{updateProgress !== null ? ` ${updateProgress}%` : ''}
        </p>
        <progress
          value={updates.downloadedBytes}
          max={updates.totalBytes ?? undefined}
          aria-label="Update download progress"
        ></progress>
      {:else if updates.status === 'ready'}
        <p role="status">Update installed and ready.</p>
        {#if updates.blockedReason}<p role="status">{updates.blockedReason}</p>{/if}
        <p>
          <button type="button" onclick={() => void updateManager.restart()}
            >Restart to update</button
          >
        </p>
      {:else if updates.status === 'error'}
        <p role="alert">{updates.errorMessage}</p>
        <p>
          <button type="button" onclick={() => void updateManager.check()}>Try again</button>
        </p>
      {/if}
      <p class="manual">
        <a href={manualDownload} target="_blank" rel="noreferrer noopener"
          >Download the latest version manually</a
        >
      </p>
    </section>
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
      clearTimeout(copyTimer);
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
    margin-bottom: var(--space-8);
  }
  .about-inline header h2 {
    font-size: 20px;
    line-height: 1.4;
    letter-spacing: normal;
  }
  .about-inline .identity {
    margin-bottom: var(--space-8);
  }
  .about-inline .app-info {
    border-top: 1px solid var(--color-line);
    padding-top: var(--space-5);
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
  .identity {
    display: flex;
    align-items: center;
    gap: var(--space-4);
  }
  .identity h3 {
    margin: 0;
    font-size: 28px;
    letter-spacing: -0.05em;
    line-height: 1.2;
  }
  .identity p {
    margin: var(--space-1) 0 0;
    font-size: 13px;
    color: var(--color-muted);
  }
  .app-info {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    margin: var(--space-6) 0;
    color: var(--color-muted);
    font-size: 13px;
  }
  .metadata {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .metadata p {
    margin: 0;
  }
  .metadata .platform {
    margin-top: var(--space-1);
  }
  .version {
    font-variant-numeric: tabular-nums;
  }
  .metadata-loading {
    margin-top: var(--space-6);
    font-size: 13px;
    color: var(--color-muted);
  }
  .updates {
    border-top: 1px solid var(--color-line);
    padding-top: var(--space-6);
  }
  .updates h3 {
    font-size: 14px;
    font-weight: 500;
    line-height: 1.35;
    margin: 0 0 var(--space-2);
  }
  .updates p {
    font-size: 13px;
    color: var(--color-muted);
    margin: 0 0 var(--space-2);
  }
  .updates .notes {
    white-space: pre-wrap;
    max-height: 120px;
    overflow-y: auto;
  }
  .updates progress {
    display: block;
    width: 100%;
    margin: 0 0 var(--space-2);
  }
  .updates .manual {
    margin-bottom: 0;
  }
  textarea {
    font-size: 12px;
    margin-bottom: var(--space-5);
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
    .about-sheet[open] {
      animation: none;
    }
  }
</style>
