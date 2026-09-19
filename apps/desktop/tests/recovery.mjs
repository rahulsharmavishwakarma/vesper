// Run through Vite at /tests/recovery.html. All native commands are mocked;
// this page never opens a microphone, loads a model, or reads saved user data.
import { mount, unmount, tick } from 'svelte';
import { mockIPC, mockWindows, clearMocks } from '@tauri-apps/api/mocks';
import { emit } from '@tauri-apps/api/event';
import QuickCapture from '../src/lib/components/QuickCapture.svelte';
import App from '../src/App.svelte';
import { ASR_MODELS, CAPTURE_DESIGNS, defaultSettings } from '../src/lib/config';
import { MODEL_PRESETS } from '../src/lib/modelPresets';
import '../src/app.css';

const target = document.querySelector('#app');
const report = document.querySelector('#report');
const results = [];
const unexpected = [];
let component;
window.isTauri = true;
window.addEventListener('unhandledrejection', (event) => unexpected.push(String(event.reason)));

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function deferred() {
  let resolve;
  const promise = new Promise((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

async function until(predicate, message) {
  const deadline = performance.now() + 4000;
  while (!predicate()) {
    if (performance.now() > deadline) throw new Error(message);
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  await tick();
}

const idle = {
  busy: false,
  stage: 'idle',
  message: '',
  modelDirectory: 'synthetic-models',
  modelsReady: true,
  audioLevel: 0,
};
const emptyDictation = {
  id: 0,
  revision: 0,
  phase: 'idle',
  mode: 'toggle',
  operationId: null,
  message: '',
  text: null,
  startedAtUnixMs: null,
  captureReadyMs: null,
  stopToFinalMs: null,
  finalToDispatchMs: null,
};
const installation = {
  ready: true,
  supported: true,
  modelDirectory: 'synthetic-models',
  files: [],
  message: '',
};

async function setup({ recording = false, preview = false, main = false, intercept } = {}) {
  const state = {
    runtime: { ...idle, ...(recording ? { busy: true, stage: 'recording' } : {}) },
    dictation: {
      ...emptyDictation,
      ...(recording ? { id: 1, phase: 'recording', startedAtUnixMs: Date.now() } : {}),
    },
    counts: {},
  };
  mockWindows(main ? 'main' : 'quick-capture');
  mockIPC(
    async (command, args) => {
      state.counts[command] = (state.counts[command] ?? 0) + 1;
      const handled = intercept?.(command, state, args);
      if (handled !== undefined) return handled;
      switch (command) {
        case 'take_menu_navigation':
          return null;
        case 'begin_shortcut_recording':
          return null;
        case 'end_shortcut_recording':
          return null;
        case 'get_dictation_status':
          return { ...state.dictation };
        case 'dictation_permissions':
          return { supported: true, accessibility: false, eventPosting: false, clipboard: false };
        case 'toggle_dictation':
          if (state.runtime.stage === 'recording') {
            state.runtime = { ...idle };
            state.dictation = {
              ...state.dictation,
              phase: 'ready',
              text: 'Synthetic final text.',
              message: 'Your transcript is ready to copy.',
            };
          } else {
            state.runtime = { ...idle, busy: true, stage: 'recording' };
            state.dictation = {
              ...emptyDictation,
              id: state.dictation.id + 1,
              phase: 'recording',
              startedAtUnixMs: Date.now(),
            };
          }
          return null;
        case 'cancel_dictation':
          state.runtime = { ...idle, stage: 'cancelled' };
          state.dictation = { ...state.dictation, phase: 'cancelled' };
          return null;
        case 'hide_dictation':
          return null;
        case 'start_capture_drag':
          return null;
        case 'copy_dictation':
          state.copiedId = args.id;
          return null;
        case 'get_audio_level':
          return 0;
        case 'get_audio_features':
          return { level: 0, pitchHz: null };
        case 'get_accelerator_capabilities':
          return {
            os: 'macos',
            arch: 'aarch64',
            backends: [{ backend: 'metal', compiled: true, devices: ['Synthetic Apple GPU'] }],
          };
        case 'get_engine_diagnostics':
          return {
            resources: {
              sampledAtMs: Date.now(),
              cpuPercent: null,
              memoryBytes: null,
              logicalCpus: 8,
              status: 'unavailable',
            },
            activity: { ...state.runtime },
            timings: null,
          };
        case 'get_settings':
          return defaultSettings();
        case 'get_automatic_learning':
          return {
            schemaVersion: 1,
            policyVersion: 1,
            expiredCandidates: 0,
            candidates: [],
            events: [],
          };
        case 'get_personalization':
          return {
            schemaVersion: 1,
            revision: 0,
            automaticLearning: true,
            speechBiasEnabled: false,
            outputIntent: 'verbatim',
            preserveFillers: false,
            applyCorrections: true,
            useRememberedIntent: false,
            activeProject: '',
            vocabulary: [],
            examples: [],
          };
        case 'get_learning_dashboard':
          return {
            profileRevision: 0,
            inspectedSessions: 0,
            reviewedSessions: 0,
            appliedSessions: 0,
            appliedOccurrences: 0,
            undoneSessions: 0,
            replayExcluded: 0,
            replay: [],
            reviewQueue: [],
          };
        case 'get_correction_suggestions':
          return [];
        case 'get_selected_adapters':
          return { speech: { capabilities: { contextualPhrases: false, nativeStreaming: false } } };
        case 'get_runtime_status':
          return { ...state.runtime };
        case 'get_model_installation':
          return installation;
        case 'list_model_installations':
        case 'list_sessions':
          return [];
        case 'get_latest_result':
          return null;
        case 'list_audio_devices':
          return [{ id: 'synthetic', name: 'Synthetic microphone', isDefault: true }];
        case 'start_recording':
          state.runtime = { ...idle, busy: true, stage: 'recording' };
          return null;
        case 'stop_recording':
          state.runtime = { ...idle };
          return {
            resultId: 'synthetic',
            rawTranscript: 'Synthetic text.',
            cleanedTranscript: null,
            session: null,
            warnings: [],
            durationSeconds: 1,
          };
        case 'get_app_status':
          return {
            name: 'transcribe',
            version: 'test',
            inference_ready: true,
            asr_model: 'Synthetic ASR',
            rewrite_model: 'Synthetic cleanup',
            message: '',
            shortcut_error: null,
          };
        case 'plugin:window|hide':
          return null;
        default:
          unexpected.push(command);
          throw new Error(`Unexpected native command: ${command}`);
      }
    },
    { shouldMockEvents: true },
  );
  component = mount(main ? App : QuickCapture, { target, props: main ? {} : { preview } });
  await tick();
  if (!main && !preview)
    await until(() => state.counts.get_runtime_status === 1, 'Initial runtime status did not load');
  if (recording && !main)
    await until(() => target.querySelector('.is-recording'), 'Initial recording did not render');
  return state;
}

async function test(name, run) {
  const filter = new URLSearchParams(location.search).get('test');
  if (filter && !name.includes(filter)) return;
  document.documentElement.dataset.currentTest = name;
  try {
    await run();
    results.push({ name, passed: true });
  } catch (error) {
    results.push({ name, passed: false, error: String(error) });
  } finally {
    if (component) await unmount(component);
    component = undefined;
    clearMocks();
    target.replaceChildren();
    report.textContent = JSON.stringify(results, null, 2);
  }
}

await test('Native shortcut state drives the overlay without WebView capture commands', async () => {
  const state = await setup();
  state.runtime = { ...idle, busy: true, stage: 'starting' };
  state.dictation = { ...state.dictation, id: 8, phase: 'starting', mode: 'hold' };
  await emit('dictation-state', { ...state.dictation });
  await until(() => target.textContent.includes('Preparing microphone'), 'Startup state missing');
  assert(!target.querySelector('.is-recording'), 'Loading a model was labeled Listening');
  state.runtime = {
    ...idle,
    busy: true,
    stage: 'recording',
    operationId: 'live-8',
    partial: {
      operationId: 'live-8',
      sequence: 1,
      committedText: '',
      provisionalText: 'Words while speaking',
      isFinal: false,
    },
  };
  state.dictation = {
    ...state.dictation,
    phase: 'recording',
    operationId: 'live-8',
    startedAtUnixMs: Date.now(),
  };
  await emit('dictation-state', { ...state.dictation });
  await until(
    () => target.textContent.includes('Words while speaking'),
    'Live words were not shown',
  );
  assert(
    !state.counts.start_recording && !state.counts.stop_recording,
    'WebView attempted to own recording',
  );
});

await test('Repeated button clicks cannot overlap a pending native request', async () => {
  const request = deferred();
  const state = await setup({
    intercept: (command, state) =>
      command === 'toggle_dictation'
        ? request.promise.then(() => {
            state.runtime = { ...idle, busy: true, stage: 'recording' };
            state.dictation = {
              ...state.dictation,
              id: 2,
              phase: 'recording',
              startedAtUnixMs: Date.now(),
            };
          })
        : undefined,
  });
  for (let i = 0; i < 6; i++) target.querySelector('.record-toggle').click();
  await tick();
  assert(state.counts.toggle_dictation === 1, 'Button sent overlapping requests');
  request.resolve();
  await until(() => target.querySelector('.is-recording'), 'Recording state missing after request');
});

await test('Native request error releases the button for retry', async () => {
  const state = await setup({
    intercept: (command, state) => {
      if (command === 'toggle_dictation' && state.counts.toggle_dictation === 1)
        return Promise.reject(new Error('Synthetic request failure'));
    },
  });
  target.querySelector('.record-toggle').click();
  await until(
    () => target.textContent.includes('Synthetic request failure'),
    'Request error missing',
  );
  assert(!target.querySelector('.record-toggle').disabled, 'Retry remained disabled');
  target.querySelector('.record-toggle').click();
  await until(() => target.querySelector('.is-recording'), 'Retry did not start recording');
  assert(state.counts.toggle_dictation === 2, 'Retry sent unexpected number of requests');
});

await test('Preview start and stop complete without native commands', async () => {
  const state = await setup({ preview: true });
  target.querySelector('.record-toggle').click();
  await tick();
  assert(target.querySelector('.is-recording'), 'Preview did not start');
  target.querySelector('.record-toggle').click();
  await until(() => target.querySelector('.is-complete'), 'Preview did not finish');
  assert(Object.keys(state.counts).length === 0, 'Preview called a native command');
});

await test('Quick-capture completion copies the final operation without starting another recording', async () => {
  const state = await setup({ recording: true });
  target.querySelector('.record-toggle').click();
  await until(() => target.querySelector('[aria-label="Copy transcript"]'), 'Copy action missing');
  target.querySelector('[aria-label="Copy transcript"]').click();
  await until(
    () => target.querySelector('[aria-label="Transcript copied"]'),
    'Copy confirmation missing',
  );
  assert(
    state.counts.copy_dictation === 1 && state.copiedId === state.dictation.id,
    'Copy was not scoped to the final operation',
  );
  assert(
    target.querySelector('.capture-status').textContent === 'Copied to clipboard',
    'Copy confirmation did not replace the existing status',
  );
  assert(
    !target.querySelector('.capture-note, .split-detail'),
    'Copy confirmation added an extra message surface',
  );
  assert(
    !state.counts.start_recording && state.counts.toggle_dictation === 1,
    'Copy started another recording',
  );
});

await test('Cancel requests native cancellation and fallback results remain visible', async () => {
  const state = await setup({ recording: true });
  target.querySelector('[aria-label="Cancel recording"]').click();
  await until(() => target.textContent.includes('Cancelled'), 'Cancel state missing');
  assert(state.counts.cancel_dictation === 1, 'Cancel did not reach native controller');
  state.dictation = {
    ...state.dictation,
    id: 9,
    phase: 'ready',
    text: 'Kept text',
    message: 'The text field changed. Copy your transcript when ready.',
  };
  await emit('dictation-state', { ...state.dictation });
  await until(
    () => target.textContent.includes('The text field changed'),
    'Paste fallback missing',
  );
  assert(
    target.querySelector('[aria-label="Copy transcript"]'),
    'Paste fallback has no Copy action',
  );
  assert(!state.counts.hide_dictation, 'Fallback disappeared before user could copy');
});

await test('Shortcut and output preferences save together and permission reads do not prompt', async () => {
  let saved = defaultSettings();
  let grants = 0;
  const state = await setup({
    main: true,
    intercept: (command, _state, args) => {
      if (command === 'get_settings') return structuredClone(saved);
      if (command === 'save_settings') {
        saved = JSON.parse(JSON.stringify(args.settings));
        return saved;
      }
      if (command === 'dictation_permissions') {
        if (args.request) grants += 1;
        return {
          supported: true,
          accessibility: grants > 0,
          eventPosting: grants > 0,
          clipboard: grants > 0,
        };
      }
    },
  });
  await until(
    () => target.querySelector('.configured-models')?.textContent.includes('Qwen'),
    'App not ready',
  );
  [...target.querySelectorAll('nav button')]
    .find((button) => button.textContent.trim() === 'Settings')
    .click();
  await tick();
  assert(grants === 0, 'Reading settings prompted for access');
  const field = target.querySelector('#hold-shortcut');
  field.click();
  await tick();
  await until(() => field.getAttribute('aria-busy') === 'false', 'Shortcut recorder not ready');
  field.dispatchEvent(
    new KeyboardEvent('keydown', {
      key: 'D',
      code: 'KeyD',
      ctrlKey: !/Mac/.test(navigator.platform),
      metaKey: /Mac/.test(navigator.platform),
      shiftKey: true,
      bubbles: true,
      cancelable: true,
    }),
  );
  field.blur();
  await tick();
  target.querySelector('#dictation-output').click();
  await tick();
  [...document.querySelectorAll('[role="option"]')]
    .find((button) => button.textContent.includes('Paste into the original'))
    .click();
  await until(
    () => target.textContent.includes('Ready to paste'),
    'Explicit paste selection did not request access',
  );
  const save = [...target.querySelectorAll('button')].find(
    (button) => button.textContent.trim() === 'Save changes',
  );
  assert(save && !save.disabled, 'Preferences could not be saved');
  save.click();
  await until(() => state.counts.save_settings === 1, 'Save not called');
  assert(
    saved.holdShortcut === `${/Mac/.test(navigator.platform) ? 'Super' : 'Control'}+Shift+D` &&
      saved.toggleShortcut === 'CommandOrControl+Shift+Space' &&
      saved.dictationOutput === 'paste',
    'Saved preferences did not include both shortcut bindings and output choice',
  );
  assert(grants === 1, 'Permission prompts repeated without a user action');
});

await test('Shortcut recorder captures keys, rejects duplicates, cancels, and recovers from save conflicts', async () => {
  let saved = defaultSettings();
  let registrationBlocked = true;
  let nativeSession;
  const state = await setup({
    main: true,
    intercept: (command, _state, args) => {
      if (command === 'get_settings') return structuredClone(saved);
      if (command === 'begin_shortcut_recording') {
        nativeSession = args.session;
        return null;
      }
      if (command === 'save_settings') {
        if (registrationBlocked)
          throw new Error(
            'Could not register shortcut: shortcut in use. Choose another shortcut in Settings or retry.',
          );
        saved = JSON.parse(JSON.stringify(args.settings));
        return saved;
      }
    },
  });
  await until(() => target.querySelector('.configured-models'), 'App not ready');
  [...target.querySelectorAll('nav button')]
    .find((button) => button.textContent.trim() === 'Settings')
    .click();
  await tick();
  const begin = async (id) => {
    const field = target.querySelector(`#${id}`);
    field.click();
    await tick();
    await until(
      () => field.getAttribute('aria-busy') === 'false' && field.classList.contains('is-recording'),
      'Recorder not ready',
    );
    return field;
  };
  const press = (field, code, modifiers = {}, key = code.replace('Key', '')) =>
    field.dispatchEvent(
      new KeyboardEvent('keydown', { key, code, ...modifiers, bubbles: true, cancelable: true }),
    );
  assert(
    !target.querySelector('.shortcut-editor, .modifier-keys, #hold-shortcut-customize'),
    'Old customization controls remain',
  );
  const hold = await begin('hold-shortcut');
  press(hold, 'KeyK', { shiftKey: true });
  await tick();
  assert(
    target.querySelector('#hold-shortcut-status').textContent.includes('Press Command') ||
      target.querySelector('#hold-shortcut-status').textContent.includes('Press Ctrl'),
    'Unsupported chord not explained',
  );
  press(hold, 'KeyK', { ctrlKey: true, shiftKey: true });
  await tick();
  assert(hold.textContent.includes('Ctrl + Shift + K'), 'Pressed shortcut did not update');
  press(hold, 'Escape');
  await tick();
  assert(hold.textContent.includes('D'), 'Escape did not restore the original shortcut');
  const saveButton = () =>
    [...target.querySelectorAll('button')].find(
      (button) => button.textContent.trim() === 'Save changes',
    );
  assert(saveButton().disabled, 'Escape left a phantom settings change');
  await begin('hold-shortcut');
  press(hold, 'KeyK', { ctrlKey: true, shiftKey: true });
  hold.blur();
  await tick();
  assert(!state.counts.save_settings, 'Capturing saved settings prematurely');
  const toggle = await begin('toggle-shortcut');
  press(toggle, 'KeyK', { ctrlKey: true, shiftKey: true });
  await tick();
  assert(
    target.querySelector('#toggle-shortcut-status').textContent.includes('Already used'),
    'Duplicate action accepted',
  );
  assert(toggle.textContent.includes('Press your shortcut'), 'Duplicate modified the binding');
  await emit('shortcut-recorded', { session: nativeSession, shortcut: 'Control+Shift+K' });
  await tick();
  assert(
    target.querySelector('#toggle-shortcut-status').textContent.includes('Already used'),
    'Native registered binding skipped validation',
  );
  await emit('shortcut-recorded', { session: 'stale-editor', shortcut: 'Alt+F9' });
  await tick();
  assert(!toggle.textContent.includes('F9'), 'Stale native event affected the new editor');
  press(toggle, 'F8', { altKey: true });
  toggle.blur();
  await tick();
  saveButton().click();
  await until(
    () => target.textContent.includes('shortcut in use'),
    'Registration failure not shown',
  );
  assert(
    saved.holdShortcut === null && saved.toggleShortcut === null,
    'Failure replaced saved bindings',
  );
  assert(hold.textContent.includes('Ctrl + Shift + K'), 'Failure discarded custom draft');
  registrationBlocked = false;
  await until(() => !saveButton().disabled, 'Save did not recover');
  saveButton().click();
  await until(() => saved.toggleShortcut === 'Alt+F8', 'Custom pair was not persisted');
  assert(saved.holdShortcut === 'Control+Shift+K', 'Hold binding was not persisted');
  [...target.querySelectorAll('nav button')]
    .find((button) => button.textContent.trim() === 'Dictate')
    .click();
  await tick();
  [...target.querySelectorAll('nav button')]
    .find((button) => button.textContent.trim() === 'Settings')
    .click();
  await tick();
  assert(
    hold.textContent.includes('Ctrl + Shift + K') && toggle.textContent.includes('F8'),
    'Bindings disappeared after navigation',
  );
  assert(
    !state.counts.start_recording && !state.counts.toggle_dictation,
    'Editing started dictation',
  );
});

await test('Shortcut recorder abandons delayed setup and handles native registered combinations', async () => {
  const waiting = deferred();
  let nativeSession;
  let ending;
  const state = await setup({
    main: true,
    intercept: (command, _state, args) => {
      if (command === 'begin_shortcut_recording') {
        nativeSession = args.session;
        return waiting.promise;
      }
      if (command === 'end_shortcut_recording') {
        ending = args.session;
        return null;
      }
    },
  });
  await until(() => target.querySelector('.configured-models'), 'Settings missing');
  [...target.querySelectorAll('nav button')]
    .find((button) => button.textContent.trim() === 'Settings')
    .click();
  await tick();
  const field = target.querySelector('#hold-shortcut');
  field.click();
  await until(() => nativeSession, 'Begin not requested');
  field.blur();
  waiting.resolve(null);
  await until(() => ending === nativeSession, 'Late native setup was not ended');
  assert(!field.classList.contains('is-recording'), 'Late setup reactivated field');
  field.click();
  await tick();
  await until(() => field.getAttribute('aria-busy') === 'false', 'Recorder not ready');
  await emit('shortcut-recorded', { session: nativeSession, shortcut: 'Alt+F7' });
  await tick();
  assert(field.textContent.includes('F7'), 'Native capture was not displayed');
  window.dispatchEvent(new Event('blur'));
  await tick();
  assert(!field.classList.contains('is-recording'), 'Window blur left editor listening');
  await emit('shortcut-recorded', { session: nativeSession, shortcut: 'Alt+F9' });
  await tick();
  assert(!field.textContent.includes('F9'), 'Native event after blur changed binding');
  assert(
    !state.counts.start_recording && !state.counts.toggle_dictation,
    'Native capture started recording',
  );
});

await test('Personal preference drafts survive navigation and save independently with failure and discard recovery', async () => {
  let saved = {
    schemaVersion: 1,
    revision: 2,
    automaticLearning: true,
    speechBiasEnabled: false,
    outputIntent: 'verbatim',
    preserveFillers: false,
    applyCorrections: true,
    useRememberedIntent: false,
    activeProject: '',
    vocabulary: [],
    examples: [],
  };
  let fail = true;
  let waitForSave;
  let supportsHints = false;
  const state = await setup({
    main: true,
    intercept: (command, _state, args) => {
      if (command === 'get_personalization') return structuredClone(saved);
      if (command === 'get_selected_adapters')
        return {
          speech: { capabilities: { contextualPhrases: supportsHints, nativeStreaming: false } },
        };
      if (command === 'save_personalization')
        return (async () => {
          await waitForSave?.promise;
          if (fail) throw new Error('Synthetic save failure');
          saved = { ...JSON.parse(JSON.stringify(args.profile)), revision: saved.revision + 1 };
          return structuredClone(saved);
        })();
    },
  });
  const navigate = async (name) => {
    [...target.querySelectorAll('nav button')]
      .find((button) => button.textContent.trim() === name)
      .click();
    await tick();
  };
  await until(() => target.querySelector('#settings-language'), 'Settings did not load');
  await navigate('Settings');
  await until(() => target.querySelector('#personal-project'), 'Preferences did not load');
  const panel = () => target.querySelector('.personalization');
  const project = () => target.querySelector('#personal-project');
  const save = () => panel().querySelector('button[type="submit"]');
  const discard = () =>
    [...panel().querySelectorAll('button')].find(
      (button) => button.textContent === 'Discard changes',
    );
  assert(save().disabled, 'Unchanged preferences can be saved');
  assert(
    project().form !== target.querySelector('.settings-form'),
    'Personal preferences belong to the application settings form',
  );
  project().value = 'Synthetic project';
  project().dispatchEvent(new Event('input', { bubbles: true }));
  await tick();
  assert(!save().disabled, 'Editing did not enable saving');
  assert(
    target.querySelector('.settings-form button[type="submit"]').disabled,
    'Personal edits marked application settings dirty',
  );
  await navigate('Models');
  supportsHints = true;
  await navigate('Settings');
  assert(project().value === 'Synthetic project', 'Navigation lost the unsaved draft');
  await until(
    () => !target.querySelector('#personal-speech-hints').disabled,
    'Returning to Settings did not refresh model support',
  );
  save().click();
  await until(() => panel().textContent.includes('Synthetic save failure'), 'Save error missing');
  assert(
    project().value === 'Synthetic project' && !save().disabled,
    'Failed save lost edits or prevented retry',
  );
  assert(!state.counts.save_settings, 'Personal save submitted application settings');
  fail = false;
  waitForSave = deferred();
  save().click();
  await tick();
  assert(
    project().matches(':disabled') &&
      target.querySelector('#personal-intent').disabled &&
      discard().disabled,
    'Controls can change during save',
  );
  waitForSave.resolve();
  await until(
    () => panel().textContent.includes('Saved for your next recording.'),
    'Retry did not save',
  );
  assert(
    saved.activeProject === 'Synthetic project' && save().disabled,
    'Saved snapshot not synchronized',
  );
  project().value = 'Unwanted edit';
  project().dispatchEvent(new Event('input', { bubbles: true }));
  await tick();
  discard().click();
  await tick();
  assert(
    project().value === 'Synthetic project' && save().disabled,
    'Discard did not restore the last successful save',
  );
  assert(
    state.counts.get_personalization === 1,
    'Discard or navigation silently reloaded the profile',
  );
  saved.activeProject = 'Updated from History';
  await navigate('History');
  await navigate('Settings');
  await until(
    () => project().value === 'Updated from History',
    'Clean preferences did not refresh changes made elsewhere',
  );
});

await test('Learning draft Copy saves once, blocks repeat clicks, and preserves the draft when learning is undone', async () => {
  let result = {
    resultId: 'learning-copy',
    rawTranscript: 'Open read me in the repo',
    cleanedTranscript: null,
    personalizedTranscript: null,
    draftTranscript: null,
    draftRevision: 0,
    session: null,
    warnings: [],
    durationSeconds: 1,
  };
  const saved = deferred();
  const copied = [];
  const clipboard = Object.getOwnPropertyDescriptor(navigator, 'clipboard');
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { writeText: async (text) => copied.push(text) },
  });
  try {
    const state = await setup({
      main: true,
      intercept: (command, _state, args) => {
        if (command === 'get_latest_result') return structuredClone(result);
        if (command === 'edit_latest_draft')
          return (async () => {
            assert(args.resultId === result.resultId && args.revision === 0, 'Wrong edit identity');
            await saved.promise;
            result = {
              ...result,
              draftRevision: 1,
              draftTranscript: args.text,
              learningOutcome: {
                activated: ['README.md'],
                status: 'Spelling remembered.',
                suspended: 0,
                provisional: 0,
              },
            };
            return structuredClone(result);
          })();
        if (command === 'undo_latest_learning') {
          assert(args.resultId === result.resultId, 'Undo targeted a different result');
          result = {
            ...result,
            learningOutcome: {
              activated: [],
              status: 'Learning undone.',
              suspended: 1,
              provisional: 0,
            },
          };
          return structuredClone(result);
        }
      },
    });
    await until(() => target.querySelector('#final-draft'), 'Draft did not load');
    const field = target.querySelector('#final-draft');
    field.value = 'Open README.md in the repo';
    field.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    field.dispatchEvent(new FocusEvent('blur'));
    const copy = [...target.querySelectorAll('button')].find((b) =>
      b.textContent.includes('Copy text'),
    );
    copy.click();
    await tick();
    const blockedDuringSave = copy.disabled;
    copy.click();
    assert(field.disabled, 'Draft remained editable during save');
    assert(
      state.counts.edit_latest_draft === 1 && copied.length === 0,
      'Copy bypassed the pending save',
    );
    saved.resolve();
    await until(() => copied.length > 0 && !field.disabled, 'Save/Copy did not finish');
    assert(blockedDuringSave, 'Copy remained enabled while waiting for draft save');
    assert(
      copied.length === 1 && copied[0] === field.value,
      'Repeated Copy duplicated or copied stale text',
    );
    const undo = [...target.querySelectorAll('button')].find(
      (b) => b.textContent === 'Undo learning',
    );
    assert(undo, 'Learning undo is absent');
    undo.click();
    await until(() => target.textContent.includes('Learning undone.'), 'Undo did not finish');
    assert(field.value === 'Open README.md in the repo', 'Undo deleted the user draft');
    assert(
      state.counts.edit_latest_draft === 1 && state.counts.undo_latest_learning === 1,
      'Undo relearned the draft',
    );
  } finally {
    saved.resolve();
    if (clipboard) Object.defineProperty(navigator, 'clipboard', clipboard);
    else delete navigator.clipboard;
  }
});

await test('Learning draft save failure retains text and retry does not treat unchanged Copy as feedback', async () => {
  let result = {
    resultId: 'learning-retry',
    rawTranscript: 'Deploy kubernets to the cluster',
    cleanedTranscript: null,
    personalizedTranscript: null,
    draftTranscript: null,
    draftRevision: 0,
    session: null,
    warnings: [],
    durationSeconds: 1,
  };
  let fail = true;
  const state = await setup({
    main: true,
    intercept: (command, _state, args) => {
      if (command === 'get_latest_result') return structuredClone(result);
      if (command === 'edit_latest_draft') {
        assert(args.revision === 0, 'Failed edit advanced the revision');
        if (fail) return Promise.reject(new Error('Synthetic learning transaction failure'));
        result = {
          ...result,
          draftTranscript: args.text,
          draftRevision: 1,
          learningOutcome: {
            activated: [],
            status: 'Waiting for independent support.',
            suspended: 0,
            provisional: 1,
          },
        };
        return structuredClone(result);
      }
    },
  });
  await until(() => target.querySelector('#final-draft'), 'Draft did not load');
  const field = target.querySelector('#final-draft');
  field.value = 'Deploy kubernetes to the cluster';
  field.dispatchEvent(new Event('input', { bubbles: true }));
  await tick();
  const copy = [...target.querySelectorAll('button')].find((b) =>
    b.textContent.includes('Copy text'),
  );
  copy.click();
  await until(
    () => target.textContent.includes('Synthetic learning transaction failure'),
    'Failure was hidden',
  );
  assert(field.value === 'Deploy kubernetes to the cluster', 'Failure lost the user edit');
  assert(!copy.disabled, 'Failed save left Copy locked');
  fail = false;
  [...target.querySelectorAll('button')].find((b) => b.textContent === 'Retry saving edit').click();
  await until(
    () => target.textContent.includes('Waiting for independent support.'),
    'Retry did not finish',
  );
  [...target.querySelectorAll('button')].find((b) => b.textContent.includes('Copy text')).click();
  await until(
    () =>
      target.textContent.includes('copied.') ||
      target.textContent.includes('Automatic copy is unavailable'),
    'Copy did not settle',
  );
  assert(state.counts.edit_latest_draft === 2, 'Unchanged Copy supplied an extra learning label');
  assert(
    !state.counts.review_transcription && !state.counts.remember_cleanup,
    'Ordinary edit became a reviewed label',
  );
});

await test('Learning forget preserves manual words, rejects dirty preferences, and retries failures', async () => {
  const word = (id, phrase, automaticSource) => ({
    id,
    phrase,
    automaticSource,
    aliases: [],
    language: 'en',
    project: '',
    context: '',
    enabled: true,
    pronunciation: false,
    learnedAt: '',
    sources: [],
    usageCount: 0,
  });
  let profile = {
    schemaVersion: 3,
    revision: 4,
    learningEpoch: 2,
    automaticLearning: true,
    speechBiasEnabled: false,
    outputIntent: 'verbatim',
    preserveFillers: false,
    applyCorrections: true,
    useRememberedIntent: false,
    activeProject: '',
    examples: [],
    vocabulary: [word('manual', 'ManualTerm', null), word('automatic', 'LearnedTerm', 'candidate')],
  };
  let fail = true;
  let pending;
  const state = await setup({
    main: true,
    intercept: (command, _state, args) => {
      if (command === 'get_personalization') return structuredClone(profile);
      if (command === 'forget_automatic_learning')
        return (async () => {
          assert(args.revision === profile.revision, 'Forget used a stale profile revision');
          await pending?.promise;
          if (fail) throw new Error('Synthetic forget failure');
          profile = {
            ...profile,
            revision: 5,
            learningEpoch: 3,
            vocabulary: profile.vocabulary.filter((entry) => !entry.automaticSource),
          };
          return structuredClone(profile);
        })();
    },
  });
  [...target.querySelectorAll('nav button')]
    .find((b) => b.textContent.trim() === 'Settings')
    .click();
  await until(() => target.querySelector('#personal-project'), 'Personal preferences did not load');
  const panel = () => target.querySelector('.personalization');
  const forget = () =>
    [...panel().querySelectorAll('button')].find(
      (b) => b.textContent === 'Forget automatic learning',
    );
  assert(!panel().textContent.includes('Learn as I dictate'), 'Retired learning opt-in reappeared');
  const project = target.querySelector('#personal-project');
  project.value = 'Unsaved project';
  project.dispatchEvent(new Event('input', { bubbles: true }));
  await tick();
  assert(forget().disabled, 'Forget discarded unsaved profile edits');
  [...panel().querySelectorAll('button')].find((b) => b.textContent === 'Discard changes').click();
  await tick();
  forget().click();
  await until(
    () => panel().textContent.includes('Synthetic forget failure'),
    'Forget failure was hidden',
  );
  assert(
    profile.vocabulary.length === 2 && !forget().disabled,
    'Failure deleted words or blocked retry',
  );
  pending = deferred();
  fail = false;
  forget().click();
  await tick();
  const locked = forget().disabled && project.matches(':disabled');
  pending.resolve();
  await until(
    () => panel().textContent.includes('Automatic learning forgotten.'),
    'Forget retry failed',
  );
  assert(locked, 'Controls were editable during Forget');
  assert(
    profile.vocabulary.length === 1 && profile.vocabulary[0].id === 'manual',
    'Forget removed a manual word',
  );
  assert(
    profile.automaticLearning && !state.counts.save_personalization,
    'Forget disabled automatic learning or resaved stale preferences',
  );
  assert(state.counts.forget_automatic_learning === 2, 'Forget submitted duplicate requests');
});

await test('Vocabulary and example dropdowns preserve edits through shared keyboard selection', async () => {
  const saved = {
    schemaVersion: 1,
    revision: 0,
    automaticLearning: true,
    speechBiasEnabled: false,
    outputIntent: 'verbatim',
    preserveFillers: false,
    applyCorrections: true,
    useRememberedIntent: false,
    activeProject: 'Test project',
    vocabulary: [],
    examples: [
      {
        id: 'example',
        sessionId: 'synthetic',
        reviewRevisionId: 'review',
        input: 'example input',
        output: 'Example output.',
        language: 'en',
        preset: 'minimal',
        intent: 'note',
        enabled: true,
        project: '',
      },
    ],
  };
  await setup({
    main: true,
    intercept: (command) =>
      command === 'get_personalization' ? structuredClone(saved) : undefined,
  });
  await until(() => target.querySelector('#settings-language'), 'Settings did not load');
  [...target.querySelectorAll('nav button')]
    .find((button) => button.textContent.trim() === 'Settings')
    .click();
  await tick();
  await until(() => target.querySelector('#personal-project'), 'Preferences did not load');
  target.querySelector('#add-vocabulary').click();
  await until(() => target.querySelector('.word-entry input'), 'Word not added');
  const word = target.querySelector('.word-entry input');
  assert(document.activeElement === word, 'Adding a word did not focus its spelling field');
  word.value = 'SyntheticName';
  word.dispatchEvent(new Event('input', { bubbles: true }));
  await tick();
  const language = target.querySelector('[id^="word-language-"]');
  language.focus();
  language.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  await tick();
  language.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true }));
  await tick();
  language.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  await tick();
  assert(
    language.textContent.includes('French') && document.activeElement === language,
    'Word language keyboard selection failed',
  );
  target.querySelector('#example-intent-example').click();
  await tick();
  [...document.querySelectorAll('[role="option"]')]
    .find((button) => button.textContent.trim() === 'Message')
    .click();
  await tick();
  assert(
    target.querySelector('#example-intent-example').textContent.includes('Message'),
    'Example output style did not change',
  );
  assert(
    !target.querySelector('.personalization select'),
    'Personal preferences still use native dropdowns',
  );
  target.querySelector('.word-entry .entry-heading button').click();
  await until(
    () => !target.querySelector('.word-entry') && document.activeElement.id === 'add-vocabulary',
    'Removing a word did not restore useful focus',
  );
});

