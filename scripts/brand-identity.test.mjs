import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import {
  canonicalIcns,
  iconPalette,
  iconSource,
  menuBarIconSource,
  nativeIconFiles,
} from './generate-brand-icons.mjs';
import { readmeBannerSource } from './generate-readme-banner.mjs';

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const bytes = (path) => readFileSync(new URL(`../${path}`, import.meta.url));
const svg = read('apps/desktop/src/assets/vesper.svg');

test('the canonical selected identity spells every letter with custom paths', () => {
  assert.deepEqual(
    [...svg.matchAll(/data-letter="(.)"/g)].map((match) => match[1]),
    [...'vesper'],
  );
  assert.match(svg, /aria-label="Vesper"/);
  assert.match(svg, /viewBox="0 0 399 104"/);
  assert.equal((svg.match(/<path\b/g) || []).length, 9);
  assert.doesNotMatch(
    svg,
    /<(?:text|script|image|use|foreignObject|animate)\b|https?:\/\/(?!www\.w3\.org)/,
  );
});

test('native source is generated from the same bird, not a separate redraw', () => {
  const ember = read('apps/desktop/src/assets/ember-bird.svg');
  const native = read('apps/desktop/src-tauri/icons/source.svg');
  assert.equal(native, iconSource(ember));
  const mark = ember.match(/<g class="vesper-mark"[\s\S]*?<\/g>/)[0];
  assert.ok(native.includes(mark));
  assert.equal(iconPalette.background, '#0a0a0a');
  assert.equal(iconPalette.bird, '#ff6b3d');
  assert.doesNotMatch(native, /<text\b|data-letter=/);
});

test('the Ember mark carries the pipeline group with no paint attributes', () => {
  const ember = read('apps/desktop/src/assets/ember-bird.svg');
  assert.match(ember, /<g class="vesper-mark">/);
  assert.match(ember, /viewBox="0 0 80 60"/);
  assert.doesNotMatch(ember, /<(?:text|script|image|use|foreignObject|animate)\b/);
});

test('the Ember eye stays open where brand CSS cannot run', () => {
  // The lid ellipse shares the bird fill, so rasterized exports (dock,
  // notifications, menu-bar template) would plug the evenodd eye cutout
  // without a collapsed default. In-app CSS still overrides this attribute
  // for the blink.
  const ember = read('apps/desktop/src/assets/ember-bird.svg');
  assert.match(
    ember,
    /<ellipse class="lid"[^>]*transform="translate\(38\.5 17\.4\) scale\(1 0\) translate\(-38\.5 -17\.4\)"/,
  );
});

test('README banner preserves every canonical path and uses existing brand tokens', () => {
  const banner = read('docs/assets/vesper-banner.svg');
  const tokens = read('apps/desktop/src/tokens.css');
  assert.equal(banner, readmeBannerSource(svg, tokens));
  assert.deepEqual(
    [...banner.matchAll(/<path\b[^>]*>/g)].map((match) => match[0]),
    [...svg.matchAll(/<path\b[^>]*>/g)].map((match) => match[0]),
  );
  assert.deepEqual(
    [...banner.matchAll(/data-letter="(.)"/g)].map((match) => match[1]),
    [...'vesper'],
  );
  assert.match(banner, /<title id="title">Vesper<\/title>/);
  assert.doesNotMatch(banner, /Hallmark|pre-emit critique|P[1-5] H[1-5] E[1-5]/i);
  assert.match(banner, /width="1200" height="320" viewBox="0 0 1200 320"/);
  assert.doesNotMatch(
    banner,
    /<(?:text|script|image|use|foreignObject|animate)\b|@import|@font-face|@keyframes|\bhref=|https?:\/\/(?!www\.w3\.org)/,
  );
});

test('README banner generator does not reintroduce design-tool metadata', () => {
  const generator = read('scripts/generate-readme-banner.mjs');
  assert.doesNotMatch(generator, /Hallmark|pre-emit critique|P[1-5] H[1-5] E[1-5]/i);
});

test('README banner fails clearly if its canonical inputs are missing', () => {
  assert.throws(() => readmeBannerSource('<svg/>', ''), /canonical Vesper SVG/);
  assert.throws(() => readmeBannerSource(svg, ''), /Missing brand token: --color-bg/);
});

