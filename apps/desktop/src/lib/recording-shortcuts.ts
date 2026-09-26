// Keep this contract aligned with vesper-core/src/shortcuts.rs. The native
// registration layer also parses bindings with the installed global-hotkey API.
export const SHORTCUT_MODIFIERS = ['Control', 'Super', 'Alt', 'Shift'] as const;
export const SHORTCUT_KEYS = [
  'Space',
  ...'ABCDEFGHIJKLMNOPQRSTUVWXYZ',
  ...'0123456789',
  ...Array.from({ length: 24 }, (_, i) => `F${i + 1}`),
  'ArrowUp',
  'ArrowDown',
  'ArrowLeft',
  'ArrowRight',
  'Home',
  'End',
  'PageUp',
  'PageDown',
  'Backquote',
  'Minus',
  'Equal',
  'BracketLeft',
  'BracketRight',
  'Backslash',
  'Semicolon',
  'Quote',
  'Comma',
  'Period',
  'Slash',
] as const;

export function isMacKeyboard(): boolean {
  return typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform);
}

export function shortcutParts(
  value: unknown,
  mac = isMacKeyboard(),
): { modifiers: string[]; key: string } | null {
  if (typeof value !== 'string' || value.length > 100) return null;
  const tokens = value.split('+');
  const key = tokens.pop()!;
  if (!SHORTCUT_KEYS.includes(key) || !tokens.length) return null;
  const modifiers = new Set<string>();
  for (const token of tokens) {
    const modifier = token === 'CommandOrControl' ? (mac ? 'Super' : 'Control') : token;
    if (!SHORTCUT_MODIFIERS.some((item) => item === modifier) || modifiers.has(modifier))
      return null;
    modifiers.add(modifier);
  }
  if (![...modifiers].some((modifier) => modifier !== 'Shift')) return null;
  return { modifiers: SHORTCUT_MODIFIERS.filter((modifier) => modifiers.has(modifier)), key };
}

export function shortcutIdentity(value: unknown, mac = isMacKeyboard()): string | null {
  const parts = shortcutParts(value, mac);
  return parts ? [...parts.modifiers, parts.key].join('+') : null;
}

export function sameShortcut(left: unknown, right: unknown, mac = isMacKeyboard()): boolean {
  const identity = shortcutIdentity(left, mac);
  return identity !== null && identity === shortcutIdentity(right, mac);
}

export function shortcutModifierLabel(modifier: string, mac = isMacKeyboard()): string {
  return (
    (
      {
        Control: 'Ctrl',
        Super: mac ? '⌘ Command' : 'Super / Windows',
        Alt: mac ? '⌥ Option' : 'Alt',
        Shift: 'Shift',
      } as Record<string, string>
    )[modifier] ?? modifier
  );
}

export function shortcutKeyLabel(key: string): string {
  return (
    (
      {
        ArrowUp: '↑',
        ArrowDown: '↓',
        ArrowLeft: '←',
        ArrowRight: '→',
        PageUp: 'Page Up',
        PageDown: 'Page Down',
        Backquote: '`',
        Minus: '−',
        Equal: '=',
        BracketLeft: '[',
        BracketRight: ']',
        Backslash: '\\',
        Semicolon: ';',
        Quote: "'",
        Comma: ',',
        Period: '.',
        Slash: '/',
      } as Record<string, string>
    )[key] ?? key
  );
}

export function shortcutLabel(value: string, mac = isMacKeyboard()): string {
  const parts = shortcutParts(value, mac);
  return parts
    ? [
        ...parts.modifiers.map((modifier) => shortcutModifierLabel(modifier, mac)),
        shortcutKeyLabel(parts.key),
      ].join(' + ')
    : value;
}

export function shortcutFromKeyboard(
  event: Pick<
    KeyboardEvent,
    'code' | 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey' | 'isComposing' | 'getModifierState'
  >,
): string | null {
  if (event.isComposing || event.getModifierState('AltGraph')) return null;
  const key = event.code.replace(/^(Key|Digit)(?=[A-Z0-9]$)/, '');
  const parts = [
    [event.ctrlKey, 'Control'],
    [event.metaKey, 'Super'],
    [event.altKey, 'Alt'],
    [event.shiftKey, 'Shift'],
  ]
    .filter(([pressed]) => pressed)
    .map(([, modifier]) => modifier);
  return shortcutIdentity([...parts, key].join('+'));
}
