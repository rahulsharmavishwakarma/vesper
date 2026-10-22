import { APPLE_GPU_ONLY_SPEECH_MODELS } from './accelerators.ts';
import {
  LANGUAGE_OPTIONS,
  catalogModel,
  defaultSettings,
  offeredModelIds,
  transcriptionLanguageOptions,
  type AppConfig,
  type ComputeBackend,
  type Language,
} from './config.ts';
import type { OptionGauge } from './optionResults.ts';
import evidence from '../../../../models/recommendations.json' with { type: 'json' };

/** One published or measured accuracy figure for a model in one language. */
export interface Published {
  metric: 'wer' | 'cer';
  value: number;
  /** Who scored it: the shared leaderboard, or the model maker's own card. */
  source: 'leaderboard' | 'maker';
  set: string;
  url: string;
}
export interface LocalResult {
  run: string;
  /** Error rate on the items it finished, in percent. */
  wer: number;
  /** Time to write one recording, in milliseconds. */
  ms: number | null;
  peakMemoryGb: number | null;
  executions: number;
  failures: number;
  passed: boolean;
  /** Why it did not pass, in the person's words. */
  why?: string;
}
export interface LocalRun {
  label: string;
  hardware: string;
  date: string;
  source: string;
}
/** A published benchmark score for a writing model, higher is better. */
export interface WriterScore {
  value: number;
  /** Benchmark and setting, e.g. "IFEval, prompt-level strict". */
  set: string;
  /** Who ran it; scores are compared only within one harness. */
  harness: string;
  url: string;
}
interface WriterEvidence {
  instructions?: WriterScore;
  multilingual?: WriterScore;
  /** Vesper's own check, e.g. end-to-end meaning preservation. */
  local?: string;
}
interface Evidence {
  writers: Record<string, WriterEvidence>;
  published: Record<string, Record<string, Published>>;
  local: { runs: Record<string, LocalRun>; results: Record<string, Record<string, LocalResult>> };
}
const EVIDENCE = evidence as unknown as Evidence;

export interface Device {
  os: string;
  arch: string;
  memoryBytes: number | null;
}

export type RecommendationState =
  /** Most accurate published figure among the models that fit this Mac. */
  | 'recommended'
  /** The model has a published figure for the language but is not the pick. */
  | 'published'
  /** Supports the language; nobody has published a figure for it. */
  | 'unmeasured'
  /** Cannot run here: language, Apple GPU or memory. */
  | 'unavailable';

export interface Candidate {
  id: string;
  state: RecommendationState;
  reason: string | null;
  published: Published | null;
  local: LocalResult | null;
  run: LocalRun | null;
}

export function publishedAccuracy(id: string, language: Language): Published | null {
  return EVIDENCE.published[id]?.[language] ?? null;
}
export function localResult(id: string, language: Language): LocalResult | null {
  return EVIDENCE.local.results[id]?.[language] ?? null;
}
export function localRun(name: string): LocalRun | null {
  return EVIDENCE.local.runs[name] ?? null;
}
export const languageLabel = (language: Language) =>
  LANGUAGE_OPTIONS.find((option) => option.value === language)?.label ?? language;

function supports(id: string, language: Language, backend: ComputeBackend): boolean {
  return transcriptionLanguageOptions(
    { ...defaultSettings().models, asr: id as AppConfig['models']['asr'], customAsr: null },
    backend,
  ).some((option) => option.value === language);
}
const appleSilicon = (device: Device | null) =>
  !device || (device.os === 'macos' && device.arch === 'aarch64');

/** Why a model cannot be offered here, or null when it can. */
function unavailableReason(
  id: string,
  language: Language,
  device: Device | null,
  backend: ComputeBackend,
): string | null {
  if (!supports(id, language, backend)) return `Doesn’t support ${languageLabel(language)}`;
  if (APPLE_GPU_ONLY_SPEECH_MODELS.includes(id) && !appleSilicon(device))
    return 'Needs an Apple silicon Mac';
  const peak = localResult(id, language)?.peakMemoryGb ?? anyPeakMemory(id);
  if (peak && device?.memoryBytes && peak * 1e9 > device.memoryBytes / 2)
    return `Needs about ${peak.toFixed(1)} GB of memory`;
  return null;
}
function anyPeakMemory(id: string): number | null {
  const peaks = Object.values(EVIDENCE.local.results[id] ?? {})
    .map((result) => result.peakMemoryGb)
    .filter((peak): peak is number => peak !== null);
  return peaks.length ? Math.max(...peaks) : null;
}

