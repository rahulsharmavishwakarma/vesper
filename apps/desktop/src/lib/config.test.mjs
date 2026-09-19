import assert from 'node:assert/strict';
import test from 'node:test';
import {
  CAPTURE_DESIGNS,
  CLEANUP_MODELS,
  CLEANUP_MODEL_OPTIONS,
  CLEANUP_QUANTIZATIONS,
  CLEANUP_QUANTIZATION_OPTIONS,
  defaultSettings,
  recordingShortcuts,
  effectiveRecordingSeconds,
  parseModelProfileJson,
  parseStoredSettings,
  transcriptionLanguageOptions,
  validateSettings,
} from './config.ts';

function cleanupProfile() {
  return {
    id: 'test-cleanup',
    name: 'Test cleanup',
    repository: 'example/test-model',
    revision: 'a'.repeat(40),
    runtime: 'llama_cpp',
    license: 'Apache-2.0',
    files: [{ role: 'weights', path: 'model.gguf', sha256: 'b'.repeat(64), sizeBytes: 42 }],
  };
}

test('older settings gain nullable profiles without losing existing preferences', () => {
  const stored = defaultSettings();
  delete stored.models.customAsr;
  delete stored.models.customRewriter;
  delete stored.computeBackend;
  delete stored.globalShortcut;
  delete stored.recordingMode;
  delete stored.dictationOutput;
  delete stored.floatingBarEnabled;
  delete stored.captureDesign;
  delete stored.notificationsEnabled;
  stored.maxRecordingSeconds = 300;
  stored.customCleanupInstruction = 'Preserve product names.';
  const restored = parseStoredSettings(stored);
  assert.equal(restored.models.customAsr, null);
  assert.equal(restored.models.customRewriter, null);
  assert.equal(restored.maxRecordingSeconds, 300);
  assert.equal(
    effectiveRecordingSeconds(restored),
    0,
    'Old settings must not reinstate a duration quota',
  );
  assert.equal(restored.customCleanupInstruction, 'Preserve product names.');
  assert.equal(restored.computeBackend, 'auto');
  assert.equal(restored.captureDesign, 'split');
  assert.equal(restored.globalShortcut, 'CommandOrControl+Shift+Space');
  assert.equal(restored.recordingMode, 'toggle');
  assert.equal(restored.dictationOutput, 'app');
  assert.equal(restored.floatingBarEnabled, true);
  assert.equal(restored.notificationsEnabled, false);
  assert.throws(() => parseStoredSettings({ ...stored, models: null }));
});

test('new settings remain formatting-only and stored context correction is preserved', () => {
  const defaults = defaultSettings();
  assert.equal(defaults.models.asr, 'CohereLabs/cohere-transcribe-03-2026');
  assert.equal(defaults.models.rewriter, 'Qwen/Qwen3.5-0.8B');
  assert.equal(defaults.cleanupEnabled, true);
  assert.equal(defaults.language, 'en');
  assert.deepEqual(validateSettings(defaults), []);
  assert.equal(defaults.cleanupPreset, 'minimal');
  const stored = { ...defaultSettings(), cleanupPreset: 'balanced' };
  assert.equal(parseStoredSettings(stored).cleanupPreset, 'balanced');
});

test('transcription language choices follow the selected speech model', () => {
  const settings = defaultSettings();
  const values = () => transcriptionLanguageOptions(settings.models).map((option) => option.value);
  assert.deepEqual(values(), ['en', 'fr']);
  settings.models.asr = 'nvidia/nemotron-3.5-asr-streaming-0.6b';
  assert.deepEqual(values(), ['auto', 'en', 'fr', 'hi']);

  settings.models.asr = 'nvidia/parakeet-tdt-0.6b-v3';
  assert.deepEqual(values(), ['auto', 'en', 'fr']);
  settings.models.asr = 'CohereLabs/cohere-transcribe-03-2026';
  assert.deepEqual(values(), ['en', 'fr']);

  settings.models.customAsr = {
    ...cleanupProfile(),
    runtime: 'sherpa_onnx_transducer',
    modelType: 'nemo_transducer',
    languages: ['hi'],
    files: ['encoder', 'decoder', 'joiner', 'tokens'].map((role) => ({
      role,
      path: `${role}.${role === 'tokens' ? 'txt' : 'onnx'}`,
      sha256: 'b'.repeat(64),
      sizeBytes: 42,
    })),
  };
  assert.deepEqual(values(), ['auto', 'hi']);
});

test('quick capture preferences accept only supported shortcuts', () => {
  const settings = defaultSettings();
  settings.globalShortcut = 'CommandOrControl+Shift+D';
  settings.floatingBarEnabled = false;
  settings.notificationsEnabled = true;
  assert.deepEqual(validateSettings(settings), []);
  settings.globalShortcut = 'Shift+Q';
  assert.ok(validateSettings(settings).some((error) => error.includes('shortcut')));
});

