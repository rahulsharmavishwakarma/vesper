import type { OnboardingState } from './api/onboarding';

/** Off until every step is built (issue #270), so no one meets a partial flow.
 * Previews and tests can still open it with `?first-run`. */
export const FIRST_RUN_ENABLED = false;

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