/** Every built-in speech model for a language on this Mac, best first.
 *
 * The owner decided on 2026-10-22 (#270) that setup ranks on published
 * accuracy until Vesper's own per-language matrix exists (#296). Figures are
 * compared only within one source: the shared leaderboard when it covers two
 * or more candidates for the language, otherwise the makers' own cards. Ties
 * within half a point go to the faster locally measured model. */
export function rankSpeechModels(
  language: Language,
  device: Device | null,
  backend: ComputeBackend = 'auto',
): Candidate[] {
  const ids = offeredModelIds('speech_recognition');
  const usable = ids.filter((id) => !unavailableReason(id, language, device, backend));
  const figures = usable.map((id) => publishedAccuracy(id, language)).filter(Boolean);
  const source: Published['source'] =
    figures.filter((figure) => figure?.source === 'leaderboard').length >= 2
      ? 'leaderboard'
      : 'maker';
  const comparable = (id: string) => {
    const figure = publishedAccuracy(id, language);
    return figure && (source === 'maker' || figure.source === 'leaderboard') ? figure : null;
  };
  const speed = (id: string) => localResult(id, language)?.ms ?? Number.POSITIVE_INFINITY;
  const error = (id: string) => comparable(id)?.value ?? Number.POSITIVE_INFINITY;
  // Figures within half a point of the best are treated as a tie and ordered
  // by measured speed; everything else follows by error rate. Models with no
  // comparable figure come last: the catalog default first, then catalog order.
  const best = Math.min(...usable.map(error));
  const tied = (id: string) => Number.isFinite(best) && error(id) - best <= 0.5;
  const isDefault = (id: string) => Number(catalogModel(id)?.status === 'default');
  const ranked = [...usable].sort((a, b) => {
    const ta = tied(a);
    const tb = tied(b);
    if (ta !== tb) return ta ? -1 : 1;
    if (ta && tb) return speed(a) - speed(b) || error(a) - error(b);
    const ea = error(a);
    const eb = error(b);
    if (ea !== eb) return ea - eb;
    return isDefault(b) - isDefault(a);
  });
  const pick = ranked[0];
  const result: Candidate[] = ranked.map((id) => {
    const local = localResult(id, language);
    return {
      id,
      state:
        id === pick && comparable(id)
          ? 'recommended'
          : publishedAccuracy(id, language)
            ? 'published'
            : 'unmeasured',
      reason: null,
      published: publishedAccuracy(id, language),
      local,
      run: local ? localRun(local.run) : null,
    };
  });
  for (const id of ids)
    if (!usable.includes(id))
      result.push({
        id,
        state: 'unavailable',
        reason: unavailableReason(id, language, device, backend),
        published: publishedAccuracy(id, language),
        local: localResult(id, language),
        run: null,
      });
  return result;
}

export function recommendedSpeechModel(
  language: Language,
  device: Device | null = null,
  backend: ComputeBackend = 'auto',
): string {
  return rankSpeechModels(language, device, backend)[0]?.id ?? defaultSettings().models.asr;
}

const percent = (value: number) => `${value.toFixed(1)}%`;
const seconds = (ms: number) =>
  ms < 1000 ? `${(ms / 1000).toFixed(2)} s` : `${(ms / 1000).toFixed(1)} s`;

/** The three ring gauges for a picker row. Accuracy is the published figure;
 * speed and memory are Vesper's own measurements, so they never borrow a
 * server GPU's throughput. */
