import type { SessionRecord } from './api/dataset';

export type HistoryFilter = 'all' | 'pending' | 'approved';

export const HISTORY_PAGE_SIZES = [10, 25, 50] as const;

/** Paginate after filtering; leave the complete history available to stats and export. */
export function paginateSessions(
  records: SessionRecord[],
  requestedPage: number,
  requestedSize: number,
) {
  const pageSize =
    HISTORY_PAGE_SIZES.find((size) => size === requestedSize) ?? HISTORY_PAGE_SIZES[0];
  const total = records.length;
  const pageCount = Math.max(1, Math.ceil(total / pageSize));
  const page = Math.min(
    pageCount,
    Math.max(1, Number.isFinite(requestedPage) ? Math.trunc(requestedPage) : 1),
  );
  const offset = (page - 1) * pageSize;
  return {
    items: records.slice(offset, offset + pageSize),
    page,
    pageSize,
    pageCount,
    total,
    from: total ? offset + 1 : 0,
    to: Math.min(offset + pageSize, total),
  };
}

export function transcriptPreview(record: SessionRecord): string {
  if (record.draftTranscript != null) return record.draftTranscript.trim();
  return (
    [
      record.correctedTranscript,
      record.verifiedTranscript,
      record.personalizationRejected ? null : record.personalizedTranscript,
      record.cleanedTranscript,
      record.rawTranscript,
    ]
      .find((text) => text?.trim())
      ?.trim() ?? ''
  );
}

export function recordingState(record: SessionRecord): string {
  if (record.reviewStatus === 'approved') return 'Reviewed';
  if (transcriptPreview(record)) return 'Needs review';
  if (
    record.rawTranscript !== null ||
    record.pipelineEvents?.some((event) =>
      ['error', 'asr_error', 'interrupted', 'cancelled'].includes(event.stage),
    )
  )
    return 'No transcript';
  return 'Audio only';
}

export function filterSessions(
  records: SessionRecord[],
  query: string,
  filter: HistoryFilter,
): SessionRecord[] {
  const terms = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  return records.filter((record) => {
    if (filter !== 'all' && record.reviewStatus !== filter) return false;
    const searchable = [
      record.originalFilename,
      record.rawTranscript,
      record.cleanedTranscript,
      record.personalizedTranscript,
      record.draftTranscript,
      record.verifiedTranscript,
      record.correctedTranscript,
    ]
      .filter(Boolean)
      .join(' ')
      .toLocaleLowerCase();
    return terms.every((term) => searchable.includes(term));
  });
}

export function recordingDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? 'Date unavailable'
    : new Intl.DateTimeFormat(undefined, {
        month: 'short',
        day: 'numeric',
        year: 'numeric',
        hour: 'numeric',
        minute: '2-digit',
      }).format(date);
}

/** A local calendar key, so a recording near midnight appears on the user's day. */
export function recordingDay(value: string | Date): string {
  const date = value instanceof Date ? value : new Date(value);
  if (Number.isNaN(date.getTime())) return '';
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}

/** Typing speed that "time saved" is measured against (words per minute). */
export const TYPING_WPM = 40;

/** `1 h 12 min`, `35 min`, `45 s` or `0 min`. */
/** Splits a duration into number and unit pairs, such as `[[2, 'h'], [50, 'min']]`. */
export function durationParts(ms: number): [number, string][] {
  const minutes = Math.floor(ms / 60_000);
  if (minutes >= 60) {
    const parts: [number, string][] = [[Math.floor(minutes / 60), 'h']];
    if (minutes % 60) parts.push([minutes % 60, 'min']);
    return parts;
  }
  if (minutes >= 1) return [[minutes, 'min']];
  const seconds = Math.round(ms / 1000);
  return seconds > 0 ? [[seconds, 's']] : [[0, 'min']];
}

export function formatDuration(ms: number): string {
  return durationParts(ms)
    .map(([value, unit]) => `${value} ${unit}`)
    .join(' ');
}

export function historyActivity(records: SessionRecord[], dayCount: 7 | 30, now = new Date()) {
  const days = Array.from({ length: dayCount }, (_, index) => {
    const date = new Date(now);
    date.setHours(0, 0, 0, 0);
    date.setDate(date.getDate() - (dayCount - index - 1));
    return {
      key: recordingDay(date),
      label: new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' }).format(date),
      count: 0,
      savedMs: 0,
    };
  });
  const byDay = new Map(days.map((day) => [day.key, day]));
  const segmenter = new Intl.Segmenter(undefined, { granularity: 'word' });
  let recordings = 0;
  let words = 0;
  let reviewed = 0;
  let audioBytes = 0;
  let withText = 0;
  let recordedMs = 0;
  let savedMs = 0;
  for (const record of records) {
    if (new Date(record.createdAt).getTime() > now.getTime()) continue;
    const day = byDay.get(recordingDay(record.createdAt));
    if (!day) continue;
    day.count += 1;
    recordings += 1;
    if (record.reviewStatus === 'approved') reviewed += 1;
    audioBytes +=
      Number.isFinite(record.audioBytes) && record.audioBytes > 0 ? record.audioBytes : 0;
    const transcript = transcriptPreview(record);
    if (transcript) withText += 1;
    let recordWords = 0;
    for (const segment of segmenter.segment(transcript)) if (segment.isWordLike) recordWords += 1;
    words += recordWords;
    const duration = record.speakerTimeline?.durationMs;
    const spokenMs = Number.isFinite(duration) && duration! > 0 ? duration! : 0;
    recordedMs += spokenMs;
    // Typing the same words would have taken this much longer than saying them.
    // Without a recorded length there is nothing to compare, so nothing counts.
    const saved = spokenMs ? Math.max(0, (recordWords / TYPING_WPM) * 60_000 - spokenMs) : 0;
    savedMs += saved;
    day.savedMs += saved;
  }
  return {
    days,
    recordings,
    words,
    reviewed,
    audioBytes,
    withText,
    withoutText: recordings - withText,
    averageWords: withText ? words / withText : 0,
    activeDays: days.filter((day) => day.count > 0).length,
    recordedMs,
    savedMs,
    /** Words per minute of recorded speech, or 0 without any duration. */
    pace: recordedMs ? words / (recordedMs / 60_000) : 0,
  };
}
