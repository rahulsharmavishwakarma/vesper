import assert from 'node:assert/strict';
import test from 'node:test';
import {
  filterSessions,
  paginateSessions,
  historyActivity,
  recordingDay,
  recordingDate,
  recordingState,
  transcriptPreview,
} from './history.ts';

const recording = (fields = {}) => ({
  id: 'one',
  originalFilename: 'recording.wav',
  reviewStatus: 'pending',
  rawTranscript: null,
  cleanedTranscript: null,
  verifiedTranscript: null,
  correctedTranscript: null,
  pipelineEvents: [],
  ...fields,
});

test('preview prefers reviewed text and skips blank outputs', () => {
  assert.equal(
    transcriptPreview(recording({ cleanedTranscript: ' ', rawTranscript: 'hello' })),
    'hello',
  );
  assert.equal(
    transcriptPreview(
      recording({ verifiedTranscript: 'correct name', cleanedTranscript: 'wrong name' }),
    ),
    'correct name',
  );
  assert.equal(
    transcriptPreview(
      recording({ correctedTranscript: 'Reviewed.', verifiedTranscript: 'um reviewed' }),
    ),
    'Reviewed.',
  );
  assert.equal(transcriptPreview(recording()), '');
});

test('remembered output is searchable and undo restores the machine fallback', () => {
  const record = recording({
    rawTranscript: 'run cube control',
    personalizedTranscript: 'run kubectl',
  });
  assert.equal(transcriptPreview(record), 'run kubectl');
  assert.equal(filterSessions([record], 'kubectl', 'all').length, 1);
  assert.equal(transcriptPreview({ ...record, personalizationRejected: true }), 'run cube control');
  assert.equal(
    transcriptPreview({ ...record, verifiedTranscript: 'run another command' }),
    'run another command',
  );
});

test('pages preserve order without dropping or repeating recordings and report exact ranges', () => {
  const records = Array.from({ length: 53 }, (_, index) => recording({ id: String(index) }));
  for (const size of [10, 25, 50]) {
    const first = paginateSessions(records, 1, size);
    assert.equal(first.from, 1);
    assert.equal(first.to, size);
    const pages = Array.from({ length: first.pageCount }, (_, index) =>
      paginateSessions(records, index + 1, size),
    );
    assert.deepEqual(
      pages.flatMap((page) => page.items),
      records,
    );
    assert.equal(pages.at(-1).to, 53);
    assert.equal(pages.at(-1).total, 53);
  }
  assert.equal(records.length, 53);
});

test('empty history, invalid inputs and shrinking results never produce an empty out-of-range page', () => {
  const records = Array.from({ length: 11 }, (_, index) => recording({ id: String(index) }));
  const last = paginateSessions(records, 9, 10);
  assert.equal(last.page, 2);
  assert.deepEqual(
    last.items.map((record) => record.id),
    ['10'],
  );
  const shrunk = paginateSessions(records.slice(0, 10), last.page, 10);
  assert.equal(shrunk.page, 1);
  assert.equal(shrunk.items.length, 10);
  for (const value of [-1, 0, NaN, Infinity])
    assert.equal(paginateSessions(records, value, 10).page, 1);
  assert.equal(paginateSessions(records, 1, 0).pageSize, 10);
  const empty = paginateSessions([], 4, 25);
  assert.deepEqual(
    [empty.page, empty.pageCount, empty.from, empty.to, empty.total],
    [1, 1, 0, 0, 0],
  );
  assert.deepEqual(empty.items, []);
});

test('search and review filters find records beyond the currently visible page', () => {
  const records = Array.from({ length: 53 }, (_, index) =>
    recording({
      id: String(index),
      rawTranscript: index === 52 ? 'A distant matching thought' : 'Another thought',
      reviewStatus: index % 2 ? 'approved' : 'pending',
    }),
  );
  assert.equal(
    paginateSessions(records, 1, 10).items.some((record) => record.id === '52'),
    false,
  );
  const searched = paginateSessions(filterSessions(records, 'distant', 'pending'), 1, 10);
  assert.deepEqual(
    searched.items.map((record) => record.id),
    ['52'],
  );
  const reviewed = filterSessions(records, '', 'approved');
  assert.equal(paginateSessions(reviewed, 3, 10).total, 26);
  assert.deepEqual(
    paginateSessions(reviewed, 3, 10).items.map((record) => record.id),
    ['41', '43', '45', '47', '49', '51'],
  );
});

test('an imported file and an empty or interrupted transcription have different states', () => {
  assert.equal(recordingState(recording()), 'Audio only');
  assert.equal(recordingState(recording({ rawTranscript: '  ' })), 'No transcript');
  assert.equal(
    recordingState(recording({ pipelineEvents: [{ stage: 'interrupted' }] })),
    'No transcript',
  );
  assert.equal(
    recordingState(
      recording({ rawTranscript: 'hello', pipelineEvents: [{ stage: 'cleanup_error' }] }),
    ),
    'Needs review',
  );
  assert.equal(
    recordingState(recording({ verifiedTranscript: 'hello', reviewStatus: 'approved' })),
    'Reviewed',
  );
});

