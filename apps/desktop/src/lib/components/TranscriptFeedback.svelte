<script lang="ts">
  import Icon from './Icon.svelte';
  import { selectionHighlight } from '../actions/selectionHighlight';
  import type { TranscriptFeedback } from '../api/dataset';
  let {
    value = null,
    disabled = false,
    onSave,
  }: {
    value?: TranscriptFeedback;
    disabled?: boolean;
    onSave: (value: TranscriptFeedback) => Promise<void>;
  } = $props();
  let saving = $state(false);
  let proposed = $state<TranscriptFeedback>(null);
  let previous = $state<TranscriptFeedback>(null);
  let canUndo = $state(false);
  let error = $state('');
  const choice = $derived(saving ? proposed : value);
  async function save(next: TranscriptFeedback, undo = false) {
    if (disabled || saving) return;
    const before = value;
    proposed = next;
    saving = true;
    error = '';
    try {
      await onSave(next);
      previous = before;
      canUndo = !undo;
    } catch (cause) {
      error = `Could not save feedback. ${cause instanceof Error ? cause.message : String(cause)}`;
    } finally {
      saving = false;
    }
  }
</script>

<div class="transcript-feedback" aria-busy={saving}>
  <div class="feedback-line">
    <div
      class="feedback-choices"
      class:negative={choice === 'dislike'}
      use:selectionHighlight={choice}
      role="group"
      aria-label="Transcript feedback"
    >
      <button
        type="button"
        aria-label="Like transcript"
        title="Helpful"
        aria-pressed={choice === 'like'}
        disabled={disabled || saving}
        onclick={() => save(choice === 'like' ? null : 'like')}
        ><Icon name="like" active={choice === 'like'} /></button
      >
      <button
        type="button"
        aria-label="Dislike transcript"
        title="Needs work"
        aria-pressed={choice === 'dislike'}
        disabled={disabled || saving}
        onclick={() => save(choice === 'dislike' ? null : 'dislike')}
        ><Icon name="dislike" active={choice === 'dislike'} /></button
      >
    </div>
    <span class="feedback-label" role="status"
      >{saving
        ? 'Saving feedback…'
        : choice === 'like'
          ? 'Helpful'
          : choice === 'dislike'
            ? 'Needs work'
            : 'Was this helpful?'}</span
    >
    {#if canUndo}<button
        class="quiet feedback-undo"
        type="button"
        disabled={disabled || saving}
        onclick={() => save(previous, true)}>Undo</button
      >{/if}
  </div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</div>

<style>
  /* H02 Quiet slide · P5 H5 E5 S5 R5 V4. The persisted value owns success. */
  .transcript-feedback {
    padding-block: var(--space-3);
    margin-bottom: var(--space-3);
    border-top: 1px solid var(--color-line);
  }
  .feedback-line {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-3);
  }
  .feedback-choices {
    display: flex;
    flex: 0 0 auto;
    gap: var(--space-1);
    padding: var(--space-1);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-pill);
  }
  .feedback-choices :global(button) {
    width: 48px;
    min-height: 44px;
    padding: var(--space-2);
    border: 0;
    color: var(--color-muted);
    border-radius: var(--radius-pill);
  }
  .feedback-choices :global(button:disabled) {
    opacity: 0.7;
  }
  .feedback-choices :global(button:hover:enabled) {
    color: var(--color-ink);
  }
  .feedback-choices:global(.sliding-selection)::before {
    border-radius: var(--radius-pill);
  }
  .negative {
    --selection-color: var(--color-error-tint);
  }
  .negative :global(button[aria-pressed='true']) {
    color: var(--color-error);
  }
  .feedback-label {
    font-size: 11px;
    color: var(--color-muted);
  }
  .feedback-undo {
    font-size: 11px;
    padding-inline: var(--space-2);
  }
  .error {
    font-size: 12px;
    margin: var(--space-2) 0 0;
    overflow-wrap: anywhere;
  }
</style>
