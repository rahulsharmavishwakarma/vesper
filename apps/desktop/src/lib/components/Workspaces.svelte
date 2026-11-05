<script lang="ts">
  import { tick } from 'svelte';
  import type { PersonalizationProfile } from '../api/personalization';
  import {
    listWorkspaces,
    renameWorkspace,
    mergeWorkspace,
    deleteWorkspace,
    workspaceKindLabel,
    type WorkspaceSummary,
    type WorkspaceUpdate,
  } from '../api/workspaces';
  import Select from './Select.svelte';
  import Icon from './Icon.svelte';
  let {
    profile = $bindable(),
    disabled = false,
    onprofilechange,
  }: {
    profile: PersonalizationProfile;
    disabled?: boolean;
    /** Rename/merge moved scoped vocabulary: the panel reloads its profile. */
    onprofilechange?: () => void;
  } = $props();
  let workspaces = $state<WorkspaceSummary[]>([]);
  let busy = $state(false);
  let error = $state('');
  let feedback = $state('');
  let renaming = $state('');
  let draftName = $state('');
  let renameError = $state('');
  let merging = $state('');
  let mergeTarget = $state('');
  let removing = $state('');
  const locked = $derived(disabled || busy);

  async function load() {
    try {
      workspaces = await listWorkspaces();
    } catch (cause) {
      error = String(cause);
    }
  }
  $effect(() => {
    void load();
  });

  async function run(action: () => Promise<WorkspaceUpdate | null>, message: string) {
    if (locked) return false;
    busy = true;
    error = '';
    feedback = '';
    try {
      const update = await action();
      if (!update) return false;
      workspaces = update.workspaces;
      feedback = message;
      if (update.profileChanged) onprofilechange?.();
      return true;
    } catch (cause) {
      error = String(cause);
      return false;
    } finally {
      busy = false;
    }
  }

  async function startRename(workspace: WorkspaceSummary) {
    merging = '';
    removing = '';
    renaming = workspace.key;
    draftName = workspace.label;
    renameError = '';
    await tick();
    document.getElementById(`workspace-name-${workspace.key}`)?.focus();
  }
  async function stopRename(key: string) {
    renaming = '';
    renameError = '';
    await tick();
    document.getElementById(`workspace-rename-${key}`)?.focus();
  }
  async function saveRename(workspace: WorkspaceSummary) {
    const name = draftName.trim();
    if (!name) {
      renameError = 'Enter a name.';
      return;
    }
    if (name === workspace.label) return stopRename(workspace.key);
    if (
      workspaces.some(
        (other) => other.key !== workspace.key && other.label.toLowerCase() === name.toLowerCase(),
      )
    ) {
      renameError = 'Another workspace has this name. Use Merge instead.';
      return;
    }
    if (await run(() => renameWorkspace(workspace.key, name), `Renamed to ${name}.`))
      await stopRename(workspace.key);
  }
  function renameKey(event: KeyboardEvent, workspace: WorkspaceSummary) {
    // Keep editing keystrokes inside the field.
    event.stopPropagation();
    if (event.key === 'Enter') {
      event.preventDefault();
      void saveRename(workspace);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      void stopRename(workspace.key);
    }
  }
  function startMerge(workspace: WorkspaceSummary) {
    renaming = '';
    removing = '';
    merging = workspace.key;
    mergeTarget = workspaces.find((other) => other.key !== workspace.key)?.key ?? '';
  }
  async function confirmMerge(workspace: WorkspaceSummary) {
    const target = workspaces.find((other) => other.key === mergeTarget);
    if (!target) return;
    if (
      await run(
        () => mergeWorkspace(workspace.key, target.key),
        `Merged ${workspace.label} into ${target.label}.`,
      )
    )
      merging = '';
  }
  // Removal cannot be undone from the app, so it asks first, inline like Merge.
  // Focus starts on Cancel, the safe choice, as in the model removal dialog.
  async function startRemove(workspace: WorkspaceSummary) {
    renaming = '';
    merging = '';
    removing = workspace.key;
    await tick();
    document.getElementById(`workspace-remove-cancel-${workspace.key}`)?.focus();
  }
  async function stopRemove(key: string) {
    removing = '';
    await tick();
    document.getElementById(`workspace-remove-${key}`)?.focus();
  }
  async function confirmRemove(workspace: WorkspaceSummary) {
    if (
      await run(
        () => deleteWorkspace(workspace.key),
        `Removed ${workspace.label}. It will not be detected again.`,
      )
    )
      removing = '';
  }
  function removeKey(event: KeyboardEvent, key: string) {
    if (event.key !== 'Escape') return;
    event.preventDefault();
    event.stopPropagation();
    void stopRemove(key);
  }
</script>

