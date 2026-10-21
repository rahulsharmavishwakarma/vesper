// Isolated visual fixtures: no microphone, filesystem, clipboard or model access.
function mockWavDataUrl() {
  const sampleRate = 8000;
  const count = Math.floor(sampleRate * 0.5);
  const buffer = new ArrayBuffer(44 + count * 2);
  const view = new DataView(buffer);
  const ascii = (offset, text) => {
    for (let index = 0; index < text.length; index++)
      view.setUint8(offset + index, text.charCodeAt(index));
  };
  ascii(0, 'RIFF');
  view.setUint32(4, 36 + count * 2, true);
  ascii(8, 'WAVE');
  ascii(12, 'fmt ');
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  ascii(36, 'data');
  view.setUint32(40, count * 2, true);
  for (let index = 0; index < count; index++)
    view.setInt16(44 + index * 2, Math.round(Math.sin((index / count) * Math.PI * 8) * 8000), true);
  const bytes = new Uint8Array(buffer);
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return `data:audio/wav;base64,${btoa(binary)}`;
}
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
  if (new URLSearchParams(location.search).get('paste') === '1') settings.dictationOutput = 'paste';
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
  // Long-paragraph record: the polished editor must grow to fit it (no mid-line clip).
  records.push({
    id: 'visual-long',
    createdAt: new Date().toISOString(),
    originalFilename: 'sample-long.wav',
    audioRelativePath: 'synthetic.wav',
    audioSha256: '',
    audioBytes: 512000,
    rawTranscript:
      'Sunlit paragraphs stretch across the afternoon while the transcriber keeps pace with every clause. ' +
      'Also, there is a very large kind of issue where see here I am transcribing very large paragraph kind of stuff. ' +
      'You can see like I am saying anything, etc. But here you can see the right side window is not in sync with the left side. ' +
      'That is the size is not properly in sync. So can you check and see and fix this also? ' +
      'That is a bug actually in UI. So can you check and fix it? The quick brown fox jumps over the lazy dog near the riverbank ' +
      'while packing boxes with five dozen liquor jugs for the evening show. Every word here should remain fully visible ' +
      'in both the Heard pane and the Polished editor, no matter how long the paragraph grows.',
    cleanedTranscript: null,
    correctedTranscript: null,
    verifiedTranscript: null,
    reviewHistory: [],
    reviewStatus: 'pending',
    transcriptFeedback: null,
    configSnapshot: settings,
    modelVersions: {},
  });
  window.previewCommands = [];
  window.previewPermissions = null;
  mockIPC(
    async (command, args) => {
      window.previewCommands.push(command);
      switch (command) {
        case 'take_menu_navigation':
        case 'get_latest_result':
          return null;
        case 'get_settings':
          return structuredClone(settings);
        case 'install_models':
        case 'install_model': {
          // A controllable download: it holds the backend's single operation
          // slot until the test calls window.previewFinishDownload().
          Object.assign(runtime, {
            busy: true,
            stage: 'installing',
            operationId: 'preview-download',
            message: 'Downloading',
            download: {
              operationId: 'preview-download',
              modelId: args?.modelId ?? null,
              quantization: null,
              progress: {
                phase: 'downloading',
                fileName: 'model.gguf',
                fileIndex: 1,
                fileCount: 1,
                completedBytes: 250,
                totalBytes: 1000,
              },
            },
          });
          await new Promise((resolve) => {
            window.previewFinishDownload = resolve;
          });
          Object.assign(runtime, { busy: false, stage: 'idle', operationId: null, download: null });
          return args?.modelId ? structuredClone(catalog[0]) : { ready: true, message: '' };
        }
        case 'save_settings':
          // Mirrors the backend: while a download runs, settings save unless
          // they change which model files are needed.
          if (
            runtime.busy &&
            JSON.stringify([
              args.settings.models,
              args.settings.rewriteQuantization,
              args.settings.computeBackend,
            ]) !==
              JSON.stringify([
                settings.models,
                settings.rewriteQuantization,
                settings.computeBackend,
              ])
          )
            throw 'A model is downloading. Wait for it to finish before changing models or the processing device.';
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
        case 'approve_like': {
          const record = records.find((r) => r.id === args.id);
          if (record.rawTranscript === args.polishedText)
            record.verifiedTranscript = args.polishedText;
          else record.correctedTranscript = args.polishedText;
          record.transcriptFeedback = 'like';
          record.reviewStatus = 'approved';
          record.reviewHistory.push({
            targetKind:
              record.rawTranscript === args.polishedText
                ? 'verbatim_transcription'
                : 'cleanup_correction',
            text: args.polishedText,
            timestamp: new Date().toISOString(),
          });
          return structuredClone(record);
        }
        case 'approve_correction_and_learn': {
          const record = records.find((r) => r.id === args.id);
          record.correctedTranscript = args.correctedText;
          record.reviewStatus = 'approved';
          record.reviewHistory.push({
            targetKind: 'cleanup_correction',
            text: args.correctedText,
            timestamp: new Date().toISOString(),
          });
          return {
            record: structuredClone(record),
            learningOutcome: {
              activated: [],
              provisional: 0,
              suspended: 0,
              status: 'Learned 1 spelling correction from the preview fixture.',
              profileRevision: 0,
              elapsedMicros: 0,
            },
          };
        }
        case 'retract_approval': {
          const record = records.find((r) => r.id === args.id);
          record.verifiedTranscript = null;
          record.correctedTranscript = null;
          record.transcriptFeedback = null;
          record.reviewStatus = 'pending';
          return structuredClone(record);
        }
        case 'session_audio_data_url':
          return mockWavDataUrl();
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
          return (
            window.previewPermissions ?? {
              supported: true,
              accessibility: false,
              eventPosting: false,
              clipboard: false,
            }
          );
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
