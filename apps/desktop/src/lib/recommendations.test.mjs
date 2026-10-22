import assert from 'node:assert/strict';
import test from 'node:test';
import evidence from '../../../../models/recommendations.json' with { type: 'json' };
import catalog from '../../../../models/catalog.json' with { type: 'json' };
import languages from '../../../../models/languages.json' with { type: 'json' };
import { defaultSettings } from './config.ts';
import { modelOptions } from './modelPresets.ts';
import {
  candidateGauges,
  decorateSpeechOptions,
  decorateWriterOptions,
  rankSpeechModels,
  recommendedSpeechModel,
} from './recommendations.ts';

const mac = (gb) => ({ os: 'macos', arch: 'aarch64', memoryBytes: gb * 1e9 });

test('every evidence entry names a catalog speech model and a language it declares', () => {
  const speech = new Set(
    catalog.models.filter((m) => m.role === 'speech_recognition').map((m) => m.id),
  );
  for (const section of [evidence.published, evidence.local.results])
    for (const [id, byLanguage] of Object.entries(section)) {
      assert.ok(speech.has(id), `${id} is not a catalog speech model`);
      for (const code of Object.keys(byLanguage))
        assert.ok(languages[id].languages.includes(code), `${id} does not declare ${code}`);
    }
  for (const results of Object.values(evidence.local.results))
    for (const result of Object.values(results))
      assert.ok(evidence.local.runs[result.run], `unknown run ${result.run}`);
  for (const byLanguage of Object.values(evidence.published))
    for (const figure of Object.values(byLanguage)) assert.match(figure.url, /^https:\/\//);
});

test('setup picks from published accuracy, breaking near-ties by measured speed', () => {
  // English leaderboard: Cohere 6.26, Parakeet 6.51 and Qwen 6.60 are within half a
  // point, so the fastest locally measured of them wins.
  assert.equal(recommendedSpeechModel('en', mac(64)), 'nvidia/parakeet-tdt-0.6b-v3');
  assert.equal(recommendedSpeechModel('hi', mac(64)), 'nvidia/nemotron-3.5-asr-streaming-0.6b');
  // German leaderboard FLEURS: Cohere 3.33 is more than half a point ahead.
  assert.equal(recommendedSpeechModel('de', mac(64)), 'CohereLabs/cohere-transcribe-03-2026');
  // Japanese has only makers' figures: Qwen (8.33) before Nemotron (13.87 CER).
  assert.equal(recommendedSpeechModel('ja', mac(64)), 'Qwen/Qwen3-ASR-0.6B');
});

test('this Mac filters what can be recommended', () => {
  const intel = { os: 'macos', arch: 'x86_64', memoryBytes: 16e9 };
  const ranked = rankSpeechModels('en', intel);
  const granite = ranked.find((c) => c.id === 'ibm-granite/granite-speech-5.0-470m-turboctc');
  assert.equal(granite.state, 'unavailable');
  assert.equal(granite.reason, 'Needs an Apple silicon Mac');
  // Cohere's measured peak (5.37 GB) is more than half of an 8 GB Mac.
  const small = rankSpeechModels('en', mac(8));
  assert.equal(
    small.find((c) => c.id === 'CohereLabs/cohere-transcribe-03-2026').state,
    'unavailable',
  );
  assert.ok(small.some((c) => c.state === 'recommended'));
});

test('gauges show published accuracy and only Vesper-measured speed and memory', () => {
  const ranked = rankSpeechModels('fr', mac(64));
  const cohere = ranked.find((c) => c.id === 'CohereLabs/cohere-transcribe-03-2026');
  const [accuracy, speed, memory] = candidateGauges(cohere, 'fr', mac(64));
  assert.equal(accuracy.text, '95.7%');
  assert.match(accuracy.title, /Open ASR Leaderboard/);
  // No French speed run exists, so speed is not borrowed from another language.
  assert.equal(speed.score, null);
  assert.equal(memory.text, '5.4 GB');
});

test('picker options are ordered best first, with tags and gauges', () => {
  const options = [
    { value: 'openai/whisper-small', description: 'English and French · INT8' },
    { value: 'nvidia/parakeet-tdt-0.6b-v3', description: '25 European languages' },
    { value: 'preset:custom', description: 'Imported model profile' },
  ];
  const decorated = decorateSpeechOptions(options, 'en', mac(64));
  assert.equal(decorated[0].value, 'nvidia/parakeet-tdt-0.6b-v3');
  assert.match(decorated[0].description, /^Best for English · /);
  assert.equal(decorated[0].gauges.length, 3);
  assert.equal(decorated.at(-1).value, 'preset:custom');
  // Automatic detection has no language to rank for.
  assert.deepEqual(decorateSpeechOptions(options, 'auto', mac(64)), options);
});

test('writers rank on published instruction following within one harness', () => {
  const writers = modelOptions(defaultSettings().models, 'rewriter');
  // English: Qwen3.5 0.8B passed Vesper's own meaning check, so it leads; the
  // rest follow the shared leaderboard (SmolLM2 1.7B first). Gemma's own report
  // is a different harness, so its higher number is not compared.
  const english = decorateWriterOptions(writers, 'en');
  assert.equal(english[0].value, 'Qwen/Qwen3.5-0.8B');
  assert.match(english[0].description, /^Best for English/);
  assert.equal(english[1].value, 'preset:hf-d152d789235fe4ec');
  assert.match(english[1].description, /^Published result/);
  // Hindi: only the multilingual writer declares it; the others are tagged.
  const hindi = decorateWriterOptions(writers, 'hi');
  assert.equal(hindi[0].value, 'Qwen/Qwen3.5-0.8B');
  assert.ok(hindi.slice(1).every((option) => option.disabled));
  // The Models page keeps them selectable.
  const page = decorateWriterOptions(writers, 'hi', { lockUnavailable: false });
  assert.ok(page.every((option) => !option.disabled));
});
