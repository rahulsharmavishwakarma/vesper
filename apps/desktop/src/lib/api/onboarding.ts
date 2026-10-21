import { invoke, isTauri } from '@tauri-apps/api/core';

export interface OnboardingState {
  version: number;
  status: 'pending' | 'completed';
  /** The step to resume at while pending (0 is Welcome). */
  step: number;
  reason: 'finished' | 'existing_install' | 'unreadable' | null;
  completedAt: number | null;
}

const COMPLETED: OnboardingState = {
  version: 1,
  status: 'completed',
  step: 7,
  reason: null,
  completedAt: null,
};

/** The first-run state. Anything unexpected reads as completed, so a failed
 * call can never hold someone in setup. The browser preview has no setup. */
export async function getOnboarding(): Promise<OnboardingState> {
  if (!isTauri()) return COMPLETED;
  try {
    const state = await invoke<OnboardingState>('get_onboarding');
    return state?.status === 'pending' || state?.status === 'completed' ? state : COMPLETED;
  } catch {
    return COMPLETED;
  }
}

export function saveOnboardingStep(step: number): Promise<OnboardingState> {
  return invoke<OnboardingState>('save_onboarding_step', { step });
}

export function completeOnboarding(): Promise<OnboardingState> {
  return invoke<OnboardingState>('complete_onboarding');
}
