<script lang="ts">
  import { untrack } from 'svelte';
  import type { SessionRecord } from '../api/dataset';
  import {
    getPersonalization,
    getCorrectionSuggestions,
    rememberCorrection,
    savePersonalization,
    undoPersonalization,
    intentLabel,
    type CorrectionSuggestion,
    type PersonalizationProfile,
  } from '../api/personalization';
  let {
    record,
    disabled = false,
    onUpdate,
    onRemember = () => {},
  }: {
    record: SessionRecord;
    disabled?: boolean;
    onUpdate: (record: SessionRecord) => void;
    onRemember?: () => void;
  } = $props();
  let profile = $state<PersonalizationProfile | null>(null);
  let suggestions = $state<CorrectionSuggestion[]>([]);
  let project = $state('');
  let context = $state('');
  let pronunciation = $state(false);
  let busy = $state(false);
  let error = $state('');
  let message = $state('');
  let loadedKey = '';
  let generation = 0;
  async function load() {
    const id = record.id;
    const attempt = ++generation;
    busy = true;
    error = '';
    suggestions = [];
    try {
      const [saved, candidates] = await Promise.all([
        getPersonalization(),
        getCorrectionSuggestions(id),
      ]);
      if (generation !== attempt || id !== record.id) return;
      profile = saved;
      suggestions = candidates;
      project = record.personalizationProject || saved.activeProject;
    } catch (cause) {
      if (generation === attempt) error = String(cause);
    } finally {
      if (generation === attempt) busy = false;
    }
  }
  $effect(() => {
    const key = `${record.id}:${record.reviewHistory.length}`;
    if (!disabled && loadedKey !== key) {
      loadedKey = key;
      untrack(() => {
        void load();
      });
    }
  });
  async function remember(suggestion: CorrectionSuggestion) {
    if (!profile || busy || disabled) return;
    busy = true;
    error = '';
    message = '';
    try {
      profile = await rememberCorrection({
        sessionId: record.id,
        reviewRevisionId: suggestion.reviewRevisionId,
        pair: suggestion.pair,
        context,
        project,
        pronunciation,
        profileRevision: profile.revision,
      });
      suggestions = suggestions.filter((s) => s !== suggestion);
      message = 'Remembered for future dictation. You can pause or remove it in Settings.';
      onRemember();
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }
  async function undo() {
    busy = true;
    error = '';
    try {
      onUpdate(await undoPersonalization(record.id));
      message = 'Remembered edits undone for this recording.';
      onRemember();
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }
  async function useIntent() {
    if (!profile || !record.personalizationEvidence?.suggestedIntent) return;
    busy = true;
    error = '';
    try {
      profile.outputIntent = record.personalizationEvidence.suggestedIntent.intent;
      profile = await savePersonalization($state.snapshot(profile));
      message = 'Output form saved for your next recording.';
      onRemember();
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }
</script>

<div class="learning-review">
  {#if record.personalizationEvidence?.applied.length}
    <p class="note">
      {record.personalizationEvidence.applied.reduce((n, a) => n + a.occurrences, 0)} remembered correction(s)
      applied.
    </p>
    {#if record.personalizedTranscript && !record.personalizationRejected}
      <button type="button" onclick={undo} disabled={busy || disabled}>Undo remembered edits</button
      >
    {:else if record.personalizationRejected}<p class="note">
        Remembered edits were undone for this recording.
      </p>{/if}
    <details>
      <summary>Memories used</summary>
      <ul>
        {#each record.personalizationEvidence.applied as application (application.entryId)}
          {@const entry = profile?.vocabulary.find((e) => e.id === application.entryId)}
          <li>
            {entry?.phrase ?? 'Removed memory'} · {application.occurrences} occurrence(s){entry?.project
              ? ` · ${entry.project}`
              : ''}
          </li>
        {/each}
      </ul>
    </details>
  {/if}
  {#if record.personalizationEvidence?.ambiguousMatches}
    <p class="note">Competing remembered spellings matched. The original wording was kept.</p>
  {/if}
  {#if record.personalizationEvidence?.budgetExceeded}<p class="note">
      This recording exceeded the remembered-correction limit; its wording was kept.
    </p>{/if}
  {#if record.personalizationEvidence?.suggestedIntent}
    <p class="note">
      Similar reviewed examples suggest: {intentLabel(
        record.personalizationEvidence.suggestedIntent.intent,
      )}.
    </p>
    <button type="button" onclick={useIntent} disabled={!profile || busy || disabled}
      >Use this output form next time</button
    >
  {/if}
  {#if suggestions.length}
    <h4>Remember a correction</h4>
    <p class="note">
      Confirm a recurring spelling or pronunciation confusion. A change of mind belongs only to this
      recording.
    </p>
    <fieldset disabled={busy || disabled}>
      <label>Project (optional)<input bind:value={project} maxlength="64" /></label>
      <label
        >Context words (optional)<input
          bind:value={context}
          maxlength="256"
          placeholder="Kubernetes deployment"
        /></label
      >
      <label class="checkbox-field"
        ><input type="checkbox" bind:checked={pronunciation} />Recurring pronunciation confusion</label
      >
      {#each suggestions as suggestion (suggestion)}
        <div class="correction-pair">
          <p><span>{suggestion.pair.heard}</span> → <strong>{suggestion.pair.intended}</strong></p>
          <button type="button" onclick={() => remember(suggestion)}
            >Remember this correction</button
          >
          <button
            class="quiet"
            type="button"
            onclick={() => {
              suggestions = suggestions.filter((s) => s !== suggestion);
            }}>This instance only</button
          >
        </div>
      {/each}
    </fieldset>
  {/if}
  {#if error}<p role="alert">{error}</p>
    <button type="button" onclick={load} disabled={busy || disabled}>Reload suggestions</button
    >{/if}
  {#if message}<p role="status">{message}</p>{/if}
</div>

<style>
  .learning-review {
    display: grid;
    gap: var(--space-3);
    margin-block: var(--space-4);
  }
  fieldset {
    border: 0;
    padding: 0;
    display: grid;
    gap: var(--space-3);
    min-width: 0;
  }
  label {
    display: grid;
    gap: var(--space-2);
  }
  label.checkbox-field {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
  [role='alert'] {
    color: var(--color-error);
  }
  [role='status'] {
    color: var(--color-accent);
  }
  button {
    justify-self: start;
  }
  .correction-pair {
    padding-block: var(--space-2);
    overflow-wrap: anywhere;
  }
  .correction-pair p {
    white-space: pre-wrap;
  }
</style>
