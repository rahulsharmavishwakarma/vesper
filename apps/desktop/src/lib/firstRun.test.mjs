import assert from 'node:assert/strict';
import test from 'node:test';
import {
  FIRST_RUN_ENABLED,
  FIRST_RUN_STEPS,
  READY_STEP,
  firstRunRequested,
  resumeStep,
  shouldShowFirstRun,
} from './firstRun.ts';

test('setup shows only for a pending profile', () => {
  assert.equal(shouldShowFirstRun({ status: 'pending' }, true), true);
  assert.equal(shouldShowFirstRun({ status: 'completed' }, true), false);
  // A completed profile never sees it again, even when a preview asks.
  assert.equal(shouldShowFirstRun({ status: 'completed' }, false, true), false);
});

test('setup stays off for real users until it is enabled', () => {
  assert.equal(FIRST_RUN_ENABLED, false);
  assert.equal(shouldShowFirstRun({ status: 'pending' }), false);
  assert.equal(shouldShowFirstRun({ status: 'pending' }, false, true), true);
});

test('a pending flow resumes at a valid saved step', () => {
  assert.equal(FIRST_RUN_STEPS.length, 7);
  assert.equal(READY_STEP, 7);
  assert.equal(resumeStep(3), 3);
  assert.equal(resumeStep(READY_STEP), READY_STEP);
  for (const bad of [-1, 8, 2.5, '3', null, undefined]) assert.equal(resumeStep(bad), 0);
});

test('previews open setup with ?first-run', () => {
  assert.equal(firstRunRequested('?first-run'), true);
  assert.equal(firstRunRequested('?first-run=1&x=2'), true);
  assert.equal(firstRunRequested('?other'), false);
});

test('the recommended speech model follows the language', async () => {
  const { recommendedSpeechModel, COHERE, QWEN_ASR, SETUP_LANGUAGES, SETUP_SPEECH_MODELS } =
    await import('./firstRun.ts');
  assert.deepEqual(
    SETUP_LANGUAGES.map((l) => l.value),
    ['en', 'fr', 'hi'],
  );
  assert.equal(recommendedSpeechModel('en'), COHERE);
  assert.equal(recommendedSpeechModel('fr'), COHERE);
  assert.equal(recommendedSpeechModel('hi'), QWEN_ASR);
  assert.ok(SETUP_SPEECH_MODELS.some((m) => m.id === COHERE));
});

test('download size counts only missing files and needs a quarter more space', async () => {
  const { remainingBytes, spaceNeeded, formatBytes } = await import('./firstRun.ts');
  const catalog = [
    {
      modelId: 'a',
      quantization: null,
      files: [
        { installed: true, sizeBytes: 500 },
        { installed: false, sizeBytes: 1000 },
      ],
    },
    { modelId: 'w', quantization: 'q4_0', files: [{ installed: false, sizeBytes: 2000 }] },
    { modelId: 'w', quantization: 'q8_0', files: [{ installed: false, sizeBytes: 9000 }] },
  ];
  assert.equal(
    remainingBytes(catalog, [
      { modelId: 'a', quantization: null },
      { modelId: 'w', quantization: 'q4_0' },
    ]),
    3000,
  );
  assert.equal(remainingBytes(catalog, [{ modelId: 'missing', quantization: null }]), 0);
  assert.equal(spaceNeeded(4000), 5000);
  assert.equal(formatBytes(4_313_000_000), '4.3 GB');
  assert.equal(formatBytes(28_000_000), '28 MB');
  assert.equal(formatBytes(214_000_000_000), '214 GB');
});

test('the sample sentence follows the style and the taught words', async () => {
  const { sampleWritten } = await import('./firstRun.ts');
  const text = (tokens) => tokens.map((t) => t.text).join(' ');
  assert.equal(
    text(sampleWritten('minimal', [])),
    'So, um, can you send the arc light notes to Priya by Friday?',
  );
  assert.equal(
    text(sampleWritten('balanced', [])),
    'Can you send the arc light notes to Priya by Friday?',
  );
  const taught = sampleWritten('balanced', ['Arclight']);
  assert.equal(text(taught), 'Can you send the Arclight notes to Priya by Friday?');
  assert.deepEqual(
    taught.filter((t) => t.taught).map((t) => t.text),
    ['Arclight'],
  );
  // A word the sentence does not contain changes nothing.
  assert.equal(text(sampleWritten('balanced', ['Zanzibar'])), text(sampleWritten('balanced', [])));
});

test('added words are trimmed, bounded and unique', async () => {
  const { vocabularyWord } = await import('./firstRun.ts');
  assert.deepEqual(vocabularyWord('  Arc   light ', []), { word: 'Arc light' });
  assert.deepEqual(vocabularyWord('   ', []), { error: 'Type a word first.' });
  assert.ok('error' in vocabularyWord('x'.repeat(41), []));
  assert.ok('error' in vocabularyWord('arclight', ['Arclight']));
});
