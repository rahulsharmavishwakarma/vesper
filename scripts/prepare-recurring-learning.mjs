// Authored synthetic speech and spelling preferences, never natural-user evidence.
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { execFileSync } from 'node:child_process';

const destination = process.argv[2];
if (!destination)
  throw new Error('Usage: node scripts/prepare-recurring-learning.mjs NEW_DIRECTORY');
const root = resolve(destination);
mkdirSync(root); // Refuse to silently replace an inspected corpus.
mkdirSync(join(root, 'audio'));
const samples = [];
const voices = [
  ['Samantha', 'en_US'],
  ['Daniel', 'en_GB'],
  ['Rishi', 'en_IN'],
];
for (const [voice, locale] of voices) {
  for (const [family, spoken, initial, revised] of [
    ['cedar', 'Cedar Point', 'CedarPoint', 'Cedar_Point'],
    ['brookside', 'Brookside Park', 'BrooksidePark', 'Brookside_Park'],
  ]) {
    const cohort = `${voice}-${family}`;
    const rows = [
      ['teach-one', 'feedback', `Inspect ${spoken} station.`, initial],
      ['duplicate', 'feedback', `Inspect ${spoken} station.`, initial],
      ['teach-two', 'feedback', `Maintain ${spoken} depot.`, initial],
      ['reuse-new-context', 'audit', `Locate ${spoken} depot.`, initial],
      ['wrong-context', 'audit', `Sketch ${spoken} sculpture.`, spoken],
      ['no-alias', 'audit', 'Schedule a visit tomorrow.', spoken],
      ['revise-target', 'feedback', `Review ${spoken} depot.`, revised],
      ['fresh-one', 'feedback', `Repair ${spoken} depot.`, revised],
      ['fresh-two', 'feedback', `Check ${spoken} depot.`, revised],
      ['reuse-revised', 'audit', `Describe ${spoken} depot.`, revised],
      ['wrong-context-revised', 'audit', `Paint ${spoken} artwork.`, spoken],
      ['forget', 'audit', `Survey ${spoken} depot.`, spoken],
    ];
    let firstAudio;
    for (const [kind, phase, text, target] of rows) {
      const id = `${cohort}-${kind}`;
      let audioPath = `audio/${id}.wav`;
      if (kind === 'duplicate') audioPath = firstAudio;
      else {
        execFileSync('/usr/bin/say', [
          '-v',
          voice,
          '-r',
          '165',
          '-o',
          join(root, audioPath),
          '--file-format=WAVE',
          '--data-format=LEI16@16000',
          text,
        ]);
      }
      if (kind === 'teach-one') firstAudio = audioPath;
      const bytes = readFileSync(join(root, audioPath));
      if (bytes.toString('ascii', 0, 4) !== 'RIFF' || bytes.length < 32044)
        throw new Error(`Missing or invalid speech WAV: ${id}`);
      samples.push({
        id,
        cohort,
        voice,
        locale,
        kind,
        phase,
        spoken,
        initial,
        revised,
        speechText: text,
        reference: text.replace(spoken, target),
        audioPath,
        sha256: createHash('sha256').update(bytes).digest('hex'),
      });
    }
  }
}
const manifest = {
  schemaVersion: 1,
  evidenceClass:
    'Authored synthetic speech and simulated spelling-preference edits; not human accent or natural-user accuracy evidence',
  selection:
    'Two fixed recurring names, three installed system voices, twelve chronological rows per cohort; fixed before recognition',
  provenance: {
    generator: 'macOS say, 165 words/minute, signed PCM16 mono 16kHz WAV',
    os: execFileSync('/usr/bin/sw_vers', ['-productVersion'], { encoding: 'utf8' }).trim(),
    osBuild: execFileSync('/usr/bin/sw_vers', ['-buildVersion'], { encoding: 'utf8' }).trim(),
    generatorSha256: createHash('sha256')
      .update(readFileSync(new URL(import.meta.url)))
      .digest('hex'),
  },
  limitations: [
    'Voice locale is a synthesis setting, not a validated human accent cohort.',
    'Preferred joined/underscore spellings are authored user preferences, not claims that spaced ASR text misrepresents speech.',
    'Each distinct utterance uses a simulated independent episode; duplicate audio must still supply no extra support.',
    'Audit labels are scoring-only. No fresh holdout or natural editing-time claim.',
  ],
  samples,
};
writeFileSync(join(root, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`, {
  flag: 'wx',
});
console.log(
  JSON.stringify({
    samples: samples.length,
    uniqueAudio: new Set(samples.map((s) => s.sha256)).size,
    cohorts: voices.length * 2,
  }),
);