export function candidateGauges(candidate: Candidate, language: Language, device: Device | null) {
  const name = languageLabel(language);
  const gauges: OptionGauge[] = [];
  const figure = candidate.published;
  if (figure) {
    const right = 100 - figure.value;
    gauges.push({
      kind: 'accuracy',
      icon: 'target',
      // Full at no errors, empty at a 20% error rate.
      score: Math.max(6, (1 - figure.value / 20) * 100),
      text: percent(right),
      title: `${right.toFixed(1)}% of ${figure.metric === 'cer' ? 'characters' : 'words'} right in ${name} (${figure.set}, ${figure.source === 'leaderboard' ? 'Open ASR Leaderboard' : 'model maker’s figure'})`,
      tone: figure.value > 20 ? 'bad' : undefined,
    });
  } else
    gauges.push({
      kind: 'accuracy',
      icon: 'target',
      score: null,
      text: 'No figure',
      title: `No published accuracy for ${name}`,
    });
  const local = candidate.local;
  gauges.push(
    local?.ms
      ? {
          kind: 'speed',
          icon: 'models',
          // Full at instant, empty at one second per recording.
          score: Math.max(6, (1 - Math.min(local.ms, 1000) / 1000) * 100),
          text: seconds(local.ms),
          title: `${seconds(local.ms)} to write a recording, measured by Vesper on ${candidate.run?.hardware ?? 'Apple silicon'}`,
          tone: local.ms > 1000 ? 'low' : undefined,
        }
      : { kind: 'speed', icon: 'models', score: null, text: '—', title: 'Speed not measured yet' },
  );
  const peak = local?.peakMemoryGb ?? anyPeakMemory(candidate.id);
  const ram = device?.memoryBytes ? device.memoryBytes / 1e9 : null;
  gauges.push(
    peak
      ? {
          kind: 'memory',
          icon: 'memory',
          score: ram ? Math.max(6, Math.min(100, (peak / ram) * 100)) : 30,
          text: `${peak.toFixed(1)} GB`,
          title: `Uses about ${peak.toFixed(1)} GB of memory${ram ? ` of this Mac’s ${Math.round(ram)} GB` : ''}, measured by Vesper`,
          tone: ram && peak / ram > 0.5 ? 'bad' : ram && peak / ram > 0.25 ? 'low' : undefined,
        }
      : {
          kind: 'memory',
          icon: 'memory',
          score: null,
          text: '—',
          title: 'Memory not measured yet',
        },
  );
  return gauges;
}

/** The row's short evidence line, placed in the picker description. */
export function candidateTag(candidate: Candidate, language: Language): string {
  const name = languageLabel(language);
  switch (candidate.state) {
    case 'recommended':
      return `Best for ${name}`;
    case 'published':
      return candidate.local && !candidate.local.passed
        ? `Failed Vesper’s test · ${candidate.local.why ?? 'see details'}`
        : 'Published result';
    case 'unmeasured':
      return 'No published result';
    case 'unavailable':
      return candidate.reason ?? 'Unavailable';
  }
}

/** Adds ranking, tags and gauges to the app's speech picker options, the same
 * way in setup and on the Models page. Built-in models are ordered best first;
 * imported profiles and presets keep their place after them. Automatic
 * detection has no language to rank for, so the options are returned as is. */
export function decorateSpeechOptions<
  Option extends { value: string; description?: string; disabled?: boolean },
>(
  options: Option[],
  language: Language,
  device: Device | null,
  backend: ComputeBackend = 'auto',
  /** Setup locks out models that cannot run here; the Models page keeps them
   * selectable so a language can be repaired after a switch. */
  { lockUnavailable = true }: { lockUnavailable?: boolean } = {},
): (Option & { gauges?: OptionGauge[] })[] {
  if (language === 'auto') return options;
  const ranked = rankSpeechModels(language, device, backend);
  const order = new Map(ranked.map((candidate, index) => [candidate.id, index]));
  const decorated = options.map((option) => {
    const candidate = ranked.find((entry) => entry.id === option.value);
    if (!candidate) return option;
    return {
      ...option,
      description: [candidateTag(candidate, language), option.description]
        .filter(Boolean)
        .join(' · '),
      disabled: option.disabled || (lockUnavailable && candidate.state === 'unavailable'),
      gauges: candidate.state === 'unavailable' ? [] : candidateGauges(candidate, language, device),
    };
  });
  const rank = (option: Option) => order.get(option.value) ?? Number.POSITIVE_INFINITY;
  return decorated
    .map((option, index) => ({ option, index }))
    .sort((a, b) => rank(a.option) - rank(b.option) || a.index - b.index)
    .map((entry) => entry.option);
}

const writerFits = (
  option: { languages?: { value: string }[]; unrestrictedLanguages?: boolean },
  language: Language,
) =>
  language === 'auto' ||
  Boolean(option.unrestrictedLanguages) ||
  Boolean(option.languages?.some((entry) => entry.value === language));

/** Writing models ranked on published instruction following (IFEval), the
 * closest public measure of cleaning a transcript without changing it. Scores
 * are compared within one harness only. A writer that passed Vesper's own
 * end-to-end meaning check comes first; the rest follow by published score. */
