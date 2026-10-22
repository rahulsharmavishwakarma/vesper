import assert from 'node:assert/strict';
import test from 'node:test';
import { liveWords } from './liveWords.ts';

test('unchanged words keep their keys between drafts; only the changed tail is new', () => {
  const first = liveWords('Can you send', ' the arc light');
  const next = liveWords('Can you send the', ' Arclight notes');
  assert.deepEqual(
    next.map((word) => word.text),
    ['Can', 'you', 'send', 'the', 'Arclight', 'notes'],
  );
  const before = new Set(first.map((word) => word.key));
  // "Can you send the" keep their keys, "the" turns from grey to committed.
  assert.deepEqual(
    next.filter((word) => before.has(word.key)).map((word) => word.text),
    ['Can', 'you', 'send', 'the'],
  );
  assert.equal(next[3].provisional, false);
  assert.equal(next[4].provisional, true);
});

test('an empty draft has no words', () => {
  assert.deepEqual(liveWords('', '  '), []);
});