await test('Stale same-operation snapshots cannot replace a newer completed result', async () => {
  const state = await setup();
  state.dictation = {
    ...emptyDictation,
    id: 7,
    revision: 5,
    phase: 'ready',
    text: 'Final words',
    message: 'Copy your transcript.',
  };
  await emit('dictation-state', { ...state.dictation });
  await until(() => target.querySelector('[aria-label="Copy transcript"]'), 'Final copy missing');
  await emit('dictation-state', { ...state.dictation, revision: 2, phase: 'starting', text: null });
  await tick();
  assert(
    target.querySelector('[aria-label="Copy transcript"]'),
    'Stale state replaced completed transcript',
  );
});

await test('Release shows finalization immediately while the runtime recording snapshot is stale', async () => {
  const state = await setup({ recording: true });
  state.dictation = { ...state.dictation, revision: 2, phase: 'finishing' };
  await emit('dictation-state', { ...state.dictation });
  await until(
    () => target.textContent.includes('Finishing transcript'),
    'Released recording still appears to be Listening',
  );
  assert(
    !target.querySelector('.is-recording'),
    'The waveform still advertises capture after release',
  );
  assert(
    target.querySelector('button[aria-label="Processing recording"]')?.disabled,
    'The recording control must stay disabled during finalization',
  );
});

