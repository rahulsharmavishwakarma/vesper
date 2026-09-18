import { isTauri } from '@tauri-apps/api/core';

/** Ask only after the user explicitly enables notifications in Settings. */
export async function ensureNotificationPermission(): Promise<boolean> {
  if (!isTauri()) return false;
  const { isPermissionGranted, requestPermission } =
    await import('@tauri-apps/plugin-notification');
  if (await isPermissionGranted()) return true;
  return (await requestPermission()) === 'granted';
}
