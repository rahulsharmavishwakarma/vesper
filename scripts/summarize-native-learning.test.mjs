import assert from 'node:assert/strict';
import test from 'node:test';
import { parseRecords, summarize, timing } from './summarize-native-learning.mjs';

const manifest = {
  samples: [
    { id: 'a', cohort: 'one', phase: 'feedback', sha256: 'sha-a' },
    { id: 'b', cohort: 'one', phase: 'audit', sha256: 'sha-b' },
  ],
};
function fixture() {
  return [
    { kind: 'header', schemaVersion: 1, manifestSha256: 'frozen', expectedRequests: 4 },
    ...[0, 1, 1, 0].map((arm, request) => {
      const s = manifest.samples[Math.floor(request / 2)];
      return {
        kind: 'request',
        request,
        sampleId: s.id,
        cohort: s.cohort,
        phase: s.phase,
        audioSha256: s.sha256,
        operationId: `webview-${request}`,
        resultId: `result-${request}`,
        arm: arm ? 'learning' : 'frozen',
        cold: request === 0,
        backendCompletionMs: request < 2 ? 900 : 10,
        stopToDomMs: request < 2 ? 910 : 12,
        stopToRenderOpportunityMs: request < 2 ? 940 : 40,
        wordEdits: 2,
        warnings: [],
        domMatches: true,
        visible: true,
        archiveEmpty: true,
        frozenUnchanged: true,
        execution: {
          asr: { initializedProvider: 'cpu', nativeGraphThreads: 8 },
          rewriter: { initializedProvider: 'metal' },
        },
      };
    }),
    { kind: 'complete', complete: true },
  ];
}
const run = (rows, exit = { exitCode: 0, signal: null }) =>
  summarize(rows, manifest, 'frozen', exit);

test('nearest-rank timings and cold-pair exclusion retain both cold rows', () => {
  assert.deepEqual(timing([1, 2, 3, 4, null]), { count: 4, meanMs: 2.5, p50Ms: 2, p95Ms: 4 });
  const result = run(fixture());
  assert.equal(result.coldPair.length, 2);
  assert.equal(result.metrics.frozen.backend.meanMs, 10);
  assert.equal(result.matchedResidentPairs, 1);
  assert.equal(result.diagnosticPassed, true);
  assert.equal(result.naturalLearningBenefitQualified, false);
});
test('a warning/error never disappears from timing or failure counts', () => {
  const rows = fixture();
  rows[3].warnings = ['fallback'];
  rows[3].backendCompletionMs = 70;
  rows[3].error = 'retained native failure';
  const result = run(rows);
  assert.deepEqual(result.problemRequests, [2]);
  assert.equal(result.metrics.learning.backend.meanMs, 70);
  assert.equal(result.diagnosticPassed, false);
});
test('missing, duplicate, reordered or altered-identity rows cannot pass', () => {
  const missing = fixture();
  missing.splice(3, 1);
  const duplicate = fixture();
  duplicate[3] = duplicate[2];
  const swapped = fixture();
  [swapped[3], swapped[4]] = [swapped[4], swapped[3]];
  const changed = fixture();
  changed[3].audioSha256 = 'different';
  for (const rows of [missing, duplicate, swapped, changed]) {
    assert.equal(run(rows).gates.structure, false);
    assert.equal(run(rows).diagnosticPassed, false);
  }
});
test('hidden/stale DOM and unavailable timings fail rather than becoming zero', () => {
  const rows = fixture();
  rows[3].stopToRenderOpportunityMs = null;
  rows[3].domMatches = false;
  rows[3].visible = false;
  const result = run(rows);
  assert.equal(result.metrics.learning.renderOpportunity, null);
  assert.equal(result.margins.renderOpportunity, null);
  assert.equal(result.diagnosticPassed, false);
});
test('mixed native execution providers are not combined', () => {
  const rows = fixture();
  rows[3].execution.rewriter.initializedProvider = 'cpu';
  const result = run(rows);
  assert.equal(result.actualEnvelopes.length, 2);
  assert.equal(result.metrics, null);
  assert.equal(result.diagnosticPassed, false);
});
test('audit harms, frozen mutations and incomplete execution remain failed gates', () => {
  const rows = fixture();
  rows[3].wordEdits = 3;
  rows[4].frozenUnchanged = false;
  const result = run(rows);
  assert.equal(result.audit.harmed, 1);
  assert.equal(result.gates.auditNoRegression, false);
  assert.equal(result.gates.allRequestsSuccessful, false);
  assert.equal(run(rows.slice(0, -1)).gates.structure, false);
});

test('a shutdown crash cannot pass even after every request and complete marker', () => {
  for (const exit of [{ exitCode: null, signal: 'SIGABRT' }, { exitCode: 1, signal: null }, null]) {
    const result = run(fixture(), exit);
    assert.equal(result.gates.allRequestsSuccessful, true);
    assert.equal(result.gates.cleanProcessExit, false);
    assert.equal(result.diagnosticPassed, false);
  }
});

test('truncated JSONL preserves complete rows but fails the report', () => {
  const text = fixture()
    .map((r) => JSON.stringify(r))
    .join('\n');
  const result = run(parseRecords(`${text}\n{"kind":"request","partial":`));
  assert.equal(result.retainedRequests, 4);
  assert.equal(result.gates.structure, false);
  assert.equal(result.diagnosticPassed, false);
  assert.equal(run(parseRecords(`${text}\nnull`)).gates.structure, false);
});

test('a displayed result cannot hide missing calibration feedback', () => {
  const rows = fixture();
  rows[2].output = 'a changed public hypothesis';
  assert.equal(run(rows).diagnosticPassed, false);
  rows[2].editSubmitMs = 12;
  rows[2].editBackendMs = 8;
  rows[2].learningOutcome = { activated: [], provisional: 1 };
  const result = run(rows);
  assert.equal(result.diagnosticPassed, true);
  assert.deepEqual(result.feedback, {
    expectedEdits: 1,
    acknowledgedEdits: 1,
    editsWithProvisionalCandidates: 1,
    activatedSpellings: 0,
    maximumActiveMemories: 0,
  });
});