await test('View transcript navigates from Settings and refreshes the latest result', async () => {
  let latest = null;
  await setup({
    main: true,
    intercept: (command) => (command === 'get_latest_result' ? Promise.resolve(latest) : undefined),
  });
  await until(
    () => target.querySelector('.configured-models')?.textContent.includes('Qwen3.5'),
    'Main app did not load',
  );
  [...target.querySelectorAll('nav button')]
    .find((button) => button.textContent.trim() === 'Settings')
    .click();
  await tick();
  assert(!target.querySelector('#recording-limit'), 'Obsolete duration control is still present');
  latest = {
    resultId: 'synthetic-quick-result',
    rawTranscript: 'Synthetic quick capture.',
    cleanedTranscript: null,
    session: null,
    warnings: [],
    durationSeconds: 601,
  };
  await emit('show-transcript');
  await until(
    () => target.querySelector('#raw-result')?.value === latest.rawTranscript,
    'Latest quick result did not load',
  );
  assert(
    target.querySelector('.studio-main').dataset.view === 'dictate',
    'View transcript left the app on Settings',
  );
  assert(target.textContent.includes('No fixed time limit'), 'General duration policy is missing');
});

await test('Corrupt settings show recovery controls and reset restores manual capture', async () => {
  let restored = false;
  const state = await setup({
    main: true,
    intercept: (command) => {
      if (!restored && (command === 'get_settings' || command === 'get_app_status'))
        throw new Error('Saved settings could not be decoded: synthetic fixture');
      if (command === 'reset_settings') {
        restored = true;
        return defaultSettings();
      }
    },
  });
  await until(
    () => target.textContent.includes('Could not load settings.'),
    'Settings error was not shown',
  );
  const button = (text) =>
    [...target.querySelectorAll('button')].find((element) => element.textContent.trim() === text);
  assert(button('Try loading again'), 'Load retry is missing');
  assert(
    !restored && !state.counts.reset_settings,
    'Settings were reset without an explicit action',
  );
  assert(button('Record').disabled, 'Recording is available with invalid settings');
  button('Restore defaults').click();
  await until(
    () => !button('Try loading again') && !button('Record').disabled,
    'Explicit reset did not restore manual capture',
  );
  assert(state.counts.reset_settings === 1, 'Reset was not called exactly once');
  assert(!state.counts.start_recording, 'Recovery started recording automatically');
});

