import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join, normalize } from 'node:path';
import test from 'node:test';
import { OUTPUT, SOURCES, TOKENS, header, readTokens, themeFor } from './render-diagrams.mjs';

const root = join(import.meta.dirname, '..');
const tokensCss = readFileSync(TOKENS, 'utf8');
const sources = readdirSync(SOURCES).filter((file) => file.endsWith('.mmd'));

test('tokens merge every light block over the dark Ember defaults', () => {
  const { dark, light } = readTokens(tokensCss);
  assert.equal(dark.bg, '#0a0a0a');
  assert.equal(dark.accent, '#ff6b3d');
  assert.equal(light.bg, '#ffffff');
  assert.equal(light.accent, '#e84d17');
  assert.equal(light['theme-mark'], '#e84d17');
  const theme = themeFor(light);
  assert.equal(theme.background, light.bg);
  assert.equal(theme.primaryTextColor, light.ink);
  assert.equal(theme.titleColor, light.accent);
});

test('every diagram source has light and dark SVGs rendered from it and the current tokens', () => {
  assert.ok(sources.length > 0);
  for (const file of sources) {
    const name = file.replace(/\.mmd$/, '');
    const expected = header(name, readFileSync(join(SOURCES, file), 'utf8'), tokensCss);
    for (const mode of ['light', 'dark']) {
      const svg = join(OUTPUT, `${name}.${mode}.svg`);
      assert.ok(existsSync(svg), `${name}.${mode}.svg is missing; run make diagrams`);
      assert.ok(
        readFileSync(svg, 'utf8').startsWith(expected),
        `${name}.${mode}.svg is stale; run make diagrams`,
      );
    }
  }
});

test('no rendered SVG is left without its source', () => {
  const names = new Set(sources.map((file) => file.replace(/\.mmd$/, '')));
  for (const svg of readdirSync(OUTPUT)) {
    assert.ok(names.has(svg.replace(/\.(light|dark)\.svg$/, '')), `${svg} has no source`);
  }
});

test('pages embed diagrams as rendered pictures, not inline Mermaid', () => {
  const pages = execFileSync('git', ['ls-files', '*.md'], { cwd: root, encoding: 'utf8' })
    .split('\n')
    .filter(Boolean);
  for (const page of pages) {
    const text = readFileSync(join(root, page), 'utf8');
    assert.ok(!/^```mermaid/m.test(text), `${page} has an inline Mermaid block`);
    const prose = text.replace(/^```[\s\S]*?^```/gm, '');
    for (const [, target] of prose.matchAll(/(?:srcset|src)="([^"]+\.(?:light|dark)\.svg)"/g)) {
      assert.ok(existsSync(normalize(join(root, dirname(page), target))), `${page}: ${target}`);
    }
  }
});