test('a pinned cleanup profile round trips through persisted settings', () => {
  const settings = defaultSettings();
  settings.models.customRewriter = parseModelProfileJson(JSON.stringify(cleanupProfile()));
  assert.deepEqual(parseStoredSettings(JSON.parse(JSON.stringify(settings))), settings);
});

test('cleanup choices expose only packaged models and quantizations', () => {
  assert.deepEqual(Object.keys(CLEANUP_MODELS), ['Qwen/Qwen3.5-0.8B']);
  assert.equal(CLEANUP_MODELS['Qwen/Qwen3.5-0.8B'].runtime, 'llama_cpp');
  assert.deepEqual(CLEANUP_MODELS['Qwen/Qwen3.5-0.8B'].quantizations, ['q4_0', 'q8_0']);
  assert.deepEqual(
    CLEANUP_MODEL_OPTIONS.map((option) => option.value),
    ['Qwen/Qwen3.5-0.8B'],
  );
  assert.deepEqual(
    CLEANUP_QUANTIZATION_OPTIONS.map((option) => option.value),
    ['q4_0', 'q8_0'],
  );
  assert.equal(CLEANUP_QUANTIZATIONS.q4_0.sizeBytes, 563_036_064);
  assert.equal(CLEANUP_QUANTIZATIONS.q8_0.sizeBytes, 833_592_096);

  const unsupported = defaultSettings();
  unsupported.models.rewriter = 'research-only/model';
  assert.ok(
    validateSettings(unsupported).some((error) => error.includes('supported writing model')),
  );
});

test('unsafe repository paths and mutable revisions cannot become profiles', () => {
  for (const path of [
    '../model.gguf',
    '/model.gguf',
    'x//model.gguf',
    'x/%2e%2e/model.gguf',
    'model.gguf?token=secret',
    'model-00001-of-00002.gguf',
  ]) {
    const profile = cleanupProfile();
    profile.files[0].path = path;
    assert.throws(() => parseModelProfileJson(JSON.stringify(profile)), path);
  }
  for (const revision of ['main', 'v1', 'A'.repeat(40)]) {
    assert.throws(() => parseModelProfileJson(JSON.stringify({ ...cleanupProfile(), revision })));
  }
});

test('malformed, oversized and unknown profile fields are rejected', () => {
  assert.throws(() => parseModelProfileJson('{'));
  assert.throws(() => parseModelProfileJson(' '.repeat(32769)));
  assert.throws(() =>
    parseModelProfileJson(JSON.stringify({ ...cleanupProfile(), trust_remote_code: true })),
  );
  const missingChecksum = cleanupProfile();
  delete missingChecksum.files[0].sha256;
  assert.throws(() => parseModelProfileJson(JSON.stringify(missingChecksum)));
});

test('profile roles cannot be exchanged between speech and cleanup settings', () => {
  const settings = defaultSettings();
  settings.models.customAsr = cleanupProfile();
  assert.ok(validateSettings(settings).length > 0);
});

test('custom speech language metadata supersedes the built-in Parakeet restriction', () => {
  const settings = defaultSettings();
  settings.models.asr = 'nvidia/parakeet-tdt-0.6b-v3';
  settings.language = 'hi';
  assert.ok(validateSettings(settings).length > 0);
  settings.models.customAsr = {
    ...cleanupProfile(),
    runtime: 'sherpa_onnx_transducer',
    modelType: 'nemo_transducer',
    languages: ['hi'],
    files: ['encoder', 'decoder', 'joiner', 'tokens'].map((role) => ({
      role,
      path: `${role}.${role === 'tokens' ? 'txt' : 'onnx'}`,
      sha256: 'b'.repeat(64),
      sizeBytes: 42,
    })),
  };
  assert.deepEqual(validateSettings(settings), []);
  settings.language = 'en';
  assert.ok(validateSettings(settings).length > 0);
  settings.language = 'auto';
  assert.deepEqual(validateSettings(settings), []);
});

test('LFM selection persists with English and CPU capabilities without changing defaults', () => {
  const settings = defaultSettings();
  settings.models.asr = 'LiquidAI/LFM2.5-Audio-1.5B';
  assert.deepEqual(
    transcriptionLanguageOptions(settings.models).map((o) => o.value),
    ['en'],
  );
  assert.deepEqual(validateSettings(settings), []);
  assert.equal(parseStoredSettings(settings).models.asr, settings.models.asr);
  for (const language of ['auto', 'fr', 'hi']) {
    assert.ok(validateSettings({ ...settings, language }).some((e) => e.includes('LFM')));
  }
  for (const computeBackend of ['metal', 'cuda', 'vulkan', 'rocm']) {
    assert.ok(validateSettings({ ...settings, computeBackend }).some((e) => e.includes('CPU')));
  }
  assert.equal(defaultSettings().models.asr, 'CohereLabs/cohere-transcribe-03-2026');
});