await test('Model presets save before explicit download and survive reopening', async () => {
  const speech = MODEL_PRESETS.find((entry) => entry.profile.id === 'fastconformer-en-de-es-fr');
  const cleanup = MODEL_PRESETS.find((entry) => entry.profile.name.startsWith('SmolLM2 360M'));
  let saved = defaultSettings();
  const installs = [];
  const installed = new Set();
  const catalog = () =>
    [saved.models.customAsr, saved.models.customRewriter].filter(Boolean).map((profile) => ({
      ...installation,
      modelId: profile.id,
      name: profile.name,
      role: profile.runtime === 'llama_cpp' ? 'text_reconstruction' : 'speech_recognition',
      selected: true,
      quantization: null,
      artifactsInstalled: installed.has(profile.id),
      runtimeReady: true,
      files: profile.files.map((file) => ({
        name: file.path,
        sizeBytes: file.sizeBytes,
        installed: installed.has(profile.id),
      })),
    }));
  const intercept = (command, state, args) => {
    if (command === 'get_settings') return structuredClone(saved);
    // Tauri serializes Svelte's draft proxy at the IPC boundary.
    if (command === 'save_settings') {
      saved = JSON.parse(JSON.stringify(args.settings));
      return structuredClone(saved);
    }
    if (command === 'list_model_installations') return catalog();
    if (command === 'install_model') {
      assert(
        [saved.models.customAsr?.id, saved.models.customRewriter?.id].includes(args.modelId),
        'Download was requested before saving the preset',
      );
      installs.push(args);
      installed.add(args.modelId);
      return catalog().find((entry) => entry.modelId === args.modelId);
    }
  };
  const button = (text) =>
    [...target.querySelectorAll('button')].find((element) => element.textContent.trim() === text);
  const openModels = async () => {
    await until(() => button('Save changes'), 'Settings did not load');
    button('Models').click();
    await tick();
  };
  const choose = async (id, name) => {
    target.querySelector(`#${id}`).click();
    await tick();
    const option = [...document.querySelectorAll('[role="option"]')].find((element) =>
      element.textContent.includes(name),
    );
    assert(option, `Missing preset option: ${name}`);
    option.click();
    await tick();
  };
  const state = await setup({ main: true, intercept });
  await openModels();
  await choose('models-asr-model', speech.profile.name);
  await choose('models-cleanup-model', cleanup.profile.name);
  assert(
    !state.counts.save_settings && installs.length === 0,
    'Selection alone saved or downloaded models',
  );
  assert(
    target.querySelector('#models-cleanup-precision').value === 'Q8_0',
    'Preset precision inherited the built-in Q4 selection',
  );
  target.querySelector('[aria-label="Speech model download"] button').click();
  await until(
    () =>
      installs.length === 1 &&
      !target.querySelector('[aria-label="Writing model download"] button').disabled,
    'Speech save and download did not complete',
  );
  target.querySelector('[aria-label="Writing model download"] button').click();
  await until(
    () =>
      installs.length === 2 &&
      !target.querySelector('[aria-label="Writing model download"] button').disabled,
    'Cleanup download did not complete',
  );
  assert(
    installs[0].modelId === speech.profile.id && installs[1].modelId === cleanup.profile.id,
    'Wrong model downloaded',
  );
  assert(
    installs.every((args) => args.quantization === null),
    'Preset download inherited built-in quantization',
  );
  await unmount(component);
  component = undefined;
  clearMocks();
  await setup({ main: true, intercept });
  await openModels();
  assert(
    target.querySelector('#models-asr-model').textContent.includes(speech.profile.name),
    'Speech selection did not survive reopening',
  );
  assert(
    target.querySelector('#models-cleanup-model').textContent.includes(cleanup.profile.name),
    'Cleanup selection did not survive reopening',
  );
  assert(
    target.querySelector('#models-cleanup-precision').value === 'Q8_0',
    'Saved preset precision changed',
  );
});

await test('Qwen and Cohere retain language choices without a recording duration cutoff', async () => {
  let saved = defaultSettings();
  saved.models.asr = 'Qwen/Qwen3-ASR-0.6B';
  saved.language = 'auto';
  const installs = [];
  const catalog = () =>
    ['Qwen/Qwen3-ASR-0.6B', 'CohereLabs/cohere-transcribe-03-2026'].map((id) => ({
      ...installation,
      modelId: id,
      name: id.startsWith('Qwen/') ? 'Qwen3-ASR 0.6B' : 'Cohere Transcribe',
      role: 'speech_recognition',
      selected: saved.models.asr === id,
      quantization: null,
      artifactsInstalled: false,
      runtimeReady: true,
      files: [{ name: 'encoder.int8.onnx', sizeBytes: 100, installed: false }],
    }));
  const intercept = (command, state, args) => {
    if (command === 'get_settings') return structuredClone(saved);
    if (command === 'save_settings') {
      saved = JSON.parse(JSON.stringify(args.settings));
      return structuredClone(saved);
    }
    if (command === 'list_model_installations') return catalog();
    if (command === 'install_model') {
      assert(saved.models.asr === args.modelId, 'Download preceded saved selection');
      installs.push(args);
      return catalog().find((entry) => entry.modelId === args.modelId);
    }
  };
  const button = (text) =>
    [...target.querySelectorAll('button')].find((element) => element.textContent.trim() === text);
  const choose = async (id, name) => {
    target.querySelector(`#${id}`).click();
    await tick();
    const option = [...document.querySelectorAll('[role="option"]')].find((element) =>
      element.textContent.includes(name),
    );
    assert(option, `Missing option: ${name}`);
    option.click();
    await tick();
  };
  const state = await setup({ main: true, intercept });
  await until(() => button('Save changes'), 'Settings did not load');
  button('Models').click();
  await tick();
  await choose('models-asr-model', 'Cohere Transcribe');
  assert(button('Save changes').disabled, 'Cohere accepted automatic language');
  assert(
    target.textContent.includes('Cohere requires an explicit language'),
    'Cohere language guidance missing',
  );
  assert(
    target.querySelector('[aria-label="Speech model download"] button').disabled,
    'Invalid language permits installation action',
  );
  button('Settings').click();
  await tick();
  await choose('settings-language', 'French');
  assert(!button('Save changes').disabled, 'Cohere rejected French');
  button('Models').click();
  await tick();
  target.querySelector('[aria-label="Speech model download"] button').click();
  await until(() => installs.length === 1, 'Cohere explicit save/download did not run');
  assert(
    installs[0].modelId === 'CohereLabs/cohere-transcribe-03-2026' &&
      installs[0].quantization === null,
    'Wrong Cohere download identity',
  );
  await until(
    () => !button('Models').disabled && !target.querySelector('#models-asr-model').disabled,
    'Download did not finish',
  );
  await choose('models-asr-model', 'Qwen3-ASR 0.6B');
  button('Settings').click();
  await tick();
  await choose('settings-language', 'Hindi');
  assert(!button('Save changes').disabled, 'Qwen rejected Hindi');
  button('Save changes').click();
  await until(
    () => saved.models.asr === 'Qwen/Qwen3-ASR-0.6B' && saved.language === 'hi',
    'Qwen settings were not saved',
  );
  assert(saved.maxRecordingSeconds === 0, 'Model limit became a general duration quota');
  button('Dictate').click();
  await tick();
  assert(
    target.textContent.includes('No fixed time limit'),
    'Recording still has a duration quota',
  );
  for (const page of ['Dictate', 'Settings', 'Models', 'History']) {
    button(page).click();
    await tick();
    assert(
      !/up to 10 seconds|clips up to|adapter limit/i.test(target.textContent),
      `Old restriction remains on ${page}`,
    );
  }
  assert(!state.counts.start_recording, 'Selection started recording');
});

await test('Long-audio progress appears in Dictate and Quick Capture without a partial completed result', async () => {
  const pending = deferred();
  await setup({
    main: true,
    intercept: (command, state) => {
      if (command === 'plugin:dialog|open') return 'synthetic-long-audio.wav';
      if (command === 'transcribe_file') {
        state.runtime = {
          ...idle,
          busy: true,
          stage: 'transcribing',
          message: 'Transcribing · 42%',
        };
        return pending.promise.then((result) => {
          state.runtime = { ...idle };
          return result;
        });
      }
    },
  });
  await until(
    () =>
      [...target.querySelectorAll('button')].find(
        (b) => b.textContent.trim() === 'Transcribe audio file' && !b.disabled,
      ),
    'File transcription not ready',
  );
  [...target.querySelectorAll('button')]
    .find((b) => b.textContent.trim() === 'Transcribe audio file')
    .click();
  await until(
    () => target.querySelector('.recorder-hint')?.textContent.includes('42%'),
    'Dictate progress missing',
  );
  assert(!target.querySelector('.transcription-result'), 'Partial result shown as completed');
  pending.resolve({
    resultId: 'long-fixture',
    rawTranscript: 'Full synthetic transcript.',
    cleanedTranscript: null,
    session: null,
    warnings: [],
    durationSeconds: 31,
  });
  await until(() => target.querySelector('.transcription-result'), 'Completed long result missing');
  await unmount(component);
  component = undefined;
  clearMocks();
  target.replaceChildren();
  await setup({
    intercept: (command, state) => {
      if (command === 'get_runtime_status')
        return {
          ...state.runtime,
          busy: true,
          stage: 'transcribing',
          message: 'Transcribing · 73%',
          maxRecordingSeconds: 0,
        };
    },
  });
  await until(() => target.textContent.includes('73%'), 'Quick Capture progress missing');
  assert(
    !target.textContent.includes('Transcript ready'),
    'Quick Capture marked incomplete work ready',
  );
});

