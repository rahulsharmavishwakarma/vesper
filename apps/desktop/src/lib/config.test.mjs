import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import {
  ASR_MODELS,
  CAPTURE_DESIGNS,
  CLEANUP_MODELS,
  CLEANUP_MODEL_OPTIONS,
  CLEANUP_QUANTIZATIONS,
  CLEANUP_QUANTIZATION_OPTIONS,
  LANGUAGE_OPTIONS,
  defaultSettings,
  recordingShortcuts,
  effectiveRecordingSeconds,
  parseModelProfileJson,
  parseStoredSettings,
  transcriptionLanguageError,
  transcriptionLanguageOptions,
  fallbackLanguage,
  pipelineLanguageOptions,
  writingLanguageError,
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
  assert.equal(Object.hasOwn(restored, 'speakerDiarization'), false);
  assert.throws(() => parseStoredSettings({ ...stored, models: null }));
});

test('automatic speaker analysis retires old settings without changing other preferences', () => {
  assert.equal(Object.hasOwn(defaultSettings(), 'speakerDiarization'), false);
  for (const value of [true, false, 'true', 1, null]) {
    const stored = { ...defaultSettings(), speakerDiarization: value, cleanupEnabled: false };
    const restored = parseStoredSettings(stored);
    assert.equal(Object.hasOwn(restored, 'speakerDiarization'), false);
    assert.equal(restored.cleanupEnabled, false);
    assert.equal(stored.speakerDiarization, value, 'Migration must not mutate the input');
    assert.deepEqual(parseStoredSettings(JSON.parse(JSON.stringify(restored))), restored);
  }
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
  // Default Cohere: 14 vendor-declared languages, no auto-detect, no Hindi.
  // Order follows the shared language options, not the manifest.
  assert.deepEqual(values(), [
    'ar',
    'zh',
    'nl',
    'en',
    'fr',
    'de',
    'el',
    'it',
    'ja',
    'ko',
    'pl',
    'pt',
    'es',
    'vi',
  ]);
  settings.models.asr = 'nvidia/nemotron-3.5-asr-streaming-0.6b';
  const nemotron = values();
  assert.equal(nemotron.length, 29);
  for (const code of ['auto', 'en', 'hi', 'zh', 'nb', 'et']) assert.ok(nemotron.includes(code));
  for (const code of ['el', 'yue']) assert.ok(!nemotron.includes(code));

  settings.models.asr = 'nvidia/parakeet-tdt-0.6b-v3';
  const parakeet = values();
  assert.equal(parakeet.length, 26);
  for (const code of ['auto', 'en', 'de', 'uk']) assert.ok(parakeet.includes(code));
  for (const code of ['hi', 'ja', 'zh']) assert.ok(!parakeet.includes(code));
  settings.models.asr = 'CohereLabs/cohere-transcribe-03-2026';
  assert.ok(!values().includes('auto') && !values().includes('hi'));

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

test('language choices respect the ASR backend and enabled writer without mutating settings', () => {
  const settings = defaultSettings();
  settings.models.asrEngine = 'standard'; // the standard ONNX/MLX engine
  const values = () => pipelineLanguageOptions(settings).map((option) => option.value);
  for (const asr of ['Qwen/Qwen3-ASR-0.6B', 'nvidia/parakeet-tdt-0.6b-v3']) {
    settings.models.asr = asr;
    settings.computeBackend = 'metal';
    assert.deepEqual(values(), ['en']);
    settings.language = 'fr';
    assert.ok(validateSettings(settings).some((error) => error.includes('processing device')));
    settings.computeBackend = 'auto';
    assert.ok(values().includes('fr'), 'Automatic can retain a compatible CPU path');
  }
  settings.models.asr = 'Qwen/Qwen3-ASR-0.6B';
  settings.models.customRewriter = { ...cleanupProfile(), languages: ['EN-us'] };
  assert.deepEqual(values(), ['en']);
  assert.equal(settings.language, 'fr', 'Filtering must not silently change a selection');
  assert.match(writingLanguageError(settings), /writing model/);
  settings.language = 'en';
  assert.deepEqual(validateSettings(settings), []);
  settings.models.customRewriter.languages = ['en', 'fr-FR'];
  assert.deepEqual(values(), ['en', 'fr']);
  settings.language = 'auto';
  assert.match(writingLanguageError(settings), /explicit language/);
  settings.cleanupEnabled = false;
  assert.equal(values().length, 31);
  assert.ok(values().includes('yue') && values().includes('fil'));
  assert.deepEqual(validateSettings(settings), []);
});

test('legacy unknown writer languages load for repair but cannot be saved enabled', () => {
  const settings = defaultSettings();
  settings.models.customRewriter = cleanupProfile();
  for (const languages of [undefined, []]) {
    settings.models.customRewriter.languages = languages;
    const stored = JSON.parse(JSON.stringify(settings));
    assert.deepEqual(parseStoredSettings(stored), stored);
    assert.ok(validateSettings(stored).some((error) => error.includes('no declared languages')));
    assert.deepEqual(pipelineLanguageOptions(stored), []);
    assert.deepEqual(validateSettings({ ...stored, cleanupEnabled: false }), []);
  }
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

test('LFM selection persists with English and native GPU capabilities without changing defaults', () => {
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
    assert.deepEqual(validateSettings({ ...settings, computeBackend }), []);
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
test('all three capture designs survive storage; malformed values fail validation', () => {
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

test('retired capture designs migrate to Split without losing other preferences', () => {
  for (const captureDesign of ['ribbon', 'underline', 'studio', 'margin', 'counter']) {
    const stored = {
      ...defaultSettings(),
      captureDesign,
      recordingMode: 'hold',
      customCleanupInstruction: 'Preserve product names.',
    };
    const restored = parseStoredSettings(JSON.parse(JSON.stringify(stored)));
    assert.equal(restored.captureDesign, 'split');
    assert.equal(restored.recordingMode, 'hold');
    assert.equal(restored.customCleanupInstruction, 'Preserve product names.');
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

test('desktop audio migrates to the managed profile and validates legacy inputs', () => {
  assert.deepEqual(parseStoredSettings({}).audioFrontend, {
    vadEnabled: true,
    autoLevelEnabled: false,
    noiseReduction: 'dpdfnet8',
  });
  assert.deepEqual(parseStoredSettings({ audioFrontend: { vadEnabled: true } }).audioFrontend, {
    vadEnabled: true,
    autoLevelEnabled: false,
    noiseReduction: 'dpdfnet8',
  });
  for (const noiseReduction of ['off', 'dpdfnet4', 'dpdfnet8']) {
    const config = {
      ...defaultSettings(),
      audioFrontend: { vadEnabled: true, autoLevelEnabled: true, noiseReduction },
    };
    assert.deepEqual(validateSettings(config), []);
    const restored = parseStoredSettings(config);
    assert.deepEqual(restored.audioFrontend, defaultSettings().audioFrontend);
    assert.equal(config.audioFrontend.noiseReduction, noiseReduction);
    assert.deepEqual(parseStoredSettings(restored), restored);
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

test('retired TextEdit observation is discarded without changing other preferences', () => {
  assert.equal(Object.hasOwn(defaultSettings(), 'textEditObservation'), false);
  for (const value of [true, false, 'true', 1, null]) {
    const stored = { ...defaultSettings(), textEditObservation: value, cleanupEnabled: false };
    const restored = parseStoredSettings(stored);
    assert.equal(Object.hasOwn(restored, 'textEditObservation'), false);
    assert.equal(restored.cleanupEnabled, false);
    assert.equal(stored.textEditObservation, value, 'Migration must not mutate the input');
    assert.deepEqual(parseStoredSettings(JSON.parse(JSON.stringify(restored))), restored);
  }
});

const languageManifest = JSON.parse(
  readFileSync(new URL('../../../../models/languages.json', import.meta.url), 'utf8'),
);

test('model switches fall back to a supported transcription language', () => {
  const cohere = () => {
    const settings = defaultSettings();
    settings.models = {
      ...settings.models,
      asr: 'CohereLabs/cohere-transcribe-03-2026',
    };
    return settings;
  };
  // Keeps the current language when the new model supports it.
  assert.equal(fallbackLanguage({ ...cohere(), language: 'fr' }), 'fr');
  assert.equal(fallbackLanguage({ ...cohere(), language: 'de' }), 'de');
  // Explicit-only Cohere: stranded auto and Hindi both fall back to English.
  assert.equal(fallbackLanguage({ ...cohere(), language: 'auto' }), 'en');
  assert.equal(fallbackLanguage({ ...cohere(), language: 'hi' }), 'en');
  // Auto-detect models keep automatic detection.
  const parakeet = defaultSettings();
  parakeet.models = { ...parakeet.models, asr: 'nvidia/parakeet-tdt-0.6b-v3' };
  assert.equal(fallbackLanguage({ ...parakeet, language: 'auto' }), 'auto');
  assert.equal(fallbackLanguage({ ...parakeet, language: 'fr' }), 'fr');
  // A custom profile keeps automatic detection when it offers it.
  const custom = defaultSettings();
  custom.models = { ...custom.models, customAsr: { languages: ['de'] } };
  assert.equal(fallbackLanguage({ ...custom, language: 'fr' }), 'auto');
  // A restricting writer narrows the fallback the same way.
  const written = cohere();
  written.cleanupEnabled = true;
  written.models = {
    ...written.models,
    customRewriter: { ...cleanupProfile(), languages: ['fr'] },
  };
  assert.equal(fallbackLanguage({ ...written, language: 'fr' }), 'fr');
  assert.equal(fallbackLanguage({ ...written, language: 'de' }), 'fr');
  // Otherwise the first offered explicit language wins.
  const narrowed = cohere();
  narrowed.cleanupEnabled = true;
  narrowed.models = {
    ...narrowed.models,
    customRewriter: { ...cleanupProfile(), languages: ['de'] },
  };
  assert.equal(fallbackLanguage({ ...narrowed, language: 'fr' }), 'de');
  assert.equal(fallbackLanguage({ ...narrowed, language: 'de' }), 'de');
});

test('every manifest speech language is offered per ASR model and validates', () => {
  const known = LANGUAGE_OPTIONS.map((option) => option.value);
  for (const [id, entry] of Object.entries(languageManifest)) {
    if (id.startsWith('_') || id === 'writers' || !entry || typeof entry !== 'object') continue;
    const settings = defaultSettings();
    settings.models = { ...settings.models, asr: id };
    const offered = transcriptionLanguageOptions(settings.models).map((option) => option.value);
    const expected = [...(entry.autoDetect ? ['auto'] : []), ...entry.languages];
    assert.deepEqual(new Set(offered), new Set(expected), `menu mismatch for ${id}`);
    for (const code of expected) {
      assert.equal(
        transcriptionLanguageError({ ...settings, language: code }),
        null,
        `${id} should accept ${code}`,
      );
    }
    const outside = known.find((code) => !expected.includes(code));
    if (outside !== undefined) {
      assert.notEqual(
        transcriptionLanguageError({ ...settings, language: outside }),
        null,
        `${id} should reject ${outside}`,
      );
    }
  }
  // Every selectable model must be declared; the manifest may additionally
  // cover evaluated-but-withheld models (Whisper: executable false, the
  // picker stays at eight choices per the documented quality gates).
  for (const id of Object.keys(ASR_MODELS)) {
    assert.ok(languageManifest[id], `selectable model ${id} must be declared in the manifest`);
  }
});

test('every manifest writer language validates on the writing path', () => {
  const writers = languageManifest.writers ?? {};
  assert.ok(Object.keys(writers).length > 0, 'manifest must declare built-in writers');
  for (const [id, entry] of Object.entries(writers)) {
    const settings = defaultSettings();
    settings.cleanupEnabled = true;
    settings.models = { ...settings.models, rewriter: id, customRewriter: null };
    const speech = transcriptionLanguageOptions(settings.models).map((option) => option.value);
    if (entry.unrestricted) {
      // No published per-language list: every speech-offered mode stays allowed.
      assert.deepEqual(
        pipelineLanguageOptions(settings).map((option) => option.value),
        speech,
        `unrestricted writer ${id} must not narrow speech options`,
      );
      for (const code of speech) {
        assert.equal(
          writingLanguageError({ ...settings, language: code }),
          null,
          `unrestricted writer ${id} should accept ${code}`,
        );
      }
      continue;
    }
    for (const code of entry.languages) {
      assert.equal(
        writingLanguageError({ ...settings, language: code }),
        null,
        `writer ${id} should accept ${code}`,
      );
    }
  }
});

test('a custom writing profile validates per declared language', () => {
  for (const code of LANGUAGE_OPTIONS.map((option) => option.value)) {
    if (code === 'auto') continue;
    const settings = defaultSettings();
    settings.cleanupEnabled = true;
    settings.models = {
      ...settings.models,
      customAsr: { languages: [code] },
      customRewriter: { ...cleanupProfile(), languages: [code] },
    };
    const settingsWithLanguage = { ...settings, language: code };
    assert.ok(
      pipelineLanguageOptions(settingsWithLanguage)
        .map((option) => option.value)
        .includes(code),
      `custom writer declaring ${code} should keep it offered`,
    );
    assert.equal(
      writingLanguageError(settingsWithLanguage),
      null,
      `custom writer declaring ${code} should validate`,
    );
  }
});

test('Automatic runs Parakeet as the unified GGUF and keeps every declared language on Metal', () => {
  const settings = defaultSettings();
  assert.equal(settings.models.asrEngine, 'automatic');
  settings.models.asr = 'nvidia/parakeet-tdt-0.6b-v3';
  const metal = (models) => transcriptionLanguageOptions(models, 'metal').map((o) => o.value);
  settings.models.asrEngine = 'standard';
  assert.deepEqual(metal(settings.models), ['en']);
  settings.models.asrEngine = 'automatic';
  assert.ok(metal(settings.models).includes('de'));
  assert.ok(metal(settings.models).includes('auto'));
});
