<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { listen } from '@tauri-apps/api/event';
  import { invoke } from '@tauri-apps/api/core';
  import {
    getAppStatus,
    getAcceleratorCapabilities,
    getEnvironment,
    getSettings,
    resetSettings,
    retryGlobalShortcut,
    saveSettings,
    type AppStatusSnapshot,
  } from './lib/api/desktop';
  import {
    AUDIO_FRONTENDS,
    MODEL_ROLE_LABELS,
    CLEANUP_QUANTIZATION_OPTIONS,
    defaultSettings,
    effectiveRecordingSeconds,
    requiresCompletionReport,
    transcriptionLanguageOptions,
    validateSettings,
    type AppConfig,
  } from './lib/config';
  import {
    modelOptions,
    presetForProfile,
    selectModel,
    selectedModelValue,
  } from './lib/modelPresets';
  import DatasetPanel from './lib/components/DatasetPanel.svelte';
  import PersonalizationPanel from './lib/components/PersonalizationPanel.svelte';
  import TranscriptionPanel from './lib/components/TranscriptionPanel.svelte';
  import ConfiguredModels from './lib/components/ConfiguredModels.svelte';
  import ModelDownload from './lib/components/ModelDownload.svelte';
  import ModelRemovalDialog from './lib/components/ModelRemovalDialog.svelte';
  import {
    downloadTarget,
    downloadOptionStatus,
    knownDownloadSelection,
    type ModelDownloadState,
    type DownloadTarget,
  } from './lib/modelDownloads';
  import type { ModelProfileRole } from './lib/config';
  import Icon from './lib/components/Icon.svelte';
  import Select from './lib/components/Select.svelte';
  import { installPressFeedback } from './lib/actions/pressFeedback';
  import { finishEntranceAnimations } from './lib/actions/entranceMotion';
  import { ensureNotificationPermission } from './lib/api/notifications';
  import QuickCapture from './lib/components/QuickCapture.svelte';
  import CaptureDesignPicker from './lib/components/CaptureDesignPicker.svelte';
  import DictationPreferences from './lib/components/DictationPreferences.svelte';
  import ApplicationStatus from './lib/components/ApplicationStatus.svelte';
  import AboutSheet from './lib/components/AboutSheet.svelte';
  import BrandLogo from './lib/components/BrandLogo.svelte';
  import DownloadProgress from './lib/components/DownloadProgress.svelte';
  import {
    acceleratorOptions,
    acceleratorReason,
    deviceLabel,
    type AcceleratorCapabilities,
  } from './lib/accelerators';
  import type { RuntimeStatus } from './lib/api/runtime';

  const environment = getEnvironment();
  const quickWindow =
    environment === 'desktop'
      ? getCurrentWindow().label === 'quick-capture'
      : new URLSearchParams(window.location.search).get('window') === 'quick-capture';
  let draft = $state<AppConfig>(defaultSettings());
  let engineSettings = $state<AppConfig | null>(null);
  let engineActivity = $state<RuntimeStatus | null>(null);
  let savedJson = $state('');
  let savedCollectionEnabled = $state(false);
  let loaded = $state(false);
  let pending = $state<'loading' | 'saving' | 'resetting' | 'retrying' | null>('loading');
  let error = $state<string | null>(null);
  let feedback = $state<string | null>(null);
  let snapshot = $state<AppStatusSnapshot | null>(null);
  let shortcutRetryError = $state<string | null>(null);
  const shortcutError = $derived(shortcutRetryError ?? snapshot?.status.shortcut_error);
  let statusError = $state<string | null>(null);
  let refreshing = $state(false);
  let pipelineBusy = $state(false);
  let transcriptionPanel: TranscriptionPanel | undefined = $state();
  let downloadRequest = $state<string | null>(null);
  let downloads = $state<ModelDownloadState>({
    catalog: [],
    checking: true,
    installingKey: null,
    installing: false,
    cancelling: false,
    details: null,
    progress: '',
    error: null,
    feedback: null,
  });
  const asrDownload = $derived(downloadTarget(draft, 'asr'));
  const cleanupDownload = $derived(downloadTarget(draft, 'rewriter'));
  let datasetRevision = $state(0);
  let removalDialog: ModelRemovalDialog | undefined = $state();
  let aboutSheet: AboutSheet | undefined = $state();
  const savedCleanupEnabled = $derived(engineSettings?.cleanupEnabled ?? true);
  let savedRecordingLimit = $state(0);
  type View = 'dictate' | 'history' | 'models' | 'settings';
  let activeView = $state<View>('dictate');
  let navigationRevision = $state(0);
  const views: {
    id: View;
    label: string;
    icon: 'mic' | 'history' | 'models' | 'settings';
    title: string;
    description: string;
  }[] = [
    {
      id: 'dictate',
      label: 'Dictate',
      icon: 'mic',
      title: 'A thought, put into words.',
      description: 'Speak naturally. Keep your voice in the writing.',
    },
    {
      id: 'history',
      label: 'History',
      icon: 'history',
      title: 'Your words, kept close.',
      description: 'Find recordings and review your transcripts.',
    },
    {
      id: 'models',
      label: 'Models',
      icon: 'models',
      title: 'Two models. Your voice.',
      description: 'Choose a speech model to transcribe and a writing model to polish the words.',
    },
    {
      id: 'settings',
      label: 'Settings',
      icon: 'settings',
      title: 'Make room for your voice.',
      description: 'Set your language, writing style, and recording preferences.',
    },
  ];
  const page = $derived(views.find((view) => view.id === activeView)!);
  let capabilities = $state<AcceleratorCapabilities | null>(null);
  let detectingDevice = $state(false);
  let deviceDetectionError = $state<string | null>(null);
  let deviceRequested = false;
  let disposed = false;
  const backendOptions = $derived(acceleratorOptions(draft, capabilities));
  const backendError = $derived(
    capabilities ? acceleratorReason(draft, capabilities, draft.computeBackend) : null,
  );
  const validationErrors = $derived([
    ...validateSettings(draft),
    ...(backendError ? [backendError] : []),
  ]);
  const dirty = $derived(loaded && JSON.stringify(draft) !== savedJson);
  const languageError = $derived(
    draft.models.customAsr
      ? draft.language !== 'auto' && !draft.models.customAsr.languages?.includes(draft.language)
        ? 'This custom speech profile does not declare support for the selected language.'
        : null
      : draft.language === 'hi' && draft.models.asr === 'nvidia/parakeet-tdt-0.6b-v3'
        ? 'Parakeet does not support Hindi. Select Nemotron to save this language.'
        : draft.models.asr === 'LiquidAI/LFM2.5-Audio-1.5B' && draft.language !== 'en'
          ? 'LFM2.5 Audio requires English.'
          : draft.models.asr === 'CohereLabs/cohere-transcribe-03-2026' &&
              draft.language !== 'en' &&
              draft.language !== 'fr'
            ? 'Cohere requires an explicit language. Select English or French.'
            : null,
  );
  const asrOptions = $derived(optionsWithDownloads('asr'));
  const asrKnown = $derived(knownDownloadSelection(draft, engineSettings, 'asr'));
  const cleanupOptions = $derived(optionsWithDownloads('rewriter'));
  const cleanupKnown = $derived(knownDownloadSelection(draft, engineSettings, 'rewriter'));
  const precisionOptions = $derived(
    CLEANUP_QUANTIZATION_OPTIONS.map((option) => ({
      ...option,
      status: downloadOptionStatus(
        { ...draft, rewriteQuantization: option.value },
        engineSettings,
        'rewriter',
        downloads,
        environment === 'desktop',
      ),
    })),
  );
  const asrPreset = $derived(presetForProfile(draft.models.customAsr));
  const cleanupPreset = $derived(presetForProfile(draft.models.customRewriter));
  const languageOptions = $derived(transcriptionLanguageOptions(draft.models));
  const languageHint = $derived(
    languageOptions.some((option) => option.value === 'auto')
      ? 'Use Auto when speaking more than one language.'
      : 'Choose the language you’ll speak.',
  );

  function modelChoiceContext(role: ModelProfileRole) {
    // Keep the saved imported profile available while browsing built-in choices.
    return role === 'asr'
      ? {
          ...draft.models,
          customAsr: draft.models.customAsr ?? engineSettings?.models.customAsr ?? null,
        }
      : {
          ...draft.models,
          customRewriter:
            draft.models.customRewriter ?? engineSettings?.models.customRewriter ?? null,
        };
  }
  function optionsWithDownloads(role: ModelProfileRole) {
    const choices = modelChoiceContext(role);
    return modelOptions(choices, role).map((option) => {
      const config = { ...draft, models: selectModel(choices, role, option.value) };
      return {
        ...option,
        status: downloadOptionStatus(
          config,
          engineSettings,
          role,
          downloads,
          environment === 'desktop',
          role === 'rewriter',
        ),
      };
    });
  }
  function message(cause: unknown): string {
    return cause instanceof Error ? cause.message : String(cause);
  }
  function navigate(view: View) {
    activeView = view;
    navigationRevision += 1;
    window.scrollTo(0, 0);
    if (view === 'models' && !deviceRequested) void detectDevice();
  }
  async function detectDevice() {
    if (environment !== 'desktop' || detectingDevice || quickWindow) return;
    deviceRequested = true;
    detectingDevice = true;
    deviceDetectionError = null;
    const timeout = setTimeout(() => {
      if (!disposed)
        deviceDetectionError =
          'Device detection is taking longer than expected. Automatic and CPU remain available.';
    }, 5000);
    try {
      const result = await getAcceleratorCapabilities();
      if (!disposed) {
        capabilities = result;
        deviceDetectionError = null;
      }
    } catch (cause) {
      if (!disposed) deviceDetectionError = `Could not detect accelerators. ${message(cause)}`;
    } finally {
      clearTimeout(timeout);
      if (!disposed) detectingDevice = false;
    }
  }
  function acceptSettings(settings: AppConfig) {
    // Settings can be a Svelte proxy when browser-preview save returns the
    // current draft. The serialized form is also the persisted contract.
    const serialized = JSON.stringify(settings);
    engineSettings = JSON.parse(serialized) as AppConfig;
    shortcutRetryError = null;
    savedJson = serialized;
    draft = JSON.parse(savedJson) as AppConfig;
    savedCollectionEnabled = settings.collectTrainingData;
    savedRecordingLimit = effectiveRecordingSeconds(settings);
    loaded = true;
    if (!deviceRequested && settings.computeBackend !== 'auto' && settings.computeBackend !== 'cpu')
      void detectDevice();
  }
  async function refreshStatus() {
    if (refreshing) return;
    refreshing = true;
    statusError = null;
    try {
      snapshot = await getAppStatus();
    } catch (cause) {
      statusError = message(cause);
    } finally {
      refreshing = false;
    }
  }
  async function loadSettings() {
    pending = 'loading';
    error = null;
    try {
      acceptSettings(await getSettings());
    } catch (cause) {
      error = `Could not load settings. ${message(cause)}`;
    } finally {
      pending = null;
    }
    await refreshStatus();
  }
  async function retryShortcut() {
    if (pending || pipelineBusy) return;
    pending = 'retrying';
    shortcutRetryError = null;
    try {
      await retryGlobalShortcut();
    } catch (cause) {
      shortcutRetryError = message(cause);
    } finally {
      pending = null;
    }
    await refreshStatus();
  }
  async function save() {
    if (pending || pipelineBusy || !loaded || validationErrors.length || !dirty) return false;
    pending = 'saving';
    error = null;
    feedback = null;
    try {
      const saved = await saveSettings(draft);
      acceptSettings(saved);
      feedback =
        environment === 'browser-preview'
          ? 'Preview settings saved in this browser.'
          : 'Settings saved on this device.';
      if (
        environment === 'desktop' &&
        saved.notificationsEnabled &&
        !(await ensureNotificationPermission())
      ) {
        feedback = 'Settings saved. Notifications are blocked in system settings.';
      }
    } catch (cause) {
      error = `Could not save settings. ${message(cause)}`;
    } finally {
      pending = null;
    }
    await refreshStatus();
    return error === null;
  }
  async function downloadSelection(role: ModelProfileRole) {
    await downloadPackage(downloadTarget(draft, role));
  }
  async function downloadPackage(target: DownloadTarget) {
    if (
      environment !== 'desktop' ||
      pending ||
      pipelineBusy ||
      downloadRequest ||
      validationErrors.length ||
      !transcriptionPanel
    )
      return;
    downloadRequest = target.modelId;
    transcriptionPanel.clearDownloadFeedback();
    try {
      if (dirty && !(await save())) return;
      await tick();
      await transcriptionPanel.downloadModel(target);
    } finally {
      downloadRequest = null;
    }
  }
  async function reset() {
    if (pending || pipelineBusy) return;
    pending = 'resetting';
    error = null;
    feedback = null;
    try {
      acceptSettings(await resetSettings());
      feedback =
        environment === 'browser-preview'
          ? 'Defaults restored and saved in this browser.'
          : 'Defaults restored and saved on this device.';
    } catch (cause) {
      error = `Could not reset settings. ${message(cause)}`;
    } finally {
      pending = null;
    }
    await refreshStatus();
  }
  function markEdited() {
    feedback = null;
    error = null;
  }
  onMount(() => {
    if (quickWindow) return;
    void loadSettings();
    const removePressFeedback = installPressFeedback();
    let unlistenTranscript: (() => void) | undefined;
    let unlistenMenu: (() => void) | undefined;
    if (environment === 'desktop') {
      void listen('show-transcript', () => {
        navigate('dictate');
        void transcriptionPanel?.revealLatestResult();
      }).then((unlisten) => {
        if (disposed) unlisten();
        else unlistenTranscript = unlisten;
      });
      const followMenuNavigation = async () => {
        try {
          const view = await invoke<string | null>('take_menu_navigation');
          if (!disposed && view === 'about') {
            aboutSheet?.open();
            return;
          }
          if (!disposed && views.some((candidate) => candidate.id === view)) navigate(view as View);
        } catch (cause) {
          if (!disposed) statusError = `Could not open the requested screen. ${message(cause)}`;
        }
      };
      void getCurrentWindow()
        .listen('menu-navigate', () => {
          void followMenuNavigation();
        })
        .then((unlisten) => {
          if (disposed) unlisten();
          else {
            unlistenMenu = unlisten;
            void followMenuNavigation();
          }
        });
    }
    return () => {
      disposed = true;
      removePressFeedback();
      unlistenTranscript?.();
      unlistenMenu?.();
    };
  });
