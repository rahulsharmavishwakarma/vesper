<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import { recordingShortcuts, type AppConfig } from '../config';
  import { getDictationPermissions, type DictationPermissions } from '../api/dictation';
  import Select from './Select.svelte';
  import ShortcutSetting from './ShortcutSetting.svelte';
  let {
    settings = $bindable(),
    desktop,
    disabled,
  }: { settings: AppConfig; desktop: boolean; disabled: boolean } = $props();
  let permissions = $state<DictationPermissions | null>(null);
  let checking = $state(false);
  let error = $state<string | null>(null);
  let checked = $state(false);
  let disposed = false;
  const shortcuts = $derived(recordingShortcuts(settings));
  function selectShortcut(mode: 'hold' | 'toggle', value: string) {
    settings.holdShortcut = mode === 'hold' ? value : shortcuts.hold;
    settings.toggleShortcut = mode === 'toggle' ? value : shortcuts.toggle;
  }
  function beginShortcutEdit() {
    const { holdShortcut, toggleShortcut } = settings;
    return () => {
      settings.holdShortcut = holdShortcut;
      settings.toggleShortcut = toggleShortcut;
    };
  }
  const ready = $derived(
    permissions?.accessibility && permissions?.eventPosting && permissions?.clipboard,
  );
  async function refresh(request = false) {
    if (!desktop || checking) return;
    checking = true;
    error = null;
    try {
      const next = await getDictationPermissions(request);
      if (!disposed) {
        permissions = next;
        checked = true;
      }
    } catch (cause) {
      if (!disposed) error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      if (!disposed) checking = false;
    }
  }
  onMount(() => {
    void refresh();
    const focus = () => {
      if (settings.dictationOutput === 'paste') void refresh();
    };
    window.addEventListener('focus', focus);
    return () => {
      disposed = true;
      window.removeEventListener('focus', focus);
    };
  });
</script>

<ShortcutSetting
  id="hold-shortcut"
  label="Hold to talk"
  value={shortcuts.hold}
  other={shortcuts.toggle}
  {disabled}
  onBegin={beginShortcutEdit}
  onChange={(value) => selectShortcut('hold', value)}
  help="Hold while speaking, then release to finish."
/>
<ShortcutSetting
  id="toggle-shortcut"
  label="Press to toggle"
  value={shortcuts.toggle}
  other={shortcuts.hold}
  {disabled}
  onBegin={beginShortcutEdit}
  onChange={(value) => selectShortcut('toggle', value)}
  help="Press once to start and again to finish."
/>
<div class="field">
  <label for="dictation-output">After shortcut dictation</label>
  <Select
    id="dictation-output"
    bind:value={settings.dictationOutput}
    options={[
      { value: 'app', label: 'Keep in app', description: 'View and copy your finished transcript' },
      {
        value: 'paste',
        label: 'Paste into the original text field',
        description:
          permissions?.supported === false
            ? 'Available on macOS'
            : 'Use your words in the app you were typing in',
        disabled: !desktop || permissions?.supported === false,
      },
    ]}
    {disabled}
    onChange={(value) => {
      if (value === 'paste') void refresh(true);
    }}
    describedBy="dictation-output-help"
  />
  <p id="dictation-output-help" class="note">
    {settings.dictationOutput === 'paste'
      ? 'Release Hold, or stop Toggle, to paste the finished transcript automatically. Keep the original text field focused until it finishes.'
      : 'Automatic paste is off. Choose Paste into the original text field to insert your words after releasing the shortcut.'}
  </p>
</div>
{#if desktop && settings.dictationOutput === 'paste'}
  <div class="permission-state" role="status">
    <span
      class="status-radio"
      data-state={checking ? 'checking' : ready ? 'ready' : 'denied'}
      aria-hidden="true"
      >{#if ready}<Icon name="check" />{:else if checking}<span class="spinning"
          ><Icon name="refresh" /></span
        >{:else}<Icon name="warning" />{/if}</span
    >
    <div class="permission-copy">
      <strong
        >{checking
          ? 'Checking paste access…'
          : ready
            ? 'Ready to paste'
            : 'Allow access to paste'}</strong
      >
      <p class="note">
        {ready
          ? 'Your original clipboard is restored when available.'
          : permissions?.accessibility && permissions?.eventPosting
            ? 'Allow clipboard access in macOS Privacy & Security so your original clipboard can be preserved.'
            : 'Enable Vesper in macOS System Settings → Privacy & Security → Accessibility, then check access.'}
      </p>
    </div>
    <div class="permission-actions">
      {#if !ready}<button
          type="button"
          onclick={() => refresh(true)}
          disabled={disabled || checking}><Icon name="check" />Allow access</button
        >{/if}
      {#if !ready}<button
          type="button"
          class="quiet"
          onclick={() => refresh()}
          disabled={disabled || checking}
          ><span class:spinning={checking} aria-hidden="true"><Icon name="refresh" /></span
          >{checking ? 'Checking…' : 'Check access'}</button
        >{/if}
    </div>
    {#if checked && permissions && !permissions.accessibility}<p class="note">
        If you just turned this on, quit and reopen Vesper, then check again — macOS can keep
        reporting the old state until the app restarts.
      </p>{/if}
  </div>
{/if}
{#if error}<p class="error" role="alert">{error}</p>{/if}

<style>
  .permission-state {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-4);
    margin-bottom: var(--space-6);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    background: var(--color-raised);
  }
  .permission-copy {
    flex: 1 1 240px;
  }
  .status-radio {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border: 1px solid var(--color-line);
    border-radius: 50%;
    background: var(--color-raised);
    color: var(--color-warning);
  }
  .permission-state strong {
    font-size: 13px;
    font-weight: 500;
  }
  .permission-state p {
    margin-bottom: 0;
  }
  .permission-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
</style>
