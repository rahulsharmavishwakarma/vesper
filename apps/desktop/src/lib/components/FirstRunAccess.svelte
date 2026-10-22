<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { relaunch } from '@tauri-apps/plugin-process';
  import Icon from './Icon.svelte';
  import { getDictationPermissions, type DictationPermissions } from '../api/dictation';
  import { microphoneAccess, openPrivacySettings, type MicrophoneAccess } from '../api/onboarding';
  import type { AppConfig } from '../config';

  let {
    settings,
    preview = false,
    onSaveSettings,
    onReady,
  }: {
    settings: AppConfig;
    preview?: boolean;
    onSaveSettings: (next: AppConfig) => Promise<AppConfig>;
    onReady: (ready: boolean) => void;
  } = $props();

  let mic = $state<MicrophoneAccess | null>(null);
  let permissions = $state<DictationPermissions | null>(null);
  let waiting = $state(false);
  let stuck = $state(false);
  let keepInApp = $state(false);
  let error = $state<string | null>(null);
  let poll: ReturnType<typeof setInterval> | undefined;
  let stuckTimer: ReturnType<typeof setTimeout> | undefined;
  let previewGranted = false;
  let saved: 'paste' | 'app' | null = null;

  const allowed = $derived(
    Boolean(permissions?.accessibility && permissions.eventPosting && permissions.clipboard),
  );
  const partial = $derived(Boolean(permissions?.accessibility) && !allowed);

  $effect(() => onReady(allowed || keepInApp));
  // Typing into apps follows the outcome: granted access turns it on, the
  // fallback keeps text in Vesper.
  $effect(() => {
    const output = allowed ? 'paste' : keepInApp ? 'app' : null;
    if (!output || output === saved) return;
    saved = output;
    void onSaveSettings({ ...settings, dictationOutput: output }).catch((cause) => {
      error = `Could not save this choice. ${String(cause)}`;
      saved = null;
    });
  });

  async function read(request = false) {
    if (preview) {
      permissions = {
        supported: true,
        accessibility: previewGranted,
        eventPosting: previewGranted,
        clipboard: previewGranted,
      };
      return;
    }
    try {
      permissions = await getDictationPermissions(request);
    } catch (cause) {
      error = String(cause);
    }
  }

  function watch() {
    clearInterval(poll);
    poll = setInterval(() => {
      void read().then(() => {
        if (allowed) {
          clearInterval(poll);
          clearTimeout(stuckTimer);
          waiting = false;
          stuck = false;
        }
      });
    }, 1500);
    clearTimeout(stuckTimer);
    stuckTimer = setTimeout(() => {
      if (!allowed) stuck = true;
    }, 8000);
  }

  async function openSettings() {
    error = null;
    waiting = true;
    if (preview) {
      setTimeout(() => (previewGranted = true), 1200);
    } else {
      await read(true);
      await openPrivacySettings('accessibility').catch(() => {});
    }
    watch();
  }

  async function finishAccess() {
    error = null;
    await read(true);
    watch();
  }

  const refreshOnFocus = () => void read();
  onMount(() => {
    void read();
    void (preview ? Promise.resolve<MicrophoneAccess>('allowed') : microphoneAccess(false))
      .then((value) => (mic = value))
      .catch(() => (mic = null));
    window.addEventListener('focus', refreshOnFocus);
  });
  onDestroy(() => {
    clearInterval(poll);
    clearTimeout(stuckTimer);
    window.removeEventListener('focus', refreshOnFocus);
  });
</script>

