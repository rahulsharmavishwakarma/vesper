import assert from 'node:assert/strict';
import test from 'node:test';
import {
  SHORTCUT_KEYS,
  shortcutIdentity,
  shortcutLabel,
  sameShortcut,
  shortcutFromKeyboard,
} from './recording-shortcuts.ts';

test('custom shortcuts normalize modifiers and distinguish platform-specific keys', () => {
  for (const mac of [false, true]) {
    const primary = mac ? 'Super' : 'Control';
    assert.equal(shortcutIdentity('Shift+CommandOrControl+K', mac), `${primary}+Shift+K`);
    assert.ok(sameShortcut('CommandOrControl+Shift+K', `Shift+${primary}+K`, mac));
    assert.ok(!sameShortcut('Super+K', 'Control+K', mac));
    assert.equal(shortcutIdentity(`CommandOrControl+${primary}+K`, mac), null);
    for (const key of SHORTCUT_KEYS) assert.ok(shortcutIdentity(`Alt+${key}`, mac), key);
    for (const valid of ['Control+Enter', 'Alt+Tab', 'Control+Backspace', 'Super+Delete']) {
      assert.ok(shortcutIdentity(valid, mac), valid);
    }
    for (const invalid of [
      'K',
      'Enter',
      'Shift+K',
      'Shift+Enter',
      'Control',
      'Fn',
      'Alt+Escape',
      'Escape',
      'Alt+F25',
      'Alt++K',
      'Ctrl+K',
      'Alt+Alt+K',
      'Alt+K+Shift',
      'Alt+😀',
    ]) {
      assert.equal(shortcutIdentity(invalid, mac), null, invalid);
    }
  }
  assert.equal(shortcutLabel('Super+Alt+K', true), '⌘ Command + ⌥ Option + K');
  assert.equal(shortcutLabel('Control+Alt+K', false), 'Ctrl + Alt + K');
  assert.ok(!sameShortcut(null, undefined));
});

test('shortcut recording captures physical codes and rejects composition and modifier-only input', () => {
  const key = {
    code: 'KeyK',
    ctrlKey: true,
    metaKey: false,
    altKey: true,
    shiftKey: false,
    isComposing: false,
    getModifierState: () => false,
  };
  assert.equal(shortcutFromKeyboard(key), 'Control+Alt+K');
  assert.equal(shortcutFromKeyboard({ ...key, code: 'Digit7' }), 'Control+Alt+7');
  assert.equal(shortcutFromKeyboard({ ...key, code: 'F8' }), 'Control+Alt+F8');
  assert.equal(shortcutFromKeyboard({ ...key, code: 'Slash' }), 'Control+Alt+Slash');
  assert.equal(shortcutFromKeyboard({ ...key, code: 'Enter' }), 'Control+Alt+Enter');
  assert.equal(shortcutFromKeyboard({ ...key, code: 'Tab' }), 'Control+Alt+Tab');
  assert.equal(shortcutFromKeyboard({ ...key, code: 'Backspace' }), 'Control+Alt+Backspace');
  assert.equal(shortcutFromKeyboard({ ...key, code: 'Delete' }), 'Control+Alt+Delete');
  for (const code of ['Escape', 'ControlLeft', 'MetaRight', 'Fn', 'Unidentified']) {
    assert.equal(shortcutFromKeyboard({ ...key, code }), null, code);
  }
  assert.equal(shortcutFromKeyboard({ ...key, isComposing: true }), null);
  assert.equal(shortcutFromKeyboard({ ...key, getModifierState: () => true }), null);
});
