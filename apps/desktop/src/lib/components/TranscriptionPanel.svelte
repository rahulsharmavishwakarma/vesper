<script lang="ts">
  import CaptureGlyph from './CaptureGlyph.svelte';
  import CopyGlyph from './CopyGlyph.svelte';
  import {
    editLatestDraft,
    undoLatestLearning,
    undoPersonalization,
    undoUnsavedPersonalization,
  } from '../api/personalization';
  import { onMount, tick } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import Icon from './Icon.svelte';
  import AudioIndicator from './AudioIndicator.svelte';
  import { liveTranscriptFromStatus, mergeLiveTranscript } from '../liveTranscript';
  import ConfiguredModels from './ConfiguredModels.svelte';
  import Select from './Select.svelte';
  import {
    downloadBytes,
    downloadKey,
    type DownloadTarget,
    type ModelDownloadState,
  } from '../modelDownloads';
  import { ASR_MODELS, CLEANUP_MODELS, CLEANUP_QUANTIZATIONS, type AppConfig } from '../config';
  import type { AppEnvironment } from '../api/desktop';
  import {
    cancelOperation,
    chooseAudioFile,
    getLatestResult,
    getModelInstallation,
    getRuntimeStatus,
    installModel,
    installModels,
    listAudioDevices,
    listModelInstallations,
    removeModel,
    startRecording,
    stopRecording,
    transcribeFile,
    type AudioDevice,
    type ModelCatalogInstallation,
    type ModelInstallation,
    type PipelineResult,
    type RuntimeStatus,
    type StreamingTranscript,
  } from '../api/runtime';

  let {
    environment,
    view,
    onNavigate,
    settingsLoaded,
    settingsDirty,
    settingsBusy,
    settingsRevision,
    cleanupEnabled,
    modelSettings,
    maxRecordingSeconds,
    onBusyChange,
    onResult,
    onDownloadState,
    onRuntimeChange,
  }: {
    environment: AppEnvironment;
    view: 'dictate' | 'history' | 'models' | 'settings';
    onNavigate: (view: 'dictate' | 'history' | 'models' | 'settings') => void;
    settingsLoaded: boolean;
    settingsDirty: boolean;
    settingsBusy: boolean;
    settingsRevision: string;
    cleanupEnabled: boolean;
    modelSettings: AppConfig | null;
    maxRecordingSeconds: number;
    onBusyChange: (busy: boolean) => void;
    onRuntimeChange?: (runtime: RuntimeStatus | null) => void;
    onResult: (result: PipelineResult) => void;
    onDownloadState: (state: ModelDownloadState) => void;
  } = $props();

  let runtime = $state<RuntimeStatus | null>(null);
  let installation = $state<ModelInstallation | null>(null);
  let modelCatalog = $state<ModelCatalogInstallation[]>([]);
  let devices = $state<AudioDevice[]>([]);
  let deviceId = $state('');
  let result = $state<PipelineResult | null>(null);
  let draft = $state('');
  let savedDraft = $state('');
  let savingDraft = $state(false);
  let composing = $state(false);
  let draftError = $state('');
  let recoveredDraft = $state('');
  let draftSave: Promise<boolean> | null = null;
  let action = $state<
    'installing' | 'starting' | 'stopping' | 'choosing' | 'transcribing' | 'removing' | null
  >(null);
  let installingModelKey = $state<string | null>(null);
  let downloadError = $state<string | null>(null);
  let downloadFeedback = $state<string | null>(null);
  let refreshing = $state(false);
  let cancelling = $state(false);
  let error = $state<string | null>(null);
  let statusError = $state<string | null>(null);
  let modelError = $state<string | null>(null);
  let deviceError = $state<string | null>(null);
  let feedback = $state<string | null>(null);
  let copied = $state(false);
  let copying = $state(false);
  let undoing = $state(false);
  let copyMessage = $state<string | null>(null);
  let copyTimer: ReturnType<typeof setTimeout> | undefined;
  let pollTimer: ReturnType<typeof setTimeout> | undefined;
  let recordingStartedAt = $state<number | null>(null);
  let recordingElapsedMs = $state(0);
  let inputLevelDetected = $state(false);
  let polling = false;
  let disposed = false;
  let refreshVersion = 0;
  let activeOperationId = $state<string | null>(null);
  let liveTranscript = $state<StreamingTranscript | null>(null);
  let unlistenPartial: (() => void) | undefined;

  const desktop = $derived(environment === 'desktop');
  const busy = $derived(Boolean(action || installingModelKey || runtime?.busy || cancelling));
  const blocked = $derived(
    savingDraft ||
      !desktop ||
      !settingsLoaded ||
      settingsBusy ||
      settingsDirty ||
      busy ||
      refreshing,
  );
  const recording = $derived(runtime?.stage === 'recording');
  const processing = $derived(busy && !recording && action !== 'choosing');
  const quietInput = $derived(recording && recordingElapsedMs > 1800 && !inputLevelDetected);
  const ready = $derived(
    statusError === null &&
      installation?.supported === true &&
      installation.ready &&
      runtime?.modelsReady === true,
  );
  const stageName = $derived(
    (
      {
        installing: 'Installing models',
        removing: 'Removing model files',
        recording: 'Recording',
        recording_complete: 'Recording finished',
        transcribing: 'Transcribing',
        rewriting: 'Polishing text',
        idle: 'Ready',
        error: 'Operation failed',
        cancelled: 'Cancelled',
      } as Record<string, string>
    )[runtime?.stage ?? ''] ?? 'Checking status',
  );
  const recorderStatus = $derived(
    cancelling
      ? 'Cancelling…'
      : action === 'starting'
        ? 'Starting microphone…'
        : action === 'stopping' && recording
          ? 'Finishing recording…'
          : action === 'choosing'
            ? 'Choose an audio file'
            : recording
              ? 'Recording'
              : busy
                ? stageName
                : !desktop
                  ? 'Record in the desktop app'
                  : !settingsLoaded
                    ? 'Loading preferences…'
                    : error ||
                        statusError ||
                        modelError ||
                        deviceError ||
                        runtime?.stage === 'error'
                      ? 'Recorder needs attention'
                      : settingsDirty
                        ? 'Save your preferences first'
                        : refreshing
                          ? 'Checking your microphone…'
                          : !ready
                            ? 'Set up your models'
                            : !devices.length
                              ? 'Connect a microphone'
                              : result
                                ? 'Ready for another recording'
                                : 'Ready to record',
  );
  const recorderHint = $derived(
    quietInput
      ? 'No microphone signal. Check your input or speak closer.'
      : processing &&
          runtime?.stage === 'transcribing' &&
          runtime.message.startsWith('Transcribing · ')
        ? `${runtime.message.slice('Transcribing · '.length)} complete`
        : desktop &&
            settingsLoaded &&
            !settingsDirty &&
            !busy &&
            !refreshing &&
            ready &&
            !devices.length
          ? 'Connect a microphone, then select Rescan.'
          : null,
  );
  const previewCatalog: ModelCatalogInstallation[] = [
    ...Object.entries(ASR_MODELS).map(([modelId, name]) => ({
      modelId,
      name,
      role: 'speech_recognition' as const,
      quantization: null,
      selected: false,
      ready: false,
      artifactsInstalled: false,
      runtimeReady: false,
      localFileCount: 0,
      downloadedBytes: 0,
      removableBytes: 0,
      removalError: null,
      modelDirectory: '',
      files: [
        {
          name: 'Model weights',
          installed: false,
          sizeBytes: (
            {
              'nvidia/parakeet-tdt-0.6b-v3': 670_478_772,
              'nvidia/nemotron-3.5-asr-streaming-0.6b': 742_090_464,
            } as Record<string, number>
          )[modelId],
        },
      ],
      supported: false,
      message: 'Download this model in the desktop app.',
    })),
    ...Object.entries(CLEANUP_MODELS).flatMap(([modelId, model]) =>
      model.quantizations.map((quantization) => {
        const details = CLEANUP_QUANTIZATIONS[quantization];
        return {
          modelId,
          name: model.name,
          role: 'text_reconstruction' as const,
          quantization,
          selected: false,
          ready: false,
          artifactsInstalled: false,
          runtimeReady: false,
          localFileCount: 0,
          downloadedBytes: 0,
          removableBytes: 0,
          removalError: null,
          modelDirectory: '',
          files: [{ name: details.label, installed: false, sizeBytes: details.sizeBytes }],
          supported: false,
          message: 'Download this model in the desktop app.',
        };
      }),
    ),
  ];
  const displayedCatalog = $derived(desktop ? modelCatalog : previewCatalog);

  function message(cause: unknown) {
    return cause instanceof Error ? cause.message : String(cause);
  }
  function acceptResult(completed: PipelineResult | null) {
    if (!completed) return;
    const newResult = result?.resultId !== completed.resultId;
    const clean = draft === savedDraft;
    if (newResult && !clean) recoveredDraft = draft;
    if (newResult || clean) {
      draft =
        completed.draftTranscript ??
        completed.personalizedTranscript ??
        completed.cleanedTranscript ??
        completed.rawTranscript;
      savedDraft = draft;
      draftError = '';
    }
    // A job can publish raw text before adding cleaned text or warnings with the
    // same ID. Deduplicate identical snapshots, not later updates to that job.
    const changed = JSON.stringify(completed) !== JSON.stringify(result);
    result = completed;
    activeOperationId = null;
    liveTranscript = null;
    if (changed) {
      copied = false;
      copyMessage = null;
      onResult(completed);
    }
  }
  function commitDraft(): Promise<boolean> {
    if (draftSave) return draftSave;
    if (!result || draft === savedDraft) return Promise.resolve(true);
    if (busy || composing) return Promise.resolve(false);
    const current = result;
    const text = draft;
    savingDraft = true;
    draftError = '';
    draftSave = editLatestDraft(current.resultId, current.draftRevision ?? 0, text)
      .then((updated) => {
        if (result?.resultId === current.resultId) {
          result = updated;
          savedDraft = text;
          copied = false;
          onResult(updated);
        }
        return result?.resultId === current.resultId;
      })
      .catch((cause) => {
        if (result?.resultId === current.resultId)
          draftError = `Your edit is still here, but could not be saved: ${message(cause)}`;
        return false;
      })
      .finally(() => {
        savingDraft = false;
        draftSave = null;
      });
    return draftSave;
  }
  async function undoLearned() {
    if (!result || busy || savingDraft || undoing) return;
    const id = result.resultId;
    undoing = true;
    draftError = '';
    try {
      const updated = await undoLatestLearning(id);
      if (result?.resultId === id) {
        result = updated;
        onResult(updated);
      }
    } catch (cause) {
      if (result?.resultId === id) draftError = message(cause);
    } finally {
      undoing = false;
    }
  }
  function acceptPartial(partial: StreamingTranscript | null) {
    liveTranscript = mergeLiveTranscript(
      liveTranscript,
      partial,
      activeOperationId,
      runtime?.partialInvalidatedFor ?? null,
    );
    if (liveTranscript) activeOperationId = liveTranscript.operationId;
  }
  async function copyResult() {
    if (!result || copying) return;
    if (!(await commitDraft())) return;
    if (!result) return;
    const source = result.resultId;
    const text = draft;
    if (!text.trim()) return;
    copied = false;
    copyMessage = null;
    copying = true;
    try {
      if (!navigator.clipboard?.writeText) throw new Error('Clipboard unavailable');
      await navigator.clipboard.writeText(text);
      if (disposed || result?.resultId !== source) return;
      copied = true;
      copyMessage = 'Transcript copied.';
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => {
        copied = false;
      }, 2000);
    } catch {
      if (disposed || result?.resultId !== source) return;
      const field = document.getElementById('final-draft') as HTMLTextAreaElement | null;
      field?.focus();
      field?.select();
      copyMessage = field
        ? 'Automatic copy is unavailable. Text is selected; press ⌘C or Ctrl+C to copy.'
        : 'Automatic copy is unavailable. Select the transcript and copy it manually.';
    } finally {
      copying = false;
    }
  }
  async function pollRuntime() {
    if (!desktop || disposed || polling) return;
    polling = true;
    try {
      runtime = await getRuntimeStatus();
      // Authoritative operation identity also covers shortcut-started captures
      // before their first partial, and failures that finish between polls.
      activeOperationId = runtime.busy ? runtime.operationId : null;
      liveTranscript = liveTranscriptFromStatus(liveTranscript, runtime);
      if (runtime.stage === 'recording') {
        recordingStartedAt ??= Date.now();
        recordingElapsedMs = Date.now() - recordingStartedAt;
        if (runtime.audioLevel >= 0.008) inputLevelDetected = true;
      } else if (!runtime.busy) {
        recordingStartedAt = null;
        recordingElapsedMs = 0;
        inputLevelDetected = false;
      }
      statusError = null;
      if (!runtime.busy) acceptResult(await getLatestResult());
    } catch (cause) {
      statusError = `Could not check transcription status. ${message(cause)}`;
    } finally {
      polling = false;
      if (!disposed && (runtime?.busy || action || installingModelKey || cancelling))
        pollTimer = setTimeout(
          () => {
            void pollRuntime();
          },
          runtime?.stage === 'recording' ? 100 : 500,
        );
    }
  }
  async function undoRemembered() {
    if (!result || undoing || busy) return;
    const current = result;
    undoing = true;
    error = null;
    try {
      const session = current.session ? await undoPersonalization(current.session.id) : null;
      if (!current.session) await undoUnsavedPersonalization(current.resultId);
      if (result?.resultId === current.resultId) {
        acceptResult({ ...current, personalizedTranscript: null, session });
        copied = false;
        if (result) onResult(result);
      }
    } catch (cause) {
      error = message(cause);
    } finally {
      undoing = false;
    }
  }
  function startPolling() {
    if (pollTimer) clearTimeout(pollTimer);
    pollTimer = undefined;
    void pollRuntime();
  }
  export async function revealLatestResult() {
    await pollRuntime();
    await tick();
    document.getElementById('final-draft')?.focus();
  }
  export async function refreshResources() {
    if (!desktop || !settingsLoaded || busy) return;
    const version = ++refreshVersion;
    refreshing = true;
    const [models, catalog, inputs] = await Promise.allSettled([
      getModelInstallation(),
      listModelInstallations(),
      listAudioDevices(),
    ]);
    if (disposed || version !== refreshVersion) return;
    if (models.status === 'fulfilled') {
      installation = models.value;
      modelError = null;
    } else {
      installation = null;
      modelError = `Could not check model files. ${message(models.reason)}`;
    }
    if (catalog.status === 'fulfilled') modelCatalog = catalog.value;
    else {
      modelCatalog = [];
      modelError = `Could not list downloadable models. ${message(catalog.reason)}`;
    }
    if (inputs.status === 'fulfilled') {
      devices = inputs.value;
      deviceError = null;
      if (deviceId && !devices.some((device) => device.id === deviceId)) deviceId = '';
      if (!deviceId)
        deviceId = devices.find((device) => device.isDefault)?.id ?? devices[0]?.id ?? '';
    } else {
      devices = [];
      deviceError = `Could not list microphones. ${message(inputs.reason)}`;
    }
    refreshing = false;
    startPolling();
  }
  async function install() {
    if (blocked || !installation?.supported) return;
    action = 'installing';
    error = null;
    feedback = null;
    startPolling();
    try {
      installation = await installModels();
      feedback = installation.ready
        ? 'Model files are ready on this device.'
        : installation.message;
    } catch (cause) {
      error = `Could not install models. ${message(cause)}`;
    } finally {
      action = null;
      await pollRuntime();
      await refreshResources();
    }
  }
  export function clearDownloadFeedback() {
    downloadError = null;
    downloadFeedback = null;
  }
  export async function downloadModel(target: DownloadTarget) {
    // Catalog refresh may still be running after Save & download. Installation
    // uses the newly persisted config; the backend validates the exact target.
    if (!desktop || !settingsLoaded || settingsBusy || settingsDirty || busy) return;
    const key = downloadKey(target);
    installingModelKey = key;
    error = null;
    feedback = null;
    downloadError = null;
    downloadFeedback = null;
    startPolling();
    try {
      const updated = await installModel(target.modelId, target.quantization);
      modelCatalog = [...modelCatalog.filter((item) => downloadKey(item) !== key), updated];
      downloadFeedback = updated.ready
        ? `${updated.name} is ready on this device.`
        : updated.message;
    } catch (cause) {
      if (message(cause).toLowerCase().includes('cancel'))
        downloadFeedback = `${target.name} download cancelled. You can try again.`;
      else downloadError = `Could not download ${target.name}. ${message(cause)}`;
    } finally {
      installingModelKey = null;
      await pollRuntime();
      await refreshResources();
    }
  }
  export async function removeDownloadedModel(model: ModelCatalogInstallation) {
    if (!desktop || !settingsLoaded || settingsBusy || busy || refreshing) {
      throw new Error('Finish the current operation before removing model files.');
    }
    action = 'removing';
    downloadError = null;
    downloadFeedback = null;
    startPolling();
    try {
      const removed = await removeModel(model);
      downloadFeedback = `${model.name}${model.quantization ? ` · ${model.quantization.slice(0, 2).toUpperCase()}` : ''} removed. ${downloadBytes(removed.removedBytes)} of model files deleted.`;
      return removed;
    } finally {
      action = null;
      await pollRuntime();
      await refreshResources();
    }
  }
  export async function cancelDownload() {
    await cancel();
  }
  async function record() {
    if (blocked || !ready || !devices.length) return;
    action = 'starting';
    error = null;
    feedback = null;
    recordingStartedAt = Date.now();
    recordingElapsedMs = 0;
    inputLevelDetected = false;
    startPolling();
    liveTranscript = null;
    activeOperationId = null;
    try {
      activeOperationId = await startRecording(deviceId || null);
    } catch (cause) {
      error = `Could not start recording. ${message(cause)}`;
    } finally {
      action = null;
      startPolling();
    }
  }
  async function stop() {
    if (!desktop || !recording || action || cancelling) return;
    action = 'stopping';
    error = null;
    feedback = null;
    startPolling();
    try {
      acceptResult(await stopRecording());
    } catch (cause) {
      error = `Could not finish transcription. ${message(cause)}`;
    } finally {
      action = null;
      startPolling();
    }
  }
  async function transcribe() {
    if (blocked || !ready) return;
    action = 'choosing';
    error = null;
    feedback = null;
    try {
      const path = await chooseAudioFile();
      if (!path) return;
      action = 'transcribing';
      startPolling();
      acceptResult(await transcribeFile(path));
    } catch (cause) {
      error = `Could not transcribe audio. ${message(cause)}`;
    } finally {
      action = null;
      startPolling();
    }
  }
  async function cancel() {
    if (!desktop || !busy || cancelling || action === 'choosing') return;
    cancelling = true;
    error = null;
    feedback = null;
    try {
      await cancelOperation();
      activeOperationId = null;
      liveTranscript = null;
      feedback = 'Cancellation requested.';
    } catch (cause) {
      error = `Could not cancel the operation. ${message(cause)}`;
    } finally {
      cancelling = false;
      startPolling();
    }
  }

  $effect(() => {
    onBusyChange(busy);
  });
  $effect(() => {
    onRuntimeChange?.(runtime);
  });
  $effect(() => {
    onDownloadState({
      catalog: displayedCatalog,
      checking: refreshing,
      installingKey: installingModelKey,
      installing: Boolean(
        installingModelKey || action === 'installing' || (runtime?.busy && runtime.download),
      ),
      cancelling: cancelling || runtime?.stage === 'cancelling',
      details:
        runtime?.busy && (runtime.stage === 'installing' || runtime.stage === 'cancelling')
          ? (runtime.download?.progress ?? null)
          : null,
      progress: runtime?.stage === 'installing' ? runtime.message : '',
      error: downloadError ?? modelError,
      feedback: downloadFeedback,
    });
  });
  $effect(() => {
    // Reading the revision explicitly subscribes this effect to saved settings.
    void settingsRevision;
    if (desktop && settingsLoaded && !settingsBusy) void refreshResources();
  });
  onMount(() => {
    startPolling();
    if (desktop) {
      void listen<StreamingTranscript>('transcription-partial', (event) =>
        acceptPartial(event.payload),
      ).then((dispose) => {
        if (disposed) dispose();
        else unlistenPartial = dispose;
      });
    }
    return () => {
      disposed = true;
      unlistenPartial?.();
      if (pollTimer) clearTimeout(pollTimer);
      if (copyTimer) clearTimeout(copyTimer);
    };
  });
