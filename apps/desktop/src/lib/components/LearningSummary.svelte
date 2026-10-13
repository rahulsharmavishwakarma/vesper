<script lang="ts">
  import { untrack } from 'svelte';
  import { getLearningDashboard, type LearningDashboard } from '../api/personalization';
  let {
    disabled = false,
    refreshKey = '',
    onReview,
  }: { disabled?: boolean; refreshKey?: string; onReview: (id: string) => void } = $props();
  let report = $state<LearningDashboard | null>(null);
  let busy = $state(false);
  let error = $state('');
  let loadedKey: string | null = null;
  async function load() {
    if (busy || disabled) return;
    busy = true;
    error = '';
    try {
      report = await getLearningDashboard();
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }
  $effect(() => {
    const key = refreshKey;
    if (!disabled && !busy && key !== loadedKey) {
      loadedKey = key;
      untrack(() => {
        void load();
      });
    }
  });
</script>

<details class="learning-summary">
  <summary>Learning on this device</summary>
  <div class="learning-content">
    {#if report}
      <p>
        {report.appliedOccurrences} remembered corrections applied across {report.appliedSessions} recordings
        · {report.undoneSessions} undone
      </p>
      <p class="note">
        {report.reviewedSessions} of {report.inspectedSessions} recent recordings reviewed.
      </p>
      {#if report.reviewQueue.length}
        <h3>A few useful reviews</h3>
        <ul>
          {#each report.reviewQueue as item (item.sessionId)}<li>
              <button
                class="quiet"
                type="button"
                disabled={disabled || busy}
                onclick={() => onReview(item.sessionId)}>{item.reason}</button
              >
            </li>{/each}
        </ul>
      {/if}
      <h3>Do reviewed memories help?</h3>
      {#if report.replay.length}
        <div class="comparison-scroll">
          <table>
            <thead
              ><tr
                ><th>Speech model / language</th><th>Reviewed recordings</th><th
                  >Original word errors</th
                ><th>With memory</th><th>Improved / worsened</th></tr
              ></thead
            >
            <tbody
              >{#each report.replay as group (`${group.modelId}:${group.language}:${group.evidenceKey}`)}<tr
                  ><td
                    >{group.modelId} / {group.language}<br /><small
                      >Settings group {group.evidenceKey.slice(0, 8)}</small
                    ></td
                  ><td>{group.samples} ({group.referenceWords} reference words)</td><td
                    >{group.baselineEdits}</td
                  ><td>{group.memoryEdits}</td><td>{group.improved} / {group.worsened}</td></tr
                >{/each}</tbody
            >
          </table>
        </div>
      {:else}
        <p class="note">
          No comparison yet — review a recording after confirming a correction to see if it helped.
        </p>
      {/if}
      <details>
        <summary>How this is measured</summary>
        <p class="note">
          Applying a correction does not by itself show an accuracy gain. This compares saved raw
          text with your later verbatim reviews using review-confirmed memories saved before those
          recordings. Automatically learned spellings aren't included. It excludes source
          recordings, duplicate audio and unsupported lengths, checks word corrections only, and
          does not retrain or rerun speech models. Up to 512 recent recordings are inspected.
        </p>
      </details>
    {/if}
    {#if error}<p role="alert">{error}</p>{/if}
    <button type="button" onclick={load} disabled={disabled || busy}
      >{busy ? 'Checking…' : 'Refresh learning report'}</button
    >
  </div>
</details>

<style>
  .learning-summary {
    margin-bottom: var(--space-5);
    border-bottom: 1px solid var(--color-line);
    padding-bottom: var(--space-3);
  }
  summary {
    cursor: pointer;
  }
  .learning-content {
    padding-block: var(--space-4);
    display: grid;
    gap: var(--space-3);
  }
  .comparison-scroll {
    overflow-x: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  th,
  td {
    padding: var(--space-3);
    text-align: left;
    border-bottom: 1px solid var(--color-line);
    overflow-wrap: anywhere;
  }
  [role='alert'] {
    color: var(--color-error);
  }
  th {
    color: var(--color-muted);
    font-weight: 500;
  }
  button {
    justify-self: start;
  }
</style>
