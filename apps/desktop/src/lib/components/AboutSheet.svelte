<script lang="ts">
  import { onDestroy, tick } from 'svelte';
  import { getAppInfo, type AppInfo } from '../api/desktop';
  import CopyGlyph from './CopyGlyph.svelte';
  import Icon from './Icon.svelte';
  import BrandLogo from './BrandLogo.svelte';

  let { fallbackFocus }: { fallbackFocus?: () => void } = $props();

  let dialog: HTMLDialogElement | undefined = $state();
  let identityMotion = $state(false);
  let info = $state<AppInfo | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let copied = $state(false);
  let copying = $state(false);
  let copyError = $state(false);
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
    if (dialog?.open) return;
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

<dialog
  bind:this={dialog}
  class="about-sheet"
  aria-labelledby="about-heading"
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
  <header>
    <h2 id="about-heading">About Vesper</h2>
    <button type="button" class="ghost close" aria-label="Close About" onclick={close}
      ><Icon name="close" /></button
    >
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
      <span><span class="version">{info.version}</span> · {buildLabel}</span>
      <button
        type="button"
        class="ghost copy"
        onclick={copy}
        disabled={copying}
        aria-busy={copying}
      >
        <CopyGlyph {copied} />{copying ? 'Copying…' : copied ? 'Copied' : 'Copy app info'}
      </button>
    </div>
    {#if copyError}
      <p class="note" role="status">
        Copy is unavailable. Select the app information below to copy it.
      </p>
      <label class="visually-hidden" for="about-copy-fallback">App information</label>
      <textarea bind:this={fallback} id="about-copy-fallback" readonly value={copyText} rows="4"
      ></textarea>
    {:else}
      <span class="visually-hidden" role="status">{copied ? 'App information copied.' : ''}</span>
    {/if}
    <section class="updates" aria-labelledby="about-updates-heading">
      <span class="eyebrow">Software updates</span>
      <h3 id="about-updates-heading">Automatic updates aren’t available yet.</h3>
      <p>This build does not check for or install application updates.</p>
    </section>
    <p class="platform">{platform}</p>
  {/if}
</dialog>

<style>
  .about-sheet {
    width: min(520px, calc(100% - 32px));
    max-height: calc(100dvh - 32px);
    overflow-y: auto;
    margin: auto;
    padding: var(--space-6);
    color: var(--color-ink);
    background: var(--color-panel);
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
    font-size: 14px;
    font-weight: 400;
    color: var(--color-muted);
  }
  .close {
    width: 44px;
    height: 44px;
    min-height: 44px;
    padding: var(--space-2);
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
    font-size: 12px;
    color: var(--color-muted);
  }
  .app-info {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin: var(--space-6) 0;
    color: var(--color-muted);
    font-size: 12px;
  }
  .version {
    font-variant-numeric: tabular-nums;
  }
  .copy {
    min-height: 44px;
    padding-inline: 0;
    font-size: 12px;
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
    font-size: 20px;
    font-weight: 500;
    line-height: 1.35;
    margin: var(--space-3) 0 var(--space-2);
  }
  .updates p {
    font-size: 13px;
    color: var(--color-muted);
    margin: 0;
  }
  .platform {
    margin: var(--space-6) 0 0;
    color: var(--color-muted);
    font-size: 12px;
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