test('README opens with accessible Vesper branding without promising a release', () => {
  const readme = read('README.md');
  assert.match(
    readme,
    /^<p align="center">\s*<img src="docs\/assets\/vesper-banner.svg" alt="Vesper" width="1200">\s*<\/p>/,
  );
  assert.match(readme, /<h1 align="center">Voice to text, on your device\.<\/h1>/);
  assert.match(readme, /<p align="center">Local models · Early&nbsp;development<\/p>/);
  assert.doesNotMatch(readme.split('## Features')[0], /\bMac(?:s|OS)?\b|Apple.silicon/i);
  assert.match(readme, /macOS DMG for Apple-silicon Macs/);
  assert.match(readme, /A public installer is not available yet/);
  assert.match(readme, /A project license has not yet been selected/);
  assert.doesNotMatch(readme, /technical identifiers retain\s+their existing names/);
});

test('all existing desktop icons have the expected format and dimensions', () => {
  for (const name of nativeIconFiles) {
    const data = bytes(`apps/desktop/src-tauri/icons/${name}`);
    if (name.endsWith('.png')) {
      assert.equal(data.subarray(0, 8).toString('hex'), '89504e470d0a1a0a', name);
      const width = data.readUInt32BE(16),
        height = data.readUInt32BE(20);
      const expected =
        name === 'icon.png'
          ? 512
          : name === 'StoreLogo.png'
            ? 50
            : name === '128x128@2x.png'
              ? 256
              : Number(name.match(/\d+/)[0]);
      assert.equal(width, expected, name);
      assert.equal(height, expected, name);
      assert.equal(data[25], 6, `${name} must retain alpha`);
    } else if (name.endsWith('.icns')) {
      assert.equal(data.toString('ascii', 0, 4), 'icns');
      assert.equal(data.readUInt32BE(4), data.length);
    } else {
      assert.equal(data.subarray(0, 4).toString('hex'), '00000100');
      assert.ok(data.readUInt16LE(4) > 0);
    }
  }
});

