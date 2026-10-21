import type { OnboardingState } from './api/onboarding';

/** On for every new profile (issue #270). Previews and tests can open it with
 * `?first-run`; a finished or existing profile never sees it. */
export const FIRST_RUN_ENABLED = true;

export const FIRST_RUN_STEPS = [
  'Welcome',
  'Voice',
  'Models',
  'Access',
  'Shortcut',
  'Your words',
  'Try it',
] as const;
/** Ready comes after the listed steps. */
export const READY_STEP = FIRST_RUN_STEPS.length;

/** Show setup only for a pending profile, and only when the flow is enabled
 * or explicitly requested by a preview. A completed profile never sees it,
 * even when a preview asks. */
export function shouldShowFirstRun(
  state: Pick<OnboardingState, 'status'>,
  enabled = FIRST_RUN_ENABLED,
  requested = false,
): boolean {
  return state.status === 'pending' && (enabled || requested);
}

/** Where a pending flow resumes. Unknown values start again at Welcome. */
export function resumeStep(step: unknown): number {
  return typeof step === 'number' && Number.isInteger(step) && step >= 0 && step <= READY_STEP
    ? step
    : 0;
}

export function firstRunRequested(search: string): boolean {
  return new URLSearchParams(search).has('first-run');
}

/** The languages setup asks about. Every other language stays in Settings. */
export const SETUP_LANGUAGES = [
  { value: 'en', label: 'English' },
  { value: 'fr', label: 'French' },
  { value: 'hi', label: 'Hindi' },
] as const;
export type SetupLanguage = (typeof SETUP_LANGUAGES)[number]['value'];

export const COHERE = 'CohereLabs/cohere-transcribe-03-2026';
export const QWEN_ASR = 'Qwen/Qwen3-ASR-0.6B';
/** Speech models setup can offer, in display order, with one plain reason each. */
export const SETUP_SPEECH_MODELS = [
  { id: COHERE, reason: 'Most accurate for English and French' },
  { id: 'nvidia/parakeet-tdt-0.6b-v3', reason: 'Fast and light' },
  { id: QWEN_ASR, reason: 'Hindi and English' },
  { id: 'nvidia/nemotron-3.5-asr-streaming-0.6b', reason: 'Words appear while you speak' },
] as const;

/** The recommended speech model follows existing evidence (ADR-0010): the
 * current default for English and French, Qwen3-ASR for Hindi. Memory does not
 * change the pick until peak memory per model is measured (issue #270). */
export function recommendedSpeechModel(language: SetupLanguage): string {
  return language === 'hi' ? QWEN_ASR : COHERE;
}

/** Bytes still to download for these catalog entries; files already present
 * count as nothing. Unknown entries count as nothing rather than guessing. */
export function remainingBytes(
  catalog: {
    modelId: string;
    quantization: string | null;
    files: { installed: boolean; sizeBytes: number }[];
  }[],
  targets: { modelId: string; quantization: string | null }[],
): number {
  return targets.reduce((sum, target) => {
    const entry = catalog.find(
      (item) => item.modelId === target.modelId && item.quantization === target.quantization,
    );
    return (
      sum + (entry?.files ?? []).reduce((a, file) => a + (file.installed ? 0 : file.sizeBytes), 0)
    );
  }, 0);
}

/** Free space needed for a download: the files plus a quarter for unpacking
 * and verification. */
export function spaceNeeded(bytes: number): number {
  return Math.ceil(bytes * 1.25);
}

export function formatBytes(bytes: number): string {
  if (bytes >= 100e9) return `${Math.round(bytes / 1e9)} GB`;
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${Math.round(bytes / 1e6)} MB`;
  return `${Math.max(1, Math.round(bytes / 1e3))} KB`;
}

/** The sample sentence Your words previews: what someone says, and what the
 * writing step returns in each style with the words they taught it. */
export const SAMPLE_HEARD = 'so um can you send the arc light notes to priya by friday';
export interface SampleToken {
  text: string;
  /** Spelled from the person's own words. */
  taught: boolean;
}
const squash = (text: string) => text.toLowerCase().replace(/[^a-z0-9]/g, '');

export function sampleWritten(style: 'minimal' | 'balanced', words: string[]): SampleToken[] {
  let tokens: SampleToken[] = SAMPLE_HEARD.split(' ').map((text) => ({ text, taught: false }));
  for (const word of words) {
    const key = squash(word);
    if (!key) continue;
    search: for (let length = 3; length >= 1; length--)
      for (let i = 0; i + length <= tokens.length; i++)
        if (
          squash(
            tokens
              .slice(i, i + length)
              .map((token) => token.text)
              .join(''),
          ) === key
        ) {
          tokens.splice(i, length, { text: word, taught: true });
          break search;
        }
  }
  const capital = (token: SampleToken): SampleToken =>
    token.taught ? token : { ...token, text: token.text[0].toUpperCase() + token.text.slice(1) };
  tokens = tokens.map((token) =>
    ['priya', 'friday'].includes(token.text) ? capital(token) : token,
  );
  if (style === 'balanced')
    tokens = tokens.filter((token, i) => !(i < 2 && ['so', 'um'].includes(token.text)));
  else
    tokens = tokens.map((token, i) =>
      i < 2 && !token.taught ? { ...token, text: `${token.text},` } : token,
    );
  tokens[0] = capital(tokens[0]);
  const last = tokens.length - 1;
  tokens[last] = { ...tokens[last], text: `${tokens[last].text}?` };
  return tokens;
}

/** A word to add, or why it cannot be added. */
export function vocabularyWord(
  raw: string,
  existing: string[],
): { word: string } | { error: string } {
  const word = raw.trim().replace(/\s+/g, ' ');
  if (!word) return { error: 'Type a word first.' };
  if (word.length > 40) return { error: 'Keep a word under 40 characters.' };
  if (existing.some((entry) => entry.toLowerCase() === word.toLowerCase()))
    return { error: `“${word}” is already in your words.` };
  return { word };
}
