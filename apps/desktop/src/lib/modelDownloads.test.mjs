import assert from 'node:assert/strict';
import test from 'node:test';
import { defaultSettings } from './config.ts';
import {
  downloadBytes,
  downloadPercent,
  downloadKey,
  downloadTarget,
  matchingDownload,
  downloadOptionStatus,
  knownDownloadSelection,
  automaticAudioDownloads,
  audioSetupRetryDelay,
} from './modelDownloads.ts';
import { MODEL_PRESETS } from './modelPresets.ts';

test('automatic audio setup respects native admission and never chooses DPDF4 or main models', () => {
  const entry = (modelId, changes = {}) => ({
    modelId,
    name: modelId,
    role: 'audio_frontend',
    quantization: null,
    selected: true,
    supported: true,
    runtimeReady: true,
    artifactsInstalled: false,
    ...changes,
  });
  const catalog = [
    entry('silero-v6.2'),
    entry('dpdfnet4'),
    entry('dpdfnet8'),
    entry('nvidia/Nemotron-3-Diarization'),
    entry(defaultSettings().models.asr, { role: 'speech_recognition' }),
  ];
  const ids = (rows) => automaticAudioDownloads(rows).map((e) => e.modelId);
  assert.deepEqual(ids(catalog), ['dpdfnet8', 'silero-v6.2', 'nvidia/Nemotron-3-Diarization']);
  for (const override of [{ selected: false }, { supported: false }, { runtimeReady: false }]) {
    assert.deepEqual(ids(catalog.map((e) => ({ ...e, ...override }))), []);
  }
  const installed = catalog.map((e) => ({ ...e, artifactsInstalled: true }));
  assert.deepEqual(ids(installed), ['dpdfnet8', 'silero-v6.2', 'nvidia/Nemotron-3-Diarization']);
  assert.deepEqual(ids([]), []);
});

test('automatic setup retry intervals are bounded and avoid immediate retry loops', () => {
  assert.deepEqual(
    [1, 2, 3, 4, 100].map(audioSetupRetryDelay),
    [30_000, 120_000, 600_000, 600_000, 600_000],
  );
});

test('progress uses byte totals, bounds invalid values and never rounds unfinished files to 100%', () => {
  assert.equal(downloadPercent(null), null);
  assert.equal(downloadPercent({ completedBytes: 1, totalBytes: 0 }), null);
  assert.equal(downloadPercent({ completedBytes: NaN, totalBytes: 10 }), null);
  assert.equal(downloadPercent({ completedBytes: 9999, totalBytes: 10000 }), 99.9);
  assert.equal(downloadPercent({ completedBytes: 5, totalBytes: 20 }), 25);
  assert.equal(downloadPercent({ completedBytes: 20, totalBytes: 20 }), 100);
  assert.equal(downloadBytes(1024 ** 3), '1 GiB');
  assert.equal(downloadBytes(0), '0 B');
});

test('speech downloads never inherit cleanup precision', () => {
  const config = defaultSettings();
  config.models.asr = 'nvidia/nemotron-3.5-asr-streaming-0.6b';
  config.rewriteQuantization = 'q8_0';
  assert.equal(downloadTarget(config, 'asr').modelId, config.models.asr);
  assert.equal(downloadTarget(config, 'asr').quantization, null);
  assert.equal(downloadTarget(config, 'rewriter').quantization, 'q8_0');
});

test('Q4 installed status cannot be used for a Q8 selection', () => {
  const config = defaultSettings();
  const q4 = { ...downloadTarget(config, 'rewriter'), artifactsInstalled: true };
  config.rewriteQuantization = 'q8_0';
  const q8 = downloadTarget(config, 'rewriter');
  assert.notEqual(downloadKey(q4), downloadKey(q8));
  assert.equal(matchingDownload([q4], q8), undefined);
  assert.equal(matchingDownload([q4, q8], q8), q8);
});

test('custom downloads use profile IDs with no built-in quantization', () => {
  const config = defaultSettings();
  config.models.customAsr = { id: 'custom-speech', name: 'Speech profile' };
  config.models.customRewriter = { id: 'custom-cleanup', name: 'Cleanup profile' };
  const speech = downloadTarget(config, 'asr');
  const cleanup = downloadTarget(config, 'rewriter');
  assert.equal(speech.modelId, 'custom-speech');
  assert.equal(cleanup.modelId, 'custom-cleanup');
  assert.equal(cleanup.quantization, null);
  assert.equal(matchingDownload([{ ...cleanup, role: 'speech_recognition' }], cleanup), undefined);
});

const stateWith = (catalog) => ({
  catalog,
  checking: false,
  installingKey: null,
  installing: false,
  cancelling: false,
  details: null,
  progress: '',
  error: null,
  feedback: null,
});

