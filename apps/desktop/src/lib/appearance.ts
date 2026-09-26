export type Appearance = 'light' | 'dark' | 'system';
export const APPEARANCE_KEY = 'vesper-appearance-v1';
export function readAppearance(): Appearance {
  try {
    const value = localStorage.getItem(APPEARANCE_KEY);
    return value === 'light' || value === 'dark' || value === 'system' ? value : 'dark';
  } catch {
    return 'dark';
  }
}
export function applyAppearance(value: Appearance): void {
  document.documentElement.dataset.theme =
    value === 'system'
      ? matchMedia('(prefers-color-scheme: dark)').matches
        ? 'dark'
        : 'light'
      : value;
}
export function saveAppearance(value: Appearance): void {
  // Persist first: a storage error leaves both the saved and displayed choice unchanged.
  localStorage.setItem(APPEARANCE_KEY, value);
  applyAppearance(value);
  window.dispatchEvent(new Event('vesper-appearance-change'));
}
export function initializeAppearance(): () => void {
  const system = matchMedia('(prefers-color-scheme: dark)');
  const refresh = () => applyAppearance(readAppearance());
  const storage = (event: StorageEvent) => {
    if (event.key === APPEARANCE_KEY || event.key === null) refresh();
  };
  refresh();
  system.addEventListener('change', refresh);
  window.addEventListener('storage', storage);
  return () => {
    system.removeEventListener('change', refresh);
    window.removeEventListener('storage', storage);
  };
}