await test('Local engine reports measured values and pauses on navigation, visibility and disposal', async () => {
  const saved = defaultSettings();
  saved.cleanupEnabled = false;
  let fail = false;
  let timings = null;
  const state = await setup({
    main: true,
    intercept: (command, state) => {
      if (command === 'get_settings') return structuredClone(saved);
      if (command === 'get_engine_diagnostics') {
        if (fail) throw new Error('Synthetic sampler failure');
        return {
          resources: {
            sampledAtMs: Date.now(),
            cpuPercent: 25,
            memoryBytes: 2 ** 30,
            logicalCpus: 8,
            status: 'ready',
          },
          activity: { ...state.runtime },
          timings,
        };
      }
    },
  });
  const pause = () => new Promise((resolve) => setTimeout(resolve, 1150));
  const button = (text) =>
    [...target.querySelectorAll('button')].find((element) => element.textContent.trim() === text);
  const panel = () => target.querySelector('.engine-status');
  const refresh = () => target.querySelector('[aria-label="Refresh application status"]').click();
  await until(() => button('Save changes'), 'Settings did not load');
  assert(!state.counts.get_engine_diagnostics, 'Resources polled outside Models');
  button('Models').click();
  await tick();
  await until(() => panel().textContent.includes('1.00'), 'RSS did not appear');
  assert(!panel().textContent.includes('25.0'), 'First CPU reading was invented');
  assert(panel().textContent.includes('Raw transcript only'), 'Cleanup off state missing');
  assert(panel().textContent.includes('Check model setup'), 'Missing model readiness was hidden');
  await until(() => panel().textContent.includes('25.0'), 'Second CPU reading did not appear');
  for (const [stage, label] of [
    ['recording', 'Recording'],
    ['transcribing', 'Transcribing'],
    ['rewriting', 'Polishing text'],
  ]) {
    state.runtime = { ...idle, busy: true, stage };
    refresh();
    await until(
      () => panel().querySelector('.engine-state').textContent.includes(label),
      `Missing ${stage} state`,
    );
  }
  timings = {
    operationId: 'synthetic',
    asrModel: 'Synthetic ASR',
    cleanupModel: null,
    speechMode: 'offline',
    outcome: 'cancelled',
    activeStage: null,
    transcribeMs: 1234,
    cleanupMs: null,
    saveMs: 12,
  };
  state.runtime = { ...idle, stage: 'cancelled' };
  refresh();
  await until(
    () => panel().textContent.includes('Cancelled · partial timings'),
    'Cancelled operation not marked partial',
  );
  assert(
    panel().textContent.includes('1.2 s') && panel().textContent.includes('12 ms'),
    'Stage durations missing',
  );
  timings = { ...timings, outcome: 'error' };
  state.runtime = { ...idle, stage: 'error' };
  refresh();
  await until(
    () => panel().textContent.includes('Failed · partial timings'),
    'Failure timings missing',
  );
  fail = true;
  refresh();
  await until(
    () => panel().textContent.includes('Synthetic sampler failure'),
    'Sampler error missing',
  );
  assert(
    !panel().querySelector('.resource-value').textContent.includes('25.0'),
    'Failed sampler kept a live number',
  );
  fail = false;
  state.runtime = { ...idle };
  refresh();
  await until(
    () => !panel().textContent.includes('Synthetic sampler failure'),
    'Refresh did not recover',
  );
  button('Settings').click();
  await tick();
  let count = state.counts.get_engine_diagnostics;
  await pause();
  assert(state.counts.get_engine_diagnostics === count, 'Hidden Models page kept polling');
  button('Models').click();
  await until(
    () => state.counts.get_engine_diagnostics > count,
    'Models navigation did not resume',
  );
  Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'hidden' });
  document.dispatchEvent(new Event('visibilitychange'));
  await tick();
  count = state.counts.get_engine_diagnostics;
  try {
    await pause();
    assert(state.counts.get_engine_diagnostics === count, 'Hidden document kept polling');
  } finally {
    delete document.visibilityState;
    document.dispatchEvent(new Event('visibilitychange'));
  }
  await until(() => state.counts.get_engine_diagnostics > count, 'Visible document did not resume');
  await unmount(component);
  component = undefined;
  count = state.counts.get_engine_diagnostics;
  await pause();
  assert(state.counts.get_engine_diagnostics === count, 'Disposed panel kept polling');
});

await test('Imported model and pipeline downloads show byte progress, verification, cancellation and retry', async () => {
  let saved = defaultSettings();
  const preset = MODEL_PRESETS.find((entry) =>
    entry.profile.name.startsWith('SmolLM2 360M'),
  ).profile;
  saved.models.customRewriter = preset;
  let gate;
  let job = 0;
  let installed = false;
  const entry = () => ({
    ...installation,
    modelId: preset.id,
    name: preset.name,
    role: 'text_reconstruction',
    quantization: null,
    selected: true,
    ready: installed,
    artifactsInstalled: installed,
    runtimeReady: true,
    files: [{ name: 'synthetic.gguf', sizeBytes: 2 ** 21, installed }],
  });
  const state = await setup({
    main: true,
    intercept: (command, state, args) => {
      if (command === 'get_settings') return structuredClone(saved);
      if (command === 'list_model_installations') return [entry()];
      if (command === 'install_model' || command === 'install_models') {
        gate = deferred();
        job++;
        state.runtime = {
          ...idle,
          busy: true,
          stage: 'installing',
          download: {
            operationId: `download-${job}`,
            modelId: args?.modelId ?? null,
            quantization: args?.quantization ?? null,
            progress: null,
          },
        };
        return gate.promise.then((outcome) => {
          state.runtime = { ...idle };
          if (outcome !== 'success') throw new Error(outcome);
          installed = true;
          return command === 'install_models' ? installation : entry();
        });
      }
      if (command === 'cancel_operation') {
        state.runtime.stage = 'cancelling';
        return null;
      }
    },
  });
  const button = (text) =>
    [...target.querySelectorAll('button')].find((element) => element.textContent.trim() === text);
  const download = () => target.querySelector('[aria-label="Writing model download"] button');
  const meter = () =>
    target.querySelector('[aria-label="Writing model download"] [role="progressbar"]');
  const progress = (phase, done) => {
    state.runtime.download.progress = {
      phase,
      fileName: 'synthetic.gguf',
      fileIndex: 2,
      fileCount: 3,
      completedBytes: done,
      totalBytes: 2 ** 21,
    };
  };
  await until(() => button('Save changes'), 'Settings did not load');
  button('Models').click();
  await until(() => download() && !download().disabled, 'Imported model download unavailable');
  download().click();
  await until(() => meter(), 'Preparing download has no progress bar');
  assert(!meter().hasAttribute('aria-valuenow'), 'Preparation invented a percentage');
  progress('downloading', 2 ** 19);
  await until(
    () => meter()?.getAttribute('aria-valuenow') === '25',
    'Imported model did not show measured 25% progress',
  );
  assert(
    target.textContent.includes('512 KiB / 2 MiB') &&
      target.textContent.includes('File 2 of 3 · synthetic.gguf'),
    'Bytes or file details missing',
  );
  progress('verifying', 2 ** 21);
  await until(
    () => target.textContent.includes('Verifying download…'),
    'Verification phase missing',
  );
  assert(
    download().disabled && meter().getAttribute('aria-valuenow') === '100',
    'Verification was treated as completion',
  );
  button('Cancel download').click();
  await until(() => button('Cancelling…')?.disabled, 'Cancellation feedback missing');
  gate.resolve('operation cancelled');
  await until(() => !meter() && !download().disabled, 'Cancelled download did not reset');
  assert(
    target.textContent.includes('download cancelled'),
    'Cancelled result falsely reported success',
  );
  download().click();
  await until(() => meter(), 'Retry did not start');
  assert(!meter().hasAttribute('aria-valuenow'), 'Retry retained the previous 100%');
  gate.resolve('Synthetic network failure');
  await until(
    () => target.textContent.includes('Synthetic network failure') && !download().disabled,
    'Failure did not enable retry',
  );
  download().click();
  await until(() => meter(), 'Second retry did not start');
  progress('downloading', 2 ** 20);
  await until(() => meter()?.getAttribute('aria-valuenow') === '50', 'Retry progress missing');
  gate.resolve('success');
  await until(() => !meter() && !download().disabled, 'Completed download did not clear progress');
  assert(
    target.textContent.includes(`${preset.name} is ready on this device.`),
    'Installed result missing',
  );
  button('Verify selected models').click();
  await until(
    () => target.querySelector('.download-progress [role="progressbar"]'),
    'Selected-pipeline install has no progress bar',
  );
  progress('downloading', 2 ** 19);
  await until(
    () =>
      target
        .querySelector('.download-progress [role="progressbar"]')
        ?.getAttribute('aria-valuenow') === '25',
    'Selected-pipeline progress missing',
  );
  gate.resolve('success');
  await until(
    () => !target.querySelector('.download-progress'),
    'Completed pipeline retained progress',
  );
});

await test('Device-aware menu preserves an unavailable saved backend and supports detection retry', async () => {
  let saved = defaultSettings();
  saved.computeBackend = 'cuda';
  saved.models.asr = 'nvidia/nemotron-3.5-asr-streaming-0.6b';
  let fail = true;
  const state = await setup({
    main: true,
    intercept: (command, state, args) => {
      if (command === 'get_settings') return structuredClone(saved);
      if (command === 'save_settings') {
        saved = JSON.parse(JSON.stringify(args.settings));
        return structuredClone(saved);
      }
      if (command === 'get_accelerator_capabilities') {
        if (fail) throw new Error('Synthetic device detection failure');
        return {
          os: 'macos',
          arch: 'aarch64',
          backends: [{ backend: 'metal', compiled: true, devices: ['Synthetic Apple GPU'] }],
        };
      }
    },
  });
  const button = (text) =>
    [...target.querySelectorAll('button')].find((element) => element.textContent.trim() === text);
  await until(() => button('Save changes'), 'Settings did not load');
  button('Models').click();
  await until(() => button('Retry device detection'), 'Device detection error had no retry');
  assert(
    !state.counts.save_settings && saved.computeBackend === 'cuda',
    'Detection rewrote the saved preference',
  );
  fail = false;
  button('Retry device detection').click();
  await until(
    () => target.textContent.includes('macOS · Apple Silicon'),
    'Native platform was not shown',
  );
  assert(
    target.querySelector('#compute-backend').textContent.includes('CUDA · unavailable'),
    'Saved unsupported backend disappeared',
  );
  target.querySelector('#compute-backend').click();
  await tick();
  const options = [...document.querySelectorAll('[role="option"]')];
  assert(options.length === 4, 'Unsupported non-selected backends leaked into the Mac menu');
  assert(
    options.find((option) => option.textContent.includes('CUDA'))?.getAttribute('aria-disabled') ===
      'true',
    'Unavailable saved CUDA remained selectable',
  );
  assert(
    options.some(
      (option) =>
        option.textContent.includes('Metal') && option.textContent.includes('Synthetic Apple GPU'),
    ),
    'Detected Metal device missing',
  );
  // The Select keeps focus on its trigger and supports Home/Enter selection.
  const select = target.querySelector('#compute-backend');
  select.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true }));
  select.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  await tick();
  assert(
    select.textContent.includes('Automatic') && !button('Save changes').disabled,
    'Keyboard selection did not recover the unavailable preference',
  );
  assert(!state.counts.save_settings, 'Choice was saved without an explicit action');
  button('Save changes').click();
  await until(() => saved.computeBackend === 'auto', 'Recovered preference was not saved');
});

