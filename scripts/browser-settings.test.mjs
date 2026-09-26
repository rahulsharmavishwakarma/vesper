import assert from 'node:assert/strict';
import test from 'node:test';
import { defaultSettings } from '../apps/desktop/src/lib/config.ts';
import {
  readPreviewSettings,
  writePreviewSettings,
} from '../apps/desktop/src/lib/browserSettings.ts';

const key = (name) => `${name}.browser-preview.settings.v1`;
const configured = (suffix) => ({ ...defaultSettings(), modelDirectory: `/models/${suffix}` });
const memory = (entries = []) => {
  const values = new Map(entries);
  return {
    values,
    getItem: (name) => values.get(name) ?? null,
    setItem: (name, value) => values.set(name, value),
  };
};

test('preview reads Vesper first, then every legacy key, without writing', () => {
  const names = ['vesper', 'transcribe', 'ukti', 'stt'];
  for (let start = 0; start < names.length; start++) {
    const entries = names
      .slice(start)
      .map((name, index) => [key(name), JSON.stringify(configured(60 + index))]);
    const storage = memory(entries);
    assert.deepEqual(readPreviewSettings(storage), readPreviewSettings(memory([entries[0]])));
    assert.deepEqual([...storage.values], entries);
  }
  const empty = memory();
  assert.deepEqual(readPreviewSettings(empty), defaultSettings());
  assert.equal(empty.values.size, 0);
});

test('explicit preview save and reset create only Vesper state and preserve legacy bytes', () => {
  const legacy = JSON.stringify(configured(90));
  const storage = memory([[key('transcribe'), legacy]]);
  writePreviewSettings(storage, defaultSettings());
  assert.deepEqual(readPreviewSettings(storage), defaultSettings());
  assert.equal(storage.values.get(key('transcribe')), legacy);
  assert.deepEqual([...storage.values.keys()], [key('transcribe'), key('vesper')]);
});

test('corrupt current preview settings and storage failures are reported, never bypassed', () => {
  for (const corrupt of ['', '{', '{"schemaVersion":999}']) {
    const storage = memory([
      [key('vesper'), corrupt],
      [key('transcribe'), JSON.stringify(defaultSettings())],
    ]);
    assert.throws(() => readPreviewSettings(storage), /could not be loaded/);
    assert.equal(storage.values.get(key('vesper')), corrupt);
  }
  assert.throws(
    () =>
      readPreviewSettings({
        getItem() {
          throw new Error('denied');
        },
      }),
    /denied/,
  );
  assert.throws(
    () =>
      writePreviewSettings(
        {
          setItem() {
            throw new Error('quota');
          },
        },
        defaultSettings(),
      ),
    /quota/,
  );
});
