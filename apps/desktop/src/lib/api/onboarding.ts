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

export interface DeviceProfile {
  os: string;
  arch: string;
  chip: string | null;
  memoryBytes: number | null;
  /** Free space on the volume that holds the model folder. */
  freeBytes: number | null;
}
export type MicrophoneAccess = 'not_asked' | 'restricted' | 'denied' | 'allowed' | 'asking';

export function getDeviceProfile(): Promise<DeviceProfile> {
  return invoke<DeviceProfile>('get_device_profile');
}
/** `request` shows the macOS prompt only when the user has not been asked. */
export function microphoneAccess(request = false): Promise<MicrophoneAccess> {
  return invoke<MicrophoneAccess>('microphone_access', { request });
}
export function openPrivacySettings(pane: 'microphone' | 'accessibility'): Promise<void> {
  return invoke<void>('open_privacy_settings', { pane });
}
export function startMicrophoneCheck(deviceId: string | null): Promise<void> {
  return invoke<void>('start_microphone_check', { deviceId });
}
export function stopMicrophoneCheck(): Promise<void> {
  return invoke<void>('stop_microphone_check');
}