await test('All screens share saved model names, writing state and model-management navigation', async () => {
  let saved = defaultSettings();
  const initialSpeech = ASR_MODELS[saved.models.asr];
  const speechPreset = MODEL_PRESETS.find(
    (entry) => entry.profile.runtime === 'sherpa_onnx_transducer',
  );
  const preset = MODEL_PRESETS.find((entry) => entry.profile.name.startsWith('SmolLM2 360M'));
  const state = await setup({
    main: true,
    intercept: (command, state, args) => {
      if (command === 'get_settings') return structuredClone(saved);
      if (command === 'save_settings') {
        saved = JSON.parse(JSON.stringify(args.settings));
        return structuredClone(saved);
      }
      if (command === 'reset_settings') {
        saved = defaultSettings();
        return structuredClone(saved);
      }
    },
  });
  const button = (text) =>
    [...target.querySelectorAll('button')].find((element) => element.textContent.trim() === text);
  const label = () => target.querySelector('.studio-recorder [aria-label="Configured models"]');
  async function checkEveryScreen(speech, writing, off) {
    const saves = state.counts.save_settings;
    for (const view of ['Dictate', 'History', 'Settings', 'Models']) {
      button(view).click();
      await tick();
      const summary = target.querySelector(
        view === 'Models'
          ? '.engine-models'
          : view === 'Dictate'
            ? '.studio-recorder .configured-models'
            : '.page-model-summary .configured-models',
      );
      assert(summary && !summary.closest('[hidden]'), `${view} model summary is hidden`);
      await until(
        () => summary.textContent.includes(speech) && summary.textContent.includes(writing),
        `${view} did not reflect both saved models`,
      );
      assert(
        summary.textContent.includes('Speech model') &&
          summary.textContent.includes('Writing model'),
        `${view} model wording differs`,
      );
      const disabled =
        view === 'Models'
          ? summary.querySelector('.engine-model:last-child strong').textContent === 'Off'
          : Boolean(summary.querySelector('.writing-off'));
      assert(disabled === off, `${view} writing state is out of sync`);
      if (view !== 'Models') {
        summary.querySelector('button').click();
        await tick();
        assert(
          button('Models').getAttribute('aria-current') === 'page',
          `${view} Manage models failed`,
        );
      }
    }
    assert(state.counts.save_settings === saves, 'Screen navigation saved settings');
    button('Dictate').click();
    await tick();
  }
  await until(
    () =>
      label()?.textContent.includes('Qwen3.5 0.8B') && label().textContent.includes(initialSpeech),
    'Both saved model names must appear in recorder',
  );
  await checkEveryScreen(initialSpeech, 'Qwen3.5 0.8B', false);
  assert(
    label().textContent.includes('Speech model') && label().textContent.includes('Writing model'),
    'Friendly role labels missing',
  );
  assert(
    !target.querySelector('.cleanup-chip, .model-strip'),
    'Old button or duplicate summary remained',
  );
  assert(
    label().querySelectorAll('button').length === 1 &&
      label().querySelector('button').textContent.includes('Manage models'),
    'Expected one Manage models action',
  );
  label().querySelector('button').click();
  await tick();
  assert(
    button('Models').getAttribute('aria-current') === 'page',
    'Manage models did not open Models',
  );
  assert(
    target.querySelector('label[for="models-cleanup-model"]').textContent === 'Writing model',
    'Model selector wording does not match',
  );
  target.querySelector('#models-asr-model').click();
  await tick();
  [...document.querySelectorAll('[role="option"]')]
    .find((option) => option.textContent.includes(speechPreset.profile.name))
    .click();
  await tick();
  target.querySelector('#models-cleanup-model').click();
  await tick();
  [...document.querySelectorAll('[role="option"]')]
    .find((option) => option.textContent.includes(preset.profile.name))
    .click();
  await tick();
  button('Dictate').click();
  await tick();
  assert(
    label().textContent.includes('Qwen3.5 0.8B') &&
      label().textContent.includes(initialSpeech) &&
      !label().textContent.includes(preset.profile.name) &&
      !label().textContent.includes(speechPreset.profile.name),
    'Unsaved models were shown as configured',
  );
  await checkEveryScreen(initialSpeech, 'Qwen3.5 0.8B', false);
  label().querySelector('button').click();
  await tick();
  button('Save changes').click();
  await until(
    () =>
      saved.models.customRewriter?.id === preset.profile.id &&
      saved.models.customAsr?.id === speechPreset.profile.id,
    'Both model choices did not save',
  );
  button('Dictate').click();
  await tick();
  await until(
    () =>
      label().textContent.includes(preset.profile.name) &&
      label().textContent.includes(speechPreset.profile.name),
    'Saved custom models did not update the labels',
  );
  await checkEveryScreen(speechPreset.profile.name, preset.profile.name, false);
  button('Settings').click();
  await tick();
  const toggle = target.querySelector('input[type="checkbox"][id="cleanup-enabled"]');
  assert(toggle, 'Cleanup switch missing');
  toggle.click();
  await tick();
  await checkEveryScreen(speechPreset.profile.name, preset.profile.name, false);
  button('Settings').click();
  await tick();
  button('Save changes').click();
  await until(() => !saved.cleanupEnabled, 'Cleanup off preference did not save');
  button('Dictate').click();
  await tick();
  await until(
    () =>
      label().querySelector('.writing-off') &&
      label().textContent.includes(preset.profile.name) &&
      label().textContent.includes(speechPreset.profile.name),
    'Disabled writing must retain both configured names with an Off state',
  );
  await checkEveryScreen(speechPreset.profile.name, preset.profile.name, true);
  const savesBeforeNavigation = state.counts.save_settings;
  label().querySelector('button').click();
  await tick();
  assert(
    button('Models').getAttribute('aria-current') === 'page',
    'Manage models did not open Models with writing off',
  );
  assert(
    state.counts.save_settings === savesBeforeNavigation && !saved.cleanupEnabled,
    'Navigation changed saved settings',
  );
  button('Settings').click();
  await tick();
  button('Restore defaults').click();
  await until(
    () => saved.models.customRewriter === null && saved.cleanupEnabled,
    'Defaults did not restore',
  );
  await checkEveryScreen(initialSpeech, 'Qwen3.5 0.8B', false);
  assert(!state.counts.start_recording, 'Label interaction started recording');
});

await test('Capture keeps one control and Reed follows acknowledged recording and real silence', async () => {
  const start = deferred();
  const state = await setup({
    main: true,
    intercept: (command, state) => {
      if (command === 'start_recording')
        return start.promise.then(() => {
          state.runtime = { ...idle, busy: true, stage: 'recording' };
        });
      if (command === 'get_audio_features')
        return { level: state.quiet ? 0 : 0.06, pitchHz: state.quiet ? null : 220 };
    },
  });
  await until(
    () =>
      target.querySelector('.record-button') && !target.querySelector('.record-button').disabled,
    'Recorder unavailable',
  );
  const button = target.querySelector('.record-button'),
    glyph = button.querySelector('.capture-glyph');
  button.click();
  await until(() => state.counts.start_recording, 'Start was not requested');
  assert(!glyph.classList.contains('recording'), 'Capture animated before acknowledgement');
  start.resolve();
  await until(
    () => glyph.classList.contains('recording') && !button.disabled,
    'Capture did not acknowledge recording',
  );
  assert(
    target.querySelector('.record-button') === button &&
      button.querySelector('.capture-glyph') === glyph,
    'Record control remounted',
  );
  const path = () => target.querySelector('.studio-recorder .wave-trace').getAttribute('d');
  await until(() => state.counts.get_audio_features > 2, 'Measured audio was not sampled');
  assert((path().match(/M /g) ?? []).length === 45, 'Full recorder is not Reed');
  const moving = path();
  await new Promise((r) => setTimeout(r, 60));
  assert(path() !== moving, 'Measured signal did not animate');
  state.quiet = true;
  await until(
    () => !/M [\d.]+ (?!49.00)[\d.]+ V/.test(path()),
    'Silent input retained a moving signal',
  );
  const quiet = path();
  await new Promise((r) => setTimeout(r, 70));
  assert(path() === quiet, 'Silence invented movement');
  button.click();
  await until(
    () => !glyph.classList.contains('recording') && !button.disabled,
    'Stop did not settle',
  );
  assert(target.querySelector('.record-button') === button, 'Stop replaced the control');
});

for (const main of [true, false])
  await test(`Microphone synchronization: ${main ? 'Dictate' : 'floating capture'} follows pitch and volume without autonomous motion`, async () => {
    let telemetry = { level: 0.025, pitchHz: 110 };
    let stalled = false;
    let nextReading = null;
    const hold = deferred();
    const state = await setup({
      main,
      recording: !main,
      intercept: (command) => {
        if (command !== 'get_audio_features') return undefined;
        if (stalled) return hold.promise;
        const reading = nextReading ?? { ...telemetry };
        nextReading = null;
        return reading;
      },
    });
    if (main) {
      await until(
        () =>
          target.querySelector('.record-button') &&
          !target.querySelector('.record-button').disabled,
        'Recorder unavailable',
      );
      target.querySelector('.record-button').click();
    }
    const path = () => target.querySelector('.voice-indicator .wave-trace')?.getAttribute('d');
    const pause = (ms) => new Promise((r) => setTimeout(r, ms));
    await until(() => state.counts.get_audio_features > 2 && path(), 'Audio telemetry missing');
    if (!matchMedia('(prefers-reduced-motion: reduce)').matches) {
      await pause(650);
      const steady = path();
      await pause(100);
      assert(
        path() === steady,
        'Unchanged pitch/volume still drive an unrelated looping animation',
      );
      telemetry.pitchHz = 330;
      await until(() => path() !== steady, 'Pitch change was ignored');
      await pause(350);
      const higherPitch = path();
      await pause(100);
      assert(path() === higherPitch, 'Pitch change did not settle');
      nextReading = { level: 0.025, pitchHz: null };
      await until(() => nextReading === null, 'Unvoiced consonant was not sampled');
      await pause(90);
      assert(path() === higherPitch, 'A brief unvoiced consonant collapsed the wavelength');
      telemetry.pitchHz = null;
      await pause(500);
      assert(path() === higherPitch, 'A longer unvoiced sound reshaped unchanged audio energy');
      telemetry.pitchHz = 330;
      await pause(120);
      assert(path() === higherPitch, 'Pitch reacquisition jumped despite the same voice');
      nextReading = { level: 0.025, pitchHz: 660 };
      await until(() => nextReading === null, 'Octave outlier was not sampled');
      await pause(90);
      assert(path() === higherPitch, 'One octave outlier reshaped the waveform');
      telemetry.level = 0.006;
      await until(() => path() !== higherPitch, 'Volume change was ignored');
      await pause(450);
      const quieter = path();
      const heights = (d) =>
        [...d.matchAll(/M [\d.]+ ([\d.]+) V ([\d.]+)/g)].map(([, top, bottom]) => +bottom - +top);
      assert(
        Math.max(...heights(quieter)) < Math.max(...heights(higherPitch)),
        'Quieter input did not reduce amplitude',
      );
      stalled = true;
      await until(
        () => heights(path()).every((height) => height <= 2.02),
        'Stalled telemetry retained microphone motion',
      );
      telemetry = { level: 0, pitchHz: null };
      hold.resolve({ level: 0.8, pitchHz: 440 });
      stalled = false;
      const afterReply = performance.now();
      while (performance.now() - afterReply < 120) {
        await new Promise(requestAnimationFrame);
        assert(
          heights(path()).every((height) => height <= 2.02),
          'An expired nonzero reply revived old audio',
        );
      }
      telemetry = { level: 0.025, pitchHz: null };
      await until(
        () => heights(path()).some((height) => height > 4),
        'Unvoiced input lost its measured volume',
      );
      await pause(650);
      const unvoiced = path();
      await pause(100);
      assert(path() === unvoiced, 'Unvoiced input invents changing pitch');
    }
  });

await test('History feedback persists, rolls back failed saves, clears and undoes independently of reviews', async () => {
  const original = {
    id: 'motion-record',
    createdAt: new Date().toISOString(),
    originalFilename: 'synthetic.wav',
    audioRelativePath: 'audio/synthetic.wav',
    audioSha256: 'synthetic',
    audioBytes: 1024,
    rawTranscript: 'Keep the recording easy to find.',
    cleanedTranscript: null,
    correctedTranscript: null,
    verifiedTranscript: null,
    reviewHistory: [],
    reviewStatus: 'pending',
    transcriptFeedback: null,
    configSnapshot: defaultSettings(),
    modelVersions: {},
  };
  let stored = structuredClone(original);
  const save = deferred();
  const state = await setup({
    main: true,
    intercept: (command, state, args) => {
      if (command === 'list_sessions') return [structuredClone(stored)];
      if (command === 'set_transcript_feedback') {
        if (state.failFeedback) throw new Error('Synthetic feedback failure');
        return (state.counts.set_transcript_feedback === 1 ? save.promise : Promise.resolve()).then(
          () => {
            stored = { ...stored, transcriptFeedback: args.feedback };
            return structuredClone(stored);
          },
        );
      }
    },
  });
  const button = (label) =>
    [...target.querySelectorAll('button')].find((b) => b.textContent.trim() === label);
  button('History').click();
  await until(() => target.querySelector('#recording-trigger-motion-record'), 'History missing');
  target.querySelector('#recording-trigger-motion-record').click();
  await tick();
  const control = () => target.querySelector('.transcript-feedback');
  const vote = (side) => control().querySelector(`[aria-label="${side} transcript"]`);
  vote('Like').click();
  await tick();
  assert(
    vote('Like').getAttribute('aria-pressed') === 'true' && vote('Dislike').disabled,
    'Pending feedback lost immediate choice or allowed duplicate requests',
  );
  assert(control().textContent.includes('Saving feedback'), 'Pending feedback claimed success');
  vote('Dislike').click();
  assert(state.counts.set_transcript_feedback === 1, 'Duplicate feedback write');
  save.resolve();
  await until(() => !vote('Like').disabled, 'Feedback save never completed');
  assert(
    stored.transcriptFeedback === 'like' &&
      stored.reviewStatus === 'pending' &&
      !stored.reviewHistory.length,
    'Like approved training data',
  );
  state.failFeedback = true;
  vote('Dislike').click();
  await until(
    () => control().textContent.includes('Synthetic feedback failure'),
    'Feedback failure hidden',
  );
  assert(
    vote('Like').getAttribute('aria-pressed') === 'true' && !vote('Dislike').disabled,
    'Failed feedback did not roll back and allow retry',
  );
  state.failFeedback = false;
  vote('Dislike').click();
  await until(
    () => stored.transcriptFeedback === 'dislike' && !vote('Dislike').disabled,
    'Feedback retry failed',
  );
  vote('Dislike').click();
  await until(
    () => stored.transcriptFeedback === null && !vote('Dislike').disabled,
    'Click selected did not clear',
  );
  control().querySelector('.feedback-undo').click();
  await until(
    () => stored.transcriptFeedback === 'dislike' && !vote('Dislike').disabled,
    'Undo did not restore prior choice',
  );
  const previousLoads = state.counts.list_sessions;
  target.querySelector('[aria-label="Refresh recordings"]').click();
  await tick();
  await until(
    () => state.counts.list_sessions > previousLoads && !vote('Dislike').disabled,
    'History refresh failed',
  );
  assert(vote('Dislike').getAttribute('aria-pressed') === 'true', 'Saved feedback lost on refresh');
  const trigger = target.querySelector('#recording-trigger-motion-record');
  trigger.click();
  await tick();
  const closing = target.querySelector('.history-reveal');
  await until(
    () => !closing.isConnected || closing.inert,
    'Closing review remained keyboard-accessible',
  );
  await until(() => !closing.isConnected, 'History did not finish closing');
  trigger.click();
  await until(() => control(), 'History did not reopen');
  assert(vote('Dislike').getAttribute('aria-pressed') === 'true', 'Saved feedback lost on reopen');
});

