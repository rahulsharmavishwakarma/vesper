import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const require = createRequire(import.meta.url);
export const nativeIconFiles = [
  '32x32.png',
  '64x64.png',
  '128x128.png',
  '128x128@2x.png',
  '256x256.png',
  'icon.png',
  'icon.ico',
  'icon.icns',
  'Square30x30Logo.png',
  'Square44x44Logo.png',
  'Square71x71Logo.png',
  'Square89x89Logo.png',
  'Square107x107Logo.png',
  'Square142x142Logo.png',
  'Square150x150Logo.png',
  'Square284x284Logo.png',
  'Square310x310Logo.png',
  'StoreLogo.png',
];

// sRGB equivalents of the approved --color-bg and --color-accent tokens.
// The SVG rasterizer needs these resolved colours rather than CSS OKLCH variables.
export const iconPalette = { background: '#0a0a0a', bird: '#ff6b3d' };

// Tauri emits ICNS entries in hash-map order. Container order is immaterial to
// rendering; canonicalize it so regeneration produces stable tracked bytes.
export function canonicalIcns(data) {
  if (
    data.length < 8 ||
    data.toString('ascii', 0, 4) !== 'icns' ||
    data.readUInt32BE(4) !== data.length
  ) {
    throw new Error('Invalid ICNS container');
  }
  const chunks = [];
  for (let offset = 8; offset < data.length;) {
    if (offset + 8 > data.length) throw new Error('Truncated ICNS entry');
    const length = data.readUInt32BE(offset + 4);
    if (length < 8 || offset + length > data.length) throw new Error('Invalid ICNS entry');
    chunks.push(data.subarray(offset, offset + length));
    offset += length;
  }
  return Buffer.concat([data.subarray(0, 8), ...chunks.sort(Buffer.compare)]);
}

function canonicalMark(svg) {
  const mark = svg.match(/<g class="vesper-mark"[\s\S]*?<\/g>/)?.[0];
  if (!mark || /<(?:script|image|use|foreignObject|animate)\b|\b(?:href|style)=/i.test(mark)) {
    throw new Error('Expected the self-contained canonical Vesper mark.');
  }
  return mark;
}

export function menuBarIconSource(svg) {
  // The viewBox frames the mark ink bounds (x 4.9-72.1, y 8-53.4) with
  // antialias-safe padding, so the bird fills the frame width. macOS scales
  // the 36px template to 18pt; a full 96-unit frame left the bird at ~9pt
  // tall against ~18pt neighbours.
  return `<svg xmlns="http://www.w3.org/2000/svg" width="36" height="36" viewBox="3 6 71 50">
  <!-- Generated from src/assets/ember-bird.svg; transparent macOS template, no tile or lettering. -->
  <g fill="#000000">
    ${canonicalMark(svg)}
  </g>
</svg>
`;
}

export function iconSource(svg) {
  const mark = canonicalMark(svg);
  return `<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512">
  <!-- Generated from src/assets/ember-bird.svg by scripts/generate-brand-icons.mjs. -->
  <rect x="16" y="16" width="480" height="480" rx="112" fill="${iconPalette.background}"/>
  <g transform="translate(56 106) scale(5)" fill="${iconPalette.bird}">
    ${mark}
  </g>
</svg>
`;
}

export function generateIcons({ check = false } = {}) {
  const directory = join(root, 'apps/desktop/src-tauri/icons');
  const svg = readFileSync(join(root, 'apps/desktop/src/assets/ember-bird.svg'), 'utf8');
  const source = iconSource(svg);
  const menuBarSource = menuBarIconSource(svg);
  const work = mkdtempSync(join(tmpdir(), 'vesper-icons-'));
  const input = join(work, 'source.svg');
  const output = join(work, 'rendered');
  writeFileSync(input, source);
  const menuBarInput = join(work, 'menu-bar-template.svg');
  const menuBarOutput = join(work, 'menu-bar');
  writeFileSync(menuBarInput, menuBarSource);
  const cli = require.resolve('@tauri-apps/cli/tauri.js');
  for (const [image, destination, sizes] of [
    [input, output, []],
    [input, output, ['--png', '64', '--png', '256']],
    [menuBarInput, menuBarOutput, ['--png', '36']],
  ]) {
    const result = spawnSync(
      process.execPath,
      [cli, 'icon', image, '--output', destination, ...sizes],
      {
        cwd: root,
        encoding: 'utf8',
      },
    );
    if (result.error) throw result.error;
    if (result.status !== 0)
      throw new Error(result.stderr || result.stdout || 'Icon generation failed.');
  }
  const expected = [
    ['source.svg', Buffer.from(source)],
    ['menu-bar-template.svg', Buffer.from(menuBarSource)],
    ['menu-bar-template.png', readFileSync(join(menuBarOutput, '36x36.png'))],
    ...nativeIconFiles.map((name) => {
      const data = readFileSync(join(output, name));
      return [name, name === 'icon.icns' ? canonicalIcns(data) : data];
    }),
  ];
  // Check every output before replacing any tracked derivative.
  if (check) {
    for (const [name, bytes] of expected) {
      if (!readFileSync(join(directory, name)).equals(bytes)) {
        throw new Error(`${name} is stale. Run node scripts/generate-brand-icons.mjs.`);
      }
    }
    console.log(
      `Canonical SVG, ${nativeIconFiles.length} desktop icons and menu-bar template match.`,
    );
  } else {
    for (const [name, data] of expected) writeFileSync(join(directory, name), data);
    console.log(
      `Generated source.svg, ${nativeIconFiles.length} desktop icons and menu-bar template.`,
    );
  }
  // Scratch output is retained in the OS temp directory for diagnosis; no mobile
  // assets or new platform declarations are copied into the repository.
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  generateIcons({ check: process.argv.includes('--check') });
}
