import assert from 'node:assert/strict';
import test from 'node:test';
import { sidebarActivity } from './sidebarActivity.ts';

const idle = {
  catalog: [],
  checking: false,
  installingKey: null,
  installing: false,
  cancelling: false,
  details: null,
  progress: '',
  error: null,
  feedback: null,
};
const runtime = (stage, extra = {}) => ({ busy: true, stage, download: null, ...extra });

test('nothing running shows nothing', () => {
  assert.equal(sidebarActivity(null, idle), null);
  assert.equal(sidebarActivity({ busy: false, stage: 'idle' }, idle), null);
});

test('a model download names the model and reports its percent', () => {
  const entry = {
    role: 'speech_recognition',
    modelId: 'nvidia/parakeet-tdt-0.6b-v3',
    quantization: null,
    name: 'Parakeet',
  };
  const downloading = {
    ...idle,
    catalog: [entry],
    installing: true,
    installingKey: 'speech_recognition:nvidia/parakeet-tdt-0.6b-v3:default',
    details: { phase: 'downloading', completedBytes: 250, totalBytes: 1000 },
  };
  const shown = sidebarActivity(runtime('installing'), downloading);
  assert.equal(shown.view, 'models');
  assert.equal(shown.percent, 25);
  assert.equal(shown.label, 'Downloading Parakeet');
  const verifying = { ...downloading, details: { ...downloading.details, phase: 'verifying' } };
  assert.equal(sidebarActivity(null, verifying).label, 'Verifying Parakeet');
  assert.equal(
    sidebarActivity(null, { ...downloading, cancelling: true }).label,
    'Cancelling download',
  );
});

test('recording and transcription point back to Dictate', () => {
  const recording = sidebarActivity(runtime('recording'), idle);
  assert.deepEqual(recording, { label: 'Recording', percent: null, view: 'dictate', live: true });
  assert.equal(sidebarActivity(runtime('transcribing'), idle).label, 'Transcribing');
  assert.equal(sidebarActivity(runtime('removing'), idle).view, 'models');
});

test('the background download from the Neural Engine work shows when nothing else runs', () => {
  const background = { running: true, progress: { completedBytes: 1, totalBytes: 4 }, error: null };
  assert.deepEqual(sidebarActivity(null, idle, background), {
    label: 'Preparing other languages',
    percent: 25,
    view: 'models',
    live: false,
  });
  // Foreground work wins over the background download.
  assert.equal(sidebarActivity(runtime('recording'), idle, background).label, 'Recording');
});