await test('Shared selection keeps its backing during rapid changes and dropdown keyboard navigation', async () => {
  const saved = defaultSettings();
  await setup({
    main: true,
    intercept: (command) => (command === 'get_settings' ? saved : undefined),
  });
  const button = (label) =>
    [...target.querySelectorAll('button')].find((b) => b.textContent.trim() === label);
  await until(() => target.querySelector('#cleanup-preset'), 'Settings did not finish loading');
  button('Settings').click();
  await tick();
  const trigger = target.querySelector('#cleanup-preset');
  trigger.scrollIntoView({ block: 'center', behavior: 'instant' });
  await new Promise(requestAnimationFrame);
  await new Promise(requestAnimationFrame);
  trigger.click();
  await tick();
  const popup = document.querySelector('.select-popup');
  await until(() => popup?.dataset.motionReady, 'Dropdown backing was not initialized');
  assert(popup.dataset.hasSelection === 'true', 'Dropdown lacks selected backing');
  const first = popup.querySelector('.is-active');
  trigger.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
  await tick();
  assert(popup.querySelector('.is-active') !== first, 'Keyboard did not move active option');
  trigger.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  await tick();
  assert(
    !document.querySelector('.select-popup') && document.activeElement === trigger,
    'Choosing did not close and restore focus',
  );
  assert(getComputedStyle(trigger).transform === 'none', 'Dropdown anchor scaled during selection');
});

// All layouts share the live controller; the design may only change geometry.
for (const design of CAPTURE_DESIGNS) {
  await test(`${design}: dense Reed, live updates, native bounds, final copy and cancel`, async () => {
    const dimensions = {
      split: [396, 192],
      ribbon: [388, 148],
      underline: [454, 224],
      studio: [412, 232],
      margin: [272, 280],
      counter: [200, 304],
    };
    const [width, height] = dimensions[design];
    const state = await setup({
      recording: true,
      intercept: (command, state) => {
        if (command === 'get_settings') return { ...defaultSettings(), captureDesign: design };
        if (command === 'get_audio_features')
          return { level: state.quiet ? 0 : 0.06, pitchHz: state.quiet ? null : 220 };
      },
    });
    await until(
      () => target.querySelector(`[data-capture-design="${design}"]`),
      'Saved layout did not load',
    );
    const capture = target.querySelector('.quick-capture');
    capture.style.width = `${width - 24}px`;
    await new Promise(requestAnimationFrame);
    const path = () => target.querySelector('.wave-trace');
    await until(
      () => state.counts.get_audio_features > 2 && path()?.getAttribute('d'),
      'Measured waveform missing',
    );
    assert((path().getAttribute('d').match(/M /g) ?? []).length === 45, 'Sparse waveform returned');
    await until(
      () =>
        Math.abs(
          parseFloat(getComputedStyle(path()).strokeWidth) -
            Math.min(3, (path().ownerSVGElement.clientWidth * 3) / 320),
        ) < 0.01,
      'Stroke thickness does not follow waveform width',
    );
    const waveBox = path().ownerSVGElement.getBoundingClientRect();
    const slotBox = target.querySelector('.capture-wave').getBoundingClientRect();
    assert(Math.abs(waveBox.height - slotBox.height) < 1, 'Waveform escaped its reserved height');
    const moving = path().getAttribute('d');
    await until(() => path().getAttribute('d') !== moving, 'Live input did not animate');
    state.quiet = true;
    await until(
      () => !/M [\d.]+ (?!49.00)[\d.]+ V/.test(path().getAttribute('d')),
      'Silence did not settle',
    );
    for (const button of target.querySelectorAll('.quick-actions button')) {
      const rect = button.getBoundingClientRect();
      assert(rect.width >= 44 && rect.height >= 44, 'Capture hit target below 44px');
    }
    // Layout events update an already-mounted quick window without capture calls.
    await emit('capture-design-changed', 'invalid');
    await tick();
    assert(
      target.querySelector(`[data-capture-design="${design}"]`),
      'Invalid layout event was accepted',
    );
    state.runtime = {
      ...idle,
      stage: 'error',
      message: 'Synthetic microphone problem. Open transcribe to choose another input.',
    };
    state.dictation = {
      ...state.dictation,
      phase: 'error',
      revision: 1,
      message: state.runtime.message,
    };
    await emit('dictation-state', { ...state.dictation });
    await until(() => target.querySelector('.is-error'), 'Error state missing');
    const wrap = target.querySelector('.layout-wrap').getBoundingClientRect();
    assert(wrap.height <= height - 24, `${design} error exceeds native height: ${wrap.height}`);
    assert(capture.scrollWidth <= width - 24, `${design} overflows native width`);
    assert(
      target.textContent.includes('Synthetic microphone problem'),
      'Error explanation hidden from content',
    );
    target.querySelector('.record-toggle').click();
    await until(() => target.querySelector('.is-recording'), 'Retry failed');
    target.querySelector('.record-toggle').click();
    await until(
      () => target.querySelector('[aria-label="Copy transcript"]'),
      'Completion has no Copy',
    );
    target.querySelector('[aria-label="Copy transcript"]').click();
    await until(() => state.copiedId === state.dictation.id, 'Wrong transcript copied');
    state.runtime = { ...idle, busy: true, stage: 'recording' };
    state.dictation = {
      ...state.dictation,
      id: state.dictation.id + 1,
      phase: 'recording',
      text: null,
      startedAtUnixMs: Date.now(),
    };
    await emit('dictation-state', { ...state.dictation });
    await until(
      () => target.querySelector('[aria-label="Cancel recording"]'),
      'Next recording missing',
    );
    target.querySelector('[aria-label="Cancel recording"]').click();
    await until(() => state.counts.cancel_dictation === 1, 'Cancel did not reach controller');
  });
}

await test('Capture drag excludes controls, guards repeats and preserves runtime actions', async () => {
  const drag = deferred();
  const state = await setup({
    recording: true,
    intercept: (command) => (command === 'start_capture_drag' ? drag.promise : undefined),
  });
  await until(() => target.querySelector('.capture.movable'), 'Native capture has no drag surface');
  const surface = target.querySelector('.capture-wave');
  const mouse = (target, button = 0) =>
    target.dispatchEvent(new MouseEvent('mousedown', { button, bubbles: true, cancelable: true }));
  mouse(target.querySelector('.record-toggle'));
  mouse(target.querySelector('.record-toggle .capture-glyph span'));
  mouse(target.querySelector('.quiet-action'));
  mouse(target.querySelector('.capture-brand'));
  mouse(surface, 2);
  await tick();
  assert(!state.counts.start_capture_drag, 'A control or non-primary click started dragging');
  mouse(surface);
  mouse(surface);
  await until(() => state.counts.start_capture_drag === 1, 'Drag was not requested exactly once');
  assert(
    target.querySelector('.capture.dragging'),
    'Drag cursor did not acknowledge pending movement',
  );
  assert(
    !state.counts.toggle_dictation && !state.counts.cancel_dictation,
    'Dragging changed recording',
  );
  drag.resolve();
  await until(
    () => !target.querySelector('.capture.dragging'),
    'Drag did not settle after release',
  );
  target.querySelector('.record-toggle').click();
  await until(
    () => target.querySelector('[aria-label="Copy transcript"]'),
    'Stop failed after dragging',
  );
});

await test('Split keeps errors and progress inside its signal pod', async () => {
  const state = await setup({ recording: true });
  state.runtime = { ...idle, busy: true, stage: 'transcribing', message: 'Transcribing · 45%' };
  state.dictation = { ...state.dictation, phase: 'finishing' };
  await emit('dictation-state', { ...state.dictation });
  await until(
    () => target.querySelector('.split-signal-pod').textContent.includes('45% complete'),
    'Progress escaped its panel',
  );
  assert(!target.querySelector('.capture-note'), 'Progress created a third surface');
  state.runtime = { ...idle, stage: 'error', message: 'Synthetic microphone problem.' };
  state.dictation = { ...state.dictation, phase: 'error', message: state.runtime.message };
  await emit('dictation-state', { ...state.dictation });
  await until(
    () =>
      target
        .querySelector('.split-signal-pod')
        .textContent.includes('Synthetic microphone problem.'),
    'Error missing inside signal pod',
  );
  assert(!target.querySelector('.capture-note'), 'Error created a third surface');
});

await test('TextEdit evaluation is opt-in, saves through existing settings, and restores off', async () => {
  let saved = defaultSettings();
  const state = await setup({
    main: true,
    intercept: (command, state, args) => {
      if (command === 'get_settings') return structuredClone(saved);
      if (command === 'save_settings') {
        saved = JSON.parse(JSON.stringify(args.settings));
        return saved;
      }
      if (command === 'reset_settings') {
        saved = defaultSettings();
        return saved;
      }
    },
  });
  const button = (label) =>
    [...target.querySelectorAll('button')].find((b) => b.textContent.trim() === label);
  await until(() => target.querySelector('#text-edit-observation'), 'Observer setting missing');
  button('Settings').click();
  await tick();
  const input = target.querySelector('#text-edit-observation');
  assert(!input.checked, 'External observation silently opted in');
  assert(
    target.querySelector('#text-edit-observation-help').textContent.includes('32'),
    'Bounded observation disclosure missing',
  );
  target.querySelector('label[for="text-edit-observation"]').click();
  await tick();
  assert(
    input.checked && !saved.textEditObservation,
    'Checkbox failed or saved before explicit Save',
  );
  input.focus();
  assert(document.activeElement === input, 'Checkbox lost native focus');
  button('Save changes').click();
  await until(() => saved.textEditObservation, 'Observer setting not saved');
  await until(() => !button('Restore defaults').disabled, 'Save stuck');
  button('Restore defaults').click();
  await until(
    () => !saved.textEditObservation && !input.checked,
    'Reset did not disable observation',
  );
  assert(
    !state.counts.start_recording && !state.counts.toggle_dictation,
    'Settings opened microphone',
  );
});

await test('Capture settings preview all six designs, save explicitly and restore Split', async () => {
  let saved = defaultSettings();
  const state = await setup({
    main: true,
    intercept: (command, state, args) => {
      if (command === 'get_settings') return structuredClone(saved);
      if (command === 'save_settings') {
        saved = JSON.parse(JSON.stringify(args.settings));
        return structuredClone(saved);
      }
      if (command === 'reset_settings') {
        saved = defaultSettings();
        return structuredClone(saved);
      }
    },
  });
  const button = (label) =>
    [...target.querySelectorAll('button')].find((b) => b.textContent.trim() === label);
  await until(() => target.querySelector('#capture-design'), 'Design setting missing');
  button('Settings').click();
  await tick();
  assert(
    target.querySelector('#capture-design').textContent.includes('Split'),
    'Split is not the default',
  );
  for (const design of ['ribbon', 'underline', 'studio', 'margin', 'counter', 'split']) {
    const previous = saved.captureDesign;
    target.querySelector('#capture-design').click();
    await tick();
    const options = [...document.querySelectorAll('[role="option"]')];
    assert(options.length === 6, 'Settings does not expose all six designs');
    options.find((o) => o.textContent.toLowerCase().startsWith(design)).click();
    await tick();
    assert(
      target.querySelector(`.design-preview [data-capture-design="${design}"]`),
      'Preview does not match selected design',
    );
    assert(saved.captureDesign === previous, 'Preview saved without Save changes');
    button('Save changes').click();
    await until(() => saved.captureDesign === design, 'Design did not save');
    await until(() => !button('Restore defaults').disabled, 'Save stayed pending');
  }
  assert(
    !state.counts.get_audio_features &&
      !state.counts.start_recording &&
      !state.counts.toggle_dictation,
    'Settings preview opened a microphone',
  );
  button('Restore defaults').click();
  await until(
    () => saved.captureDesign === 'split' && state.counts.reset_settings,
    'Defaults did not restore Split',
  );
});

