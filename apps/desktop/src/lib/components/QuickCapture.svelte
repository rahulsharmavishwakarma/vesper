<script lang="ts">
  import CaptureGlyph from './CaptureGlyph.svelte';
  import CopyGlyph from './CopyGlyph.svelte';
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { invoke } from '@tauri-apps/api/core';
  import Icon from './Icon.svelte';
  import AudioIndicator from './AudioIndicator.svelte';
  import { liveTranscriptFromStatus } from '../liveTranscript';
  import CaptureLayout from './CaptureLayout.svelte';
  import { getSettings } from '../api/desktop';
  import { CAPTURE_DESIGNS, type CaptureDesign } from '../config';
  import { getRuntimeStatus, type RuntimeStatus, type StreamingTranscript } from '../api/runtime';
  import {
    getDictationStatus,
    toggleDictation,
    cancelDictation,
    hideDictation,
    type DictationStatus,
  } from '../api/dictation';

  let { preview = false }: { preview?: boolean } = $props();
  let design = $state<CaptureDesign>('split');
  let designRevision = 0;
  let unlistenDesign: (() => void) | undefined;
  let runtime = $state<RuntimeStatus | null>(null);
  let session = $state<DictationStatus | null>(null);
  let action = $state(false);
  let error = $state<string | null>(null);
  let recordingElapsedMs = $state(0);
  let inputLevelDetected = $state(false);
  let liveTranscript = $state<StreamingTranscript | null>(null);
  let copied = $state(false);
  let copying = $state(false);
  let dragging = $state(false);
  let disposed = false;
  let reading = false;
  let refreshAgain = false;
  let pollTimer: ReturnType<typeof setTimeout> | undefined;
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  let previewTimer: ReturnType<typeof setTimeout> | undefined;
  let unlisten: (() => void) | undefined;
  let previewStage = $state<'idle' | 'recording' | 'processing'>('idle');
  let captureBar: HTMLElement | undefined = $state();
  const active = $derived(
    session && ['starting', 'recording', 'finishing', 'pasting'].includes(session.phase),
  );
  const starting = $derived(session?.phase === 'starting' || runtime?.stage === 'starting');
  const recording = $derived(
    preview
      ? previewStage === 'recording'
      : !starting &&
          runtime?.stage === 'recording' &&
          (!session?.id || session.phase === 'recording'),
  );
  const busy = $derived(preview ? previewStage !== 'idle' : Boolean(active || runtime?.busy));
  const processing = $derived(busy && !recording);
  const quietInput = $derived(recording && recordingElapsedMs > 1800 && !inputLevelDetected);
  const failed = $derived(
    Boolean(error || session?.phase === 'error' || runtime?.stage === 'error'),
  );
  const complete = $derived(
    preview
      ? session?.phase === 'ready'
      : session?.text !== null && (session?.phase === 'ready' || session?.phase === 'sent'),
  );
  const elapsed = $derived(
    `${Math.floor(recordingElapsedMs / 60000)}:${String(Math.floor(recordingElapsedMs / 1000) % 60).padStart(2, '0')}`,
  );
  const heading = $derived(
    failed
      ? 'Capture needs attention'
      : starting
        ? 'Preparing microphone'
        : recording
          ? 'Listening'
          : session?.phase === 'pasting'
            ? 'Pasting text'
            : processing
              ? runtime?.stage === 'rewriting'
                ? 'Polishing text'
                : 'Finishing transcript'
              : complete && copied
                ? 'Copied to clipboard'
                : session?.phase === 'sent'
                  ? 'Paste sent'
                  : complete
                    ? 'Transcript ready'
                    : session?.phase === 'cancelled'
                      ? 'Cancelled'
                      : 'Ready to record',
  );
  const liveText = $derived(
    liveTranscript ? `${liveTranscript.committedText}${liveTranscript.provisionalText}`.trim() : '',
  );
  const status = $derived(
    error ??
      (failed
        ? session?.message || runtime?.message || 'Open the app for details.'
        : complete
          ? copied
            ? ''
            : session?.message
          : liveText && (recording || processing)
            ? liveText
            : recording
              ? quietInput
                ? 'Microphone is very quiet.'
                : session?.mode === 'hold'
                  ? 'Release the shortcut to finish.'
                  : 'Press the shortcut again to finish.'
              : starting
                ? 'Listening starts when the microphone is ready.'
                : processing
                  ? runtime?.stage === 'rewriting'
                    ? 'Applying your writing preferences.'
                    : runtime?.message.startsWith('Transcribing · ')
                      ? `${runtime.message.slice('Transcribing · '.length)} complete`
                      : 'Working on your device.'
                  : session?.phase === 'cancelled'
                    ? 'Ready when you are.'
                    : 'Use your recording shortcut to begin.'),
  );

  function message(cause: unknown) {
    return cause instanceof Error ? cause.message : String(cause);
  }
  function applySession(next: DictationStatus) {
    if (
      session &&
      (next.id < session.id || (next.id === session.id && next.revision < session.revision))
    )
      return;
    if (next.id !== session?.id) {
      recordingElapsedMs = 0;
      copied = false;
      liveTranscript = null;
      inputLevelDetected = false;
      error = null;
      pauseHide();
    }
    const previous = session?.phase;
    session = next;
    if (next.phase !== previous && (next.phase === 'sent' || next.phase === 'cancelled'))
      scheduleHide();
  }
  async function readRuntime() {
    if (disposed || preview) return;
    if (reading) {
      refreshAgain = true;
      return;
    }
    reading = true;
    try {
      const [next, nextRuntime] = await Promise.all([getDictationStatus(), getRuntimeStatus()]);
      if (disposed) return;
      applySession(next);
      runtime = nextRuntime;
      liveTranscript = liveTranscriptFromStatus(liveTranscript, runtime);
      if (runtime.stage === 'recording' && session?.startedAtUnixMs) {
        recordingElapsedMs = Math.max(0, Date.now() - session.startedAtUnixMs);
        if (runtime.audioLevel >= 0.008) inputLevelDetected = true;
      } else if (!runtime.busy) {
        liveTranscript = null;
      }
    } catch (cause) {
      if (!disposed) error = `Status unavailable. ${message(cause)}`;
    } finally {
      reading = false;
      if (!disposed && (refreshAgain || active || runtime?.busy)) {
        refreshAgain = false;
        if (pollTimer) clearTimeout(pollTimer);
        pollTimer = setTimeout(
          () => {
            void readRuntime();
          },
          recording ? 90 : 250,
        );
      }
    }
  }
  function pauseHide() {
    if (hideTimer) clearTimeout(hideTimer);
  }
  function scheduleHide() {
    pauseHide();
    if (preview || disposed || dragging || captureBar?.matches(':hover, :focus-within')) return;
    hideTimer = setTimeout(() => {
      void hideDictation();
    }, 6000);
  }
  function resumeHide() {
    if (session?.phase === 'sent' || session?.phase === 'cancelled') scheduleHide();
  }
  async function toggle() {
    if (action) return;
    error = null;
    pauseHide();
    action = true;
    try {
      if (preview) {
        if (previewStage === 'recording') {
          previewStage = 'processing';
          previewTimer = setTimeout(() => {
            previewStage = 'idle';
            session = {
              id: 1,
              revision: 0,
              phase: 'ready',
              mode: 'toggle',
              operationId: null,
              message: 'Your transcript is ready to copy.',
              text: 'A thought, put into words.',
              startedAtUnixMs: null,
              captureReadyMs: null,
              stopToFinalMs: null,
              finalToDispatchMs: null,
            };
          }, 900);
        } else if (previewStage === 'idle') {
          session = null;
          previewStage = 'recording';
        }
      } else {
        await toggleDictation();
        await readRuntime();
      }
    } catch (cause) {
      error = message(cause);
    } finally {
      action = false;
    }
  }
  async function copy() {
    if (!session?.text || copying) return;
    const id = session.id;
    copying = true;
    error = null;
    pauseHide();
    try {
      if (preview) await navigator.clipboard.writeText(session.text);
      else {
        try {
          await invoke('copy_dictation', { id });
        } catch (cause) {
          // Other desktop platforms retain explicit browser clipboard copying.
          // The final ID check prevents acknowledging a newer operation.
          if (session?.id !== id) throw cause;
          await navigator.clipboard.writeText(session.text);
        }
      }
      if (session?.id === id) copied = true;
    } catch (cause) {
      if (session?.id === id) error = `Could not copy. ${message(cause)}`;
    } finally {
      copying = false;
    }
  }
  async function drag() {
    if (preview || dragging) return;
    dragging = true;
    pauseHide();
    try {
      await invoke('start_capture_drag');
    } catch (cause) {
      if (!disposed) error = `Could not move the capture panel. ${message(cause)}`;
    } finally {
      dragging = false;
      if (!disposed) resumeHide();
    }
  }
  async function openMain(showTranscript = false) {
    pauseHide();
    if (preview) {
      window.location.search = '';
      return;
    }
    const main = await WebviewWindow.getByLabel('main');
    await main?.show();
    if (showTranscript) await main?.emit('show-transcript');
    await main?.setFocus();
    await hideDictation();
  }
  async function dismiss() {
    pauseHide();
    if (preview) {
      clearTimeout(previewTimer);
      previewStage = 'idle';
      session = null;
      return;
    }
    try {
      if (busy) {
        await cancelDictation();
        await readRuntime();
      } else await hideDictation();
    } catch (cause) {
      error = message(cause);
    }
  }
  onMount(() => {
    document.body.classList.add('quick-window');
    async function loadDesign() {
      const revision = ++designRevision;
      try {
        const settings = await getSettings({ browserPreview: preview });
        if (!disposed && revision === designRevision) design = settings.captureDesign;
      } catch (cause) {
        if (!disposed) error = `Capture settings unavailable. ${message(cause)}`;
      }
    }
    if (preview) void loadDesign();
    else
      void getCurrentWindow()
        .listen<CaptureDesign>('capture-design-changed', (event) => {
          if (!disposed && CAPTURE_DESIGNS.includes(event.payload)) {
            ++designRevision;
            design = event.payload;
          }
        })
        .then((dispose) => {
          if (disposed) {
            dispose();
            return;
          }
          unlistenDesign = dispose;
          void loadDesign();
        })
        .catch((cause) => {
          if (!disposed) error = `Capture settings unavailable. ${message(cause)}`;
        });
    const storageChanged = () => {
      if (preview) void loadDesign();
    };
    window.addEventListener('storage', storageChanged);
    if (!preview) {
      void getCurrentWindow()
        .listen<DictationStatus>('dictation-state', (event) => {
          if (!disposed) {
            applySession(event.payload);
            void readRuntime();
          }
        })
        .then((dispose) => {
          if (disposed) dispose();
          else unlisten = dispose;
        });
      void readRuntime();
    }
    const keydown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') void dismiss();
    };
    window.addEventListener('keydown', keydown);
    return () => {
      disposed = true;
      document.body.classList.remove('quick-window');
      window.removeEventListener('keydown', keydown);
      window.removeEventListener('storage', storageChanged);
      unlistenDesign?.();
      unlisten?.();
      clearTimeout(pollTimer);
      clearTimeout(hideTimer);
      clearTimeout(previewTimer);
    };
  });
