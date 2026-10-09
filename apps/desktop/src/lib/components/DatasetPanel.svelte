<script lang="ts">
  import TranscriptCard from './TranscriptCard.svelte';
  import { selectionHighlight } from '../actions/selectionHighlight';
  import { historyReveal, revealAccessibility } from '../actions/historyReveal';
  import Select from './Select.svelte';
  import { onDestroy, onMount, tick, untrack } from 'svelte';
  import Icon from './Icon.svelte';
  import HistoryStats from './HistoryStats.svelte';
  import LearningReview from './LearningReview.svelte';
  import LearningSummary from './LearningSummary.svelte';
  import {
    filterSessions,
    paginateSessions,
    HISTORY_PAGE_SIZES,
    recordingDay,
    recordingDate,
    recordingState,
    transcriptPreview,
    type HistoryFilter,
  } from '../history';
  import type { AppEnvironment } from '../api/desktop';
  import {
    chooseAndExportDataset,
    chooseAndImportAudio,
    listSessions,
    type SessionRecord,
  } from '../api/dataset';

  let {
    environment,
    collectionEnabled,
    settingsLoaded,
    refreshKey = 0,
    externalBusy = false,
    view = 'full',
    onOpenHistory,
  }: {
    environment: AppEnvironment;
    collectionEnabled: boolean;
    settingsLoaded: boolean;
    refreshKey?: number;
    externalBusy?: boolean;
    view?: 'recent' | 'full' | 'hidden';
    onOpenHistory: () => void;
  } = $props();
  let records = $state<SessionRecord[]>([]);
  let selectedId = $state('');
  let query = $state('');
  let filter = $state<HistoryFilter>('all');
  let selectedDay = $state('');
  let page = $state(1);
  let pageSize = $state(10);
  let feedbackTarget = $state('');
  let pending = $state<'loading' | 'importing' | 'exporting' | null>(null);
  let error = $state<string | null>(null);
  let feedback = $state<string | null>(null);
  let approvedOnly = $state(true);
  let hasLoaded = $state(false);
  let learningRevision = $state(0);
  const desktop = $derived(environment === 'desktop');
  const filtered = $derived(
    filterSessions(records, query, filter).filter(
      (record) => !selectedDay || recordingDay(record.createdAt) === selectedDay,
    ),
  );
  const pagination = $derived(paginateSessions(filtered, page, pageSize));
  const selected = $derived(pagination.items.find((record) => record.id === selectedId));
  const approvedCount = $derived(
    records.filter((record) => record.reviewStatus === 'approved').length,
  );
  const exportCount = $derived(approvedOnly ? approvedCount : records.length);
  const controlsBusy = $derived(pending !== null || externalBusy);
  let appliedRefreshKey = 0;

  function message(cause: unknown) {
    return cause instanceof Error ? cause.message : String(cause);
  }
  function handleCardUpdate(updated: SessionRecord) {
    records = records.map((item) => (item.id === updated.id ? updated : item));
  }
  async function refresh() {
    if (!desktop || pending) return;
    feedbackTarget = '';
    pending = 'loading';
    error = null;
    feedback = null;
    try {
      records = await listSessions();
      hasLoaded = true;
      if (!records.some((record) => record.id === selectedId)) selectedId = '';
    } catch (cause) {
      error = `Could not load recordings. ${message(cause)}`;
    } finally {
      pending = null;
    }
  }
  async function importAudio() {
    if (!desktop || controlsBusy || !collectionEnabled || !settingsLoaded) return;
    feedbackTarget = '';
    pending = 'importing';
    error = null;
    feedback = null;
    try {
      const record = await chooseAndImportAudio();
      if (record) {
        records = [record, ...records.filter((item) => item.id !== record.id)];
        query = '';
        filter = 'all';
        selectedDay = '';
        page = 1;
        selectedId = record.id;
        hasLoaded = true;
        feedback = 'Audio imported. Review its transcript before exporting for training.';
      }
    } catch (cause) {
      error = `Could not import audio. ${message(cause)}`;
    } finally {
      pending = null;
    }
  }
  async function exportDataset() {
    if (!desktop || controlsBusy || !exportCount) return;
    feedbackTarget = 'export';
    pending = 'exporting';
    error = null;
    feedback = null;
    try {
      const result = await chooseAndExportDataset(approvedOnly);
      if (result)
        feedback = `Exported ${result.sessionCount} ${result.sessionCount === 1 ? 'example' : 'examples'} to ${result.destination}`;
    } catch (cause) {
      error = `Could not export the dataset. ${message(cause)}`;
    } finally {
      pending = null;
    }
  }
  async function selectRecording(record: SessionRecord, trigger: HTMLButtonElement) {
    selectedId = view !== 'recent' && selectedId === record.id ? '' : record.id;
    feedback = null;
    error = null;
    if (view === 'recent') {
      clearFilters();
      onOpenHistory();
    }
    await tick();
    trigger =
      (document.getElementById(`recording-trigger-${record.id}`) as HTMLButtonElement) ?? trigger;
    const bounds = trigger.getBoundingClientRect();
    const detailBottom = bounds.bottom + (trigger.nextElementSibling?.scrollHeight ?? 0);
    if (
      selectedId === record.id &&
      (bounds.top < 24 || Math.min(detailBottom, bounds.top + 360) > window.innerHeight)
    ) {
      trigger.scrollIntoView({ block: 'start', behavior: 'instant' });
    } else if (bounds.top < 0 || bounds.bottom > window.innerHeight) {
      trigger.scrollIntoView({ block: 'nearest', behavior: 'instant' });
    }
  }
  function recordingTitle(record: SessionRecord) {
    const text = transcriptPreview(record).replace(/\s+/g, ' ');
    if (text) return text.length > 160 ? `${text.slice(0, 160).trimEnd()}…` : text;
    return /^recording-[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}\.wav$/i.test(
      record.originalFilename,
    )
      ? 'Voice recording'
      : record.originalFilename || 'Untitled recording';
  }
  function clearFilters() {
    calmSwap(() => {
      query = '';
      filter = 'all';
      selectedDay = '';
      page = 1;
    });
  }
  let calmRows = $state(false);
  let calmTimer: ReturnType<typeof setTimeout> | undefined;
  onDestroy(() => clearTimeout(calmTimer));
  // A filter, search or day swap replaces rows under the gliding pill; hold
  // row entrances for one beat so the switch reads as a single motion.
  // Pagination turns keep replaying entrances; the timer only stills swaps.
  function calmSwap(mutator: () => void) {
    mutator();
    calmRows = true;
    clearTimeout(calmTimer);
    calmTimer = setTimeout(() => {
      calmRows = false;
    }, 450);
  }
  async function openLearningReview(id: string) {
    clearFilters();
    const index = records.findIndex((r) => r.id === id);
    if (index < 0) return;
    page = Math.floor(index / pageSize) + 1;
    selectedId = id;
    await tick();
    document.getElementById(`recording-details-${id}`)?.focus({ preventScroll: true });
  }
  async function revealPage() {
    await tick();
    const count = document.getElementById('history-result-count');
    count?.focus({ preventScroll: true });
    count?.scrollIntoView({ block: 'start', behavior: 'instant' });
  }
  function changePage(next: number) {
    if (controlsBusy) return;
    const nextPage = paginateSessions(filtered, next, pageSize).page;
    if (nextPage === pagination.page) return;
    page = nextPage;
    selectedId = '';
    void revealPage();
  }
  function changePageSize(size: string) {
    if (controlsBusy) return;
    pageSize = Number(size);
    page = 1;
    selectedId = '';
    void revealPage();
  }
  $effect(() => {
    if (page !== pagination.page) page = pagination.page;
  });
  onMount(() => {
    if (desktop) void refresh();
  });
  $effect(() => {
    const nextKey = refreshKey;
    if (desktop && !externalBusy && pending === null && nextKey !== appliedRefreshKey) {
      appliedRefreshKey = nextKey;
      untrack(() => {
        void refresh();
      });
    }
  });
  // View switches do not remount this panel, so returning to History or the
  // dictate tab would otherwise keep showing the list from before a shortcut
  // recording completed in another app.
  let previousView: typeof view | undefined;
  $effect(() => {
    const current = view;
    const changed = previousView !== undefined && current !== previousView;
    previousView = current;
    if (changed && current !== 'hidden') {
      untrack(() => {
        if (desktop && !externalBusy && pending === null) void refresh();
      });
    }
  });