await test('Capture preview animates locally, pauses when hidden, and finishes with replay', async () => {
  const state = await setup({ main: true });
  await until(() => target.querySelector('#capture-design'), 'Capture design missing');
  const navigate = (name) =>
    [...target.querySelectorAll('.sidebar nav button')]
      .find((button) => button.textContent.trim() === name)
      .click();
  const path = () => target.querySelector('.preview-wave path').getAttribute('d');
  const preview = () => target.querySelector('.design-preview');
  const control = () => target.querySelector('.preview-motion');
  const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const initial = path();
  await wait(100);
  assert(path() === initial, 'Preview animated on another screen');
  navigate('Settings');
  await tick();
  preview().scrollIntoView({ block: 'center' });
  if (!matchMedia('(prefers-reduced-motion: reduce)').matches) {
    await until(() => path() !== initial, 'Visible Settings preview did not animate');
    assert(
      (path().match(/M /g) ?? []).length === 45,
      'Preview does not use the shared Reed design',
    );
    control().click();
    await tick();
    const paused = path();
    await wait(150);
    assert(
      path() === paused && control().getAttribute('aria-label') === 'Resume preview',
      'Pause did not freeze the preview',
    );
    control().click();
    await tick();
    await until(() => path() !== paused, 'Resume did not continue the animation');
    navigate('Dictate');
    await tick();
    const hidden = path();
    await wait(150);
    assert(path() === hidden, 'Changing screens left the preview rendering');
    navigate('Settings');
    await tick();
    preview().scrollIntoView({ block: 'center' });
    await until(() => path() !== hidden, 'Returning to preview did not resume');
    const deadline = performance.now() + 10_500;
    let processed = false;
    while (
      control().getAttribute('aria-label') !== 'Replay preview' &&
      performance.now() < deadline
    ) {
      processed ||= preview().dataset.previewStage === 'processing';
      await wait(50);
    }
    assert(processed, 'Preview skipped transcribing');
    assert(
      preview().dataset.previewStage === 'complete' &&
        control().getAttribute('aria-label') === 'Replay preview',
      'Preview did not finish with Replay',
    );
    const complete = path();
    await wait(150);
    assert(path() === complete, 'Finished preview is still moving');
    control().click();
    await tick();
    assert(preview().dataset.previewStage === 'listening', 'Replay did not restart the sequence');
    await until(() => path() !== complete, 'Replay remains static');
  } else {
    assert(control().disabled, 'Reduced motion should suppress preview playback');
  }
  assert(
    !state.counts.get_audio_features &&
      !state.counts.start_recording &&
      !state.counts.toggle_dictation &&
      !state.counts.save_settings,
    'Preview invoked recording or saved settings',
  );
});

await test('Navigation changes icon state while keeping its target stationary', async () => {
  await setup({ main: true });
  await until(() => target.querySelector('#settings-language'), 'Settings not loaded');
  const buttons = [...target.querySelectorAll('.sidebar nav button')];
  for (const button of buttons) {
    const before = button.getBoundingClientRect();
    button.click();
    await tick();
    assert(button.getAttribute('aria-current') === 'page', 'Navigation selection missing');
    assert(button.querySelector('.icon.active'), 'Selected navigation icon did not change state');
    assert(
      target.querySelectorAll('.sidebar nav .icon.active').length === 1,
      'Multiple navigation icons are selected',
    );
    assert(
      getComputedStyle(button).transform === 'none',
      'Navigation target moved during activation',
    );
    const after = button.getBoundingClientRect();
    assert(
      before.width === after.width && before.height === after.height,
      'Navigation target changed size',
    );
    if (!matchMedia('(prefers-reduced-motion: reduce)').matches) {
      const detail = button.querySelector('.nav-detail');
      const animation = detail?.getAnimations()[0];
      assert(
        animation && animation.effect.getTiming().duration >= 500,
        'Icon motion is missing or too brief to follow',
      );
    }
  }
  const knob = target.querySelector('[data-icon="settings"] .first');
  await until(
    () => getComputedStyle(knob).transform === 'matrix(1, 0, 0, 1, 6, 0)',
    'Selected Settings icon did not settle',
  );
  const selected = buttons.at(-1);
  selected.click();
  await tick();
  if (!matchMedia('(prefers-reduced-motion: reduce)').matches) {
    const replay = [...selected.querySelectorAll('.nav-detail')].flatMap((part) =>
      part.getAnimations(),
    );
    assert(replay.length === 2, 'Selected icon click did not replay both slider details');
    await new Promise((resolve) => setTimeout(resolve, 300));
    assert(
      replay.some((animation) => animation.playState === 'running'),
      'Icon motion disappears too early',
    );
    buttons[0].click();
    await tick();
    assert(
      replay.every((animation) => animation.playState === 'idle'),
      'Switching away left the previous icon moving',
    );
  }
});

await test('Quiet model choices retain creator identity through selection and keyboard focus', async () => {
  await setup({ main: true });
  await until(() => target.querySelector('#models-asr-model'), 'Model choices not loaded');
  [...target.querySelectorAll('.sidebar nav button')]
    .find((button) => button.textContent.trim() === 'Models')
    .click();
  await tick();
  const speech = target.querySelector('#models-asr-model');
  const writing = target.querySelector('#models-cleanup-model');
  assert(speech.querySelector('.creator-name')?.textContent === 'Cohere', 'Speech creator missing');
  assert(
    writing.querySelector('.creator-name')?.textContent === 'Qwen · Alibaba',
    'Writing creator missing',
  );
  assert(
    speech.getAttribute('aria-label').includes('Cohere Transcribe'),
    'Selected model missing from accessible label',
  );
  speech.click();
  await tick();
  const speechOptions = [...document.querySelectorAll('[role="option"]')];
  assert(
    speechOptions.every((option) => option.querySelector('.creator-name')),
    'Some model options omit creator identity',
  );
  speechOptions
    .find((option) =>
      option.querySelector('.option-label').textContent.includes('Parakeet TDT 0.6B v2'),
    )
    .click();
  await tick();
  assert(
    speech.querySelector('.creator-name').textContent === 'NVIDIA',
    'Community-packaged model lost original creator identity',
  );
  assert(document.activeElement === speech, 'Speech selection did not restore trigger focus');
  writing.click();
  await tick();
  writing.dispatchEvent(new KeyboardEvent('keydown', { key: 'g', bubbles: true }));
  await tick();
  writing.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  await tick();
  assert(
    writing.querySelector('.creator-name').textContent === 'Google',
    'Typeahead selection attributed Gemma to its package publisher',
  );
  assert(
    writing.querySelector('.selected-label').textContent.includes('Gemma'),
    'Model identity did not update with selection',
  );
  assert(document.activeElement === writing, 'Writing selection did not retain focus');
});

await test('Model status icons distinguish downloaded files from selection and missing or unknown files', async () => {
  const saved = defaultSettings();
  const catalog = Object.entries(ASR_MODELS).map(([modelId, name], index) => ({
    ...installation,
    modelId,
    name,
    role: 'speech_recognition',
    quantization: null,
    selected: modelId === saved.models.asr,
    artifactsInstalled: index === 0,
    runtimeReady: true,
    localFileCount: 0,
    downloadedBytes: 0,
    removableBytes: 0,
    removalError: null,
  }));
  const state = await setup({
    main: true,
    intercept: (command) =>
      command === 'list_model_installations' ? structuredClone(catalog) : undefined,
  });
  await until(() => target.querySelector('#models-asr-model'), 'Model choices not loaded');
  [...target.querySelectorAll('.sidebar nav button')]
    .find((button) => button.textContent.trim() === 'Models')
    .click();
  await tick();
  const trigger = target.querySelector('#models-asr-model');
  trigger.click();
  await tick();
  const options = [...document.querySelectorAll('[role="option"]')];
  const installed = options.find((option) => option.querySelector('[data-status="downloaded"]'));
  const selected = options.find((option) => option.getAttribute('aria-selected') === 'true');
  assert(
    installed && installed.getAttribute('aria-label').includes('Downloaded'),
    'Downloaded status lost its accessible name',
  );
  assert(
    !installed.textContent.includes('Downloaded'),
    'Repeated downloaded text is still visible',
  );
  assert(
    !installed.querySelector('.selected-check svg') &&
      installed.querySelector('[data-icon="downloaded"]'),
    'Downloaded state looks like the selection check',
  );
  assert(
    selected.querySelector('.selected-check svg') &&
      selected.querySelector('[data-status="missing"]'),
    'Selected model falsely appears downloaded',
  );
  assert(
    installed.querySelector('.model-status').title === 'Downloaded',
    'Downloaded icon has no hover explanation',
  );
  assert(
    options.some((option) => option.querySelector('[data-status="unknown"]')),
    'Unknown packages were labeled missing',
  );
  trigger.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true }));
  await tick();
  trigger.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  await tick();
  assert(
    document.activeElement === trigger && trigger.textContent.includes('Parakeet'),
    'Icon rows broke keyboard selection or focus',
  );
  assert(
    !state.counts.install_model && !state.counts.save_settings && !state.counts.remove_model,
    'Browsing icons caused a native mutation',
  );
});

await test('Checkbox labels preserve native toggling and focus with visible field boundaries', async () => {
  await setup({ main: true });
  await until(() => target.querySelector('#cleanup-enabled'), 'Settings not loaded');
  [...target.querySelectorAll('.sidebar nav button')]
    .find((b) => b.textContent.trim() === 'Settings')
    .click();
  await tick();
  const checkbox = target.querySelector('#cleanup-enabled');
  const initial = checkbox.checked;
  target.querySelector('label[for="cleanup-enabled"]').click();
  await tick();
  assert(checkbox.checked !== initial, 'Styled label no longer toggles the checkbox');
  assert(checkbox.type === 'checkbox', 'Native checkbox semantics were replaced');
  // A synthetic label click is not keyboard input. Move script focus from a
  // text input (which requires visible focus), matching keyboard-focus inheritance.
  const focusAnchor = document.createElement('input');
  document.body.appendChild(focusAnchor);
  focusAnchor.focus();
  checkbox.focus();
  focusAnchor.remove();
  assert(document.activeElement === checkbox, 'Native checkbox focus was lost');
  target.querySelector('label[for="cleanup-enabled"]').click();
  await tick();
  assert(checkbox.checked === initial, 'Repeated toggle lost its state');
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 1;
  const context = canvas.getContext('2d');
  const luminance = (color) => {
    context.clearRect(0, 0, 1, 1);
    context.fillStyle = color;
    context.fillRect(0, 0, 1, 1);
    const rgb = [...context.getImageData(0, 0, 1, 1).data]
      .slice(0, 3)
      .map((c) => c / 255)
      .map((c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
    return rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722;
  };
  const ratio = (a, b) => {
    const values = [luminance(a), luminance(b)].sort((a, b) => b - a);
    return (values[0] + 0.05) / (values[1] + 0.05);
  };
  const tokens = getComputedStyle(document.documentElement);
  const panel = tokens.getPropertyValue('--color-panel');
  assert(
    ratio(tokens.getPropertyValue('--color-control-edge'), panel) >= 3,
    'Field boundary contrast is too low',
  );
  assert(ratio(tokens.getPropertyValue('--color-focus'), panel) >= 3, 'Focus contrast is too low');
  assert(
    ratio(
      tokens.getPropertyValue('--color-accent-light'),
      tokens.getPropertyValue('--color-on-accent'),
    ) >= 4.5,
    'Primary gradient label contrast is too low',
  );
});

await test('Shared disclosure arrows track native open state and retain keyboard focus', async () => {
  await setup({ main: true });
  await until(() => target.querySelector('#settings-language'), 'Settings not loaded');
  [...target.querySelectorAll('.sidebar nav button')]
    .find((b) => b.textContent.trim() === 'Models')
    .click();
  await tick();
  const details = target.querySelector('.engine-definitions');
  const summary = details.querySelector('summary');
  summary.focus();
  summary.click();
  await tick();
  assert(
    details.open && document.activeElement === summary,
    'Disclosure did not open with focus retained',
  );
  assert(getComputedStyle(summary, '::after').content === '""', 'Disclosure arrow is missing');
  summary.click();
  await tick();
  assert(
    !details.open && document.activeElement === summary,
    'Disclosure did not close with focus retained',
  );
});

if (unexpected.length)
  results.push({
    name: 'No unexpected native commands or unhandled errors',
    passed: false,
    error: unexpected.join(', '),
  });
window.regressionResults = results;
document.documentElement.dataset.testStatus = results.every((result) => result.passed)
  ? 'passed'
  : 'failed';
report.textContent = JSON.stringify(results, null, 2);
