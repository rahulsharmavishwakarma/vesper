// Convert pinned public hypotheses to chronological simulated edits. No inference.
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';

const files = process.argv.slice(2);
if (files.length !== 5)
  throw new Error(
    'Usage: node scripts/prepare-automatic-learning-replay.mjs CALIBRATION_JSON CALIBRATION_RESULTS_JSON HOLDOUT_JSON HOLDOUT_RESULTS_JSON OUTPUT_JSON',
  );
const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');
const inputs = files.slice(0, 4).map((file) => {
  const bytes = readFileSync(file);
  return { file, sha256: digest(bytes), data: JSON.parse(bytes) };
});
const episodes = [];
const ids = new Set();
const sources = new Set();
const attribution = new Map();
const languages = { 'en-US': 'en', 'en-IN': 'en', en: 'en', 'hi-IN': 'hi', hi: 'hi' };
for (const offset of [0, 2]) {
  const manifest = inputs[offset];
  const results = inputs[offset + 1].data;
  if (results.manifestSha256 !== manifest.sha256)
    throw new Error('Results do not match their exact input manifest');
  const asr = results.modelVersions?.asr;
  if (
    asr?.modelId !== 'nvidia/nemotron-3.5-asr-streaming-0.6b' ||
    !asr.artifactFiles?.includes('nemotron-3.5-asr-streaming-0.6b.q8_0.gguf')
  ) {
    throw new Error('This preparation protocol requires the pinned Nemotron Q8 hypotheses');
  }
  if (results.runs.length !== manifest.data.samples.length)
    throw new Error('Missing or extra upstream results');
  const runs = new Map();
  for (const run of results.runs) {
    if (runs.has(run.sampleId)) throw new Error(`Duplicate result: ${run.sampleId}`);
    // Stop rather than silently remove failed ASR from the quality denominator.
    if (run.error || typeof run.rawHypothesis !== 'string')
      throw new Error(`Upstream ASR failed: ${run.sampleId}`);
    runs.set(run.sampleId, run);
  }
  for (const source of manifest.data.sources) attribution.set(source.id, source);
  for (const sample of manifest.data.samples) {
    const run = runs.get(sample.id);
    if (!run) throw new Error(`Missing result: ${sample.id}`);
    if (ids.has(sample.id) || sources.has(sample.sha256))
      throw new Error('Repeated recording across replay splits');
    if (!/^[a-f0-9]{64}$/.test(sample.sha256)) throw new Error('Missing source fingerprint');
    if (!languages[sample.language]) throw new Error(`Unsupported language: ${sample.language}`);
    if (typeof sample.reference !== 'string') throw new Error('Missing reference');
    ids.add(sample.id);
    sources.add(sample.sha256);
    episodes.push({
      id: sample.id,
      sourceKey: sample.sha256,
      atMs: (episodes.length + 1) * 180000,
      project: 'corpus',
      language: languages[sample.language],
      modelId: 'Nemotron-Q8-pinned-cached',
      input: run.rawHypothesis,
      reference: sample.reference,
      edited: offset === 0 ? sample.reference : null,
      manuallyRemember: offset === 0,
      expectedAutomatic: null,
    });
  }
}
const output = {
  schemaVersion: 1,
  source: {
    kind: 'cached public ASR outputs with simulated correction feedback; no new recognition',
    attribution: [...attribution.values()],
    inputs: inputs.map(({ file, sha256 }) => ({ file, sha256 })),
    learningEpisodes: inputs[0].data.samples.length,
    heldoutEpisodes: inputs[2].data.samples.length,
    limitation:
      'One reference edit per calibration recording, not actual personal feedback. No duplicated teaching episodes to force promotion. Manual comparator blindly remembers generated review suggestions and is not a realistic user study.',
  },
  episodes,
};
writeFileSync(files[4], `${JSON.stringify(output, null, 2)}\n`);
console.log(
  `Prepared ${episodes.length} episodes; held-out references never become edit feedback.`,
);
