import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join, normalize } from 'node:path';
import test from 'node:test';
import { OUTPUT, TOKENS, palette, readTokens, renderAll } from './render-diagrams.mjs';

const root = join(import.meta.dirname, '..');
const rendered = await renderAll();

test('tokens merge every light block over the dark Ember defaults', () => {
  const { dark, light } = readTokens(readFileSync(TOKENS, 'utf8'));
  assert.equal(dark.bg, '#0a0a0a');
  assert.equal(dark.accent, '#ff6b3d');
  assert.equal(light.bg, '#ffffff');
  assert.equal(light.accent, '#e84d17');
  assert.equal(light['theme-mark'], '#e84d17');
  const colours = palette(light);
  assert.equal(colours.bg, light.bg);
  assert.equal(colours.app.stroke, light.accent);
  assert.equal(colours.runtime.stroke, light.warning);
});

test('every committed SVG matches its source and the current tokens', () => {
  assert.ok(Object.keys(rendered).length > 0);
  for (const [file, svg] of Object.entries(rendered)) {
    const path = join(OUTPUT, file);
    assert.ok(existsSync(path), `${file} is missing; run make diagrams`);
    assert.equal(readFileSync(path, 'utf8'), svg, `${file} is stale; run make diagrams`);
  }
});

test('no rendered SVG is left without its source', () => {
  for (const file of readdirSync(OUTPUT)) {
    assert.ok(file in rendered, `${file} has no source in docs/diagrams`);
  }
});

test('every diagram a page embeds exists', () => {
  const pages = execFileSync('git', ['ls-files', '*.md'], { cwd: root, encoding: 'utf8' })
    .split('\n')
    .filter(Boolean);
  for (const page of pages) {
    const prose = readFileSync(join(root, page), 'utf8').replace(/^```[\s\S]*?^```/gm, '');
    for (const [, target] of prose.matchAll(/(?:srcset|src)="([^"]+\.(?:light|dark)\.svg)"/g)) {
      assert.ok(existsSync(normalize(join(root, dirname(page), target))), `${page}: ${target}`);
    }
  }
});