test('the macOS menu bar uses only the canonical still bird as a template', () => {
  const ember = read('apps/desktop/src/assets/ember-bird.svg');
  const source = read('apps/desktop/src-tauri/icons/menu-bar-template.svg');
  assert.equal(source, menuBarIconSource(ember));
  assert.ok(source.includes(ember.match(/<g class="vesper-mark"[\s\S]*?<\/g>/)[0]));
  assert.match(source, /fill="#000000"/);
  assert.doesNotMatch(source, /<rect\b|data-letter=|<animate\b|<style\b/);
  const png = bytes('apps/desktop/src-tauri/icons/menu-bar-template.png');
  assert.equal(png.readUInt32BE(16), 36);
  assert.equal(png.readUInt32BE(20), 36);
  assert.equal(png[25], 6, 'template retains alpha');
  const tray = read('apps/desktop/src-tauri/src/menu_bar.rs');
  assert.match(tray, /include_image!\("icons\/menu-bar-template\.png"\)/);
  assert.match(tray, /builder\.icon\(MENU_BAR_ICON\)\.icon_as_template\(true\)/);
  assert.doesNotMatch(tray, /\.title\(|\.set_title\(/);
  assert.match(tray, /\.tooltip\(format!\("Vesper —/);
  assert.match(tray, /\.show_menu_on_left_click\(true\)/);
});

test('Vesper identity retains explicit compatibility for earlier storage', () => {
  const config = JSON.parse(read('apps/desktop/src-tauri/tauri.conf.json'));
  assert.equal(config.productName, 'Vesper');
  assert.equal(config.identifier, 'com.vesper.desktop');
  assert.deepEqual(
    config.app.windows.map((window) => window.label),
    ['main', 'quick-capture'],
  );
  assert.ok(config.app.windows.every((window) => window.title.startsWith('Vesper')));
  assert.equal(JSON.parse(read('apps/desktop/package.json')).name, '@vesper/desktop');
  assert.match(
    read('apps/desktop/src/lib/browserSettings.ts'),
    /vesper\.browser-preview\.settings\.v1/,
  );
  assert.match(read('apps/desktop/src-tauri/src/storage.rs'), /com\.transcribe\.desktop/);
  assert.match(read('scripts/package-release.sh'), /productName/);
  assert.match(read('scripts/package-release.sh'), /vesper-macos-/);
  assert.match(read('.github/workflows/check.yml'), /productName/);
});

test('current product copy consistently names Vesper', () => {
  const expected = [
    ['README.md', 'alt="Vesper"'],
    ['CONTRIBUTING.md', '# Contributing to Vesper'],
    ['SECURITY.md', 'Until Vesper publishes stable releases'],
    ['.github/ISSUE_TEMPLATE/bug_report.yml', 'Start Vesper with'],
    ['.github/ISSUE_TEMPLATE/feature_request.yml', 'improvement to Vesper'],
    ['apps/desktop/src/lib/components/PersonalizationPanel.svelte', 'Vesper automatically learns'],
    ['apps/desktop/src-tauri/src/lib.rs', 'failed to build the Vesper desktop application'],
    ['apps/desktop/src-tauri/examples/learning_webview.rs', 'Vesper — public audio qualification'],
    ['apps/desktop/src-tauri/capabilities/default.json', 'local Vesper application'],
    ['pyproject.toml', 'tooling for the native Vesper app'],
    ['.pre-commit-config.yaml', 'name: Vesper Python locks'],
    ['docs/research.md', 'The application is named **Vesper**'],
    ['docs/validation/native-device-checklist.md', 'target/debug/bundle/macos/Vesper.app'],
  ];
  for (const [path, copy] of expected) assert.ok(read(path).includes(copy), path);
  for (const path of [
    'apps/desktop/src/app.css',
    'apps/desktop/src/tokens.css',
    'apps/desktop/src/lib/components/Select.svelte',
    'apps/desktop/src/lib/components/PersonalizationPanel.svelte',
  ]) {
    assert.doesNotMatch(read(path), /transcribe Studio/i, path);
  }
  for (const path of ['apps/desktop/src/app.css', 'apps/desktop/src/tokens.css']) {
    assert.match(read(path), /Vesper Ember/, path);
  }
});

test('naming cleanup retains working technical names and historical evidence', () => {
  assert.match(read('pyproject.toml'), /name = "vesper-tooling"/);
  assert.match(read('.pre-commit-config.yaml'), /id: vesper-fast-check/);
  assert.match(read('.pre-commit-config.yaml'), /id: vesper-push-check/);
  assert.match(read('README.md'), /github\.com\/rahulsharmavishwakarma\/transcribe\.git/);
  assert.match(read('README.md'), /download\/vesper-macos-arm64\.dmg/);
  assert.match(read('apps/desktop/src/lib/config.ts'), /'Cohere Transcribe'/);
  assert.match(read('apps/desktop/src/lib/components/QuickCapture.svelte'), /Stop and transcribe/);
  assert.match(read('docs/development.md'), /### Product name and compatibility/);
  assert.match(read('docs/validation.md'), /id="transcribe-integration-update"/);
  assert.match(read('docs/validation.md'), /Historical branding integration \(before Vesper\)/);
  assert.match(read('docs/validation.md'), /target\/debug\/bundle\/macos\/transcribe\.app/);
});

test('component motion targets the mark details only and supplies a still alternative', () => {
  const component = read('apps/desktop/src/lib/components/BrandLogo.svelte');
  assert.match(component, /prefers-reduced-motion: reduce/);
  assert.match(component, /animate = false/);
  assert.match(component, /import artwork from '\.\.\/\.\.\/assets\/ember-bird\.svg\?raw'/);
  assert.match(component, /ember-chirp/);
  assert.match(component, /ember-blink/);
  assert.doesNotMatch(component, /vesper-call-|data-letter=/);
  const animated = [...component.matchAll(/animation:\s*([\w-]+)/g)].map((match) => match[1]);
  assert.deepEqual(animated, ['ember-chirp', 'ember-blink', 'none']);
});

test('ICNS entry normalization is stable and rejects truncated containers', () => {
  const original = bytes('apps/desktop/src-tauri/icons/icon.icns');
  assert.deepEqual(canonicalIcns(original), original);
  const chunks = [];
  for (let offset = 8; offset < original.length;) {
    const length = original.readUInt32BE(offset + 4);
    chunks.unshift(original.subarray(offset, offset + length));
    offset += length;
  }
  assert.deepEqual(canonicalIcns(Buffer.concat([original.subarray(0, 8), ...chunks])), original);
  assert.throws(() => canonicalIcns(original.subarray(0, original.length - 1)), /Invalid ICNS/);
});