test('menus identify downloaded cleanup precisions without claiming the other one is installed', () => {
  const saved = defaultSettings();
  const draft = { ...saved, rewriteQuantization: 'q8_0' };
  const q4 = {
    ...downloadTarget(saved, 'rewriter'),
    artifactsInstalled: true,
    supported: true,
    localFileCount: 1,
  };
  const q8 = {
    ...downloadTarget(draft, 'rewriter'),
    artifactsInstalled: false,
    supported: true,
    localFileCount: 0,
  };
  const state = stateWith([q4, q8]);
  assert.equal(
    downloadOptionStatus(draft, saved, 'rewriter', state, true, true).label,
    'Q4 downloaded',
  );
  assert.equal(downloadOptionStatus(draft, saved, 'rewriter', state, true).label, 'Not downloaded');
  assert.equal(downloadOptionStatus(saved, saved, 'rewriter', state, true).label, 'Downloaded');
  assert.equal(downloadOptionStatus(saved, saved, 'rewriter', state, true).kind, 'downloaded');
  assert.equal(downloadOptionStatus(draft, saved, 'rewriter', state, true).kind, 'missing');
  assert.equal(downloadOptionStatus(draft, saved, 'rewriter', state, true, true).detail, 'Q4');
  q8.localFileCount = 1;
  assert.equal(
    downloadOptionStatus(draft, saved, 'rewriter', state, true).label,
    'Incomplete download',
  );
  assert.equal(downloadOptionStatus(draft, saved, 'rewriter', state, true).kind, 'warning');
  q4.artifactsInstalled = false;
  q4.localFileCount = 0;
  assert.equal(downloadOptionStatus(saved, saved, 'rewriter', state, true).label, 'Not downloaded');
});

test('unknown and pending model status cannot masquerade as not downloaded', () => {
  const saved = defaultSettings();
  assert.equal(
    downloadOptionStatus(saved, saved, 'asr', stateWith([]), true).label,
    'Status unavailable',
  );
  assert.equal(downloadOptionStatus(saved, saved, 'asr', stateWith([]), true).kind, 'unknown');
  assert.equal(
    downloadOptionStatus(saved, saved, 'asr', { ...stateWith([]), checking: true }, true).label,
    'Checking…',
  );
  assert.equal(
    downloadOptionStatus(saved, saved, 'asr', { ...stateWith([]), checking: true }, true).kind,
    'busy',
  );
  assert.equal(
    downloadOptionStatus(saved, saved, 'asr', stateWith([]), false).label,
    'Desktop only',
  );
  const entry = { ...downloadTarget(saved, 'asr'), artifactsInstalled: true, localFileCount: 1 };
  assert.equal(
    downloadOptionStatus(
      saved,
      saved,
      'asr',
      { ...stateWith([entry]), installingKey: downloadKey(entry) },
      true,
    ).label,
    'Downloading…',
  );
});

test('browsing a shipped package can inspect files without activating the selection', () => {
  const saved = defaultSettings();
  const before = JSON.stringify(saved);
  const draft = structuredClone(saved);
  draft.models.asr = 'nvidia/parakeet-tdt-0.6b-v3';
  assert.equal(knownDownloadSelection(draft, saved, 'asr'), true);
  draft.models.customRewriter = structuredClone(
    MODEL_PRESETS.find((p) => p.profile.runtime === 'llama_cpp').profile,
  );
  assert.equal(knownDownloadSelection(draft, saved, 'rewriter'), true);
  assert.equal(JSON.stringify(saved), before);
});

test('an edited imported profile or a shadowed preset cannot reuse another declaration’s file status', () => {
  const saved = defaultSettings();
  saved.models.customRewriter = structuredClone(
    MODEL_PRESETS.find((p) => p.profile.runtime === 'llama_cpp').profile,
  );
  const draft = structuredClone(saved);
  assert.equal(knownDownloadSelection(draft, saved, 'rewriter'), true);
  draft.models.customRewriter.files[0].sha256 = 'a'.repeat(64);
  assert.equal(knownDownloadSelection(draft, saved, 'rewriter'), false);
  assert.equal(knownDownloadSelection(saved, draft, 'rewriter'), false);
  draft.models.customRewriter.id = 'new-import';
  assert.equal(knownDownloadSelection(draft, saved, 'rewriter'), false);
  assert.equal(knownDownloadSelection(saved, null, 'rewriter'), false);
});

test('an existing CPU installation stays ready while an optimized update is available', () => {
  const config = defaultSettings();
  const entry = {
    ...downloadTarget(config, 'asr'),
    ready: true,
    artifactsInstalled: false,
    supported: true,
    localFileCount: 6,
  };
  assert.deepEqual(downloadOptionStatus(config, config, 'asr', stateWith([entry]), true), {
    label: 'Ready · update available',
    tone: 'ready',
    kind: 'downloaded',
  });
});

test('backend and language edits cannot reuse saved speech artifact readiness', () => {
  const saved = defaultSettings();
  for (const draft of [
    { ...saved, computeBackend: 'cpu' },
    { ...saved, language: 'fr' },
  ]) {
    assert.equal(knownDownloadSelection(draft, saved, 'asr'), false);
    assert.equal(knownDownloadSelection(draft, saved, 'rewriter'), true);
  }
});

test('a re-check keeps every known model row on its last status', () => {
  const saved = defaultSettings();
  const speech = { ...downloadTarget(saved, 'asr'), artifactsInstalled: true, localFileCount: 1 };
  const writer = {
    ...downloadTarget(saved, 'rewriter'),
    supported: true,
    artifactsInstalled: false,
    localFileCount: 0,
  };
  // Verifying the speech model re-checks the whole catalog; the writer row
  // must not flip to "Checking…" while that runs.
  const rechecking = { ...stateWith([speech, writer]), checking: true };
  assert.equal(downloadOptionStatus(saved, saved, 'asr', rechecking, true).label, 'Downloaded');
  assert.equal(
    downloadOptionStatus(saved, saved, 'rewriter', rechecking, true).label,
    'Not downloaded',
  );
});
