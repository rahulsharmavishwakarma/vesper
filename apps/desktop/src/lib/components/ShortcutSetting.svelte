<script lang="ts">
  import { onMount } from 'svelte';
  import {
    isMacKeyboard,
    sameShortcut,
    shortcutFromKeyboard,
    shortcutIdentity,
    shortcutLabel,
  } from '../recording-shortcuts';
  import { shortcutRecording } from '../api/shortcutRecorder';

  let {
    id,
    value,
    other,
    label,
    help,
    disabled,
    onChange,
    onBegin,
  }: {
    id: string;
    value: string;
    other: string;
    label: string;
    help: string;
    disabled: boolean;
    onChange: (value: string) => void;
    onBegin: () => () => void;
  } = $props();
  let field: HTMLButtonElement;
  let editing = $state(false);
  let preparing = $state(false);
  let captured = $state(false);
  let error = $state<string | null>(null);
  let session: ReturnType<typeof shortcutRecording> | null = null;
  let rollback: (() => void) | null = null;
  const hint = $derived(
    preparing ? 'Getting ready…' : editing ? (captured ? 'Recorded' : 'Press keys') : 'Change',
  );

  function accept(shortcut: string) {
    if (!editing || disabled || !field.getClientRects().length || document.activeElement !== field)
      return;
    const candidate = shortcutIdentity(shortcut);
    if (!candidate) {
      error = 'That key combination is not supported.';
      return;
    }
    if (sameShortcut(candidate, other)) {
      error = 'Already used by the other recording action. Try another shortcut.';
      return;
    }
    error = null;
    captured = true;
    if (!sameShortcut(candidate, value)) onChange(candidate);
  }
  async function begin() {
    if (editing || disabled || field.matches(':disabled') || !field.getClientRects().length) return;
    field.focus();
    if (document.activeElement !== field) return;
    rollback = onBegin();
    editing = true;
    preparing = true;
    captured = false;
    error = null;
    const next = shortcutRecording(accept);
    session = next;
    try {
      await next.ready;
      if (session === next) {
        if (document.activeElement !== field || !field.getClientRects().length) stop();
        else preparing = false;
      }
    } catch (cause) {
      if (session !== next) return;
      stop();
      error = cause instanceof Error ? cause.message : String(cause);
    }
  }
  function stop(cancel = false) {
    session?.stop();
    session = null;
    if (cancel) rollback?.();
    rollback = null;
    editing = false;
    preparing = false;
    if (cancel) {
      error = null;
      captured = false;
    }
  }
  function keydown(event: KeyboardEvent) {
    if (!editing) return; // Enter/Space activates the normal button.
    // Bare Tab and Shift+Tab keep moving focus; Tab with another modifier records.
    if (event.key === 'Tab' && !event.ctrlKey && !event.metaKey && !event.altKey) {
      stop();
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    if (event.key === 'Escape') {
      stop(true);
      return;
    }
    if (preparing || event.repeat || /^(Control|Meta|Alt|Shift)$/.test(event.key)) return;
    const shortcut = shortcutFromKeyboard(event);
    if (shortcut) accept(shortcut);
    else
      error = isMacKeyboard()
        ? 'Press Command, Ctrl or Option with another key — Enter, Tab, Backspace and Delete work too.'
        : 'Press Ctrl, Alt or Super with another key — Enter, Tab, Backspace and Delete work too.';
  }
  $effect(() => {
    if (disabled) stop();
  });
  onMount(() => {
    const blur = () => stop();
    const visibility = () => {
      if (document.hidden) stop();
    };
    window.addEventListener('blur', blur);
    document.addEventListener('visibilitychange', visibility);
    return () => {
      stop();
      window.removeEventListener('blur', blur);
      document.removeEventListener('visibilitychange', visibility);
    };
  });
</script>

<div class="field">
  <label for={id}>{label}</label>
  <button
    bind:this={field}
    {id}
    type="button"
    class="shortcut-field"
    class:is-recording={editing}
    class:has-error={!!error}
    {disabled}
    aria-label={`${label}: ${shortcutLabel(value)}. Press to change shortcut.`}
    aria-describedby={`${id}-help ${id}-status`}
    aria-busy={preparing}
    onclick={begin}
    onkeydown={keydown}
    onkeyup={(event) => {
      if (editing && event.key !== 'Tab') event.preventDefault();
    }}
    onblur={() => stop()}
  >
    <span class="binding"
      >{editing && !captured
        ? preparing
          ? 'Getting ready…'
          : 'Press your shortcut…'
        : shortcutLabel(value)}</span
    >
    <span class="binding-hint" aria-hidden="true">{hint}</span>
  </button>
  <p id={`${id}-help`} class="note">{help}</p>
  <p id={`${id}-status`} class="shortcut-status note" class:error={!!error} aria-live="polite">
    {error ??
      (editing
        ? captured
          ? 'Shortcut recorded. It saves automatically, or Esc to cancel.'
          : 'Press a key combination. Esc to cancel.'
        : '')}
  </p>
</div>

<style>
  .shortcut-field {
    display: flex;
    justify-content: space-between;
    gap: var(--space-3);
    width: 100%;
    min-height: 44px;
    padding: 12px 16px;
    text-align: left;
    background: var(--color-bg);
    border: 1px solid var(--color-control-edge);
    border-radius: var(--radius-sm);
  }
  .shortcut-field:hover:enabled {
    background: var(--color-raised);
    border-color: var(--color-control-hover);
  }
  .shortcut-field:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 3px;
  }
  .shortcut-field:active:enabled {
    transform: none;
    border-color: var(--color-accent);
  }
  .shortcut-field.is-recording {
    border-color: var(--color-accent);
  }
  .shortcut-field.has-error {
    border-color: var(--color-error);
  }
  .shortcut-field:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .binding {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .binding-hint {
    flex-shrink: 0;
    color: var(--color-muted);
    font-size: 11px;
  }
  .is-recording .binding-hint {
    color: var(--color-accent);
  }
  .shortcut-status {
    font-size: 11px;
    margin-top: var(--space-2);
    margin-bottom: 0;
  }
</style>