test('dictation preferences preserve explicit choices and reject unknown behavior', () => {
  const settings = { ...defaultSettings(), recordingMode: 'hold', dictationOutput: 'paste' };
  assert.deepEqual(parseStoredSettings(settings), settings);
  assert.ok(validateSettings({ ...settings, recordingMode: 'hybrid' }).length);
  assert.ok(validateSettings({ ...settings, dictationOutput: 'send' }).length);
});

// This preference must migrate without dropping model/recording choices.
test('all six capture designs survive storage; malformed values fail validation', () => {
  for (const captureDesign of CAPTURE_DESIGNS) {
    const config = { ...defaultSettings(), captureDesign, recordingMode: 'hold' };
    assert.deepEqual(parseStoredSettings(JSON.parse(JSON.stringify(config))), config);
  }
  for (const captureDesign of ['unknown', 'Split', '', null, 4]) {
    assert.throws(
      () => parseStoredSettings({ ...defaultSettings(), captureDesign }),
      /capture design/,
    );
  }
});

test('separate shortcuts preserve every legacy binding and reject explicit duplicates', () => {
  for (const recordingMode of ['hold', 'toggle']) {
    for (const globalShortcut of [
      'Alt+Shift+D',
      'CommandOrControl+Shift+D',
      'CommandOrControl+Shift+Space',
    ]) {
      const restored = parseStoredSettings({ recordingMode, globalShortcut });
      const keys = recordingShortcuts(restored);
      assert.equal(keys[recordingMode], globalShortcut);
      assert.notEqual(keys.hold, keys.toggle);
      assert.ok(
        validateSettings({
          ...restored,
          holdShortcut: globalShortcut,
          toggleShortcut: globalShortcut,
        }).some((error) => error.includes('different')),
      );
    }
  }
  const selected = {
    ...defaultSettings(),
    holdShortcut: 'CommandOrControl+Shift+D',
    toggleShortcut: 'Alt+Shift+D',
  };
  assert.deepEqual(recordingShortcuts(parseStoredSettings(JSON.parse(JSON.stringify(selected)))), {
    hold: selected.holdShortcut,
    toggle: selected.toggleShortcut,
  });
});

test('custom bindings persist, reject equivalent duplicates, and preserve an available fallback', () => {
  const selected = {
    ...defaultSettings(),
    holdShortcut: 'Control+Alt+K',
    toggleShortcut: 'Alt+F8',
  };
  assert.deepEqual(parseStoredSettings(JSON.parse(JSON.stringify(selected))), selected);
  assert.ok(
    validateSettings({ ...selected, toggleShortcut: 'Alt+Control+K' }).some((error) =>
      error.includes('different'),
    ),
  );
  for (const holdShortcut of [
    'K',
    'Shift+K',
    'Control',
    'Alt+Escape',
    'Alt+F25',
    'Fn',
    '',
    'Alt+Alt+K',
  ]) {
    assert.ok(validateSettings({ ...selected, holdShortcut }).length > 0, holdShortcut);
  }
  const legacy = parseStoredSettings({ recordingMode: 'toggle', globalShortcut: 'Shift+Alt+D' });
  assert.equal(recordingShortcuts(legacy).hold, 'CommandOrControl+Shift+D');
});

test('audio processing migrates off and validates opt-in model choices', () => {
  assert.deepEqual(parseStoredSettings({}).audioFrontend, {
    vadEnabled: false,
    autoLevelEnabled: false,
    noiseReduction: 'off',
  });
  assert.deepEqual(parseStoredSettings({ audioFrontend: { vadEnabled: true } }).audioFrontend, {
    vadEnabled: true,
    autoLevelEnabled: false,
    noiseReduction: 'off',
  });
  for (const noiseReduction of ['off', 'dpdfnet4', 'dpdfnet8']) {
    const config = {
      ...defaultSettings(),
      audioFrontend: { vadEnabled: true, autoLevelEnabled: true, noiseReduction },
    };
    assert.deepEqual(validateSettings(config), []);
    assert.deepEqual(parseStoredSettings(config).audioFrontend, config.audioFrontend);
  }
  for (const audioFrontend of [
    null,
    { vadEnabled: true, autoLevelEnabled: false, noiseReduction: 'unknown' },
    { vadEnabled: false, autoLevelEnabled: 'yes', noiseReduction: 'off' },
    { vadEnabled: false, noiseReduction: 'off', skipSilence: true },
  ]) {
    assert.ok(validateSettings({ ...defaultSettings(), audioFrontend }).length);
  }
});

test('external TextEdit evaluation is separate, defaults off, and rejects invalid consent', () => {
  assert.equal(defaultSettings().textEditObservation, false);
  const old = defaultSettings();
  delete old.textEditObservation;
  assert.equal(parseStoredSettings(old).textEditObservation, false);
  for (const invalid of [null, 'true', 1]) {
    assert.ok(validateSettings({ ...defaultSettings(), textEditObservation: invalid }).length > 0);
  }
});