</script>

{#snippet waveform()}
  <AudioIndicator
    {recording}
    {processing}
    complete={Boolean(complete)}
    {failed}
    desktop={!preview}
    onSignal={() => {
      inputLevelDetected = true;
    }}
    fit
  />
{/snippet}
{#snippet controls(labeled: boolean)}
  <button
    class="quiet-action"
    type="button"
    onclick={dismiss}
    aria-label={busy ? 'Cancel recording' : 'Hide quick capture'}
    title={busy ? 'Cancel recording' : 'Hide quick capture'}><Icon name="close" /></button
  >
  <button
    class="record-toggle"
    type="button"
    onclick={() => (complete ? copy() : toggle())}
    disabled={action || copying || (!complete && busy && !recording)}
    aria-label={complete
      ? copied
        ? 'Transcript copied'
        : 'Copy transcript'
      : recording
        ? 'Stop and transcribe'
        : processing
          ? 'Processing recording'
          : failed
            ? 'Retry recording'
            : 'Start recording'}
    title={complete
      ? copied
        ? 'Copied'
        : 'Copy transcript'
      : recording
        ? 'Stop and transcribe'
        : processing
          ? 'Processing recording'
          : failed
            ? 'Retry recording'
            : 'Start recording'}
  >
    {#if complete}<CopyGlyph {copied} />{:else}<CaptureGlyph {recording} />{/if}
    {#if labeled}<span
        >{complete
          ? copied
            ? 'Copied'
            : 'Copy'
          : recording
            ? 'Stop'
            : processing
              ? 'Wait'
              : failed
                ? 'Retry'
                : 'Record'}</span
      >{/if}
  </button>
{/snippet}

<main
  bind:this={captureBar}
  class="quick-capture"
  aria-label="Quick capture"
  class:is-recording={recording}
  class:is-processing={processing}
  class:is-complete={complete}
  class:is-quiet={quietInput}
  class:is-error={failed}
  onmouseenter={pauseHide}
  onmouseleave={resumeHide}
  onfocusin={pauseHide}
  onfocusout={resumeHide}
>
  <CaptureLayout
    {design}
    onDrag={preview ? undefined : drag}
    {dragging}
    {heading}
    {status}
    {elapsed}
    {failed}
    attention={quietInput}
    complete={Boolean(complete)}
    detailVisible={processing || Boolean(liveText && recording)}
    phase={recording ? 'recording' : processing ? 'processing' : 'idle'}
    {recording}
    {waveform}
    actions={controls}
    onOpen={() => {
      void openMain(Boolean(complete));
    }}
  />
</main>

<style>
  :global(html:has(body.quick-window)),
  :global(body.quick-window) {
    background: var(--color-transparent);
    overflow-x: clip;
  }
  :global(body.quick-window) {
    min-width: 0;
  }
  :global(body.quick-window #app) {
    min-height: 100vh;
    padding: 12px;
  }
  .quick-capture {
    display: flex;
    justify-content: center;
    width: 100%;
    margin: 0;
  }
</style>
