#!/usr/bin/env node
// Native replay evidence only. No natural-user or microphone latency claim.
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const finite = (n) => typeof n === 'number' && Number.isFinite(n) && n >= 0;
export function parseRecords(text) {
  return text.split('\n').flatMap((line, index) => {
    if (!line.trim()) return [];
    try {
      const value = JSON.parse(line);
      if (value && typeof value === 'object' && !Array.isArray(value)) return [value];
    } catch {
      // Preserve complete rows and fail the report even if a crash interrupted its last write.
    }
    return [{ kind: 'parseError', line: index + 1 }];
  });
}
export function timing(values) {
  const sorted = values.filter(finite).sort((a, b) => a - b);
  if (!sorted.length) return null;
  return {
    count: sorted.length,
    meanMs: sorted.reduce((a, b) => a + b, 0) / sorted.length,
    p50Ms: sorted[Math.ceil(sorted.length * 0.5) - 1],
    p95Ms: sorted[Math.ceil(sorted.length * 0.95) - 1],
  };
}
const envelope = (r) =>
  JSON.stringify([
    r.execution?.asr?.initializedProvider ?? null,
    r.execution?.asr?.nativeGraphThreads ?? null,
    r.execution?.rewriter?.initializedProvider ?? null,
  ]);

export function summarize(records, manifest, manifestSha256, processResult) {
  const headers = records.filter((r) => r.kind === 'header');
  const rows = records.filter((r) => r.kind === 'request');
  const completions = records.filter((r) => r.kind === 'complete');
  const structural = [];
  if (records.some((r) => !['header', 'request', 'complete', 'exit'].includes(r.kind))) {
    structural.push('Malformed JSONL or unknown record kinds; original input retained');
  }
  const expected = manifest.samples.length * 2;
  if (
    headers.length !== 1 ||
    headers[0].schemaVersion !== 1 ||
    headers[0].manifestSha256 !== manifestSha256 ||
    headers[0].expectedRequests !== expected
  ) {
    structural.push('Missing, duplicate or mismatched header/manifest');
  }
  if (rows.length !== expected)
    structural.push(`Expected ${expected} requests; retained ${rows.length}`);
  if (completions.length !== 1 || completions[0].complete !== true || completions[0].error != null)
    structural.push('Replay did not complete');
  const keys = new Set();
  rows.forEach((row, index) => {
    const sampleIndex = Math.floor(index / 2);
    const sample = manifest.samples[sampleIndex];
    const armIndex = (index % 2) ^ ((Math.floor(sampleIndex / 2) + (sampleIndex % 2)) % 2);
    const arm = armIndex === 0 ? 'frozen' : 'learning';
    const key = `${row.sampleId}/${row.arm}`;
    if (keys.has(key)) structural.push(`Duplicate ${key}`);
    keys.add(key);
    if (
      !sample ||
      row.sampleId !== sample.id ||
      row.audioSha256 !== sample.sha256 ||
      row.cohort !== sample.cohort ||
      row.phase !== sample.phase ||
      row.arm !== arm ||
      row.request !== index ||
      row.operationId !== `webview-${index}` ||
      row.cold !== (index === 0)
    ) {
      structural.push(`Request ${index} differs from the frozen order/identity`);
    }
  });
  const needsEdit = (r) =>
    r.arm === 'learning' &&
    r.phase === 'feedback' &&
    manifest.samples.find((sample) => sample.id === r.sampleId)?.reference !== r.output;
  const feedbackProblem = (r) =>
    needsEdit(r) &&
    (!finite(r.editSubmitMs) ||
      !finite(r.editBackendMs) ||
      !Array.isArray(r.learningOutcome?.activated) ||
      !Number.isInteger(r.learningOutcome?.provisional) ||
      r.learningOutcome.provisional < 0);
  const problems = rows.filter(
    (r) =>
      feedbackProblem(r) ||
      r.error ||
      r.editError ||
      r.uiError ||
      r.warnings?.length ||
      r.visible !== true ||
      r.domMatches !== true ||
      r.archiveEmpty !== true ||
      r.frozenUnchanged !== true ||
      !finite(r.backendCompletionMs) ||
      !finite(r.stopToDomMs) ||
      !finite(r.stopToRenderOpportunityMs) ||
      r.stopToRenderOpportunityMs < r.stopToDomMs ||
      typeof r.resultId !== 'string' ||
      !Array.isArray(r.warnings) ||
      !r.execution?.asr?.initializedProvider ||
      !r.execution?.rewriter?.initializedProvider ||
      !Number.isInteger(r.execution?.asr?.nativeGraphThreads),
  );
  const pairs = manifest.samples
    .map((sample) => ({
      sample,
      frozen: rows.filter((r) => r.sampleId === sample.id && r.arm === 'frozen'),
      learning: rows.filter((r) => r.sampleId === sample.id && r.arm === 'learning'),
    }))
    .filter((pair) => pair.frozen.length === 1 && pair.learning.length === 1)
    .map((pair) => ({ ...pair, frozen: pair.frozen[0], learning: pair.learning[0] }));
  const residentPairs = pairs.filter((p) => p.sample.id !== manifest.samples[0]?.id);
  const envelopes = [...new Set(rows.map(envelope))];
  const sameEnvelope = envelopes.length === 1 && !envelopes[0].includes('null');
  if (!sameEnvelope)
    structural.push(
      'Missing/mixed actual providers or thread settings; timing aggregates withheld',
    );
  const metrics = sameEnvelope
    ? Object.fromEntries(
        ['frozen', 'learning'].map((arm) => [
          arm,
          {
            backend: timing(residentPairs.map((p) => p[arm].backendCompletionMs)),
            dom: timing(residentPairs.map((p) => p[arm].stopToDomMs)),
            renderOpportunity: timing(residentPairs.map((p) => p[arm].stopToRenderOpportunityMs)),
            asr: timing(residentPairs.map((p) => p[arm].stageTimings?.transcribeMs)),
            cleanup: timing(residentPairs.map((p) => p[arm].stageTimings?.cleanupMs)),
            save: timing(residentPairs.map((p) => p[arm].stageTimings?.saveMs)),
          },
        ]),
      )
    : null;
  const auditPairs = pairs.filter((p) => p.sample.phase === 'audit');
  const scored = auditPairs.filter(
    (p) =>
      Number.isInteger(p.frozen.wordEdits) &&
      p.frozen.wordEdits >= 0 &&
      Number.isInteger(p.learning.wordEdits) &&
      p.learning.wordEdits >= 0,
  );
  const audit = {
    expectedPairs: manifest.samples.filter((s) => s.phase === 'audit').length,
    scoredPairs: scored.length,
    frozenWordEdits: scored.reduce((n, p) => n + p.frozen.wordEdits, 0),
    learningWordEdits: scored.reduce((n, p) => n + p.learning.wordEdits, 0),
    improved: scored.filter((p) => p.learning.wordEdits < p.frozen.wordEdits).length,
    harmed: scored.filter((p) => p.learning.wordEdits > p.frozen.wordEdits).length,
    frozenOutputsNeedingCorrection: scored.filter((p) => p.frozen.wordEdits > 0).length,
    learningOutputsNeedingCorrection: scored.filter((p) => p.learning.wordEdits > 0).length,
  };
  const margin = (metric) => {
    const a = metrics?.frozen[metric],
      b = metrics?.learning[metric];
    if (
      !a ||
      !b ||
      a.count !== manifest.samples.length - 1 ||
      b.count !== a.count ||
      a.meanMs <= 0 ||
      a.p95Ms <= 0
    )
      return null;
    return {
      meanPercent: (b.meanMs / a.meanMs - 1) * 100,
      p95Percent: (b.p95Ms / a.p95Ms - 1) * 100,
    };
  };
  const margins = { backend: margin('backend'), renderOpportunity: margin('renderOpportunity') };
  const gates = {
    cleanProcessExit: processResult?.exitCode === 0 && processResult?.signal === null,
    structure: structural.length === 0,
    allRequestsSuccessful: problems.length === 0 && rows.length === expected,
    auditNoRegression:
      audit.scoredPairs === audit.expectedPairs &&
      audit.harmed === 0 &&
      audit.learningWordEdits <= audit.frozenWordEdits,
    exploratoryTiming: Object.values(margins).every(
      (m) => m !== null && m.meanPercent <= 10 && m.p95Percent <= 10,
    ),
  };
  return {
    schemaVersion: 1,
    evidenceClass: headers[0]?.evidenceClass ?? null,
    manifestSha256,
    expectedRequests: expected,
    retainedRequests: rows.length,
    structuralErrors: structural,
    problemRequests: problems.map((r) => r.request),
    coldPair: rows.filter((r) => r.sampleId === manifest.samples[0]?.id),
    matchedResidentPairs: residentPairs.length,
    actualEnvelopes: envelopes,
    metrics,
    margins,
    audit,
    feedback: {
      expectedEdits: rows.filter(needsEdit).length,
      acknowledgedEdits: rows.filter((r) => needsEdit(r) && !feedbackProblem(r)).length,
      editsWithProvisionalCandidates: rows.filter((r) => r.learningOutcome?.provisional > 0).length,
      activatedSpellings: rows.reduce((n, r) => n + (r.learningOutcome?.activated?.length ?? 0), 0),
      maximumActiveMemories: rows.reduce((n, r) => Math.max(n, r.activeMemories ?? 0), 0),
    },
    scriptedEditSubmit: timing(rows.map((r) => r.editSubmitMs)),
    editBackend: timing(rows.map((r) => r.editBackendMs)),
    gates,
    processResult: processResult ?? null,
    diagnosticPassed: Object.values(gates).every(Boolean),
    nativeMicrophoneStopToVisibleMs: null,
    naturalHumanEditingTimeMs: null,
    naturalLearningBenefitQualified: false,
    limitations: [
      'Previously screened public corpus with scripted reference edits, not natural user feedback.',
      'Stop-to-DOM and render-opportunity include replay IPC/runtime/UI; microphone stop and physical presentation are unmeasured.',
      'The entire first pair is separate; failed/warning requests remain in counts and all available timings.',
      'Uncontrolled-host diagnostic; the 10% margin is not a release qualification or proof of speedup.',
    ],
    rows,
  };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [input, environment, output] = process.argv.slice(2);
  if (!input || !environment || !output)
    throw Error(
      'Usage: summarize-native-learning.mjs INPUT.jsonl ENVIRONMENT.json NEW_OUTPUT.json',
    );
  const bytes = await readFile(
    new URL('../benchmarks/asr/manifests/native-learning-2026-09-19.json', import.meta.url),
  );
  const records = parseRecords(await readFile(input, 'utf8'));
  const result = summarize(
    records,
    JSON.parse(bytes),
    createHash('sha256').update(bytes).digest('hex'),
    JSON.parse(await readFile(environment, 'utf8')),
  );
  result.summarizerSha256 = createHash('sha256')
    .update(await readFile(fileURLToPath(import.meta.url)))
    .digest('hex');
  await writeFile(output, `${JSON.stringify(result, null, 2)}\n`, { flag: 'wx' });
  console.log(
    JSON.stringify(
      {
        requests: result.retainedRequests,
        audit: result.audit,
        margins: result.margins,
        gates: result.gates,
      },
      null,
      2,
    ),
  );
  if (!result.diagnosticPassed) process.exitCode = 1;
}