test('search covers filenames and every transcript and composes with review filters', () => {
  const records = [
    recording({ id: 'raw', originalFilename: 'meeting.wav', rawTranscript: 'Launch tomorrow' }),
    recording({
      id: 'reviewed',
      verifiedTranscript: 'लहर',
      correctedTranscript: 'Launch Friday',
      reviewStatus: 'approved',
    }),
    recording({ id: 'empty' }),
  ];
  assert.deepEqual(
    filterSessions(records, '  MEETING launch  ', 'pending').map((r) => r.id),
    ['raw'],
  );
  assert.deepEqual(
    filterSessions(records, 'launch', 'approved').map((r) => r.id),
    ['reviewed'],
  );
  assert.deepEqual(
    filterSessions(records, 'लहर', 'all').map((r) => r.id),
    ['reviewed'],
  );
  assert.equal(filterSessions(records, 'absent', 'all').length, 0);
  assert.equal(filterSessions(records, ' ', 'all').length, 3);
  assert.deepEqual(
    records.map((r) => r.id),
    ['raw', 'reviewed', 'empty'],
  );
});

test('malformed historical timestamps cannot crash the recording list', () => {
  assert.equal(recordingDate('invalid-date'), 'Date unavailable');
  assert.ok(recordingDate('2026-09-14T10:00:00.000Z').includes('2026'));
});

test('activity includes local calendar boundaries and zero days, but excludes old and future records', () => {
  const now = new Date(2026, 8, 14, 12);
  const records = [
    recording({
      createdAt: new Date(2026, 8, 8, 0).toISOString(),
      rawTranscript: 'Hello world.',
      audioBytes: 100,
    }),
    recording({
      createdAt: new Date(2026, 8, 14, 9).toISOString(),
      verifiedTranscript: 'Reviewed words.',
      reviewStatus: 'approved',
      audioBytes: 200,
    }),
    recording({
      createdAt: new Date(2026, 8, 7, 23, 59).toISOString(),
      rawTranscript: 'Older recording',
      audioBytes: 300,
    }),
    recording({ createdAt: new Date(2026, 8, 14, 13).toISOString(), audioBytes: 999 }),
    recording({ createdAt: 'invalid', audioBytes: 999 }),
  ];
  const week = historyActivity(records, 7, now);
  assert.equal(week.days.length, 7);
  assert.equal(week.days[0].key, '2026-09-08');
  assert.equal(week.days[6].key, recordingDay(now));
  assert.equal(week.recordings, 2);
  assert.equal(week.words, 4);
  assert.equal(week.reviewed, 1);
  assert.equal(week.audioBytes, 300);
  assert.equal(week.activeDays, 2);
  assert.equal(week.days[3].count, 0);
  assert.equal(historyActivity(records, 30, now).recordings, 3);
});

test('audio-only sessions count as recordings without inventing words or duration', () => {
  const now = new Date(2026, 8, 14, 12);
  const stats = historyActivity(
    [recording({ createdAt: now.toISOString(), audioBytes: 123 })],
    7,
    now,
  );
  assert.equal(stats.recordings, 1);
  assert.equal(stats.words, 0);
  assert.equal(stats.reviewed, 0);
  assert.equal(stats.audioBytes, 123);
  assert.equal(historyActivity([], 30, now).recordings, 0);
});

test('text coverage and average words use non-empty preferred transcripts in the selected period', () => {
  const now = new Date(2026, 8, 14, 12);
  const createdAt = now.toISOString();
  const stats = historyActivity(
    [
      recording({ createdAt, rawTranscript: 'One two three', cleanedTranscript: 'One two' }),
      recording({
        createdAt,
        rawTranscript: 'Generated text',
        verifiedTranscript: 'Three four five six',
      }),
      recording({ createdAt, rawTranscript: '  ', cleanedTranscript: '' }),
      recording({ createdAt }),
      recording({
        createdAt: new Date(2026, 7, 1).toISOString(),
        rawTranscript: 'Outside this period',
      }),
    ],
    7,
    now,
  );
  assert.equal(stats.withText, 2);
  assert.equal(stats.withoutText, 2);
  assert.equal(stats.averageWords, 3);
  assert.equal(stats.activeDays, 1);
  const empty = historyActivity([], 7, now);
  assert.equal(empty.averageWords, 0);
  assert.equal(empty.withText, 0);
  assert.equal(empty.withoutText, 0);
});

test('ordinary drafts display and search without becoming reviewed labels', () => {
  const edited = recording({ rawTranscript: 'Open read me', draftTranscript: 'Open README.md' });
  assert.equal(transcriptPreview(edited), 'Open README.md');
  assert.equal(recordingState(edited), 'Needs review');
  assert.equal(filterSessions([edited], 'README.md', 'all').length, 1);
  assert.equal(
    transcriptPreview(recording({ rawTranscript: 'deleted text', draftTranscript: '' })),
    '',
  );
});
