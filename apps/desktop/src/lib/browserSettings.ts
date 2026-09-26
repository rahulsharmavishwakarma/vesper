import { defaultSettings, parseSettings, parseStoredSettings, type AppConfig } from './config.ts';

const PREVIEW_SETTINGS_KEY = 'vesper.browser-preview.settings.v1';
const LEGACY_PREVIEW_SETTINGS_KEYS = [
  'transcribe.browser-preview.settings.v1',
  'ukti.browser-preview.settings.v1',
  'stt.browser-preview.settings.v1',
];

/** Read in place; a corrupt current value must not silently expose an older profile. */
export function readPreviewSettings(storage: Pick<Storage, 'getItem'>): AppConfig {
  try {
    for (const key of [PREVIEW_SETTINGS_KEY, ...LEGACY_PREVIEW_SETTINGS_KEYS]) {
      const stored = storage.getItem(key);
      if (stored !== null) return parseStoredSettings(JSON.parse(stored));
    }
    return defaultSettings();
  } catch (cause) {
    const message = cause instanceof Error ? cause.message : String(cause);
    throw new Error(`Browser preview settings could not be loaded. ${message}`, { cause });
  }
}

/** Only an explicit Save/Reset creates Vesper state; legacy bytes remain untouched. */
export function writePreviewSettings(storage: Pick<Storage, 'setItem'>, settings: AppConfig): void {
  storage.setItem(PREVIEW_SETTINGS_KEY, JSON.stringify(parseSettings(settings)));
}
