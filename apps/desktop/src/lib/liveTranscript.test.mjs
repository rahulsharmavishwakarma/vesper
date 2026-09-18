import assert from 'node:assert/strict';
import test from 'node:test';
import { liveTranscriptFromStatus, mergeLiveTranscript } from './liveTranscript.ts';

const partial = (operationId, sequence) => ({
  operationId,
  sequence,
  committedText: 'Draft',
  provisionalText: '',
  audioProcessedSeconds: 1,
  emittedAtUnixMs: 10,
  isFinal: false,
});

test('fallback clears a visible draft and rejects delayed events for that recording', () => {
  let draft = mergeLiveTranscript(null, partial('a', 2), 'a', null);
  assert.equal(draft.sequence, 2);
  draft = mergeLiveTranscript(draft, null, 'a', 'a');
  assert.equal(draft, null);
  assert.equal(mergeLiveTranscript(draft, partial('a', 3), 'a', 'a'), null);
  assert.equal(mergeLiveTranscript(draft, partial('b', 1), 'b', 'a').operationId, 'b');
});

test('events cannot adopt another operation and stale sequences cannot replace text', () => {
  const draft = partial('b', 3);
  assert.equal(mergeLiveTranscript(draft, partial('a', 9), 'b', null), draft);
  assert.equal(mergeLiveTranscript(draft, partial('b', 2), 'b', null), draft);
  assert.equal(mergeLiveTranscript(draft, null, 'b', null), draft);
  assert.equal(mergeLiveTranscript(draft, partial('c', 1), 'b', 'a'), draft);
  assert.equal(mergeLiveTranscript(draft, null, 'b', 'a'), draft);
});

test('failure that finishes between polls and a shortcut restart cannot reuse an old draft', () => {
  const old = partial('failed', 2);
  const idle = { busy: false, operationId: null, partial: null, partialInvalidatedFor: null };
  assert.equal(liveTranscriptFromStatus(old, idle), null);
  const starting = { ...idle, busy: true, operationId: 'next' };
  assert.equal(liveTranscriptFromStatus(old, starting), null, 'Even if the idle poll was missed');
  assert.equal(liveTranscriptFromStatus(old, { ...starting, partial: partial('failed', 3) }), null);
  assert.equal(
    liveTranscriptFromStatus(old, { ...starting, partial: partial('next', 1) }).operationId,
    'next',
  );
});
