<script lang="ts">
  import ShortcutSetting from './ShortcutSetting.svelte';
  import { recordingShortcuts, type AppConfig } from '../config';

  let {
    settings,
    onSaveSettings,
  }: {
    settings: AppConfig;
    onSaveSettings: (next: AppConfig) => Promise<AppConfig>;
  } = $props();

  let error = $state<string | null>(null);
  const shortcuts = $derived(recordingShortcuts(settings));

  // The same recorder as Settings → Shortcuts: activate the field, press the
  // keys, and it saves; Escape cancels. Conflicts show beside the field.
  async function choose(mode: 'hold' | 'toggle', value: string) {
    error = null;
    try {
      await onSaveSettings({
        ...settings,
        holdShortcut: mode === 'hold' ? value : shortcuts.hold,
        toggleShortcut: mode === 'toggle' ? value : shortcuts.toggle,
      });
    } catch (cause) {
      error = `Could not save the shortcut. ${String(cause)}`;
    }
  }
</script>

<div class="card">
  <div class="sc">
    <ShortcutSetting
      id="setup-hold-shortcut"
      label="Hold to talk"
      value={shortcuts.hold}
      other={shortcuts.toggle}
      disabled={false}
      onBegin={() => () => {}}
      onChange={(value) => void choose('hold', value)}
      help="Hold while you speak, release to finish."
    />
  </div>
  <div class="sc">
    <ShortcutSetting
      id="setup-toggle-shortcut"
      label="Press to toggle"
      value={shortcuts.toggle}
      other={shortcuts.hold}
      disabled={false}
      onBegin={() => () => {}}
      onChange={(value) => void choose('toggle', value)}
      help="Press once to start, again to finish."
    />
  </div>
</div>
{#if error}<p class="error" role="alert">{error}</p>{/if}
<p class="hint">Use at least one of ⌘ ⌥ ⌃ ⇧ so it won’t fire while you type.</p>

<style>
  .card {
    border: 1px solid var(--color-line);
    border-radius: var(--radius-lg);
    background: var(--color-raised);
    overflow: hidden;
  }
  .sc {
    padding: 20px 26px;
  }
  .sc + .sc {
    border-top: 1px solid var(--color-line);
  }
  .hint,
  .error {
    margin: 14px 0 0;
    font-size: 13px;
    color: var(--color-muted);
  }
  .error {
    color: var(--color-error);
  }
  @container first-run (max-width: 760px) {
    .sc {
      padding: 18px;
    }
  }
</style>
