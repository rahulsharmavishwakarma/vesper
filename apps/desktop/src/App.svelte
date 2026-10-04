<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { listen } from '@tauri-apps/api/event';
  import { invoke } from '@tauri-apps/api/core';
  import { Autosaver } from './lib/autosave.svelte';
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
    MODEL_ROLE_LABELS,
    CLEANUP_QUANTIZATION_OPTIONS,
    defaultSettings,
    effectiveRecordingSeconds,
    requiresCompletionReport,
    pipelineLanguageOptions,
    transcriptionLanguageOptions,
    transcriptionLanguageError,
    writingLanguageError,
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
  import QuickCapture from './lib/components/QuickCapture.svelte';
  import CaptureDesignPicker from './lib/components/CaptureDesignPicker.svelte';
  import DictationPreferences from './lib/components/DictationPreferences.svelte';
  import ApplicationStatus from './lib/components/ApplicationStatus.svelte';
  import AboutSheet from './lib/components/AboutSheet.svelte';
  import AppearanceSettings from './lib/components/AppearanceSettings.svelte';
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
  let savedRecordingLimit = $state(0);
  type View = 'dictate' | 'history' | 'models' | 'settings';
  let activeView = $state<View>(window.location.hash === '#settings' ? 'settings' : 'dictate');
  let navigationRevision = $state(0);
  const settingsSections = [
    { id: 'recording', label: 'Recording' },
    { id: 'writing', label: 'Writing' },
    { id: 'shortcuts', label: 'Shortcuts' },
    { id: 'appearance', label: 'Appearance' },
    { id: 'about', label: 'About' },
  ] as const;
  type SettingsSection = (typeof settingsSections)[number]['id'];
  let settingsSection = $state<SettingsSection>('recording');
  function showSettingsSection(section: SettingsSection) {
    settingsSection = section;
    const top = document.querySelector('.settings-layout')?.getBoundingClientRect().top ?? 0;
    if (top < 0) window.scrollBy(0, top - 16);
  }
  // Debounced autosave: every draft change schedules a write 800ms out.
  // Invalid edits never reach the backend (validationErrors stay on screen
  // until fixed), and the pill below reflects saver + vocabulary-panel state.
  const settingsSaver = new Autosaver(saveNow, 800);
  // The sidebar pill is a transient toast: hidden until an edit arrives,
  // Saving… while a write (or silent retry) is in flight, Saved with the
  // clock time once it lands, then it slides away on its own.
  const PILL_DISMISS_MS = 2500;
  let pillVisible = $state(false);
  let pillTimer: ReturnType<typeof setTimeout> | undefined;
  let wasSaving = false;
  const pillSlide = (node: HTMLElement) =>
    slide(node, {
      duration: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : 220,
      easing: cubicOut,
    });
  $effect(() => {
    const saving = saveInFlight;
    if (saving) {
      clearTimeout(pillTimer);
      pillTimer = undefined;
      pillVisible = true;
    } else if (wasSaving) {
      pillVisible = true;
      clearTimeout(pillTimer);
      pillTimer = setTimeout(() => {
        pillVisible = false;
      }, PILL_DISMISS_MS);
    }
    wasSaving = saving;
  });
  onDestroy(() => {
    clearTimeout(pillTimer);
    settingsSaver.dispose();
  });
  let panelSaving = $state(false);
  let panelSavedAt = $state<number | null>(null);
  const saveInFlight = $derived(settingsSaver.status === 'saving' || panelSaving);
  const lastSavedAt = $derived(Math.max(settingsSaver.savedAt ?? 0, panelSavedAt ?? 0) || null);
  const savedClockTime = $derived(
    lastSavedAt
      ? new Date(lastSavedAt).toLocaleTimeString([], {
          hour: '2-digit',
          minute: '2-digit',
          second: '2-digit',
        })
      : null,
  );

  $effect(() => {
    const snapshot = JSON.stringify(draft);
    const writable = loaded && pending === null && !pipelineBusy && validationErrors.length === 0;
    if (writable && snapshot !== savedJson) settingsSaver.notify();
  });
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
      title: 'Make yourself at home.',
      description: 'Your voice, your preferences.',
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
  // The notice only renders for actionable states; idle settings show nothing.
  const hasNotice = $derived(
    !loaded ||
      (error !== null && activeView !== 'models') ||
      (loaded && validationErrors.length > 0) ||
      feedback !== null ||
      saveInFlight,
  );
  const writerError = $derived(writingLanguageError(draft));
  const languageError = $derived(transcriptionLanguageError(draft) ?? writerError);
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
  const languageOptions = $derived(pipelineLanguageOptions(draft));
  const languageHint = $derived(
    languageOptions.some((option) => option.value === 'auto')
      ? 'Automatic detection lets the speech model identify the language.'
      : 'Choose a language supported by the speech model and, when enabled, the writing model.',
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
    settingsSaver.noteSaved();
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
  // Write-through used by the debounced autosaver. Returns true when the
  // draft matches what is stored (or was just stored); false keeps the
  // status pill on "Saving" and schedules a silent retry.
  async function saveNow(): Promise<boolean> {
    if (pending || pipelineBusy || !loaded || validationErrors.length) return false;
    if (JSON.stringify(draft) === savedJson) return true;
    try {
      acceptSettings(await saveSettings(draft));
    } catch {
      return false;
    }
    await refreshStatus();
    return true;
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
      if (dirty && !(await settingsSaver.flush())) return;
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
  let previousCapturePreferences = '';
  $effect(() => {
    const current = JSON.stringify([draft.floatingBarEnabled, draft.captureDesign]);
    if (previousCapturePreferences && previousCapturePreferences !== current && dirty) {
      feedback = null;
      error = null;
    }
    previousCapturePreferences = current;
  });
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
  <div class="app-shell" class:settings-rail={activeView === 'settings'}>
    <aside class="sidebar">
      <button
        type="button"
        class="brand"
        aria-label="Vesper — dictate"
        onclick={() => navigate('dictate')}
        ><BrandLogo decorative mark={activeView === 'settings'} /></button
      >
      <nav aria-label="Application">
        {#each views as view (view.id)}
          <button
            type="button"
            aria-current={activeView === view.id ? 'page' : undefined}
            title={view.label}
            onclick={() => navigate(view.id)}
            ><Icon
              name={view.icon}
              active={activeView === view.id}
              motionKey={activeView === view.id ? navigationRevision : 0}
            /><span class:visually-hidden={activeView === 'settings'}>{view.label}</span
            >{#if saveInFlight && (view.id === 'settings' || view.id === 'models')}<span
                class="draft-dot"
                aria-label="Saving settings"
              ></span>{/if}</button
          >
        {/each}
      </nav>
      <div class="sidebar-foot">
        {#if pipelineBusy}<button
            type="button"
            class="activity-link"
            aria-label="Operation in progress"
            onclick={() => navigate('dictate')}
            ><span class="status-dot"></span><span class:visually-hidden={activeView === 'settings'}
              >Operation in progress</span
            ></button
          >{/if}
        <span class="environment-tag" hidden={activeView === 'settings'}
          >{environment === 'browser-preview' ? 'Browser preview' : 'On your device'}</span
        >
        {#if pillVisible}<p
            class="save-status"
            role="status"
            aria-label={saveInFlight ? 'Saving settings' : 'Settings saved'}
            data-state={saveInFlight ? 'saving' : 'saved'}
            transition:pillSlide
          >
            {#if saveInFlight}
              <span class="spinning" aria-hidden="true"><Icon name="refresh" /></span><span
                class="save-status-text">Saving…</span
              >
            {:else}
              <Icon name="check" /><span class="save-status-text"
                >Saved{savedClockTime ? ` ${savedClockTime}` : ''}</span
              >
            {/if}
          </p>{/if}
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

      <div class="page-model-summary" hidden={activeView !== 'history'}>
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
      {#if activeView === 'models' || downloads.installing || downloads.audioSetupPaused}
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
          {#if downloads.audioSetupPaused && !downloads.installing}
            <p class="note">Audio setup will retry automatically when the app is idle.</p>
          {/if}
        </div>
      {/if}
      <div class:settings-layout={activeView === 'settings'}>
        <nav class="settings-nav" aria-label="Settings sections" hidden={activeView !== 'settings'}>
          {#each settingsSections as section (section.id)}
            <button
              type="button"
              aria-current={settingsSection === section.id ? 'page' : undefined}
              aria-controls={`settings-${section.id}-panel`}
              onclick={() => showSettingsSection(section.id)}>{section.label}</button
            >
          {/each}
        </nav>
        <div class:settings-content={activeView === 'settings'}>
          <form
            class="settings-form"
            id="application-settings"
            hidden={activeView === 'history' ||
              (activeView === 'settings' &&
                (settingsSection === 'about' || settingsSection === 'appearance') &&
                !dirty &&
                !error &&
                loaded)}
            onsubmit={(event) => {
              event.preventDefault();
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
                  class="settings-section"
                  hidden={activeView !== 'settings' || settingsSection !== 'recording'}
                  id="settings-recording-panel"
                  aria-labelledby="speech-heading"
                >
                  <h2 id="speech-heading">Recording</h2>
                  <p class="settings-description">Choose how you capture and keep your words.</p>
                  <div class="field">
                    <label for="settings-language">Transcription language</label>
                    <Select
                      id="settings-language"
                      bind:value={draft.language}
                      options={languageOptions}
                      disabled={pending !== null || pipelineBusy}
                      invalid={languageError !== null}
                      describedBy={languageError
                        ? 'settings-language-error'
                        : 'settings-language-help'}
                    />
                    {#if languageError}<p id="settings-language-error" class="error">
                        {languageError}
                      </p>{:else}<p id="settings-language-help" class="note">{languageHint}</p>{/if}
                  </div>
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
                      Notifications never include transcript text. Your system may ask for
                      permission when you save.
                    </p>
                  </div>
                </section>
                <section
                  class="settings-section"
                  hidden={activeView !== 'settings' || settingsSection !== 'writing'}
                  id="settings-writing-panel"
                  aria-labelledby="cleanup-heading"
                >
                  <h2 id="cleanup-heading">Writing</h2>
                  <p class="settings-description">Polish your words while keeping their meaning.</p>
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
                      Turn this off to skip writing-model edits. Confirmed vocabulary corrections
                      can still apply.
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
                  hidden={activeView !== 'settings' || settingsSection !== 'shortcuts'}
                  id="settings-shortcuts-panel"
                  aria-labelledby="quick-capture-heading"
                >
                  <h2 id="quick-capture-heading">Shortcuts</h2>
                  <p class="settings-description">
                    Click a field, then press your key combination.
                  </p>
                  <DictationPreferences
                    bind:settings={draft}
                    desktop={environment === 'desktop'}
                    disabled={pending !== null || pipelineBusy}
                  />
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
                    <label class="visually-hidden" for="models-asr-model"
                      >{MODEL_ROLE_LABELS.asr}</label
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
                      {#if asrPreset}{asrPreset.description}.{/if}
                      Available language modes: {transcriptionLanguageOptions(
                        draft.models,
                        draft.computeBackend,
                      )
                        .map((option) => option.label)
                        .join(', ')}.
                    </p>
                    {#if languageError}<p class="error">
                        {languageError}
                        <button
                          type="button"
                          class="inline-link"
                          onclick={() => navigate('dictate')}>Change transcription language</button
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
                      invalid={writerError !== null}
                      describedBy={writerError
                        ? 'models-writer-language-error'
                        : 'models-writer-language-help'}
                      label={MODEL_ROLE_LABELS.rewriter}
                      value={selectedModelValue(draft.models, 'rewriter')}
                      options={cleanupOptions}
                      disabled={pending !== null || pipelineBusy}
                      onChange={(value) => {
                        draft.models = selectModel(
                          modelChoiceContext('rewriter'),
                          'rewriter',
                          value,
                        );
                        markEdited();
                      }}
                    />
                  </div>
                  <p id="models-writer-language-help" class="note">
                    Writing languages: {draft.models.customRewriter
                      ? draft.models.customRewriter.languages?.join(', ') || 'Not declared'
                      : 'English, French and Hindi'}.
                  </p>
                  {#if writerError}<p id="models-writer-language-error" class="error">
                      {writerError}
                    </p>{/if}
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
                      {#if environment !== 'desktop'}Device detection is available in the desktop
                        app.
                      {:else if capabilities}<span>{deviceLabel(capabilities)}</span><span
                          >{[
                            ...new Set(capabilities.backends.flatMap((entry) => entry.devices)),
                          ].join(' · ') || 'No supported GPU detected.'}</span
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
                </section>
                <section
                  class="settings-section"
                  hidden={activeView !== 'settings' || settingsSection !== 'recording'}
                  aria-labelledby="storage-heading"
                >
                  <h3 id="storage-heading">Storage and improvement</h3>
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
                  <details class="settings-advanced">
                    <summary>Advanced storage and improvement</summary>
                    <div class="field improvement-field">
                      <p id="improvement-help" class="note">
                        Spelling learning runs automatically from eligible edits to Your text.
                        History reviews need your confirmation. Automatic model training and
                        activation are not available.
                      </p>
                    </div>
                    <details class="settings-defaults">
                      <summary>Restore default preferences</summary>
                      <p class="note">
                        Restore and save the application's default settings. Your recordings and
                        vocabulary stay.
                      </p>
                      <button
                        type="button"
                        onclick={reset}
                        disabled={pending !== null || pipelineBusy}
                      >
                        {pending === 'resetting' ? 'Restoring…' : 'Restore defaults'}
                      </button>
                    </details>
                  </details>
                  <div class="settings-model-summary">
                    <ConfiguredModels
                      settings={engineSettings}
                      onManage={() => navigate('models')}
                    />
                  </div>
                </section>
              </fieldset>
            {:else if pending === 'loading'}<p class="loading" role="status">
                Loading your settings…
              </p>{/if}
          </form>
          <section
            class="settings-section settings-appearance"
            id="settings-appearance-panel"
            hidden={activeView !== 'settings' || settingsSection !== 'appearance'}
            aria-labelledby="appearance-heading"
          >
            <h2 id="appearance-heading">Appearance</h2>
            <p class="settings-description">Make Vesper feel at home on your screen.</p>
            <AppearanceSettings />
            <div class="capture-appearance">
              <h3>Floating capture</h3>
              <fieldset
                disabled={!loaded || pending !== null || pipelineBusy || downloadRequest !== null}
              >
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
                  visible={activeView === 'settings' && settingsSection === 'appearance'}
                  disabled={pending !== null || pipelineBusy || !draft.floatingBarEnabled}
                />
              </fieldset>
              <p class="note">Capture preferences save automatically.</p>
            </div>
          </section>
          <div
            id="settings-about-panel"
            hidden={activeView !== 'settings' || settingsSection !== 'about'}
          >
            <AboutSheet inline visible={activeView === 'settings' && settingsSection === 'about'} />
          </div>
          <div
            class="settings-notice"
            hidden={activeView === 'history' ||
              (activeView === 'settings' &&
                ['appearance', 'about'].includes(settingsSection) &&
                !dirty &&
                !error &&
                loaded) ||
              (!dirty &&
                !error &&
                loaded &&
                activeView !== 'settings' &&
                activeView !== 'models') ||
              !hasNotice}
          >
            <div class="form-feedback" aria-live="polite">
              {#if error && activeView !== 'models'}<p class="error" role="alert">{error}</p>{/if}
              {#if loaded && validationErrors.length}
                <div class="validation-errors" role="alert">
                  <p>Fix these settings before they can save:</p>
                  <ul>
                    {#each validationErrors as issue, index (index)}<li>{issue}</li>{/each}
                  </ul>
                </div>
              {:else if feedback}<p class="success">{feedback}</p>
              {:else if saveInFlight}<p class="note">Saving…</p>{/if}
            </div>
            {#if !loaded}<div class="form-actions">
                <button
                  type="button"
                  onclick={loadSettings}
                  disabled={pending !== null || pipelineBusy}>Try loading again</button
                >
                <button type="button" onclick={reset} disabled={pending !== null || pipelineBusy}>
                  {pending === 'resetting' ? 'Restoring…' : 'Restore defaults'}
                </button>
              </div>{/if}
          </div>
          {#if loaded}
            <PersonalizationPanel
              desktop={environment === 'desktop'}
              visible={activeView === 'settings' && settingsSection === 'writing'}
              disabled={pending !== null || pipelineBusy || downloadRequest !== null}
              onSaveActivity={(saving, at) => {
                panelSaving = saving;
                panelSavedAt = at;
              }}
            />
          {/if}
        </div>
      </div>
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
        downloadRequested={downloadRequest !== null}
        modelSettings={engineSettings}
        maxRecordingSeconds={savedRecordingLimit}
        transcriptionLanguage={draft.language}
        {languageOptions}
        {languageError}
        {languageHint}
        onTranscriptionLanguage={(value) => {
          draft.language = value;
          markEdited();
        }}
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