<div class="settings-group workspaces" id="workspaces">
  <div class="group-heading">
    <h3>Workspaces</h3>
    {#if workspaces.length}<span class="entry-count">{workspaces.length}</span>{/if}
  </div>
  <div class="group-box">
    <div class="setting-toggle">
      <label class="checkbox-field" for="automatic-workspaces"
        ><input
          id="automatic-workspaces"
          type="checkbox"
          bind:checked={profile.automaticWorkspaces}
          {disabled}
          aria-describedby="automatic-workspaces-help"
        /><span>Detect workspaces automatically</span></label
      >
      <p id="automatic-workspaces-help" class="note checkbox-help">
        Notes the folder or website you dictate in, never its contents. Each keeps its own words.
      </p>
    </div>
    {#if workspaces.length === 0 && profile.automaticWorkspaces}
      <p class="note">None yet. One appears after about five dictations on two days.</p>
    {/if}
    {#each workspaces as workspace (workspace.key)}
      <div class="workspace">
        {#if renaming === workspace.key}
          <div class="rename-bar">
            <input
              id={`workspace-name-${workspace.key}`}
              aria-label={`New name for ${workspace.label}`}
              aria-invalid={renameError ? 'true' : undefined}
              aria-describedby={renameError ? `workspace-name-error-${workspace.key}` : undefined}
              bind:value={draftName}
              maxlength="64"
              disabled={locked}
              onkeydown={(event) => renameKey(event, workspace)}
            />
            <button
              type="button"
              class="primary"
              disabled={locked}
              onclick={() => saveRename(workspace)}>Save</button
            >
            <button
              type="button"
              class="quiet"
              disabled={locked}
              onclick={() => stopRename(workspace.key)}>Cancel</button
            >
          </div>
          {#if renameError}<p
              id={`workspace-name-error-${workspace.key}`}
              class="note invalid"
              role="alert"
            >
              {renameError}
            </p>{/if}
        {:else}
          <div class="workspace-heading">
            <span class="workspace-label">{workspace.label}</span>
            <span class="entry-count"
              >{workspaceKindLabel(workspace)} · {workspace.dictations}
              {workspace.dictations === 1 ? 'dictation' : 'dictations'}</span
            >
          </div>
        {/if}
        {#if merging === workspace.key}
          <div class="merge-bar">
            <label for={`workspace-merge-${workspace.key}`}>Merge {workspace.label} into</label>
            <Select
              id={`workspace-merge-${workspace.key}`}
              bind:value={mergeTarget}
              options={workspaces
                .filter((other) => other.key !== workspace.key)
                .map((other) => ({ value: other.key, label: other.label }))}
              disabled={locked}
            />
            <div class="actions">
              <button
                type="button"
                class="primary"
                disabled={locked || !mergeTarget}
                onclick={() => confirmMerge(workspace)}>Merge</button
              >
              <button type="button" class="quiet" disabled={locked} onclick={() => (merging = '')}
                >Cancel</button
              >
            </div>
          </div>
        {:else if removing === workspace.key}
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <div
            class="remove-bar"
            role="group"
            aria-labelledby={`workspace-remove-question-${workspace.key}`}
            onkeydown={(event) => removeKey(event, workspace.key)}
          >
            <p id={`workspace-remove-question-${workspace.key}`}>
              Remove {workspace.label}? It leaves this list and is never detected again.
            </p>
            <div class="actions">
              <button
                type="button"
                class="remove-confirm"
                disabled={locked}
                onclick={() => confirmRemove(workspace)}><Icon name="trash" />Remove</button
              >
              <button
                id={`workspace-remove-cancel-${workspace.key}`}
                type="button"
                class="quiet"
                disabled={locked}
                onclick={() => stopRemove(workspace.key)}>Cancel</button
              >
            </div>
          </div>
        {:else if renaming !== workspace.key}
          <div class="actions">
            <button
              id={`workspace-rename-${workspace.key}`}
              type="button"
              class="quiet"
              disabled={locked}
              onclick={() => startRename(workspace)}>Rename</button
            >
            {#if workspaces.length > 1}
              <button
                type="button"
                class="quiet"
                disabled={locked}
                onclick={() => startMerge(workspace)}>Merge…</button
              >
            {/if}
            <button
              id={`workspace-remove-${workspace.key}`}
              type="button"
              class="quiet danger"
              aria-label={`Remove workspace ${workspace.label}…`}
              disabled={locked}
              onclick={() => startRemove(workspace)}>Remove…</button
            >
          </div>
        {/if}
      </div>
    {/each}
  </div>
  {#if error}<p class="note invalid" role="alert">{error}</p>{/if}
  {#if feedback}<p class="note" role="status">{feedback}</p>{/if}
</div>

<style>
  .group-heading,
  .workspace-heading {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-2);
  }
  .group-heading {
    margin-bottom: var(--space-3);
  }
  .group-heading h3 {
    margin: 0;
  }
  .entry-count {
    color: var(--color-muted);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  /* One line per workspace: name and details left, actions right. */
  .workspace {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2) var(--space-4);
    padding-block: var(--space-3);
  }
  .workspace-heading {
    flex: 1 1 160px;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    min-width: 0;
  }
  .workspace > :is(.rename-bar, .merge-bar, .remove-bar, .note) {
    flex-basis: 100%;
  }
  .workspace .actions {
    gap: var(--space-1);
  }
  .workspace-label {
    font-size: 13px;
    font-weight: 500;
    overflow-wrap: anywhere;
    min-width: 0;
  }
  .actions,
  .rename-bar {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .rename-bar input {
    flex: 1 1 160px;
    min-width: 0;
  }
  .merge-bar,
  .remove-bar {
    display: grid;
    gap: var(--space-2);
  }
  .remove-bar p {
    margin: 0;
    font-size: 13px;
    overflow-wrap: anywhere;
  }
  /* The destructive action reads as one: danger text, set apart from Rename
     and Merge… so a near miss lands in empty space. */
  .workspace .actions .danger {
    margin-inline-start: var(--space-4);
    color: var(--color-danger);
  }
  .workspace .actions .danger:hover:enabled {
    color: var(--color-danger);
    background: color-mix(in srgb, var(--color-danger) 10%, var(--color-transparent));
  }
  .remove-confirm {
    border-color: var(--color-danger);
    color: var(--color-danger);
  }
  .remove-confirm:hover:enabled {
    background: color-mix(in srgb, var(--color-danger) 10%, var(--color-transparent));
  }
</style>
