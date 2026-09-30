// Isolated visual fixtures: no microphone, filesystem, clipboard or model access.
import { mount } from 'svelte';
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import App from '../src/App.svelte';
import InteractionStates from './InteractionStates.svelte';
import { ASR_MODELS, defaultSettings } from '../src/lib/config';
import { downloadTarget } from '../src/lib/modelDownloads';
import '../src/app.css';

const states = new URLSearchParams(location.search).has('states');
if (states) {
  mount(InteractionStates, { target: document.querySelector('#app') });
} else {
  window.isTauri = true;
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: {
      writeText: async (text) => {
        window.previewClipboard = text;
      },
    },
  });
  mockWindows('main');
  let settings = defaultSettings();
  const catalog = [
    ...Object.entries(ASR_MODELS).map(([modelId, name]) => ({
      modelId,
      name,
      role: 'speech_recognition',
      quantization: null,
    })),
    downloadTarget(settings, 'rewriter'),
    { ...downloadTarget(settings, 'rewriter'), quantization: 'q8_0' },
  ].map((target, index) => ({
    ...target,
    ready: index !== 2,
    supported: true,
    artifactsInstalled: index !== 2 && target.quantization !== 'q8_0',
    runtimeReady: true,
    selected: target.modelId === settings.models.asr || target.modelId === settings.models.rewriter,
    files: [],
    modelDirectory: 'visual-fixture',
    localFileCount: index === 2 || target.quantization === 'q8_0' ? 0 : 1,
    downloadedBytes: index === 2 || target.quantization === 'q8_0' ? 0 : 2 ** 29,
    removableBytes: 2 ** 29,
    removalError: null,
    message: 'Synthetic model installation',
  }));
  let profile = {
    schemaVersion: 1,
    revision: 0,
    automaticLearning: false,
    speechBiasEnabled: false,
    outputIntent: 'verbatim',
    preserveFillers: false,
    applyCorrections: true,
    useRememberedIntent: false,
    activeProject: '',
    projectMode: 'manual',
    projectRules: [],
    vocabulary: [],
    examples: [],
  };
  const runtime = {
    busy: false,
    stage: 'idle',
    message: '',
    modelDirectory: 'visual-fixture',
    modelsReady: true,
    audioLevel: 0,
  };
  const samples = [
    'Plan the next prototype review.',
    'The waveform should follow my voice.',
    'Make the little interactions feel consistent.',
  ];
  const records = Array.from({ length: 14 }, (_, i) => ({
    id: `visual-${i}`,
    createdAt: new Date(Date.now() - i * 15_000_000).toISOString(),
    originalFilename: `sample-${i + 1}.wav`,
    audioRelativePath: 'synthetic.wav',
    audioSha256: '',
    audioBytes: 64000 + i * 8000,
    rawTranscript: samples[i % samples.length],
    cleanedTranscript: null,
    correctedTranscript: null,
    verifiedTranscript: i % 4 === 0 ? samples[i % samples.length] : null,
    reviewHistory: [],
    reviewStatus: i % 4 === 0 ? 'approved' : 'pending',
    transcriptFeedback: null,
    configSnapshot: settings,
    modelVersions: {},
  }));
  window.previewCommands = [];
  mockIPC(
    async (command, args) => {
      window.previewCommands.push(command);
      switch (command) {
        case 'take_menu_navigation':
        case 'get_latest_result':
          return null;
        case 'get_settings':
          return structuredClone(settings);
        case 'save_settings':
          await new Promise((r) => setTimeout(r, 450));
          settings = JSON.parse(JSON.stringify(args.settings));
          return settings;
        case 'reset_settings':
          settings = defaultSettings();
          return settings;
        case 'get_runtime_status':
          return { ...runtime };
        case 'get_app_status':
          return {
            name: 'Vesper',
            version: 'visual fixture',
            inference_ready: true,
            asr_model: 'Cohere Transcribe',
            rewrite_model: 'Qwen3.5 0.8B',
            message: '',
            shortcut_error: null,
          };
        case 'get_app_info':
          return {
            name: 'Vesper',
            version: '0.1.0',
            buildType: 'development',
            os: 'macos',
            arch: 'aarch64',
          };
        case 'get_engine_diagnostics':
          return {
            resources: {
              sampledAtMs: Date.now(),
              cpuPercent: 3,
              memoryBytes: 268435456,
              logicalCpus: 8,
              status: 'available',
            },
            activity: runtime,
            timings: null,
          };
        case 'get_model_installation':
          return {
            ready: true,
            supported: true,
            modelDirectory: 'visual-fixture',
            files: [],
            message: 'Synthetic model installation',
          };
        case 'list_model_installations':
          return structuredClone(catalog);
        case 'list_sessions':
          return structuredClone(records);
        case 'set_transcript_feedback': {
          const record = records.find((r) => r.id === args.id);
          record.transcriptFeedback = args.feedback;
          return structuredClone(record);
        }
        case 'plugin:dialog|open':
          if (window.previewDialogCancelled) return null;
          return args.options.directory ? '/Projects/Synthetic' : '/Applications/TextEdit.app';
        case 'read_project_application':
          return { id: 'com.apple.TextEdit', name: 'TextEdit' };
        case 'resolve_project_folder':
          return args.path;
        case 'get_personalization':
          return structuredClone(profile);
        case 'save_personalization':
          await new Promise((r) => setTimeout(r, 450));
          profile = JSON.parse(JSON.stringify(args.profile));
          return profile;
        case 'get_automatic_learning':
          return {
            schemaVersion: 1,
            policyVersion: 1,
            expiredCandidates: 0,
            candidates: [],
            events: [],
          };
        case 'get_selected_adapters':
          return { speech: { capabilities: { contextualPhrases: false, nativeStreaming: false } } };
        case 'get_correction_suggestions':
          return [];
        case 'get_learning_dashboard':
          return {
            profileRevision: 0,
            inspectedSessions: 14,
            reviewedSessions: 4,
            appliedSessions: 0,
            appliedOccurrences: 0,
            undoneSessions: 0,
            replayExcluded: 0,
            replay: [],
            reviewQueue: [],
          };
        case 'list_audio_devices':
          return [{ id: 'fixture', name: 'Synthetic microphone', isDefault: true }];
        case 'get_accelerator_capabilities':
          return {
            os: 'macos',
            arch: 'aarch64',
            backends: [{ backend: 'metal', compiled: true, devices: ['Synthetic Apple GPU'] }],
          };
        case 'dictation_permissions':
          return { supported: true, accessibility: false, eventPosting: false, clipboard: false };
        default:
          throw new Error(`Visual fixture does not run ${command}.`);
      }
    },
    { shouldMockEvents: true },
  );
  mount(App, { target: document.querySelector('#app') });
  const label = document.createElement('span');
  label.textContent = 'Synthetic UI preview';
  label.style.cssText =
    'position:fixed;bottom:8px;right:12px;font:11px var(--font-body);color:var(--color-muted);background:var(--color-panel);padding:4px 8px;border-radius:var(--radius-sm);pointer-events:none;z-index:1200';
  document.body.append(label);
}