</script>

<section
  class="transcription-panel"
  hidden={view !== 'dictate' && view !== 'models'}
  aria-label="Local transcription"
>
  <div hidden={view !== 'models'} class="model-installation">
    <div class="section-heading">
      <h2>Selected models</h2>
      <button
        class="quiet"
        type="button"
        onclick={refreshResources}
        disabled={!desktop || busy || refreshing || settingsBusy || !settingsLoaded}
        ><Icon name="refresh" />{refreshing ? 'Checking…' : 'Check files'}</button
      >
    </div>
    {#if !desktop}<p class="note">
        Use the download buttons under the model dropdowns in the desktop app.
      </p>
    {:else if installation}<p class="note">{installation.message}</p>
      <p class="note model-path">{installation.modelDirectory}</p>{/if}
    {#if desktop}
      <div class="dataset-actions">
        <button
          class="primary"
          type="button"
          onclick={install}
          disabled={blocked || installation?.supported !== true}
          >{action === 'installing'
            ? 'Checking model files…'
            : installation?.files.every((file) => file.installed)
              ? 'Verify selected models'
              : 'Install selected models'}</button
        >
      </div>
    {/if}
    {#if settingsLoaded && !cleanupEnabled}<p class="note">
        The writing model is off. Install the speech model and any enabled audio processing models.
      </p>{/if}
    {#if busy && action !== 'choosing'}<div class="dataset-actions">
        <button type="button" onclick={cancel} disabled={cancelling}
          >{cancelling ? 'Cancelling…' : 'Cancel operation'}</button
        >
      </div>{/if}
  </div>

  <div class="dictate-capture" hidden={view !== 'dictate'}>
    <div class="studio-recorder" class:is-recording={recording} class:is-processing={processing}>
      <div class="recorder-toolbar">
        <div class="microphone-control">
          <label for="microphone">Microphone</label>
          <div class="microphone-picker">
            <Icon name="mic" />
            <Select
              id="microphone"
              bind:value={deviceId}
              options={devices.length
                ? devices.map((device) => ({
                    value: device.id,
                    label: device.name,
                    description: device.isDefault ? 'System default' : undefined,
                  }))
                : [
                    {
                      value: deviceId,
                      label: !desktop
                        ? 'Desktop app required'
                        : refreshing
                          ? 'Scanning microphones…'
                          : 'No input selected',
                    },
                  ]}
              disabled={!desktop || busy || refreshing || !devices.length}
            />
            <button
              type="button"
              class="microphone-refresh quiet icon-button"
              aria-label={refreshing ? 'Scanning microphones' : 'Rescan microphones'}
              title={refreshing ? 'Scanning microphones' : 'Rescan microphones'}
              aria-busy={refreshing}
              onclick={refreshResources}
              disabled={!desktop || busy || refreshing || settingsBusy || !settingsLoaded}
            >
              <span class:spinning={refreshing} aria-hidden="true"><Icon name="refresh" /></span>
            </button>
          </div>
        </div>
        <span class="recorder-limit"
          >{maxRecordingSeconds > 0
            ? `Up to ${maxRecordingSeconds} seconds with this model`
            : 'No fixed time limit'}</span
        >
      </div>
      <div class="recorder-stage">
        <div class="recorder-main">
          <div class="recorder-heading">
            <h2 class="recorder-label"><span role="status">{recorderStatus}</span></h2>
            {#if recording}<span class="recording-timer" aria-label="Recording duration"
                >{String(Math.floor(recordingElapsedMs / 60000)).padStart(2, '0')}:{String(
                  Math.floor(recordingElapsedMs / 1000) % 60,
                ).padStart(2, '0')}</span
              >{/if}
          </div>
          <div class="recorder-guidance" aria-live="polite">
            {#if recorderHint}<p class="recorder-hint" class:quiet-input={quietInput}>
                {recorderHint}
              </p>{/if}
          </div>
          <div class="capture-actions">
            <button
              class="primary record-button"
              class:capturing={recording}
              type="button"
              onclick={() => (recording ? stop() : record())}
              disabled={recording
                ? action !== null || cancelling
                : blocked || !ready || !devices.length}
            >
              <CaptureGlyph {recording} />{recording
                ? action === 'stopping'
                  ? 'Finishing…'
                  : 'Stop and transcribe'
                : action === 'starting'
                  ? 'Starting…'
                  : 'Record'}
            </button>
            <button
              class="quiet transcribe-file"
              type="button"
              onclick={transcribe}
              disabled={blocked || !ready}><Icon name="upload" />Transcribe audio file</button
            >
            {#if busy && action !== 'choosing'}<button
                class="quiet"
                type="button"
                onclick={cancel}
                disabled={cancelling}>{cancelling ? 'Cancelling…' : 'Cancel operation'}</button
              >{/if}
          </div>
          {#if ready && installation?.files.some((file) => !file.installed)}<p class="note">
              {installation.message}
            </p>{/if}
          {#if desktop && settingsLoaded && !ready && !settingsDirty && !busy}<button
              class="quiet setup-link"
              type="button"
              onclick={() => onNavigate('models')}>Set up models <Icon name="arrow" /></button
            >{/if}
        </div>
        <div class="recorder-signal">
          <AudioIndicator
            {recording}
            {processing}
            {desktop}
            visible={view === 'dictate'}
            complete={Boolean(result) && !busy}
            failed={Boolean(error || statusError || runtime?.stage === 'error')}
            onSignal={() => {
              inputLevelDetected = true;
            }}
          />
        </div>
      </div>
      <ConfiguredModels settings={modelSettings} onManage={() => onNavigate('models')} />
    </div>
  </div>
  <div class="runtime-feedback" class:is-active={busy} aria-live="polite">
    {#if settingsDirty && view === 'models'}<p class="note">
        Save your changes before installing models.
      </p>{/if}
    {#if desktop && runtime && ((busy && view === 'models') || runtime.stage === 'error' || runtime.stage === 'cancelled')}<p
        class="runtime-stage"
      >
        {stageName}
      </p>
      <p class="note">{runtime.message}</p>{/if}
    {#if modelError}<p class="error" role="alert">{modelError}</p>{/if}
    {#if deviceError}<p class="error" role="alert">{deviceError}</p>{/if}
    {#if statusError}<p class="error" role="alert">{statusError}</p>{/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if feedback}<p class="success">{feedback}</p>{/if}
  </div>
  {#if liveTranscript && (recording || processing)}
    <div class="live-transcript" aria-live="polite" aria-atomic="true">
      <div class="section-heading">
        <h2>Live transcript</h2>
        <span class="note">{liveTranscript.isFinal ? 'Finalizing' : 'Draft'}</span>
      </div>
      <p>
        <span>{liveTranscript.committedText}</span><span class="provisional"
          >{liveTranscript.provisionalText}</span
        >
      </p>
    </div>
  {/if}
  {#if result}
    <div class="transcription-result" hidden={view !== 'dictate'}>
      <div class="section-heading">
        <div>
          <h2>Your latest transcription</h2>
          <span class="note">{result.durationSeconds.toFixed(1)} seconds</span>
        </div>
        <button
          class="quiet"
          class:copy-confirmed={copied}
          type="button"
          onclick={copyResult}
          disabled={copying || busy || !draft.trim()}
          aria-busy={copying}
          ><CopyGlyph {copied} />{copying ? 'Copying…' : copied ? 'Copied' : 'Copy text'}</button
        >
      </div>
      <div class="field">
        <label for="final-draft">Your text</label><textarea
          id="final-draft"
          rows="5"
          bind:value={draft}
          maxlength="100000"
          disabled={busy || savingDraft || undoing}
          oncompositionstart={() => {
            composing = true;
          }}
          oncompositionend={() => {
            composing = false;
          }}
          onblur={() => {
            void commitDraft();
          }}
          aria-describedby="final-draft-help"></textarea>
        <p id="final-draft-help" class="note">
          Edit naturally. Changes save when you leave the field or copy. Eligible spelling
          corrections automatically teach future dictation on this device.
        </p>
      </div>
      {#if savingDraft}<p class="note" aria-live="polite">Saving your edit…</p>{/if}
      {#if draftError}<p class="error" role="alert">{draftError}</p>
        <button
          type="button"
          disabled={busy || savingDraft}
          onclick={() => {
            void commitDraft();
          }}>Retry saving edit</button
        >{/if}
      {#if recoveredDraft}<details open>
          <summary>Unsaved text from the previous result</summary><textarea
            aria-label="Recovered unsaved draft"
            readonly
            rows="3"
            value={recoveredDraft}></textarea>
          <p class="note">
            A new recording arrived before this edit saved. Select this text to copy it.
          </p>
          <button
            type="button"
            onclick={() => {
              recoveredDraft = '';
            }}>Dismiss recovered text</button
          >
        </details>{/if}
      {#if result.learningOutcome}<p class="note" aria-live="polite">
          {result.learningOutcome.status}{#if result.learningOutcome.activated.length}
            · {result.learningOutcome.activated.join(', ')}
            <button
              type="button"
              class="quiet"
              disabled={busy || savingDraft || undoing}
              onclick={undoLearned}>Undo learning</button
            >{/if}
        </p>{/if}
      <details>
        <summary>Original outputs</summary>
        <div class="transcript-columns">
          <div>
            <label for="raw-result">Raw transcript</label><textarea
              id="raw-result"
              rows="4"
              readonly
              value={result.rawTranscript}></textarea>
          </div>
          {#if result.cleanedTranscript !== null}<div>
              <label for="cleaned-result">Polished transcript</label><textarea
                id="cleaned-result"
                rows="4"
                readonly
                value={result.cleanedTranscript}></textarea>
            </div>{/if}
        </div>
        {#if result.personalizedTranscript != null}<div>
            <label for="personalized-result">With your remembered corrections</label><textarea
              id="personalized-result"
              rows="4"
              readonly
              value={result.personalizedTranscript}></textarea><button
              class="quiet"
              type="button"
              disabled={undoing || busy || savingDraft || draft !== savedDraft}
              onclick={undoRemembered}>Undo remembered edits</button
            >
          </div>{/if}
      </details>
      {#if result.session}<p class="note">Saved to History with audio.</p>{:else}<p class="note">
          Not saved to History.
        </p>{/if}
      {#if result.warnings.length}<ul class="pipeline-warnings">
          {#each result.warnings as warning, index (index)}<li>{warning}</li>{/each}
        </ul>{/if}
      <div aria-live="polite">
        {#if copyMessage}<p class="note">{copyMessage}</p>{/if}
      </div>
    </div>
  {/if}
</section>
