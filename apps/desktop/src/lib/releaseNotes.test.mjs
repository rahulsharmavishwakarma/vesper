import assert from 'node:assert/strict';
import test from 'node:test';
import { parseReleaseNotes, previewNotes } from './releaseNotes.ts';

test('changelog sections become plain grouped items', () => {
  const notes = [
    '### Added',
    '',
    '- Self-improving dictation that works with every speech',
    '  model, with a [correction gate](docs/gate.md).',
    '- Rule-based formatting for **English**.',
    '',
    '### Security',
    '',
    '- Patched `zstd-sys`.',
  ].join('\n');
  assert.deepEqual(parseReleaseNotes(notes), [
    {
      title: 'Added',
      items: [
        'Self-improving dictation that works with every speech model, with a correction gate.',
        'Rule-based formatting for English.',
      ],
    },
    { title: 'Security', items: ['Patched zstd-sys.'] },
  ]);
});

test("GitHub's generated notes lose credits, comments and the compare link", () => {
  const notes = [
    '<!-- Release notes generated using configuration in .github/release.yml at main -->',
    '',
    "## What's Changed",
    '### Features',
    '* Settings regrouped by task by @someone in https://github.com/example/vesper/pull/359',
    '## New Contributors',
    '* @newcomer made their first contribution in https://github.com/example/vesper/pull/1',
    '',
    '**Full Changelog**: https://github.com/example/vesper/compare/v0.1.1...v0.1.2-rc.1',
  ].join('\n');
  assert.deepEqual(parseReleaseNotes(notes), [
    { title: 'Features', items: ['Settings regrouped by task'] },
  ]);
});

test('a plain sentence is one untitled item, and empty notes give nothing', () => {
  assert.deepEqual(parseReleaseNotes('Nightly 0.1.2 built from main at abc1234.'), [
    { title: '', items: ['Nightly 0.1.2 built from main at abc1234.'] },
  ]);
  assert.deepEqual(parseReleaseNotes(''), []);
});

test('the preview keeps the first items and their headings', () => {
  const sections = [
    { title: 'Added', items: ['a', 'b', 'c'] },
    { title: 'Fixed', items: ['d', 'e'] },
  ];
  assert.deepEqual(previewNotes(sections, 4), [
    { title: 'Added', items: ['a', 'b', 'c'] },
    { title: 'Fixed', items: ['d'] },
  ]);
  assert.deepEqual(previewNotes(sections, 2), [{ title: 'Added', items: ['a', 'b'] }]);
});
