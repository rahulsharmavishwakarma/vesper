import assert from 'node:assert/strict';
import test from 'node:test';

import { summarize } from './summarize-asr-benchmarks.mjs';

test('groups only comparable runs and retains failures', () => {
  const evidence = summarize([
    {
      manifestSha256: 'a'.repeat(64),
      environment: {
        label: 'test',
        backend: 'cpu',
        hardware: 'test',
        operatingSystem: 'test',
        powerMode: 'AC',
        precision: 'int8',
        threads: 1,
        applicationRevision: 'abc',
        runtimeRevision: 'def',
        driver: 'none',
      },
      modelVersions: { asr: { modelId: 'example/asr' } },
      runs: [
        {
          language: 'hi',
          configuredLanguage: 'hi',
          mode: 'streaming',
          error: null,
          rawScore: { wordErrorRate: { rate: 0.1 } },
          realTimeFactor: 0.2,
          stopToFinalMs: 20,
        },
        {
          language: 'hi',
          configuredLanguage: 'hi',
          mode: 'streaming',
          error: null,
          rawScore: { wordErrorRate: { rate: 0.3 } },
          realTimeFactor: 0.4,
          stopToFinalMs: 40,
        },
        {
          language: 'hi',
          configuredLanguage: 'hi',
          mode: 'streaming',
          error: 'failed',
          rawScore: null,
          stopToFinalMs: null,
        },
      ],
    },
  ]);
  assert.deepEqual(evidence, [
    {
      modelId: 'example/asr',
      language: 'hi',
      configuredLanguage: 'hi',
      mode: 'streaming',
      backend: 'cpu',
      medianWordErrorRate: 0.2,
      medianRealTimeFactor: 0.30000000000000004,
      p95StopToFinalMs: 40,
      peakMemoryBytes: null,
      successfulSamples: 2,
      failedSamples: 1,
    },
  ]);
});

test('rejects files that are not benchmark reports', () => {
  assert.throws(() => summarize([{}]), /missing/u);
});

test('personal memory experiments cannot silently qualify a base model', () => {
  assert.throws(() => summarize([{ personalization: { profile: {} } }]), /personalized runs/u);
});

test('rejects comparisons across different manifests or hardware envelopes', () => {
  const report = (manifestSha256, hardware) => ({
    manifestSha256,
    environment: {
      label: 'test',
      backend: 'cpu',
      hardware,
      operatingSystem: 'test',
      powerMode: 'AC',
      precision: 'int8',
      threads: 1,
      applicationRevision: 'abc',
      runtimeRevision: 'def',
      driver: 'none',
    },
    modelVersions: { asr: { modelId: 'example/asr' } },
    runs: [
      {
        language: 'hi',
        configuredLanguage: 'hi',
        mode: 'offline',
        pace: null,
        error: null,
        rawScore: { wordErrorRate: { rate: 0.1 } },
        realTimeFactor: 0.2,
      },
    ],
  });
  assert.throws(
    () => summarize([report('a'.repeat(64), 'one'), report('b'.repeat(64), 'one')]),
    /incompatible benchmark envelopes/u,
  );
  assert.throws(
    () => summarize([report('a'.repeat(64), 'one'), report('a'.repeat(64), 'two')]),
    /incompatible benchmark envelopes/u,
  );
});

test('keeps automatic and explicit language evidence separate', () => {
  const report = {
    manifestSha256: 'a'.repeat(64),
    environment: {
      label: 'test',
      backend: 'cpu',
      hardware: 'test',
      operatingSystem: 'test',
      powerMode: 'AC',
      precision: 'int8',
      threads: 1,
      applicationRevision: 'abc',
      runtimeRevision: 'def',
      driver: 'none',
    },
    modelVersions: { asr: { modelId: 'example/asr' } },
    runs: [
      {
        language: 'hi',
        configuredLanguage: 'hi',
        mode: 'offline',
        pace: null,
        error: null,
        rawScore: { wordErrorRate: { rate: 0.1 } },
        realTimeFactor: 0.2,
      },
      {
        language: 'hi',
        configuredLanguage: 'auto',
        mode: 'offline',
        pace: null,
        error: null,
        rawScore: { wordErrorRate: { rate: 0.2 } },
        realTimeFactor: 0.2,
      },
    ],
  };
  assert.deepEqual(
    summarize([report]).map((item) => item.configuredLanguage),
    ['auto', 'hi'],
  );
});
