import { invoke, isTauri } from '@tauri-apps/api/core';
import desktopPackage from '../../../package.json' with { type: 'json' };
import {
  ASR_MODELS,
  CLEANUP_MODELS,
  defaultSettings,
  parseSettings,
  parseStoredSettings,
  type AppConfig,
} from '../config';
import type { EngineDiagnostics } from '../engineStatus';
import type { AcceleratorCapabilities } from '../accelerators';

export function getAcceleratorCapabilities(): Promise<AcceleratorCapabilities> {
  if (!isTauri())
    return Promise.reject(new Error('Device detection is available in the desktop app.'));
  return invoke<AcceleratorCapabilities>('get_accelerator_capabilities');
}

export function getEngineDiagnostics(): Promise<EngineDiagnostics> {
  if (!isTauri())
    return Promise.reject(new Error('Live resource readings require the desktop app.'));
  return invoke<EngineDiagnostics>('get_engine_diagnostics');
}

const PREVIEW_SETTINGS_KEY = 'transcribe.browser-preview.settings.v1';
const LEGACY_PREVIEW_SETTINGS_KEYS = [
  'ukti.browser-preview.settings.v1',
  'stt.browser-preview.settings.v1',
];
export type AppEnvironment = 'desktop' | 'browser-preview';

export function getEnvironment(): AppEnvironment {
  return isTauri() ? 'desktop' : 'browser-preview';
}

export interface AppInfo {
  name: string;
  version: string;
  buildType: 'development' | 'release' | 'browser-preview';
  os: string;
  arch: string;
}

export async function getAppInfo(): Promise<AppInfo> {
  if (isTauri()) return invoke<AppInfo>('get_app_info');
  return {
    name: 'transcribe',
    version: desktopPackage.version,
    buildType: 'browser-preview',
    os: 'browser',
    arch: '',
  };
}

/** The response contract for the Rust `get_app_status` command. */
export interface AppStatus {
  name: string;
  version: string;
  inference_ready: boolean;
  asr_model: string;
  rewrite_model: string;
  message: string;
  shortcut_error: string | null;
}

export interface AppStatusSnapshot {
  environment: AppEnvironment;
  status: AppStatus;
}

export async function getSettings({
  browserPreview = false,
}: { browserPreview?: boolean } = {}): Promise<AppConfig> {
  if (!browserPreview && isTauri()) return parseSettings(await invoke<unknown>('get_settings'));
  const stored =
    localStorage.getItem(PREVIEW_SETTINGS_KEY) ??
    LEGACY_PREVIEW_SETTINGS_KEYS.map((key) => localStorage.getItem(key)).find(
      (value) => value !== null,
    ) ??
    null;
  if (stored === null) return defaultSettings();
  try {
    return parseStoredSettings(JSON.parse(stored));
  } catch (cause) {
    const message = cause instanceof Error ? cause.message : String(cause);
    throw new Error(`Browser preview settings could not be loaded. ${message}`, { cause });
  }
}

export async function saveSettings(settings: AppConfig): Promise<AppConfig> {
  const validated = parseSettings(settings);
  if (isTauri())
    return parseSettings(await invoke<unknown>('save_settings', { settings: validated }));
  localStorage.setItem(PREVIEW_SETTINGS_KEY, JSON.stringify(validated));
  return validated;
}

export async function resetSettings(): Promise<AppConfig> {
  if (isTauri()) return parseSettings(await invoke<unknown>('reset_settings'));
  return saveSettings(defaultSettings());
}

export async function retryGlobalShortcut(): Promise<void> {
  if (!isTauri()) throw new Error('Shortcut registration requires the desktop app.');
  await invoke('retry_global_shortcut');
}

export async function getAppStatus(): Promise<AppStatusSnapshot> {
  if (!isTauri()) {
    const settings = await getSettings();
    return {
      environment: 'browser-preview',
      status: {
        name: 'transcribe',
        version: desktopPackage.version,
        inference_ready: false,
        asr_model: settings.models.customAsr?.name ?? ASR_MODELS[settings.models.asr],
        rewrite_model:
          settings.models.customRewriter?.name ?? CLEANUP_MODELS[settings.models.rewriter].name,
        message:
          'Browser preview. Settings are stored only in this browser; inference is not connected.',
        shortcut_error: null,
      },
    };
  }

  return {
    environment: 'desktop',
    status: await invoke<AppStatus>('get_app_status'),
  };
}