</script>

{#snippet statusMessage()}
  <div aria-live="polite" class="history-feedback">
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if feedback}<p class="success">{feedback}</p>{/if}
  </div>
{/snippet}

{#snippet recordingDetails(record: SessionRecord)}
  <article
    class="session-editor history-reveal"
    use:revealAccessibility
    transition:historyReveal
    id={`recording-details-${record.id}`}
    aria-label="Recording details"
    tabindex="-1"
  >
    <TranscriptCard {record} disabled={controlsBusy || !desktop} onUpdate={handleCardUpdate} />
    <LearningReview
      {record}
      disabled={controlsBusy || !desktop}
      onUpdate={(updated) => {
        records = records.map((r) => (r.id === updated.id ? updated : r));
        learningRevision++;
      }}
      onRemember={() => {
        learningRevision++;
      }}
    />
    {#if feedbackTarget === record.id}{@render statusMessage()}{/if}
  </article>
{/snippet}

<section
  class="dataset-panel"
  class:recent-panel={view === 'recent'}
  class:ember-rise={view === 'recent'}
  style="--rise-delay: 0.21s;"
  data-view={view}
  hidden={view === 'hidden'}
  aria-labelledby="dataset-heading"
  aria-busy={pending !== null}
>
  {#if view === 'full' && desktop && hasLoaded}<LearningSummary
      disabled={controlsBusy}
      refreshKey={`${refreshKey}:${learningRevision}:${records.reduce((n, r) => n + r.reviewHistory.length, 0)}`}
      onReview={openLearningReview}
    />{/if}
  {#if view !== 'recent' && desktop && hasLoaded}<HistoryStats
      {records}
      {selectedDay}
      onSelectDay={(day) => {
        calmSwap(() => {
          selectedDay = day;
          page = 1;
        });
      }}
    />{/if}
  <div class="section-heading history-heading">
    <div>
      <h2 id="dataset-heading">{view === 'recent' ? 'Recent thoughts' : 'Recordings'}</h2>
      {#if view !== 'recent' && hasLoaded}<p class="note">
          {records.length} saved · {approvedCount} reviewed
        </p>{/if}
    </div>
    {#if view === 'recent'}
      <button class="quiet" type="button" onclick={onOpenHistory}
        >View all <Icon name="arrow" /></button
      >
    {:else}
      <div class="history-actions">
        <button
          class="quiet icon-button"
          type="button"
          onclick={refresh}
          disabled={!desktop || controlsBusy}
          aria-label={pending === 'loading' ? 'Loading recordings' : 'Refresh recordings'}
          title="Refresh recordings"
          ><span class:history-refreshing={pending === 'loading'}><Icon name="refresh" /></span
          ></button
        >
        <button
          type="button"
          onclick={importAudio}
          disabled={!desktop || controlsBusy || !collectionEnabled || !settingsLoaded}
          ><Icon name="upload" />{pending === 'importing' ? 'Importing…' : 'Import audio'}</button
        >
      </div>
    {/if}
  </div>
  {#if view !== 'recent'}
    {#if !desktop}<p class="preview-note">Your recordings are available in the desktop app.</p>
    {:else if !settingsLoaded}<p class="note">Load your settings before importing audio.</p>
    {:else if !collectionEnabled}<p class="note">
        Saving is off. Enable “Save recordings and transcripts” in Settings to keep new recordings
        and imports.
      </p>{/if}
    <div class="history-toolbar">
      <div class="history-search">
        <Icon name="search" /><label for="history-search" class="visually-hidden"
          >Search recordings</label
        ><input
          id="history-search"
          type="search"
          placeholder="Search words or filenames…"
          bind:value={query}
          oninput={() => {
            calmSwap(() => {
              page = 1;
            });
          }}
          disabled={!desktop || !hasLoaded}
        />{#if query}<button
            class="quiet icon-button"
            type="button"
            aria-label="Clear search"
            onclick={() => {
              query = '';
              page = 1;
            }}><Icon name="close" /></button
          >{/if}
      </div>
      <div
        class="history-filters"
        use:selectionHighlight={filter}
        role="group"
        aria-label="Filter recordings"
      >
        {#each [{ value: 'all', label: 'All', count: records.length }, { value: 'pending', label: 'Needs review', count: records.length - approvedCount }, { value: 'approved', label: 'Reviewed', count: approvedCount }] as choice (choice.value)}
          <button
            type="button"
            aria-pressed={filter === choice.value}
            disabled={!desktop || !hasLoaded}
            onclick={() => {
              calmSwap(() => {
                if (filter !== choice.value) page = 1;
                filter = choice.value as HistoryFilter;
              });
            }}>{choice.label}<span>{choice.count}</span></button
          >
        {/each}
      </div>
    </div>
  {/if}
  {#if feedbackTarget !== 'export' && (!selected || feedbackTarget !== selected.id || view === 'recent')}{@render statusMessage()}{/if}
  {#if pending === 'loading' && !hasLoaded}<p class="loading" role="status">
      Loading recordings…
    </p>{/if}
  {#if (!desktop || hasLoaded) && !records.length}
    <div class="empty-history">
      <Icon name="history" />
      <div>
        <p>No saved recordings</p>
        <span class="note"
          >{desktop
            ? 'Record in Dictate or import an audio file.'
            : 'Record audio in the desktop app.'}</span
        >
      </div>
    </div>
  {:else if hasLoaded}
    <div class="history-workspace">
      <div class="history-results">
        {#if view !== 'recent'}<p
            class="history-result-count"
            id="history-result-count"
            tabindex="-1"
            aria-live="polite"
            aria-atomic="true"
          >
            {#if pagination.total}Showing <span
                >{pagination.from.toLocaleString()}–{pagination.to.toLocaleString()}</span
              >
              of {pagination.total.toLocaleString()}
              {pagination.total === 1 ? 'recording' : 'recordings'}{query ||
              filter !== 'all' ||
              selectedDay
                ? ' found'
                : ''}{:else}0 recordings found{/if}
          </p>{/if}
        {#if view !== 'recent' && !filtered.length}
          <div class="history-no-results">
            <Icon name="search" />
            <h3>No matching recordings</h3>
            <p class="note">Try another word or show all recordings.</p>
            <button type="button" onclick={clearFilters}>Clear filters</button>
          </div>
        {:else}
          <ul
            class="session-list"
            id="history-recordings"
            aria-label="Saved recordings"
            data-calm={calmRows || undefined}
          >
            {#each view === 'recent' ? records.slice(0, 3) : pagination.items as record, index (record.id)}
              <li
                class:session-expanded={view !== 'recent' && selected?.id === record.id}
                style:--row-enter-delay={`${Math.min(index, 5) * 30}ms`}
              >
                <button
                  class="session-button"
                  id={`recording-trigger-${record.id}`}
                  type="button"
                  disabled={controlsBusy}
                  aria-expanded={view === 'recent' ? undefined : selected?.id === record.id}
                  aria-controls={view === 'recent' ? undefined : `recording-details-${record.id}`}
                  onclick={(event) => selectRecording(record, event.currentTarget)}
                >
                  <span class="session-copy">
                    <span class="session-preview">{recordingTitle(record)}</span>
                    <span class="session-meta"
                      ><time datetime={record.createdAt}>{recordingDate(record.createdAt)}</time
                      ><span
                        class="review-badge"
                        class:approved={record.reviewStatus === 'approved'}
                        >{recordingState(record)}</span
                      ></span
                    >
                  </span>
                  <Icon
                    name={view === 'recent' ? 'arrow' : 'chevron'}
                    active={view !== 'recent' && selected?.id === record.id}
                  />
                </button>
                {#if view !== 'recent' && selected?.id === record.id}{@render recordingDetails(
                    record,
                  )}{/if}
              </li>
            {/each}
          </ul>
          {#if view !== 'recent'}
            <nav class="history-pagination" aria-label="Recording pages">
              <div class="history-page-size">
                <label for="history-page-size">Per page</label>
                <div>
                  <Select
                    id="history-page-size"
                    value={String(pageSize)}
                    options={HISTORY_PAGE_SIZES.map((size) => ({
                      value: String(size),
                      label: String(size),
                    }))}
                    disabled={controlsBusy}
                    onChange={changePageSize}
                  />
                </div>
              </div>
              <div class="history-page-controls">
                <button
                  class="quiet"
                  type="button"
                  aria-label="Previous page of recordings"
                  aria-controls="history-recordings"
                  disabled={controlsBusy || pagination.page === 1}
                  onclick={() => changePage(pagination.page - 1)}
                  ><span class="history-page-back"><Icon name="arrow" /></span><span
                    class="history-page-button-label">Previous</span
                  ></button
                >
                <span class="history-page-position"
                  >Page <strong>{pagination.page.toLocaleString()}</strong> of {pagination.pageCount.toLocaleString()}</span
                >
                <button
                  class="quiet"
                  type="button"
                  aria-label="Next page of recordings"
                  aria-controls="history-recordings"
                  disabled={controlsBusy || pagination.page === pagination.pageCount}
                  onclick={() => changePage(pagination.page + 1)}
                  ><span class="history-page-button-label">Next</span><Icon name="arrow" /></button
                >
              </div>
            </nav>
          {/if}
        {/if}
      </div>
    </div>
  {/if}
  {#if view !== 'recent'}
    <details class="export-options">
      <summary
        ><span class="export-summary"
          ><span><Icon name="download" />Export training data</span><span class="note"
            >{records.length} saved</span
          ></span
        ></summary
      >
      <p class="note">
        Exports include original audio and saved labels. Pages, search, date and review filters do
        not change the export. Unsaved edits are excluded.
      </p>
      <label class="checkbox-field" for="approved-only"
        ><input
          id="approved-only"
          type="checkbox"
          bind:checked={approvedOnly}
          disabled={!desktop || controlsBusy}
        /><span>Reviewed examples only</span></label
      >
      <p class="note checkbox-help">Recommended for training. Turn off to archive every example.</p>
      <div class="dataset-actions">
        <button
          type="button"
          onclick={exportDataset}
          disabled={!desktop || controlsBusy || !exportCount}
          ><Icon name="download" />{pending === 'exporting'
            ? 'Exporting…'
            : `Export ${exportCount} ${exportCount === 1 ? 'example' : 'examples'}`}</button
        >
      </div>
      {#if feedbackTarget === 'export'}{@render statusMessage()}{/if}
      <p class="note">
        Import audio preserves the original file (up to 512 MiB) without running transcription.
      </p>
    </details>
  {/if}
</section>

<style>
  .history-result-count {
    scroll-margin-top: var(--space-6);
  }
  .history-result-count span {
    color: var(--color-ink);
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }
  .history-pagination {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-3) var(--space-5);
    padding-top: var(--space-4);
  }
  .history-page-size {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .history-page-size label {
    margin: 0;
    font-size: 12px;
    font-weight: 400;
    color: var(--color-muted);
  }
  .history-page-size > div {
    width: 76px;
  }
  .history-page-controls {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .history-page-controls button {
    padding-inline: var(--space-3);
  }
  .history-page-controls :global(.icon) {
    width: 16px;
    height: 16px;
    transition: transform 140ms var(--ease);
  }
  .history-page-controls button:hover:enabled :global(.icon) {
    transform: translateX(2px);
  }
  .history-page-back {
    display: flex;
    transform: rotate(180deg);
  }
  .history-page-position {
    font-size: 12px;
    color: var(--color-muted);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    padding-inline: var(--space-2);
  }
  .history-page-position strong {
    font-weight: 500;
    color: var(--color-ink);
  }
  @container history (max-width: 540px) {
    .history-page-button-label {
      display: none;
    }
    .history-page-controls button {
      width: 44px;
      padding: var(--space-2);
    }
  }
  @container history (max-width: 350px) {
    .history-pagination {
      gap: var(--space-2);
    }
    .history-page-controls {
      width: 100%;
      justify-content: space-between;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .history-page-controls :global(.icon) {
      transition: none;
    }
    .history-page-controls button:hover:enabled :global(.icon) {
      transform: none;
    }
  }
</style>