function rankWriters<
  Option extends {
    value: string;
    languages?: { value: string }[];
    unrestrictedLanguages?: boolean;
  },
>(options: Option[], language: Language): Option[] {
  const usable = options.filter((option) => writerFits(option, language));
  const scores = usable
    .map((option) => EVIDENCE.writers[option.value]?.instructions)
    .filter(Boolean);
  const harnesses = new Map<string, number>();
  for (const score of scores)
    harnesses.set(score!.harness, (harnesses.get(score!.harness) ?? 0) + 1);
  const harness = [...harnesses.entries()].sort((a, b) => b[1] - a[1])[0]?.[0];
  const score = (value: string) => {
    const entry = EVIDENCE.writers[value]?.instructions;
    return entry && entry.harness === harness ? entry.value : Number.NEGATIVE_INFINITY;
  };
  const local = (value: string) => Number(Boolean(EVIDENCE.writers[value]?.local));
  // Owner decision 2026-10-22 (#270): a writer Vesper has checked end to end
  // for keeping meaning ranks ahead of published scores, which measure general
  // instruction following rather than transcript cleanup.
  return [...usable].sort(
    (a, b) => local(b.value) - local(a.value) || score(b.value) - score(a.value),
  );
}

const score = (value: number) => `${Math.round(value)}`;

/** Adds ranking, tags, gauges and an evidence sentence to the writing model
 * picker, the same way in setup and on the Models page. */
export function decorateWriterOptions<
  Option extends {
    value: string;
    description?: string;
    disabled?: boolean;
    languages?: { value: string; label: string }[];
    unrestrictedLanguages?: boolean;
  },
>(
  options: Option[],
  language: Language,
  { lockUnavailable = true }: { lockUnavailable?: boolean } = {},
): (Option & { gauges?: OptionGauge[]; evidence: string; disabled?: boolean })[] {
  const name = language === 'auto' ? null : languageLabel(language);
  const ranked = rankWriters(options, language);
  const best = ranked[0]?.value;
  const order = new Map(ranked.map((option, index) => [option.value, index]));
  return options
    .map((option, index) => {
      const fits = writerFits(option, language);
      const entry = EVIDENCE.writers[option.value];
      const tag = !fits
        ? `Doesn’t support ${name}`
        : option.value === best && entry?.instructions
          ? name
            ? `Best for ${name}`
            : 'Best published result'
          : entry?.instructions
            ? 'Published result'
            : 'No published result';
      const gauges: OptionGauge[] = fits
        ? [
            entry?.instructions
              ? {
                  kind: 'instructions',
                  icon: 'shield',
                  score: Math.max(6, Math.min(100, entry.instructions.value)),
                  text: score(entry.instructions.value),
                  title: `Follows instructions: ${entry.instructions.value.toFixed(1)} on ${entry.instructions.set} (${entry.instructions.harness})`,
                }
              : {
                  kind: 'instructions',
                  icon: 'shield',
                  score: null,
                  text: '—',
                  title: 'No published instruction-following score',
                },
            entry?.multilingual
              ? {
                  kind: 'languages',
                  icon: 'globe',
                  score: Math.max(6, Math.min(100, entry.multilingual.value)),
                  text: score(entry.multilingual.value),
                  title: `Multilingual: ${entry.multilingual.value.toFixed(1)} on ${entry.multilingual.set} (${entry.multilingual.harness})`,
                }
              : {
                  kind: 'languages',
                  icon: 'globe',
                  score: null,
                  text: '—',
                  title: 'No published multilingual score',
                },
          ]
        : [];
      const evidence = [
        entry?.instructions
          ? `Follows instructions ${entry.instructions.value.toFixed(1)} on ${entry.instructions.set}, published by ${entry.instructions.harness}.`
          : 'No instruction-following score is published for this writer.',
        entry?.local ?? 'Vesper hasn’t tested how faithfully it keeps your meaning.',
      ].join(' ');
      return {
        option: {
          ...option,
          description: [tag, option.description].filter(Boolean).join(' · '),
          disabled: option.disabled || (lockUnavailable && !fits),
          gauges,
          evidence,
        },
        index,
      };
    })
    .sort(
      (a, b) =>
        (order.get(a.option.value) ?? Number.POSITIVE_INFINITY) -
          (order.get(b.option.value) ?? Number.POSITIVE_INFINITY) || a.index - b.index,
    )
    .map((entry) => entry.option);
}