<div class="card">
  <div class="perm">
    <span class="ic"><Icon name="mic" /></span>
    <div><b>Microphone</b><span>Hear you when you speak.</span></div>
    <span class="pill" class:ok={mic === 'allowed'}
      >{#if mic === 'allowed'}<Icon name="check" />Allowed{:else}Not allowed{/if}</span
    >
  </div>
  <div class="perm">
    <span class="ic"><Icon name="arrow" /></span>
    <div><b>Accessibility</b><span>Type the transcript into other apps.</span></div>
    <div class="right">
      {#if allowed}<span class="pill ok"><Icon name="check" />Allowed</span>
      {:else if keepInApp}<span class="pill">Kept in Vesper</span>
      {:else if partial}<button type="button" class="primary" onclick={finishAccess}
          >Finish access</button
        >
      {:else}
        <span class="pill warn">{waiting ? 'Waiting' : 'Not allowed'}</span>
        <button type="button" class={waiting ? '' : 'primary'} onclick={openSettings}
          >{waiting ? 'Open again' : 'Open System Settings'}</button
        >
      {/if}
    </div>
  </div>
  {#if waiting && !allowed && !keepInApp}
    <div class="more">
      <p class="hint">
        In System Settings → Privacy &amp; Security → Accessibility, switch <b>Vesper</b> on. Vesper notices
        the moment you do.
      </p>
      {#if stuck}
        <div class="note">
          <Icon name="warning" />
          <div>
            <b>Still waiting?</b> If you turned it on and this still says Waiting, restart Vesper;
            setup comes back to this step. You can also keep your text in Vesper and copy it from
            there.
            <div class="acts">
              {#if !preview}<button type="button" onclick={() => void relaunch()}
                  >Restart Vesper</button
                >{/if}
              <button type="button" class="quiet" onclick={() => (keepInApp = true)}
                >Keep text in Vesper</button
              >
            </div>
          </div>
        </div>
      {/if}
    </div>
  {/if}
  {#if keepInApp && !allowed}
    <div class="more">
      <p class="hint">
        Your words appear in Vesper, ready to copy. Turn on typing into apps any time in Settings →
        Shortcuts.
      </p>
    </div>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</div>
<p class="foot-hint">Vesper only types what you dictate. It never reads other apps.</p>

<style>
  .card {
    border: 1px solid var(--color-line);
    border-radius: var(--radius-lg);
    background: var(--color-raised);
    overflow: hidden;
  }
  .perm {
    display: grid;
    grid-template-columns: 44px 1fr auto;
    gap: 16px;
    align-items: center;
    padding: 20px 26px;
  }
  .perm + .perm {
    border-top: 1px solid var(--color-line);
  }
  .ic {
    width: 44px;
    height: 44px;
    border-radius: 12px;
    display: grid;
    place-items: center;
    background: var(--color-bg);
    border: 1px solid var(--color-line);
    color: var(--color-accent);
  }
  .ic :global(svg) {
    width: 22px;
    height: 22px;
  }
  .perm b {
    display: block;
    font-size: 16px;
    font-weight: 600;
  }
  .perm span {
    font-size: 13.5px;
    color: var(--color-muted);
  }
  .right {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
    justify-content: flex-end;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px !important;
    font-weight: 600;
    padding: 3px 10px;
    border-radius: 99px;
    border: 1px solid var(--color-line);
    color: var(--color-muted);
  }
  .pill.ok {
    color: var(--color-accent);
    border-color: var(--color-accent);
    background: var(--color-tint);
  }
  .pill.warn {
    color: var(--color-warning);
    border-color: var(--color-warning);
  }
  .pill :global(svg) {
    width: 12px;
    height: 12px;
  }
  .more {
    padding: 0 26px 20px;
  }
  .hint {
    font-size: 13.5px;
    color: var(--color-muted);
    margin: 0;
  }
  .note {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    margin-top: 14px;
    padding: 14px 16px;
    border: 1px solid var(--color-warning);
    border-radius: 14px;
    background: var(--color-bg);
    font-size: 14px;
    color: var(--color-muted);
  }
  .note b {
    color: var(--color-ink);
  }
  .note > :global(svg) {
    width: 18px;
    height: 18px;
    flex: none;
    margin-top: 2px;
    color: var(--color-warning);
  }
  .acts {
    display: flex;
    gap: 8px;
    margin-top: 10px;
    flex-wrap: wrap;
  }
  .error {
    margin: 0 26px 18px;
    color: var(--color-error);
    font-size: 13px;
  }
  .foot-hint {
    margin: 14px 0 0;
    font-size: 13px;
    color: var(--color-muted);
  }
  @container first-run (max-width: 760px) {
    .perm {
      grid-template-columns: 40px 1fr;
      padding: 18px;
    }
    .right {
      grid-column: 1 / -1;
      justify-content: flex-start;
    }
    .more {
      padding: 0 18px 18px;
    }
  }
</style>
