#!/usr/bin/env node
// Assembles the GitHub Pages site (issue #329): copies `site/`, adds the app's
// Bricolage Grotesque font with its licence, and writes `benchmarks.json` for
// the landing page from Vesper's own evidence in models/recommendations.json.
// pages.yml runs this, then adds the release feeds to the same directory.
//
//   node scripts/build-site.mjs --out=_site
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const FONT_FILES = ['bricolage-grotesque.ttf', 'bricolage-OFL.txt'];

/**
 * The landing page's benchmark data: only results Vesper measured itself
 * through the application with an explicit language, and only those that met
 * the quality bar. Published leaderboard figures stay separate and labelled.
 */
export function landingBenchmarks(recommendations, catalog) {
  const names = new Map((catalog.models ?? []).map((m) => [m.id, m.name]));
  const name = (id) => names.get(id) ?? id;
  const runs = recommendations.local?.runs ?? {};
  const results = [];
  for (const [id, byLanguage] of Object.entries(recommendations.local?.results ?? {})) {
    for (const [language, r] of Object.entries(byLanguage)) {
      if (!r.passed) continue;
      if (![r.wer, r.ms, r.peakMemoryGb].every(Number.isFinite)) continue;
      results.push({
        model: name(id),
        id,
        language,
        wer: r.wer,
        ms: r.ms,
        peakMemoryGb: r.peakMemoryGb,
        run: r.run,
      });
    }
  }
  results.sort((a, b) => a.language.localeCompare(b.language) || a.wer - b.wer);

  const used = [...new Set(results.map((r) => r.run))].map((key) => runs[key]).filter(Boolean);
  const unique = (values) => [...new Set(values.filter(Boolean))];
  const hardware = unique(used.map((r) => r.hardware));
  const dates = unique(used.map((r) => r.date)).sort();
  const recordings = unique(used.map((r) => Number(/^(\d+)\s/.exec(r.label ?? '')?.[1]) || null));

  const display = new Intl.DisplayNames(['en'], { type: 'language' });
  const codes = [...new Set(results.map((r) => r.language))].sort((a, b) =>
    a === 'en' ? -1 : b === 'en' ? 1 : display.of(a).localeCompare(display.of(b)),
  );
  const languages = Object.fromEntries(codes.map((code) => [code, display.of(code)]));

  // Leaderboard averages only (not makers' own claims), for models on the page.
  const onPage = new Set(results.map((r) => r.id));
  const board = Object.entries(recommendations.published ?? {})
    .filter(
      ([id, byLanguage]) =>
        onPage.has(id) && byLanguage.en?.source === 'leaderboard' && byLanguage.en.metric === 'wer',
    )
    .map(([id, byLanguage]) => ({
      model: name(id),
      wer: byLanguage.en.value,
      set: byLanguage.en.set,
      url: byLanguage.en.url,
    }))
    .sort((a, b) => a.wer - b.wer);

  return {
    hardware: hardware.length === 1 ? hardware[0] : null,
    date: dates.at(-1) ?? null,
    recordings: recordings.length === 1 ? recordings[0] : null,
    languages,
    results: results.map(({ run: _run, ...r }) => r),
    published: board.length
      ? {
          note: `Open ASR Leaderboard, ${unique(board.map((b) => b.set)).join('; ')}`,
          url: board[0].url,
          english: board.map(({ model, wer }) => ({ model, wer })),
        }
      : null,
  };
}

export function buildSite(out, root = ROOT) {
  fs.rmSync(out, { recursive: true, force: true });
  fs.cpSync(path.join(root, 'site'), out, { recursive: true });
  const assets = path.join(out, 'assets');
  fs.mkdirSync(assets, { recursive: true });
  for (const file of FONT_FILES)
    fs.copyFileSync(path.join(root, 'apps/desktop/src/assets', file), path.join(assets, file));
  const read = (file) => JSON.parse(fs.readFileSync(path.join(root, file), 'utf8'));
  const data = landingBenchmarks(read('models/recommendations.json'), read('models/catalog.json'));
  if (!data.results.length) throw new Error('No passing benchmark results to publish');
  fs.writeFileSync(path.join(out, 'benchmarks.json'), `${JSON.stringify(data, null, 2)}\n`);
  return data;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const arg = process.argv.slice(2).find((a) => a.startsWith('--out='));
  if (!arg) throw new Error('Usage: node scripts/build-site.mjs --out=DIR');
  const out = path.resolve(arg.slice('--out='.length));
  const data = buildSite(out);
  console.log(
    `Site: ${out} (${data.results.length} benchmark results in ${Object.keys(data.languages).length} languages)`,
  );
}