</script>

{#if quickWindow}
  <QuickCapture preview={environment === 'browser-preview'} />
{:else}
  <div class="app-shell">
    <aside class="sidebar">
      <button
        type="button"
        class="brand"
        aria-label="Vesper — dictate"
        onclick={() => navigate('dictate')}><BrandLogo decorative /></button
      >
      <nav aria-label="Application">
        {#each views as view (view.id)}
          <button
            type="button"
            aria-current={activeView === view.id ? 'page' : undefined}
            onclick={() => navigate(view.id)}
            ><Icon
              name={view.icon}
              active={activeView === view.id}
              motionKey={activeView === view.id ? navigationRevision : 0}
            /><span>{view.label}</span
            >{#if dirty && (view.id === 'settings' || view.id === 'models')}<span
                class="draft-dot"
                aria-label="Unsaved settings"
              ></span>{/if}</button
          >
        {/each}
      </nav>
      <div class="sidebar-foot">
        {#if pipelineBusy}<button
            type="button"
            class="activity-link"
            onclick={() => navigate('dictate')}
            ><span class="status-dot"></span>Operation in progress</button
          >{/if}
        <span class="environment-tag"
          >{environment === 'browser-preview' ? 'Browser preview' : 'On your device'}</span
        >
      </div>
    </aside>
    <main
      class="studio-main"
      data-view={activeView}
      onpointerdowncapture={finishEntranceAnimations}
      onclickcapture={finishEntranceAnimations}
      onfocusincapture={finishEntranceAnimations}
      onpointerovercapture={finishEntranceAnimations}
    >
      {#key activeView}<header class="page-header">
          <h1>{page.title}</h1>
          <p class="intro">{page.description}</p>
          {#if environment === 'browser-preview'}
            <p class="preview-note">Preview only. Record audio in the desktop app.</p>
          {/if}
        </header>{/key}

      {#if shortcutError}
        <div class="validation-errors" role="alert">
          <p>Recording shortcut needs attention. {shortcutError}</p>
          <p>You can still use the Record button.</p>
          <div class="form-actions">
            <button
              type="button"
              onclick={retryShortcut}
              disabled={pending !== null || pipelineBusy}
              >{pending === 'retrying' ? 'Retrying…' : 'Retry saved shortcut'}</button
            >
            <button type="button" onclick={() => navigate('settings')}>Open Settings</button>
          </div>
        </div>
      {/if}

      <div
        class="page-model-summary"
        hidden={activeView !== 'settings' && activeView !== 'history'}
      >
        <ConfiguredModels settings={engineSettings} onManage={() => navigate('models')} />
      </div>
      <ApplicationStatus
        {environment}
        visible={activeView === 'models'}
        settings={engineSettings}
        catalog={downloads.catalog}
        activity={engineActivity}
        {refreshing}
        {statusError}
        onRefresh={async () => {
          await Promise.all([refreshStatus(), transcriptionPanel?.refreshResources()]);
        }}
      />
      {#if activeView === 'models'}
        <div class="model-download-feedback" aria-live="polite">
          {#if downloads.installing}<div class="download-progress">
              <DownloadProgress
                progress={downloads.details}
                cancelling={downloads.cancelling}
              /><button
                type="button"
                disabled={downloads.cancelling}
                onclick={() => {
                  void transcriptionPanel?.cancelDownload();
                }}>{downloads.cancelling ? 'Cancelling…' : 'Cancel download'}</button
              >
            </div>{/if}
          {#if error}<p class="error" role="alert">{error}</p>{/if}
          {#if downloads.error}<p class="error" role="alert">{downloads.error}</p>{/if}
          {#if downloads.feedback}<p class="success">{downloads.feedback}</p>{/if}
        </div>
      {/if}
      <form
        class="settings-form"
        onsubmit={(event) => {
          event.preventDefault();
          void save();
        }}
        oninput={markEdited}
        onchange={markEdited}
        aria-busy={pending !== null}
        novalidate
      >
        {#if loaded}
          <fieldset disabled={pending !== null || pipelineBusy || downloadRequest !== null}>
            <legend class="visually-hidden">Application settings</legend>
            <section
              class="transcription-language"
              hidden={activeView !== 'dictate'}
              aria-labelledby="dictate-language-heading"
            >
              <div class="transcription-language-copy">
                <h2 id="dictate-language-heading">Transcription language</h2>
                <p id="dictate-language-help" class="note">{languageHint}</p>
              </div>
              <div class="transcription-language-control">
                <label for="dictate-language">Language mode</label>
                <Select
                  id="dictate-language"
                  bind:value={draft.language}
                  options={languageOptions}
                  disabled={pending !== null || pipelineBusy}
                  invalid={languageError !== null}
                  describedBy={languageError ? 'dictate-language-error' : 'dictate-language-help'}
                />
                {#if languageError}<p id="dictate-language-error" class="error">
                    {languageError}
                  </p>{/if}
              </div>
            </section>
            <section
              class="settings-section"
              hidden={activeView !== 'settings'}
              aria-labelledby="speech-heading"
            >
              <h2 id="speech-heading">Speech recognition</h2>
              <div class="field">
                <label for="settings-language">Transcription language</label>
                <Select
                  id="settings-language"
                  bind:value={draft.language}
                  options={languageOptions}
                  disabled={pending !== null || pipelineBusy}
                  invalid={languageError !== null}
                  describedBy={languageError ? 'settings-language-error' : 'settings-language-help'}
                />
                {#if languageError}<p id="settings-language-error" class="error">
                    {languageError}
                  </p>{:else}<p id="settings-language-help" class="note">{languageHint}</p>{/if}
              </div>
            </section>
            <section
              class="settings-section"
              hidden={activeView !== 'settings'}
              aria-labelledby="cleanup-heading"
            >
              <h2 id="cleanup-heading">Writing preferences</h2>
              <div class="setting-toggle">
                <label class="checkbox-field" for="cleanup-enabled"
                  ><input
                    id="cleanup-enabled"
                    type="checkbox"
                    bind:checked={draft.cleanupEnabled}
                    aria-describedby="cleanup-enabled-help"
                  /><span>Use the writing model to polish transcripts</span></label
                >
                <p id="cleanup-enabled-help" class="note checkbox-help">
                  Turn this off to skip writing-model edits. Confirmed vocabulary corrections can
                  still apply.
                </p>
              </div>
              <fieldset disabled={!draft.cleanupEnabled} class="cleanup-options">
                <legend class="visually-hidden">Writing preferences</legend>
                <div class="field">
                  <label for="cleanup-preset">Writing style</label>
                  <Select
                    id="cleanup-preset"
                    bind:value={draft.cleanupPreset}
                    options={[
                      {
                        value: 'minimal',
                        label: 'Formatting only',
                        description: 'Punctuation and capitalization without word changes',
                      },
                      {
                        value: 'balanced',
                        label: 'Context correction',
                        description: 'Correct recognition errors and improve phrasing',
                      },
                    ]}
                    disabled={pending !== null || pipelineBusy || !draft.cleanupEnabled}
                    describedBy="cleanup-help"
                  />
                  <p id="cleanup-help" class="note">
                    Formatting only keeps your words. Context correction can change wording. The
                    original transcript stays available.
                  </p>
                </div>
                <details>
                  <summary>Advanced writing instructions</summary>
                  <div class="field">
                    <label for="cleanup-instruction">Custom writing instruction</label>
                    <textarea
                      id="cleanup-instruction"
                      rows="4"
                      bind:value={draft.customCleanupInstruction}
                      aria-describedby="instruction-help"
                      aria-invalid={[...draft.customCleanupInstruction].length > 4000}
                      placeholder="For example: Use short sentences and retain technical terms."
                    ></textarea>
                    <p id="instruction-help" class="note">
                      Replaces the selected preset; leave blank to use the preset. {[
                        ...draft.customCleanupInstruction,
                      ].length.toLocaleString()} / 4,000 characters.
                    </p>
                  </div>
                </details>
              </fieldset>
            </section>
            <section
              class="settings-section"
              hidden={activeView !== 'settings'}
              aria-labelledby="quick-capture-heading"
            >
              <h2 id="quick-capture-heading">Quick capture</h2>
              <DictationPreferences
                bind:settings={draft}
                desktop={environment === 'desktop'}
                disabled={pending !== null || pipelineBusy}
              />
              <div class="setting-toggle">
                <label class="checkbox-field" for="floating-bar"
                  ><input
                    id="floating-bar"
                    type="checkbox"
                    bind:checked={draft.floatingBarEnabled}
                    aria-describedby="floating-bar-help"
                  /><span>Show the floating capture strip</span></label
                >
                <p id="floating-bar-help" class="note checkbox-help">
                  The recording shortcut works even when the strip is hidden.
                </p>
              </div>
              <CaptureDesignPicker
                bind:value={draft.captureDesign}
                visible={activeView === 'settings'}
                disabled={pending !== null || pipelineBusy || !draft.floatingBarEnabled}
              />
              <div class="setting-toggle">
                <label class="checkbox-field" for="notifications"
                  ><input
                    id="notifications"
                    type="checkbox"
                    bind:checked={draft.notificationsEnabled}
                    aria-describedby="notifications-help"
                  /><span>Notify when transcription finishes or fails</span></label
                >
                <p id="notifications-help" class="note checkbox-help">
                  Notifications never include transcript text. Your system may ask for permission
                  when you save.
                </p>
              </div>
            </section>
            <section
              class="settings-section model-choice"
              hidden={activeView !== 'models'}
              aria-labelledby="asr-choice-heading"
            >
              <div class="model-role-heading">
                <Icon name="mic" />
                <div>
                  <h2 id="asr-choice-heading">{MODEL_ROLE_LABELS.asr}</h2>
                  <p class="note">Turns your recording into a transcript.</p>
                </div>
              </div>
              <div class="field">
                <label class="visually-hidden" for="models-asr-model">{MODEL_ROLE_LABELS.asr}</label
                >
                <Select
                  id="models-asr-model"
                  identity
                  label={MODEL_ROLE_LABELS.asr}
                  value={selectedModelValue(draft.models, 'asr')}
                  options={asrOptions}
                  disabled={pending !== null || pipelineBusy}
                  describedBy="models-asr-help"
                  onChange={(value) => {
                    draft.models = selectModel(modelChoiceContext('asr'), 'asr', value);
                    markEdited();
                  }}
                />
                <p id="models-asr-help" class="note">
                  {#if asrPreset}{asrPreset.description}.{:else if draft.models.customAsr}Languages: {draft.models.customAsr.languages?.join(
                      ', ',
                    ) || 'Auto'}.{:else if draft.models.asr === 'LiquidAI/LFM2.5-Audio-1.5B'}English
                    only. Transcribes after recording stops; longer recordings are processed in
                    segments.{:else if requiresCompletionReport(draft.models)}{draft.models.asr ===
                    'CohereLabs/cohere-transcribe-03-2026'
                      ? 'Choose English or French as your transcription language.'
                      : 'Supports Auto, English, French and Hindi.'}{:else if draft.models.asr === 'nvidia/parakeet-tdt-0.6b-v3'}Supports
                    European languages, including English and French. Hindi is unavailable.{:else}Supports
                    multiple languages, including English, French and Hindi.{/if}
                </p>
                {#if languageError}<p class="error">
                    {languageError}
                    <button type="button" class="inline-link" onclick={() => navigate('dictate')}
                      >Change transcription language</button
                    >
                  </p>{/if}
              </div>
              <ModelDownload
                target={asrDownload}
                knownSelection={asrKnown}
                onRemove={(model) => {
                  void removalDialog?.confirmRemoval(model);
                }}
                state={downloads}
                desktop={environment === 'desktop'}
                {dirty}
                disabled={pending !== null || pipelineBusy || downloadRequest !== null}
                downloadDisabled={validationErrors.length > 0}
                fallbackBytes={draft.models.customAsr?.files.reduce(
                  (sum, file) => sum + file.sizeBytes,
                  0,
                )}
                onDownload={() => {
                  void downloadSelection('asr');
                }}
              />
            </section>
            <section
              class="settings-section model-choice"
              hidden={activeView !== 'models'}
              aria-labelledby="cleanup-choice-heading"
            >
              <div class="model-role-heading">
                <Icon name="models" />
                <div>
                  <h2 id="cleanup-choice-heading">{MODEL_ROLE_LABELS.rewriter}</h2>
                  <p class="note">Polishes the transcript while preserving its meaning.</p>
                </div>
              </div>
              <div class="field">
                <label class="visually-hidden" for="models-cleanup-model"
                  >{MODEL_ROLE_LABELS.rewriter}</label
                >
                <Select
                  id="models-cleanup-model"
                  identity
                  label={MODEL_ROLE_LABELS.rewriter}
                  value={selectedModelValue(draft.models, 'rewriter')}
                  options={cleanupOptions}
                  disabled={pending !== null || pipelineBusy}
                  onChange={(value) => {
                    draft.models = selectModel(modelChoiceContext('rewriter'), 'rewriter', value);
                    markEdited();
                  }}
                />
              </div>
              <div class="field">
                <label for="models-cleanup-precision">Model precision</label>
                {#if draft.models.customRewriter}
                  <input
                    id="models-cleanup-precision"
                    value={cleanupPreset?.precision ?? 'Included with this model'}
                    readonly
                    aria-describedby="models-cleanup-precision-help"
                  />
                {:else}
                  <Select
                    id="models-cleanup-precision"
                    bind:value={draft.rewriteQuantization}
                    options={precisionOptions}
                    disabled={pending !== null || pipelineBusy}
                    describedBy="models-cleanup-precision-help"
                  />
                {/if}
                <p id="models-cleanup-precision-help" class="note">
                  {draft.models.customRewriter
                    ? 'Precision is fixed for this download.'
                    : 'Q4 uses less memory and disk space. Q8 offers higher precision. Each is a separate download.'}
                </p>
              </div>
              <ModelDownload
                target={cleanupDownload}
                knownSelection={cleanupKnown}
                onRemove={(model) => {
                  void removalDialog?.confirmRemoval(model);
                }}
                state={downloads}
                desktop={environment === 'desktop'}
                {dirty}
                disabled={pending !== null || pipelineBusy || downloadRequest !== null}
                downloadDisabled={validationErrors.length > 0}
                fallbackBytes={draft.models.customRewriter?.files.reduce(
                  (sum, file) => sum + file.sizeBytes,
                  0,
                )}
                onDownload={() => {
                  void downloadSelection('rewriter');
                }}
              />
              <p class="note">
                The writing model is {draft.cleanupEnabled ? 'enabled' : 'off'}.
                <button type="button" class="inline-link" onclick={() => navigate('settings')}
                  >Adjust writing preferences <Icon name="arrow" /></button
                >
              </p>
            </section>
            <section
              class="settings-section"
              hidden={activeView !== 'models'}
              aria-labelledby="audio-processing-heading"
            >
              <h2 id="audio-processing-heading">
                Audio processing <span class="note">Experimental</span>
              </h2>
              <div class="setting-toggle">
                <label class="checkbox-field" for="models-auto-level"
                  ><input
                    id="models-auto-level"
                    type="checkbox"
                    bind:checked={draft.audioFrontend.autoLevelEnabled}
                    disabled={pending !== null || pipelineBusy}
                    aria-describedby="models-auto-level-help"
                  /><span>Automatically adjust quiet speech</span></label
                >
                <p id="models-auto-level-help" class="note checkbox-help">
                  Adjusts quiet speech with noise and volume limits, for live and completed
                  recordings. Uses the speech detection model. Very quiet words may still be missed.
                </p>
              </div>
              <div class="setting-toggle">
                <label class="checkbox-field" for="models-vad"
                  ><input
                    id="models-vad"
                    type="checkbox"
                    bind:checked={draft.audioFrontend.vadEnabled}
                    disabled={pending !== null || pipelineBusy}
                    aria-describedby="models-vad-help"
                  /><span>Detect speech activity</span></label
                >
                <p id="models-vad-help" class="note checkbox-help">
                  Adds speech timing annotations. All recorded audio is retained for transcription.
                </p>
              </div>
              <div class="field">
                <label for="models-noise-reduction">Noise reduction</label>
                <Select
                  id="models-noise-reduction"
                  bind:value={draft.audioFrontend.noiseReduction}
                  options={[
                    { value: 'off', label: 'Off' },
                    { value: 'dpdfnet4', label: 'DPDFNet4 · 12 MB' },
                    { value: 'dpdfnet8', label: 'DPDFNet8 · 15 MB' },
                  ]}
                  disabled={pending !== null || pipelineBusy}
                  describedBy="models-noise-help"
                />
                <p id="models-noise-help" class="note">
                  Applies to live transcription and completed recordings. Noise reduction can reduce
                  transcription accuracy, especially for quiet speech.
                </p>
              </div>
              <details class="advanced-settings">
                <summary>Audio model downloads</summary>
                {#each Object.entries(AUDIO_FRONTENDS) as [modelId, name] (modelId)}
                  <p>{name}</p>
                  <ModelDownload
                    target={{ modelId, name, role: 'audio_frontend', quantization: null }}
                    knownSelection={draft.modelDirectory === engineSettings?.modelDirectory}
                    onRemove={(model) => {
                      void removalDialog?.confirmRemoval(model);
                    }}
                    state={downloads}
                    desktop={environment === 'desktop'}
                    {dirty}
                    disabled={pending !== null || pipelineBusy || downloadRequest !== null}
                    downloadDisabled={validationErrors.length > 0}
                    onDownload={() => {
                      void downloadPackage({
                        modelId,
                        name,
                        role: 'audio_frontend',
                        quantization: null,
                      });
                    }}
                  />
                {/each}
              </details>
            </section>
            <section
              class="settings-section"
              hidden={activeView !== 'models'}
              aria-labelledby="model-device-heading"
            >
              <h2 id="model-device-heading">Device and storage</h2>
              <div class="field">
                <label for="compute-backend">Processing device</label>
                <Select
                  id="compute-backend"
                  bind:value={draft.computeBackend}
                  options={backendOptions}
                  disabled={pending !== null || pipelineBusy}
                  describedBy="compute-device-help compute-backend-help"
                  invalid={Boolean(backendError)}
                />
                <div id="compute-device-help" class="note device-capabilities" role="status">
                  {#if environment !== 'desktop'}Device detection is available in the desktop app.
                  {:else if capabilities}<span>{deviceLabel(capabilities)}</span><span
                      >{[...new Set(capabilities.backends.flatMap((entry) => entry.devices))].join(
                        ' · ',
                      ) || 'No supported GPU detected.'}</span
                    >
                  {:else if detectingDevice}<span>Detecting devices…</span>{/if}
                  {#if deviceDetectionError}<span>{deviceDetectionError}</span
                    >{#if !detectingDevice}<button
                        type="button"
                        class="quiet"
                        onclick={() => {
                          void detectDevice();
                        }}>Retry device detection</button
                      >{/if}{/if}
                  {#if backendError}<span class="error"
                      >{backendError} Your saved choice has not been changed.</span
                    >{/if}
                </div>
                <p id="compute-backend-help" class="note">
                  Automatic chooses a supported device and can fall back to CPU. {#if draft.models.customAsr}Speech
                    uses CPU; this preference applies to the writing model.{:else if requiresCompletionReport(draft.models) || draft.models.asr === 'LiquidAI/LFM2.5-Audio-1.5B' || draft.models.asr === 'nvidia/parakeet-tdt-0.6b-v3'}This
                    speech model supports Automatic and CPU.{:else if draft.computeBackend === 'rocm'}Speech
                    uses Vulkan; writing uses ROCm.{/if}
                </p>
              </div>
              <div class="field">
                <label for="model-directory">Model folder</label>
                <input
                  id="model-directory"
                  type="text"
                  value={draft.modelDirectory ?? ''}
                  oninput={(event) => {
                    const path = event.currentTarget.value;
                    draft.modelDirectory = path.trim() ? path : null;
                  }}
                  placeholder="Use the application’s default folder"
                  aria-describedby="directory-help"
                  spellcheck="false"
                />
                <p id="directory-help" class="note">
                  Enter a folder path, or leave empty to use the default. Saving this path does not
                  move or download models.
                </p>
              </div>
            </section>
            <section
              class="settings-section"
              hidden={activeView !== 'settings'}
              aria-labelledby="storage-heading"
            >
              <h2 id="storage-heading">Storage and improvement</h2>
              <div class="setting-toggle">
                <label class="checkbox-field" for="collect-training"
                  ><input
                    id="collect-training"
                    type="checkbox"
                    bind:checked={draft.collectTrainingData}
                    aria-describedby="collect-training-help"
                  /><span>Save recordings and transcripts</span></label
                >
                <p id="collect-training-help" class="note checkbox-help">
                  Saved on this device for review and export from History.
                </p>
              </div>
              <div class="setting-toggle">
                <label class="checkbox-field" for="text-edit-observation"
                  ><input
                    id="text-edit-observation"
                    type="checkbox"
                    bind:checked={draft.textEditObservation}
                    aria-describedby="text-edit-observation-help"
                  /><span>Evaluate corrections in TextEdit</span></label
                >
                <p id="text-edit-observation-help" class="note checkbox-help">
                  Optional on Mac. After automatic paste, observe that text and up to 32 nearby
                  characters on each side for up to one minute. Stores the inserted and edited text
                  locally for evaluation; does not teach your dictionary yet. Requires TextEdit in
                  plain-text mode with English menus. Forget learned spellings also deletes this
                  data.
                </p>
              </div>
              <div class="field improvement-field">
                <label for="improvement-mode">Improvement policy</label>
                <Select
                  id="improvement-mode"
                  bind:value={draft.improvementMode}
                  options={[
                    {
                      value: 'review_required',
                      label: 'Review before activation',
                      description: 'Approve evaluated model versions',
                    },
                    {
                      value: 'automatic',
                      label: 'Automatic activation',
                      description: 'Use evaluated versions automatically',
                    },
                  ]}
                  disabled={pending !== null || pipelineBusy}
                  describedBy="improvement-help"
                />
                <p id="improvement-help" class="note">
                  Model training is not available yet. This preference has no effect today.
                </p>
              </div>
            </section>
          </fieldset>
        {:else if pending === 'loading'}<p class="loading" role="status">
            Loading your settings…
          </p>{/if}
        <div
          class="savebar"
          hidden={!dirty &&
            !error &&
            loaded &&
            activeView !== 'settings' &&
            activeView !== 'models'}
        >
          <div class="form-feedback" aria-live="polite">
            {#if error && activeView !== 'models'}<p class="error" role="alert">{error}</p>{/if}
            {#if loaded && validationErrors.length}
              <div class="validation-errors" role="alert">
                <p>Fix these settings before saving:</p>
                <ul>
                  {#each validationErrors as issue, index (index)}<li>{issue}</li>{/each}
                </ul>
              </div>
            {:else if feedback}<p class="success">{feedback}</p>
            {:else if dirty}<p class="note">You have unsaved changes.</p>
            {:else if loaded}<p class="note">No unsaved changes.</p>{/if}
          </div>
          <div class="form-actions">
            {#if loaded}<button
                class="primary"
                type="submit"
                disabled={pending !== null || pipelineBusy || !dirty || validationErrors.length > 0}
                >{pending === 'saving' ? 'Saving…' : 'Save changes'}</button
              >
            {:else}<button
                type="button"
                onclick={loadSettings}
                disabled={pending !== null || pipelineBusy}>Try loading again</button
              >{/if}
            <button type="button" onclick={reset} disabled={pending !== null || pipelineBusy}
              >{pending === 'resetting' ? 'Resetting…' : 'Restore defaults'}</button
            >
          </div>
        </div>
      </form>
      {#if loaded}
        <PersonalizationPanel
          desktop={environment === 'desktop'}
          visible={activeView === 'settings'}
          settingsRevision={savedJson}
          disabled={pending !== null || pipelineBusy || downloadRequest !== null}
        />
      {/if}
      <section class="settings-about" hidden={activeView !== 'settings'} aria-label="About">
        <button
          type="button"
          class="settings-section settings-about-row"
          aria-haspopup="dialog"
          onclick={() => aboutSheet?.open()}
        >
          <span>About Vesper</span>
          <Icon name="chevron" />
        </button>
      </section>
      <TranscriptionPanel
        onRuntimeChange={(runtime) => {
          engineActivity = runtime;
        }}
        bind:this={transcriptionPanel}
        onDownloadState={(state) => {
          downloads = state;
        }}
        {environment}
        view={activeView}
        onNavigate={navigate}
        settingsLoaded={loaded}
        settingsDirty={dirty}
        settingsBusy={pending !== null}
        settingsRevision={savedJson}
        cleanupEnabled={savedCleanupEnabled}
        modelSettings={engineSettings}
        maxRecordingSeconds={savedRecordingLimit}
        onBusyChange={(busy) => {
          pipelineBusy = busy;
        }}
        onResult={(result) => {
          if (result.session) datasetRevision += 1;
          void refreshStatus();
        }}
      />

      <ModelRemovalDialog
        bind:this={removalDialog}
        disabled={pending !== null || pipelineBusy || downloadRequest !== null}
        onRemove={async (model) => {
          if (!transcriptionPanel) throw new Error('Model controls are still loading.');
          return transcriptionPanel.removeDownloadedModel(model);
        }}
      />

      <DatasetPanel
        {environment}
        view={activeView === 'dictate' ? 'recent' : activeView === 'history' ? 'full' : 'hidden'}
        onOpenHistory={() => navigate('history')}
        collectionEnabled={savedCollectionEnabled}
        settingsLoaded={loaded}
        refreshKey={datasetRevision}
        externalBusy={pipelineBusy}
      />
    </main>
    <AboutSheet
      bind:this={aboutSheet}
      fallbackFocus={() =>
        document
          .querySelector<HTMLElement>('.sidebar nav button[aria-current="page"]')
          ?.focus({ preventScroll: true })}
    />
  </div>
{/if}
