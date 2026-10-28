<script lang="ts">
  import {
    SOURCE_LABELS,
    forgetTermSource,
    getTermInventory,
    removeInventoryTerm,
    resumeTermSource,
    type TermInventorySummary,
    type TermSource,
  } from '../api/terms';
  let { disabled = false }: { disabled?: boolean } = $props();
  const PAGE = 40;
  let summary = $state<TermInventorySummary | null>(null);
  let busy = $state(false);
  let error = $state('');
  let showAll = $state(false);
  let visibleRows = $derived(summary ? (showAll ? summary.rows : summary.rows.slice(0, PAGE)) : []);

  async function run(action: () => Promise<TermInventorySummary>) {
    if (busy) return;
    busy = true;
    error = '';
    try {
      summary = await action();
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }
  function toggled(event: Event) {
    if ((event.currentTarget as HTMLDetailsElement).open && !summary) void run(getTermInventory);
  }
  function lastChecked(ms: number) {
    return new Date(ms).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
  }
  const label = (source: TermSource) => SOURCE_LABELS[source] ?? source;
</script>

<details class="group-box term-inventory" ontoggle={toggled}>
  <summary>
    Words Vesper found{#if summary}<span class="entry-count">{summary.usable}</span>{/if}
  </summary>
  <p class="note">Spellings from your Mac’s Text Replacements and spelling dictionary.</p>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if summary}
    {#if summary.sources.length}
      <ul class="source-list" aria-label="Sources">
        {#each summary.sources as source (source.source)}
          <li>
            <span class="source-name">{label(source.source)}</span>
            <span class="entry-count"
              >{source.paused
                ? 'Paused'
                : `${source.terms} ${source.terms === 1 ? 'word' : 'words'}`}</span
            >
            {#if source.paused}
              <button
                type="button"
                class="quiet"
                disabled={disabled || busy}
                onclick={() => run(() => resumeTermSource(source.source))}>Resume</button
              >
            {:else}
              <button
                type="button"
                class="quiet"
                title="Remove these words and stop reading this source"
                disabled={disabled || busy || source.terms === 0}
                onclick={() => run(() => forgetTermSource(source.source))}>Forget</button
              >
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    {#if summary.workspaces.length}
      <p class="note">
        {#each summary.workspaces as workspace, index (workspace.scope)}{index
            ? ' · '
            : ''}{workspace.label}: {workspace.terms}{/each}
      </p>
    {/if}
    {#if visibleRows.length}
      <ul class="term-list" aria-label="Words Vesper found">
        {#each visibleRows as row (row.id)}
          <li class="found-entry">
            <span class="found-term">{row.surface}</span>
            <span class="chips">
              {#each row.sources as source (source)}<span class="chip">{label(source)}</span>{/each}
              {#each row.workspaces as workspace (workspace)}<span class="chip">{workspace}</span
                >{/each}
            </span>
            <button
              type="button"
              class="icon-btn"
              aria-label={`Remove ${row.surface}`}
              title="Remove; it will not be added again"
              disabled={disabled || busy}
              onclick={() => run(() => removeInventoryTerm(row.id))}
              ><svg
                viewBox="0 0 16 16"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" /></svg
              ></button
            >
          </li>
        {/each}
      </ul>
      {#if !showAll && summary.rows.length > PAGE}
        <button type="button" class="quiet" onclick={() => (showAll = true)}
          >Show {summary.rows.length - PAGE} more</button
        >
      {/if}
    {:else}
      <p class="note">No words found yet. Vesper checks about once an hour when the Mac is idle.</p>
    {/if}
    {#if summary.lastRun}
      <p class="note">Last checked {lastChecked(summary.lastRun.startedMs)}</p>
    {/if}
  {:else if busy}
    <p class="note">Loading…</p>
  {/if}
</details>

<style>
  .term-inventory {
    padding-block: var(--space-4);
  }
  .term-inventory summary .entry-count {
    margin-left: var(--space-2);
  }
  .entry-count {
    color: var(--color-muted);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .source-list,
  .term-list {
    list-style: none;
    margin: 0 0 var(--space-3);
    padding: 0;
  }
  .source-list li,
  .found-entry {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) 0;
    border-top: 1px solid var(--color-line);
  }
  .source-name,
  .found-term {
    flex: 1 1 auto;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .found-term {
    font-weight: 600;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--space-1);
  }
  .chip {
    padding: 0 var(--space-2);
    border-radius: var(--radius-pill);
    background: var(--color-raised);
    color: var(--color-muted);
    font-size: 11px;
    white-space: nowrap;
  }
  .icon-btn {
    appearance: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-muted);
    cursor: pointer;
  }
  .icon-btn:hover:enabled {
    background: var(--color-raised);
    color: var(--color-ink);
  }
  .icon-btn:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }
  .icon-btn svg {
    width: 14px;
    height: 14px;
  }
  .icon-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  @media (max-width: 480px) {
    .found-entry {
      flex-wrap: wrap;
    }
    .chips {
      justify-content: flex-start;
    }
  }
</style>
