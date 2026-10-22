import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import {
  ICONS,
  TITLE_BAR,
  WINDOW,
  dmgBackgroundSource,
  pngInfo,
  reedPath,
  tagRetina,
} from './generate-dmg-background.mjs';

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const bytes = (path) => readFileSync(new URL(`../${path}`, import.meta.url));
const source = () =>
  dmgBackgroundSource(
    read('apps/desktop/src/assets/ember-bird.svg'),
    read('apps/desktop/src/tokens.css'),
  );

test('the committed DMG background source matches its generator', () => {
  assert.equal(read('apps/desktop/src-tauri/dmg/background.svg'), source());
});

test('the DMG background draws the in-app waveform between the two icons', () => {
  const strokes = reedPath().match(/M [\d.]+ [\d.]+ V [\d.]+/g);
  assert.equal(strokes.length, 17);
  const xs = strokes.map((stroke) => Number(stroke.split(' ')[1]));
  assert.ok(xs[0] > ICONS.app.x + 56, 'waveform starts clear of the app icon');
  assert.ok(xs.at(-1) < ICONS.applications.x - 56, 'waveform ends clear of the Applications icon');
  // Bell-shaped like the app's reed: the middle stroke is taller than the ends.
  const heights = strokes.map((stroke) => {
    const [, , y1, , y2] = stroke.split(' ');
    return Number(y2) - Number(y1);
  });
  assert.ok(heights[8] > heights[0] * 4);
});

test('the DMG background uses brand colours and keeps text above the Finder title-bar crop', () => {
  const svg = source();
  for (const colour of ['#0a0a0a', '#ff6b3d', '#fff5ee', '#9a9088'])
    assert.match(svg, new RegExp(colour));
  assert.doesNotMatch(
    svg,
    /class="(?:bd|dsh|lid)/,
    'bird fills are resolved, not left to page CSS',
  );
  const lowest = Math.max(
    ...[...svg.matchAll(/<text [^>]*y="(\d+)"/g)].map((match) => Number(match[1])),
  );
  assert.ok(lowest <= WINDOW.height - 40);
});

test('the committed DMG image is 2x and tagged 144 dpi for Retina Finder', () => {
  const info = pngInfo(bytes('apps/desktop/src-tauri/dmg/background.png'));
  assert.deepEqual(info, {
    width: WINDOW.width * 2,
    height: WINDOW.height * 2,
    pixelsPerMetre: 5669,
  });
});

test('retina tagging keeps the image data intact', () => {
  const png = bytes('apps/desktop/src-tauri/dmg/background.png');
  const tagged = tagRetina(png);
  assert.equal(pngInfo(tagged).pixelsPerMetre, 5669);
  assert.equal(
    tagged.length,
    png.length + 21,
    'tagging an already tagged file adds one more chunk',
  );
});

test('Tauri places the icons where the artwork expects them', () => {
  const dmg = JSON.parse(read('apps/desktop/src-tauri/tauri.macos.conf.json')).bundle.macOS.dmg;
  assert.equal(dmg.background, 'dmg/background.png');
  assert.deepEqual(dmg.windowSize, { width: WINDOW.width, height: WINDOW.height + TITLE_BAR });
  assert.deepEqual(dmg.appPosition, ICONS.app);
  assert.deepEqual(dmg.applicationFolderPosition, ICONS.applications);
});

test('artwork whose comments reassemble after stripping is refused', () => {
  const tokens = read('apps/desktop/src/tokens.css');
  assert.throws(
    () => dmgBackgroundSource('<svg viewBox="0 0 1 1"><!-<!---->-><path/></svg>', tokens),
    /Unexpected comment markup/,
  );
  assert.doesNotMatch(
    dmgBackgroundSource('<svg viewBox="0 0 1 1"><!-- note --!><path/></svg>', tokens),
    /note/,
  );
});
