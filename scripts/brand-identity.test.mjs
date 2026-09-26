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
  const native = read('apps/desktop/src-tauri/icons/source.svg');
  assert.equal(native, iconSource(svg));
  const mark = svg.match(/<g class="vesper-mark"[\s\S]*?<\/g>/)[0];
  assert.ok(native.includes(mark));
  assert.equal(iconPalette.background, '#111512');
  assert.equal(iconPalette.bird, '#b1d3a3');
  assert.doesNotMatch(native, /<text\b|data-letter=/);
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
  const source = read('apps/desktop/src-tauri/icons/menu-bar-template.svg');
  assert.equal(source, menuBarIconSource(svg));
  assert.ok(source.includes(svg.match(/<g class="vesper-mark"[\s\S]*?<\/g>/)[0]));
  assert.match(source, /color="#000000"/);
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
  assert.match(read('scripts/package-release.sh'), /transcribe-macos-/);
  assert.match(read('.github/workflows/check.yml'), /productName/);
});

test('current product copy consistently names Vesper', () => {
  const expected = [
    ['README.md', '# Vesper'],
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
    assert.match(read(path), /Vesper Studio/, path);
  }
});

test('naming cleanup retains working technical names and historical evidence', () => {
  assert.match(read('pyproject.toml'), /name = "vesper-tooling"/);
  assert.match(read('.pre-commit-config.yaml'), /id: vesper-fast-check/);
  assert.match(read('.pre-commit-config.yaml'), /id: vesper-push-check/);
  assert.match(read('README.md'), /github\.com\/rahulsharmavishwakarma\/transcribe\.git/);
  assert.match(read('README.md'), /download\/transcribe-macos-arm64\.dmg/);
  assert.match(read('apps/desktop/src/lib/config.ts'), /'Cohere Transcribe'/);
  assert.match(read('apps/desktop/src/lib/components/QuickCapture.svelte'), /Stop and transcribe/);
  assert.match(read('docs/development.md'), /### Product name and compatibility/);
  assert.match(read('docs/validation.md'), /id="transcribe-integration-update"/);
  assert.match(read('docs/validation.md'), /Historical branding integration \(before Vesper\)/);
  assert.match(read('docs/validation.md'), /target\/debug\/bundle\/macos\/transcribe\.app/);
});

test('component motion targets calls only and supplies a still alternative', () => {
  const component = read('apps/desktop/src/lib/components/BrandLogo.svelte');
  assert.match(component, /prefers-reduced-motion: reduce/);
  assert.match(component, /animate = false/);
  assert.doesNotMatch(component, /infinite|scale\(|scaleX\(|scaleY\(/);
  assert.match(component, /vesper-call-1/);
  assert.match(component, /vesper-call-2/);
  assert.match(component, /import artwork from '\.\.\/\.\.\/assets\/vesper.svg\?raw'/);
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
