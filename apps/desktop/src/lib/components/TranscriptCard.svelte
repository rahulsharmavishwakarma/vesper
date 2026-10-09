<script lang="ts">
  import Icon from './Icon.svelte';
  import CopyGlyph from './CopyGlyph.svelte';
  import { onDestroy } from 'svelte';
  import {
    approveCorrectionAndLearn,
    approveLike,
    retractApproval,
    sessionAudioDataUrl,
    setTranscriptFeedback,
    type SessionRecord,
  } from '../api/dataset';
  import { rememberCleanup } from '../api/personalization';

  let {
    record,
    disabled = false,
    onUpdate,
  }: {
    record: SessionRecord;
    disabled?: boolean;
    onUpdate: (updated: SessionRecord) => void;
  } = $props();

  let editing = $state(false);
  let editText = $state('');
  let detailsOpen = $state(false);
  let pending = $state<'like' | 'dislike' | 'save' | 'undo' | 'audio' | 'remember' | null>(null);
  let message = $state<string | null>(null);
  let error = $state<string | null>(null);
  let audioUrl = $state<string | null>(null);
  let audioMissing = $state(false);
  let playing = $state(false);
  let position = $state(0);
  let duration = $state(0);
  let speed = $state(1);
  let copied = $state(false);
  let remembered = $state(false);
  let audio: HTMLAudioElement | undefined = $state();
  let copyTimer: ReturnType<typeof setTimeout> | undefined;
  let messageTimer: ReturnType<typeof setTimeout> | undefined;

  onDestroy(() => {
    clearTimeout(copyTimer);
    clearTimeout(messageTimer);
  });

  const heard = $derived(record.rawTranscript ?? '');
  const polished = $derived(
    record.correctedTranscript ??
      record.personalizedTranscript ??
      record.cleanedTranscript ??
      record.rawTranscript ??
      '',
  );
  const hasTranscript = $derived(Boolean(heard.trim() || polished.trim()));
  const how = $derived(
    record.verifiedTranscript ? 'verified' : record.correctedTranscript ? 'learned' : null,
  );
  const editDirty = $derived(editText.trim() !== '' && editText !== polished);
  const busy = $derived(disabled || pending !== null);
  const projectName = $derived(
    record.projectSelection?.project || record.personalizationProject || 'Shared',
  );
  const reviewTime = $derived.by(() => {
    const items = Array.isArray(record.reviewHistory) ? record.reviewHistory : [];
    const want = how === 'verified' ? 'verbatim_transcription' : 'cleanup_correction';
    for (let index = items.length - 1; index >= 0; index--) {
      const item = items[index] as { targetKind?: unknown; timestamp?: unknown };
      if (item && item.targetKind === want && typeof item.timestamp === 'string') {
        const time = new Date(item.timestamp);
        if (!Number.isNaN(time.getTime()))
          return time.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
      }
    }
    return null;
  });
  const speakerRows = $derived(record.speakerTimeline?.segments ?? []);
  const barHeights = $derived.by(() => {
    let seed = 0;
    for (const char of record.id) seed = (seed * 31 + char.charCodeAt(0)) % 997;
    return Array.from({ length: 36 }, (_, index) => {
      seed = (seed * 37 + index * 11 + 7) % 100;
      return 18 + ((seed * 7 + index * 13) % 82);
    });
  });

  function fmt(totalSeconds: number) {
    const total = Math.max(0, Math.floor(totalSeconds));
    return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`;
  }

  function fmtMs(ms: number) {
    return fmt(ms / 1000);
  }

  function showMessage(text: string) {
    message = text;
    if (messageTimer) clearTimeout(messageTimer);
    messageTimer = setTimeout(() => {
      message = null;
    }, 4000);
  }

  function fail(cause: unknown, prefix: string) {
    error = `${prefix} ${cause instanceof Error ? cause.message : String(cause)}`;
  }

  // Words heard but missing from the polished text, matched case-insensitively
  // as a multiset so repeated words align one-to-one.
  function heardParts(): { text: string; changed: boolean }[] {
    const counts: Record<string, number> = {};
    for (const word of polished
      .toLowerCase()
      .replace(/[^\w\s']/g, '')
      .split(/\s+/)) {
      if (word) counts[word] = (counts[word] ?? 0) + 1;
    }
    return heard.split(/(\s+)/).map((token) => {
      const key = token.toLowerCase().replace(/[^\w']/g, '');
      if (!key || /^\s+$/.test(token)) return { text: token, changed: false };
      const left = counts[key] ?? 0;
      if (left > 0) {
        counts[key] = left - 1;
        return { text: token, changed: false };
      }
      return { text: token, changed: true };
    });
  }

  async function ensureAudio(playAfterLoad: boolean): Promise<boolean> {
    if (audioMissing) return false;
    if (!audioUrl && !pending) {
      pending = 'audio';
      error = null;
      try {
        const url = await sessionAudioDataUrl(record.id);
        if (!url) {
          audioMissing = true;
          error = 'Audio unavailable for this recording.';
          return false;
        }
        audioUrl = url;
        await Promise.resolve();
      } catch (cause) {
        fail(cause, 'Could not load audio.');
        return false;
      } finally {
        pending = null;
      }
    }
    if (!audio || !audioUrl) return false;
    if (playAfterLoad && audio.src !== audioUrl) audio.src = audioUrl;
    return true;
  }

  async function togglePlay() {
    if (busy && !playing) return;
    if (playing) {
      audio?.pause();
      return;
    }
    if (!(await ensureAudio(true))) return;
    try {
      await audio?.play();
    } catch (cause) {
      fail(cause, 'Could not play audio.');
    }
  }

  function seekTo(fraction: number) {
    const target = Math.min(0.999, Math.max(0, fraction)) * (duration || 0);
    if (audio && audioUrl) {
      if (audio.src !== audioUrl) audio.src = audioUrl;
      audio.currentTime = target;
      position = target;
      if (audio.paused) void audio.play().catch(() => {});
    } else {
      position = target;
      void togglePlay();
    }
  }

  function seekFromPointer(event: MouseEvent | KeyboardEvent, node: HTMLElement) {
    const bounds = node.getBoundingClientRect();
    const at = position / (duration || 1);
    let fraction: number;
    if (event instanceof MouseEvent) fraction = (event.clientX - bounds.left) / bounds.width;
    else if (event.key === 'ArrowLeft') fraction = at - 0.05;
    else if (event.key === 'ArrowRight') fraction = at + 0.05;
    else if (event.key === 'Home') fraction = 0;
    else if (event.key === 'End') fraction = 1;
    else return;
    seekTo(fraction);
  }

  function cycleSpeed() {
    const order = [1, 1.5, 2, 0.75];
    speed = order[(order.indexOf(speed) + 1) % order.length];
    if (audio) audio.playbackRate = speed;
  }

  async function like() {
    if (busy || !polished.trim()) return;
    pending = 'like';
    error = null;
    try {
      const updated = await approveLike(record.id, polished);
      editing = false;
      onUpdate(updated);
      showMessage(
        updated.verifiedTranscript ? 'Saved as a verified label' : 'Saved as a reviewed correction',
      );
    } catch (cause) {
      fail(cause, 'Could not save your vote.');
    } finally {
      pending = null;
    }
  }

  async function dislike() {
    if (busy) return;
    pending = 'dislike';
    error = null;
    try {
      const updated = await setTranscriptFeedback(record.id, 'dislike');
      onUpdate(updated);
      editText = polished;
      editing = true;
    } catch (cause) {
      fail(cause, 'Could not save your vote.');
    } finally {
      pending = null;
    }
  }

  function cancelEdit() {
    editText = polished;
    editing = false;
    error = null;
  }

  async function saveCorrection() {
    if (busy || !editDirty) return;
    pending = 'save';
    error = null;
    try {
      const { record: updated, learningOutcome } = await approveCorrectionAndLearn(
        record.id,
        editText,
      );
      editing = false;
      editText = '';
      onUpdate(updated);
      showMessage(learningOutcome.status || 'Vesper learned from your edit');
    } catch (cause) {
      fail(cause, 'Could not save the correction.');
    } finally {
      pending = null;
    }
  }

  async function undoReview() {
    if (busy) return;
    pending = 'undo';
    error = null;
    try {
      const updated = await retractApproval(record.id);
      editText = '';
      editing = false;
      onUpdate(updated);
      showMessage('Review undone');
    } catch (cause) {
      fail(cause, 'Could not undo the review.');
    } finally {
      pending = null;
    }
  }

  async function rememberExample() {
    if (busy || remembered) return;
    pending = 'remember';
    error = null;
    try {
      await rememberCleanup(record.id);
      remembered = true;
      showMessage('Example remembered. Manage it in Settings → Your vocabulary and writing.');
    } catch (cause) {
      fail(cause, 'Could not remember this example.');
    } finally {
      pending = null;
    }
  }

  function copyPolished() {
    if (!polished) return;
    navigator.clipboard
      .writeText(polished)
      .then(() => {
        copied = true;
        if (copyTimer) clearTimeout(copyTimer);
        copyTimer = setTimeout(() => {
          copied = false;
        }, 1800);
      })
      .catch((cause: unknown) => fail(cause, 'Could not copy the transcript.'));
  }

  const shownDuration = $derived(duration || (record.speakerTimeline?.durationMs ?? 0) / 1000);
</script>

<article class="tc-card" aria-label="Recording transcript card">
  <audio
    bind:this={audio}
    src={audioUrl ?? undefined}
    preload="metadata"
    hidden
    ontimeupdate={() => {
      if (audio) position = audio.currentTime;
    }}
    onloadedmetadata={() => {
      if (audio) {
        duration = Number.isFinite(audio.duration) ? audio.duration : 0;
        audio.playbackRate = speed;
      }
    }}
    onplay={() => (playing = true)}
    onpause={() => (playing = false)}
    onended={() => {
      playing = false;
      position = 0;
    }}
  ></audio>
  <div class="tc-player">
    <button
      class="tc-play"
      class:playing
      type="button"
      disabled={busy && !playing}
      onclick={togglePlay}
      aria-label={playing ? 'Pause recording' : 'Play recording'}
    ></button>
    <div
      class="tc-wave"
      role="slider"
      tabindex="0"
      aria-label="Seek recording"
      aria-valuemin="0"
      aria-valuemax={Math.round(shownDuration)}
      aria-valuenow={Math.floor(position)}
      aria-valuetext={`${fmt(position)} of ${fmt(shownDuration)}`}
      onclick={(event) => seekFromPointer(event, event.currentTarget)}
      onkeydown={(event) => {
        if (['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) {
          event.preventDefault();
          seekFromPointer(event, event.currentTarget);
        }
      }}
    >
      {#each barHeights as height, index (index)}<i
          aria-hidden="true"
          style={`height: ${height}%`}
          class:on={shownDuration > 0 &&
            index < Math.round((position / shownDuration) * barHeights.length)}
        ></i>{/each}
    </div>
    <span class="tc-time">{fmt(position)} / {fmt(shownDuration)}</span>
    <button
      class="tc-speed"
      type="button"
      onclick={cycleSpeed}
      disabled={busy}
      aria-label="Playback speed"
      title="Playback speed">{speed}×</button
    >
  </div>

  {#if !hasTranscript}
    <p class="note">No transcript was produced for this recording.</p>
  {:else}
    <div class="section-heading transcript-heading">
      <h3>Transcript</h3>
      <button
        class="quiet transcript-copy"
        class:copy-confirmed={copied}
        type="button"
        onclick={copyPolished}
        aria-label={copied ? 'Transcript copied' : 'Copy transcript'}
        title="Copy transcript"><CopyGlyph {copied} />{copied ? 'Copied' : 'Copy text'}</button
      >
    </div>
    <div class="tc-cols">
      <div class="tc-col">
        <span class="tc-label">Heard</span>
        <!-- Marks are informational; seeking stays on the slider above. -->
        <p class="tc-heard">
          {#each heardParts() as part, index (index)}{#if part.changed}<mark>{part.text}</mark
              >{:else}{part.text}{/if}{/each}
        </p>
      </div>
      <div class="tc-col">
        <label class="tc-label" for={`tc-polished-${record.id}`}>Polished · edit to teach</label>
        {#if editing}
          <textarea
            id={`tc-polished-${record.id}`}
            class="tc-polished"
            rows="4"
            spellcheck="false"
            value={editText}
            oninput={(event) => {
              editText = event.currentTarget.value;
            }}
            disabled={busy}></textarea>
        {:else}
          <p class="tc-polished-text">{polished}</p>
        {/if}
      </div>
    </div>
    <div class="tc-foot">
      {#if editing}
        <span class="hint">Fix the wording, then save. Vesper learns from the change.</span>
        <div class="tc-actions">
          <button class="quiet" type="button" onclick={cancelEdit} disabled={busy}>Cancel</button>
          <button
            class="primary"
            type="button"
            onclick={saveCorrection}
            disabled={busy || !editDirty}
          >
            {pending === 'save' ? 'Saving…' : 'Save correction'}
          </button>
        </div>
      {:else if how}
        <div class="tc-state">
          <span class="tc-state-pill"
            ><Icon name="check" />{how === 'verified'
              ? 'Verified label'
              : 'Learned from your edit'}{#if reviewTime}
              · {reviewTime}{/if}</span
          >
          <button class="tc-undo" type="button" onclick={undoReview} disabled={busy}>Undo</button>
        </div>
      {:else}
        <div class="tc-vote">
          <div class="tc-vote-buttons">
            <button
              class="tc-vote-button"
              class:active={record.transcriptFeedback === 'like'}
              type="button"
              onclick={like}
              disabled={busy}
              aria-label="Looks right"
              aria-pressed={record.transcriptFeedback === 'like'}
              title="Looks right"><Icon name="like" /></button
            >
            <button
              class="tc-vote-button tc-dislike"
              class:active={record.transcriptFeedback === 'dislike'}
              type="button"
              onclick={dislike}
              disabled={busy}
              aria-label="Needs a fix"
              aria-pressed={record.transcriptFeedback === 'dislike'}
              title="Needs a fix"><Icon name="dislike" /></button
            >
          </div>
          <span class="tc-question">Is the polished text right?</span>
        </div>
      {/if}
    </div>
  {/if}

  <button
    class="tc-details-toggle"
    type="button"
    onclick={() => (detailsOpen = !detailsOpen)}
    aria-expanded={detailsOpen}
    aria-controls={`tc-details-${record.id}`}
    ><span>Details</span><Icon name="chevron" active={detailsOpen} /></button
  >
  {#if detailsOpen}
    <div class="tc-details" id={`tc-details-${record.id}`}>
      <dl class="tc-facts">
        <div>
          <dt>Original file</dt>
          <dd>{record.originalFilename}</dd>
        </div>
        <div>
          <dt>File size</dt>
          <dd>
            {(record.audioBytes / 1024 ** 2).toLocaleString(undefined, {
              maximumFractionDigits: 2,
            })} MiB
          </dd>
        </div>
        <div>
          <dt>Archived file</dt>
          <dd>{record.audioRelativePath}</dd>
        </div>
        <div>
          <dt>Project</dt>
          <dd>{projectName}</dd>
        </div>
      </dl>
      {#if speakerRows.length}
        <p class="note">
          Speaker numbers apply to this recording only. Overlapping ranges mean simultaneous speech.
        </p>
        <div class="tc-speakers" role="region" aria-label="Estimated speaker activity">
          <table>
            <thead
              ><tr
                ><th scope="col">Speaker</th><th scope="col">Start</th><th scope="col">End</th></tr
              ></thead
            >
            <tbody>
              {#each speakerRows as segment, index (index)}<tr>
                  <td>Speaker {segment.speaker}</td>
                  <td>{fmtMs(segment.startMs)}</td>
                  <td>{fmtMs(segment.endMs)}</td>
                </tr>{/each}
            </tbody>
          </table>
        </div>
      {/if}
      {#if record.correctedTranscript && !remembered}
        <button class="quiet" type="button" onclick={rememberExample} disabled={busy}>
          {pending === 'remember' ? 'Remembering…' : 'Remember as writing example'}
        </button>
      {/if}
    </div>
  {/if}

  <div aria-live="polite">
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if message}<p class="success">{message}</p>{/if}
  </div>
</article>

<style>
  .tc-card {
    background: var(--color-raised);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-md);
    padding: var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    min-width: 0;
  }
  /* The card lives in a grid row: let every section shrink instead of
     forcing the whole recording list wider than the viewport. */
  .tc-card > * {
    min-width: 0;
  }
  .tc-player {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .tc-play {
    flex: none;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    border: 1px solid var(--color-control-edge);
    background: var(--color-tint);
    cursor: pointer;
    position: relative;
  }
  .tc-play::before {
    content: '';
    position: absolute;
    left: 17px;
    top: 13px;
    border-left: 12px solid var(--color-ink);
    border-top: 9px solid transparent;
    border-bottom: 9px solid transparent;
  }
  .tc-play.playing::before {
    content: '';
    left: 15px;
    top: 13px;
    width: 5px;
    height: 18px;
    border: none;
    background: var(--color-ink);
    box-shadow: 9px 0 0 var(--color-ink);
  }
  .tc-play:hover:enabled {
    border-color: var(--color-accent);
  }
  .tc-play:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }
  .tc-wave {
    flex: 1 1 0;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 2px;
    height: 44px;
    cursor: pointer;
  }
  .tc-wave i {
    flex: 1 1 0;
    min-width: 0;
    background: var(--color-control-edge);
    border-radius: 1px;
  }
  .tc-wave i.on {
    background: var(--color-accent-light);
  }
  .tc-wave:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }
  .tc-time {
    flex: none;
    font-size: 12px;
    color: var(--color-muted);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .tc-speed {
    flex: none;
    min-width: 44px;
    min-height: 44px;
    background: none;
    border: none;
    color: var(--color-muted);
    font-size: 13px;
    cursor: pointer;
    border-radius: var(--radius-sm);
  }
  .tc-speed:hover:enabled {
    color: var(--color-ink);
  }
  .tc-cols {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-3);
  }
  @media (max-width: 560px) {
    .tc-cols {
      grid-template-columns: 1fr;
    }
  }
  .tc-label {
    display: block;
    font-size: 12px;
    font-weight: 600;
    color: var(--color-muted);
    margin-bottom: var(--space-2);
  }
  .tc-heard {
    margin: 0;
    font-size: 14px;
    line-height: 1.6;
    background: var(--color-panel);
    border: 1px solid var(--color-line);
    border-radius: var(--radius-sm);
    padding: var(--space-3);
  }
  .tc-heard mark {
    background: color-mix(in srgb, var(--color-warning) 28%, transparent);
    color: var(--color-ink);
    border-radius: 3px;
    padding-inline: 1px;
  }
  .tc-heard {
    cursor: text;
  }
  .tc-polished-text {
    margin: 0;
    font-size: 14px;
    line-height: 1.6;
    padding: var(--space-3);
    border: 1px solid transparent;
  }
  .tc-polished {
    width: 100%;
    box-sizing: border-box;
  }
  .tc-foot {
    border-top: 1px solid var(--color-line);
    padding-top: var(--space-3);
  }
  .tc-vote {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .tc-vote-buttons {
    display: flex;
    gap: var(--space-2);
  }
  .tc-vote-button {
    width: 44px;
    height: 44px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    border: 1px solid var(--color-control-edge);
    background: transparent;
    color: var(--color-muted);
    cursor: pointer;
  }
  .tc-vote-button :global(.icon) {
    width: 20px;
    height: 20px;
  }
  .tc-vote-button:hover:enabled {
    color: var(--color-ink);
    border-color: var(--color-control-hover);
  }
  .tc-vote-button:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }
  .tc-vote-button:active:enabled {
    transform: scale(0.96);
  }
  .tc-vote-button.active {
    color: var(--color-accent-light);
    border-color: var(--color-accent);
    background: var(--color-tint);
  }
  .tc-vote-button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .tc-question {
    font-size: 13px;
    color: var(--color-muted);
  }
  .tc-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
  .hint {
    font-size: 13px;
    color: var(--color-muted);
  }
  .tc-state {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2) var(--space-3);
  }
  .tc-state-pill {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font-size: 13px;
    font-weight: 600;
    color: var(--color-accent-light);
  }
  .tc-state-pill :global(.icon) {
    width: 16px;
    height: 16px;
  }
  .tc-undo {
    background: none;
    border: none;
    padding: var(--space-2);
    min-height: 44px;
    color: var(--color-muted);
    font-size: 13px;
    text-decoration: underline;
    text-underline-offset: 3px;
    cursor: pointer;
  }
  .tc-undo:hover:enabled {
    color: var(--color-ink);
  }
  .tc-details-toggle {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    align-self: flex-start;
    background: none;
    border: none;
    color: var(--color-muted);
    font-size: 13px;
    min-height: 44px;
    cursor: pointer;
    padding: var(--space-2) 0;
  }
  .tc-details-toggle:hover:enabled {
    color: var(--color-ink);
  }
  .tc-details {
    border-top: 1px solid var(--color-line);
    padding-top: var(--space-3);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .tc-facts {
    margin: 0;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: var(--space-2) var(--space-4);
  }
  .tc-facts dt {
    font-size: 12px;
    color: var(--color-muted);
  }
  .tc-facts dd {
    margin: 0;
    font-size: 13px;
    overflow-wrap: anywhere;
  }
  .tc-speakers {
    overflow-x: auto;
  }
  .tc-speakers table {
    border-collapse: collapse;
    font-size: 13px;
  }
  .tc-speakers th,
  .tc-speakers td {
    text-align: left;
    padding: var(--space-1) var(--space-3) var(--space-1) 0;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .tc-speakers th {
    font-weight: 600;
    color: var(--color-muted);
  }
  @media (prefers-reduced-motion: reduce) {
    .tc-vote-button:active:enabled {
      transform: none;
    }
  }
</style>
