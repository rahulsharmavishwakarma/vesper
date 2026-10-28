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
      ? 'Pastes when you finish. Keep the text field focused.'
      : 'Your words stay in Vesper to copy.'}
  </p>
</div>
{#if desktop && settings.dictationOutput === 'paste'}
  <div class="permission-state" role="status">
    <div class="permission-copy">
      <span
        class="status-radio"
        data-state={checking ? 'checking' : ready ? 'ready' : 'denied'}
        aria-hidden="true"
        >{#if ready}<Icon name="check" />{:else if checking}<span class="spinning"
            ><Icon name="refresh" /></span
          >{:else}<Icon name="warning" />{/if}</span
      >
      <strong
        >{checking
          ? 'Checking paste access…'
          : ready
            ? 'Ready to paste'
            : 'Allow access to paste'}</strong
      >
      <p class="note">
        {ready
          ? 'Your clipboard is put back after pasting.'
          : permissions?.accessibility && permissions?.eventPosting
            ? 'Allow clipboard access in Privacy & Security to keep your clipboard.'
            : 'Turn on Vesper in System Settings → Privacy & Security → Accessibility.'}
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
    {#if checked && permissions && !permissions.accessibility}<p class="note restart-hint">
        Just turned it on? Then quit and reopen Vesper and check again.
      </p>{/if}
  </div>
{/if}
{#if error}<p class="error" role="alert">{error}</p>{/if}

<style>
  /* A row of the Shortcuts box like any other: status and help on the left,
     actions on the right. No box of its own (docs/ui-design.md#settings-grouping). */
  .permission-state {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2) var(--space-3);
  }
  .permission-copy {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    column-gap: var(--space-2);
    align-items: center;
    flex: 1 1 240px;
    min-width: 0;
  }
  .status-radio {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    color: var(--color-warning);
  }
  .permission-state strong {
    font-size: 13px;
    font-weight: 500;
  }
  .permission-state p {
    grid-column: 2;
    margin: 2px 0 0;
  }
  /* Lines up with the status text, past the icon. */
  .permission-state .restart-hint {
    flex-basis: 100%;
    margin: 0;
    padding-left: calc(18px + var(--space-2));
  }
  .permission-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .permission-actions :global(button) {
    min-height: 32px;
    padding: 6px 12px;
    font-size: 13px;
  }
</style>
