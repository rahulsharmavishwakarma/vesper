<script lang="ts">
  import CopyGlyph from './CopyGlyph.svelte';
  import {
    approveCorrectionAndLearn,
    approveLike,
    retractApproval,
    sessionAudioDataUrl,
    setTranscriptFeedback,
    type SessionRecord,
  } from '../api/dataset';
  import { onDestroy, tick } from 'svelte';

  let {
    record,
    disabled = false,
    onUpdate,
  }: {
    record: SessionRecord;
    disabled?: boolean;
    onUpdate: (updated: SessionRecord) => void;
  } = $props();

  // Thumb, play, pause and check glyphs follow the approved history mock.
  const THUMB =
    'M7 11v9H4v-9zM7 11l4-8c1.5 0 2.5 1 2.5 2.5V9H19a2 2 0 0 1 2 2.3l-1.2 7A2 2 0 0 1 17.8 20H7';
  const PLAY = 'M8 5v14l11-7z';
  const PAUSE = 'M8 5v14M16 5v14';
  const CHECK = 'M5 12.5l4.5 4.5L19 7';

  let editing = $state(false);
  let editText = $state('');
  let detailsOpen = $state(false);
  let pending = $state<'like' | 'dislike' | 'save' | 'undo' | 'audio' | null>(null);
  let message = $state<string | null>(null);
  let error = $state<string | null>(null);
  let audioUrl = $state<string | null>(null);
  let audioMissing = $state(false);
  let playing = $state(false);
  let position = $state(0);
  let duration = $state(0);
  let speed = $state(1);
  let audio: HTMLAudioElement | undefined = $state();
  let editor: HTMLTextAreaElement | undefined = $state();
  let messageTimer: ReturnType<typeof setTimeout> | undefined;

  onDestroy(() => clearTimeout(messageTimer));

  const heard = $derived(record.rawTranscript ?? '');
  const polished = $derived(
    record.correctedTranscript ??
      record.personalizedTranscript ??
      record.cleanedTranscript ??
      record.rawTranscript ??
      '',
  );
  const hasTranscript = $derived(Boolean(heard.trim() || polished.trim()));
  const approved = $derived(record.reviewStatus === 'approved');
  const editDirty = $derived(editText.trim() !== '' && editText !== polished);
  const busy = $derived(disabled || pending !== null);
  const projectName = $derived(
    record.projectSelection?.project || record.personalizationProject || 'Shared',
  );
  const speakerRows = $derived(record.speakerTimeline?.segments ?? []);
  const shownDuration = $derived(duration || (record.speakerTimeline?.durationMs ?? 0) / 1000);
  // Waveform bars follow the mock generator so the shape matches the design.
  const barHeights = $derived.by(() => {
    const dur = shownDuration || 5;
    const count = Math.min(72, Math.max(30, Math.round(dur * 2) + 24));
    let seed = 0;
    for (const char of record.id) seed = (seed * 31 + char.charCodeAt(0)) % 233280;
    seed = (seed * 9301 + 49297) % 233280;
    const heights: number[] = [];
    for (let index = 0; index < count; index++) {
      seed = (seed * 9301 + 49297) % 233280;
      const random = seed / 233280;
      heights.push(Math.round(16 + 74 * Math.abs(Math.sin(index * 0.37) * 0.55 + random * 0.45)));
    }
    return heights;
  });

  // Keep the editor in sync with the saved text whenever it is not being edited.
  $effect(() => {
    if (!editing) {
      editText = polished;
      if (editor) fitEditor();
    }
  });

  function fmt(totalSeconds: number) {
    const total = Math.max(0, Math.floor(totalSeconds));
    return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`;
  }

  function fmtMs(ms: number) {
    return fmt(ms / 1000);
  }

  function showToast(text: string, isError: boolean) {
    if (isError) error = text;
    else message = text;
    if (messageTimer) clearTimeout(messageTimer);
    messageTimer = setTimeout(
      () => {
        message = null;
        error = null;
      },
      isError ? 4000 : 1900,
    );
  }

  function fail(cause: unknown, prefix: string) {
    showToast(`${prefix} ${cause instanceof Error ? cause.message : String(cause)}`, true);
  }

  function fitEditor() {
    if (!editor) return;
    editor.style.height = 'auto';
    editor.style.height = `${Math.max(140, editor.scrollHeight + 2)}px`;
  }

  // Words heard but missing from the polished text, matched case-insensitively
  // as a multiset so repeated words align one-to-one, exactly like the mock.
  function heardParts(): { text: string; changed: boolean; index: number }[] {
    const counts: Record<string, number> = {};
    for (const word of polished
      .toLowerCase()
      .replace(/[^\w\s']/g, '')
      .split(/\s+/)) {
      if (word) counts[word] = (counts[word] ?? 0) + 1;
    }
    let wordIndex = 0;
    return heard.split(/(\s+)/).map((token) => {
      const key = token.toLowerCase().replace(/[^\w']/g, '');
      if (!key || /^\s+$/.test(token)) return { text: token, changed: false, index: -1 };
      const left = counts[key] ?? 0;
      const index = wordIndex++;
      if (left > 0) {
        counts[key] = left - 1;
        return { text: token, changed: false, index };
      }
      return { text: token, changed: true, index };
    });
  }

  const wordCount = $derived(heardParts().filter((part) => part.index >= 0).length || 1);
  const saidThrough = $derived(
    shownDuration > 0 ? Math.floor((position / shownDuration) * wordCount) : 0,
  );

  async function ensureAudio(): Promise<boolean> {
    if (audioMissing) return false;
    if (!audioUrl && !pending) {
      pending = 'audio';
      try {
        const url = await sessionAudioDataUrl(record.id);
        if (!url) {
          audioMissing = true;
          showToast('Audio unavailable for this recording.', true);
          return false;
        }
        audioUrl = url;
        await tick();
      } catch (cause) {
        fail(cause, 'Could not load audio.');
        return false;
      } finally {
        pending = null;
      }
    }
    if (!audio || !audioUrl) return false;
    if (audio.src !== audioUrl) audio.src = audioUrl;
    return true;
  }

  function updateProgress() {
    if (audio) position = audio.currentTime;
  }

  async function togglePlay() {
    if (playing) {
      audio?.pause();
      return;
    }
    if (busy) return;
    if (!(await ensureAudio())) return;
    if (position >= shownDuration - 0.05) {
      position = 0;
      if (audio) audio.currentTime = 0;
    }
    try {
      await audio?.play();
    } catch (cause) {
      fail(cause, 'Could not play audio.');
    }
  }

  function seekTo(fraction: number) {
    const target = Math.min(0.999, Math.max(0, fraction)) * (duration || shownDuration);
    position = target;
    if (audio && audioUrl) {
      if (audio.src !== audioUrl) audio.src = audioUrl;
      audio.currentTime = target;
      if (audio.paused) void audio.play().catch(() => {});
    } else {
      void togglePlay();
    }
  }

  function seekWord(index: number) {
    seekTo(index / wordCount);
  }

  function sliderPointer(event: PointerEvent, node: HTMLElement) {
    const move = (moveEvent: PointerEvent) => {
      const bounds = node.getBoundingClientRect();
      seekTo((moveEvent.clientX - bounds.left) / bounds.width);
    };
    move(event);
    node.setPointerCapture(event.pointerId);
    const up = () => {
      node.removeEventListener('pointermove', move);
      node.removeEventListener('pointerup', up);
    };
    node.addEventListener('pointermove', move);
    node.addEventListener('pointerup', up);
  }

  function sliderKey(event: KeyboardEvent) {
    const step = 5;
    if (event.key === 'ArrowRight') seekTo((position + step) / (duration || shownDuration || 1));
    else if (event.key === 'ArrowLeft')
      seekTo((position - step) / (duration || shownDuration || 1));
    else if (event.key === 'Home') seekTo(0);
    else if (event.key === 'End') seekTo(1);
    else if (event.key === ' ' || event.key === 'Enter') void togglePlay();
    else return;
    event.preventDefault();
  }

  function cycleSpeed() {
    const order = [1, 1.5, 2, 0.75];
    speed = order[(order.indexOf(speed) + 1) % order.length];
    if (audio) audio.playbackRate = speed;
  }

  async function like() {
    if (busy || !polished.trim()) return;
    pending = 'like';
    try {
      const updated = await approveLike(record.id, polished);
      editing = false;
      onUpdate(updated);
      showToast('Saved as a verified label', false);
    } catch (cause) {
      fail(cause, 'Could not save your vote.');
    } finally {
      pending = null;
    }
  }

  async function dislike() {
    if (busy) return;
    pending = 'dislike';
    try {
      const updated = await setTranscriptFeedback(record.id, 'dislike');
      onUpdate(updated);
      editText = polished;
      editing = true;
      await tick();
      fitEditor();
      editor?.focus();
      editor?.setSelectionRange(editor.value.length, editor.value.length);
    } catch (cause) {
      fail(cause, 'Could not save your vote.');
    } finally {
      pending = null;
    }
  }

  function enterEdit() {
    if (!editing) editing = true;
  }

  function cancelEdit() {
    editText = polished;
    editing = false;
  }

  async function saveCorrection() {
    if (busy || !editDirty) return;
    pending = 'save';
    try {
      const { record: updated, learningOutcome } = await approveCorrectionAndLearn(
        record.id,
        editText,
      );
      editing = false;
      onUpdate(updated);
      showToast(learningOutcome.status || 'Vesper learned from your edit', false);
    } catch (cause) {
      fail(cause, 'Could not save the correction.');
    } finally {
      pending = null;
    }
  }

  async function undoReview() {
    if (busy) return;
    pending = 'undo';
    try {
      const updated = await retractApproval(record.id);
      editing = false;
      onUpdate(updated);
      showToast('Review undone', false);
    } catch (cause) {
      fail(cause, 'Could not undo the review.');
    } finally {
      pending = null;
    }
  }

  // The approved state mirrors the mock: a like reads as a verified label,
  // while only an edited correction reads as learned from the edit. The
  // backend still routes verbatim and cleanup labels to their honest
  // training tasks regardless of this wording.
  function stateLabel(): string | null {
    if (!approved) return null;
    if (record.verifiedTranscript) return 'Verified label';
    const history = Array.isArray(record.reviewHistory) ? record.reviewHistory : [];
    const last = history[history.length - 1] as { targetKind?: unknown } | undefined;
    if (last?.targetKind === 'verbatim_transcription') return 'Verified label';
    if (record.transcriptFeedback === 'like') return 'Verified label';
    if (record.correctedTranscript) return 'Learned from your edit';
    return 'Verified label';
  }

  function copyText() {
    const text = editing ? editText : polished;
    if (!text) return;
    navigator.clipboard
      .writeText(text)
      .then(() => showToast('Copied', false))
      .catch((cause: unknown) => fail(cause, 'Could not copy the transcript.'));
  }

  const label = $derived(stateLabel());
</script>

<div class="xp" role="article" aria-label="Recording transcript card">
  <audio
    bind:this={audio}
    src={audioUrl ?? undefined}
    preload="metadata"
    hidden
    ontimeupdate={updateProgress}
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
  <div class="pl" class:on={playing}>
    <button
      class="pp"
      type="button"
      disabled={busy && !playing}
      onclick={togglePlay}
      aria-label={playing ? 'Pause recording' : 'Play recording'}
    >
      {#if playing}<svg viewBox="0 0 24 24" aria-hidden="true"
          ><path d={PAUSE} stroke-width="3.2" stroke-linecap="round" /></svg
        >{:else}<svg viewBox="0 0 24 24" aria-hidden="true"><path d={PLAY} /></svg>{/if}
    </button>
    <div
      class="wf"
      role="slider"
      tabindex="0"
      aria-label="Seek recording"
      aria-valuemin="0"
      aria-valuemax={Math.round(shownDuration)}
      aria-valuenow={Math.floor(position)}
      aria-valuetext={`${fmt(position)} of ${fmt(shownDuration)}`}
      onpointerdown={(event) => sliderPointer(event, event.currentTarget)}
      onkeydown={sliderKey}
    >
      {#each barHeights as height, index (index)}<i
          aria-hidden="true"
          style={`height: ${height}%`}
          class:on={shownDuration > 0 &&
            index < Math.round((position / shownDuration) * barHeights.length)}
        ></i>{/each}
    </div>
    <span class="tmn">{fmt(position)} / {fmt(shownDuration)}</span>
    <button
      class="sx"
      type="button"
      onclick={cycleSpeed}
      disabled={busy}
      aria-label="Playback speed"
      title="Playback speed">{speed}×</button
    >
  </div>

  {#if !hasTranscript}
    <p class="empty">No transcript was produced for this recording.</p>
  {:else}
    <div class="th">
      <b>Transcript</b>
      <button
        class="gh"
        type="button"
        onclick={copyText}
        aria-label="Copy transcript"
        title="Copy transcript"
      >
        <CopyGlyph copied={false} />Copy text</button
      >
    </div>
    <div class="cols">
      <div class="col">
        <label>Heard</label>
        <!-- Word tap seeks like the mock; the slider stays the keyboard path. -->
        <p class="raw">
          {#each heardParts() as part, index (index)}{#if part.changed}<span
                class="w"
                data-i={part.index}
                class:said={part.index < saidThrough}
                class:now={playing && part.index === saidThrough}
                onclick={() => seekWord(part.index)}><mark>{part.text}</mark></span
              >{:else if part.index >= 0}<span
                class="w"
                data-i={part.index}
                class:said={part.index < saidThrough}
                class:now={playing && part.index === saidThrough}
                onclick={() => seekWord(part.index)}>{part.text}</span
              >{:else}{part.text}{/if}{/each}
        </p>
      </div>
      <div class="col">
        <label for={`p${record.id}`}>Polished · edit to teach</label>
        <textarea
          id={`p${record.id}`}
          class="pol"
          class:editing
          bind:this={editor}
          spellcheck="false"
          value={editText}
          oninput={(event) => {
            editText = event.currentTarget.value;
            enterEdit();
            fitEditor();
          }}
          onfocus={enterEdit}
          onkeydown={(event) => {
            if (event.key === 'Escape') cancelEdit();
          }}
          disabled={busy}></textarea>
      </div>
    </div>
    <div class="tf">
      {#if editing}
        <span class="hint">Fix the wording, then save. Vesper learns from the change.</span>
        <div class="acts">
          <button class="btn" type="button" onclick={cancelEdit} disabled={busy}>Cancel</button>
          <button
            class="btn p"
            type="button"
            onclick={saveCorrection}
            disabled={busy || !editDirty}
          >
            {pending === 'save' ? 'Saving…' : 'Save correction'}
          </button>
        </div>
      {:else if label}
        <div class="lft">
          <span class="state"
            ><svg viewBox="0 0 24 24" aria-hidden="true"><path d={CHECK} /></svg>{label}</span
          ><button class="ln" type="button" onclick={undoReview} disabled={busy}>Undo</button>
        </div>
      {:else}
        <div class="lft">
          <div class="vt">
            <button
              class="vb"
              type="button"
              onclick={like}
              disabled={busy}
              aria-label="Looks right"
              aria-pressed={record.transcriptFeedback === 'like'}
              title="Looks right"
              ><svg viewBox="0 0 24 24" aria-hidden="true"><path d={THUMB} /></svg></button
            >
            <button
              class="vb dn"
              type="button"
              onclick={dislike}
              disabled={busy}
              aria-label="Needs a fix"
              aria-pressed={record.transcriptFeedback === 'dislike'}
              title="Needs a fix"
              ><svg viewBox="0 0 24 24" aria-hidden="true"><path d={THUMB} /></svg></button
            >
          </div>
          <span class="q">Is the polished text right?</span>
        </div>
      {/if}
    </div>
  {/if}

  {#if record.audioRelativePath}
    <button
      class="dt"
      type="button"
      onclick={() => (detailsOpen = !detailsOpen)}
      aria-expanded={detailsOpen}
      aria-controls={`tc-details-${record.id}`}
      >Details<svg class="i" viewBox="0 0 24 24" aria-hidden="true"><path d="M6 9l6 6 6-6" /></svg
      ></button
    >
    {#if detailsOpen}
      <div class="dd" id={`tc-details-${record.id}`}>
        <div class="dl">
          <div><small>Original file</small><span>{record.originalFilename}</span></div>
          <div>
            <small>File size</small><span>
              {(record.audioBytes / 1024 ** 2).toLocaleString(undefined, {
                maximumFractionDigits: 2,
              })} MiB
            </span>
          </div>
          <div><small>Archived file</small><span>{record.audioRelativePath}</span></div>
          <div><small>Project</small><span>{projectName}</span></div>
        </div>
        {#if speakerRows.length}
          <div>
            <p class="nt">
              Speaker numbers apply to this recording only. Overlapping ranges mean simultaneous
              speech.
            </p>
            <div class="spk" tabindex="0" aria-label="Estimated speaker activity">
              <table>
                <thead
                  ><tr
                    ><th scope="col">Speaker</th><th scope="col">Start</th><th scope="col">End</th
                    ></tr
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
          </div>
        {/if}
      </div>
    {/if}
  {/if}
  {#if message || error}<div class="toast show" role="status">{error ?? message}</div>{/if}
</div>

<style>
  .xp {
    padding: 4px 4px 24px;
    min-width: 0;
  }
  /* The expanded area lives in a grid row: sections shrink instead of
     forcing the recording list wider than the viewport. */
  .xp > * {
    min-width: 0;
  }
  .pl {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 8px 18px 8px 8px;
    border: 1px solid var(--color-line);
    border-radius: 999px;
    margin-bottom: 22px;
    transition: border-color 0.3s;
  }
  .pl.on {
    border-color: var(--color-accent);
  }
  .pp {
    width: 42px;
    height: 42px;
    border-radius: 50%;
    border: 0;
    background: var(--color-accent);
    color: var(--color-on-accent);
    display: grid;
    place-items: center;
    flex: none;
    cursor: pointer;
    transition: transform 0.25s var(--ease-spring);
  }
  .pp:hover:enabled {
    transform: scale(1.07);
  }
  .pp:active:enabled {
    transform: scale(0.94);
  }
  .pp:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }
  .pp svg {
    width: 18px;
    height: 18px;
    fill: currentColor;
    stroke: currentColor;
    stroke-width: 1.5;
    stroke-linejoin: round;
  }
  .wf {
    flex: 1 1 0;
    min-width: 0;
    height: 38px;
    display: flex;
    align-items: center;
    gap: 2px;
    cursor: pointer;
    touch-action: none;
    border-radius: 8px;
  }
  .wf i {
    flex: 1 1 0;
    min-width: 0;
    max-width: 5px;
    border-radius: 2px;
    background: var(--color-muted);
    opacity: 0.4;
    transition:
      opacity 0.15s,
      background 0.15s;
  }
  .wf i.on {
    background: var(--color-accent);
    opacity: 1;
  }
  .wf:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }
  .tmn {
    font-size: 13.5px;
    color: var(--color-muted);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    flex: none;
  }
  .sx {
    border: 1px solid var(--color-line);
    background: transparent;
    border-radius: 999px;
    padding: 3px 10px;
    font-size: 13px;
    color: var(--color-muted);
    min-width: 46px;
    flex: none;
    cursor: pointer;
  }
  .sx:hover:enabled {
    color: var(--color-ink);
    border-color: var(--color-control-hover);
  }
  .th {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 14px;
  }
  .th b {
    font-weight: 800;
    font-size: 20px;
    letter-spacing: -0.03em;
  }
  .gh {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    border: 0;
    background: transparent;
    color: var(--color-muted);
    font-size: 14px;
    padding: 6px 8px;
    border-radius: 8px;
    cursor: pointer;
  }
  .gh:hover:enabled {
    color: var(--color-ink);
    background: var(--color-tint);
  }
  .cols {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
  }
  @media (max-width: 560px) {
    .cols {
      grid-template-columns: 1fr;
    }
  }
  .col > label {
    display: block;
    font-size: 12.5px;
    color: var(--color-muted);
    margin-bottom: 8px;
    letter-spacing: 0.02em;
  }
  .col .raw,
  .col textarea {
    width: 100%;
    box-sizing: border-box;
    margin: 0;
    padding: 16px 18px;
    border-radius: 14px;
    font-size: 15.5px;
    line-height: 1.65;
    min-height: 140px;
  }
  .col .raw {
    border: 1px solid var(--color-line);
    color: var(--color-muted);
    white-space: pre-wrap;
  }
  .col .raw mark {
    background: transparent;
    color: var(--color-muted);
    text-decoration: line-through;
    text-decoration-color: var(--color-accent);
    text-decoration-thickness: 1.5px;
  }
  .col textarea {
    display: block;
    resize: none;
    overflow: hidden;
    background: var(--color-raised);
    border: 1px solid var(--color-line);
    color: var(--color-ink);
    outline: none;
    transition:
      border-color 0.2s,
      box-shadow 0.2s;
  }
  .col textarea:hover:enabled {
    border-color: var(--color-muted);
  }
  .col textarea:focus,
  .col textarea.editing {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 3px var(--color-tint);
  }
  .raw .w {
    border-radius: 4px;
    cursor: pointer;
    transition:
      color 0.15s,
      background 0.15s;
  }
  .raw .w:hover {
    background: var(--color-tint);
  }
  .raw .w.said {
    color: var(--color-ink);
  }
  .raw .w.now {
    background: var(--color-tint);
    color: var(--color-accent);
  }
  .tf {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    flex-wrap: wrap;
    margin-top: 16px;
    min-height: 44px;
  }
  .vt {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px;
    border: 1px solid var(--color-line);
    border-radius: 999px;
  }
  .vb {
    width: 38px;
    height: 38px;
    border-radius: 50%;
    border: 0;
    background: transparent;
    color: var(--color-muted);
    display: grid;
    place-items: center;
    cursor: pointer;
    transition:
      background 0.2s,
      color 0.2s,
      transform 0.3s var(--ease-spring);
  }
  .vb:hover:enabled {
    background: var(--color-tint);
    color: var(--color-accent);
    transform: translateY(-2px);
  }
  .vb:active:enabled {
    transform: scale(0.9);
  }
  .vb:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }
  .vb svg {
    width: 18px;
    height: 18px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .vb.dn svg {
    transform: rotate(180deg);
  }
  .vb:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .q {
    font-size: 14px;
    color: var(--color-muted);
  }
  .lft {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
  }
  .state {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 7px 14px;
    border-radius: 999px;
    background: var(--color-tint);
    color: var(--color-accent);
    font-size: 14px;
    font-weight: 500;
  }
  .state svg {
    width: 16px;
    height: 16px;
    fill: none;
    stroke: currentColor;
    stroke-width: 2.2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .ln {
    border: 0;
    background: transparent;
    color: var(--color-muted);
    font-size: 14px;
    text-decoration: underline;
    text-underline-offset: 3px;
    padding: 6px;
    cursor: pointer;
  }
  .ln:hover:enabled {
    color: var(--color-ink);
  }
  .hint {
    font-size: 14px;
    color: var(--color-muted);
    flex: 1;
    min-width: 200px;
  }
  .btn {
    height: 40px;
    padding: 0 18px;
    border-radius: 10px;
    border: 1px solid var(--color-line);
    background: transparent;
    color: var(--color-ink);
    font-size: 14.5px;
    cursor: pointer;
    transition:
      transform 0.25s var(--ease-spring),
      border-color 0.2s;
  }
  .btn:hover:enabled {
    border-color: var(--color-muted);
  }
  .btn.p {
    background: var(--color-accent);
    border-color: var(--color-accent);
    color: var(--color-on-accent);
    font-weight: 500;
  }
  .btn.p:hover:enabled {
    transform: translateY(-1px);
  }
  .btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .acts {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .dt {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 18px;
    border: 0;
    background: transparent;
    color: var(--color-muted);
    font-size: 14px;
    padding: 6px 0;
    cursor: pointer;
  }
  .dt:hover:enabled {
    color: var(--color-ink);
  }
  .dt svg {
    width: 18px;
    height: 18px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
    transition: transform 0.4s var(--ease-spring);
  }
  .dt[aria-expanded='true'] svg {
    transform: rotate(180deg);
  }
  .dd {
    margin-top: 10px;
    display: grid;
    gap: 18px;
  }
  .dl {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 14px 24px;
    font-size: 14px;
  }
  .dl small {
    display: block;
    color: var(--color-muted);
    font-size: 12.5px;
    margin-bottom: 2px;
  }
  .dl span {
    overflow-wrap: anywhere;
  }
  .spk {
    max-height: 168px;
    overflow-y: auto;
    border: 1px solid var(--color-line);
    border-radius: 12px;
  }
  .spk table {
    width: 100%;
    border-collapse: collapse;
    font-size: 14px;
    font-variant-numeric: tabular-nums;
  }
  .spk th {
    position: sticky;
    top: 0;
    background: var(--color-bg);
    text-align: left;
    font-weight: 500;
    color: var(--color-muted);
    padding: 10px 16px;
    border-bottom: 1px solid var(--color-line);
  }
  .spk td {
    padding: 9px 16px;
    border-top: 1px solid var(--color-line);
  }
  .spk tbody tr:first-child td {
    border-top: 0;
  }
  .spk:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }
  .nt {
    margin: 0 0 6px;
    color: var(--color-muted);
    font-size: 13.5px;
  }
  .empty {
    color: var(--color-muted);
    margin: 6px 0 0;
    font-size: 15.5px;
  }
  .toast {
    position: fixed;
    left: 50%;
    bottom: 24px;
    transform: translate(-50%, 16px);
    background: var(--color-raised);
    color: var(--color-ink);
    border: 1px solid var(--color-line);
    padding: 10px 18px;
    border-radius: 999px;
    font-size: 14px;
    z-index: 20;
    box-shadow: 0 10px 30px rgb(0 0 0 / 25%);
    max-width: min(90vw, 560px);
  }
  .toast.show {
    transform: translate(-50%, 0);
  }
  @media (prefers-reduced-motion: reduce) {
    *,
    *::before,
    *::after {
      animation: none !important;
      transition: none !important;
    }
  }
</style>
