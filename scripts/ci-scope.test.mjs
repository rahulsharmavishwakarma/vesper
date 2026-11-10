import assert from 'node:assert/strict';
import test from 'node:test';
import { ciScope } from './ci-scope.mjs';

const all = { rust: true, browser: true, python: true, glib: true };

test('documentation-only pull requests skip the heavy jobs', () => {
  assert.deepEqual(ciScope(['docs/user-guide.md', 'README.md', 'notebooks/a.ipynb']), {
    rust: false,
    browser: false,
    python: false,
    glib: false,
  });
});

test('frontend changes run the browser suite only', () => {
  assert.deepEqual(ciScope(['apps/desktop/src/lib/updates.ts', 'apps/desktop/tests/a.spec.mjs']), {
    rust: false,
    browser: true,
    python: false,
    glib: false,
  });
});

test('native changes run Rust only', () => {
  assert.deepEqual(
    ciScope(['crates/vesper-core/src/lib.rs', 'native/mlx/Package.resolved', 'Cargo.lock']),
    { rust: true, browser: false, python: false, glib: true },
  );
});

test('files Rust embeds keep the Rust job', () => {
  for (const file of ['models/catalog.json', 'benchmarks/cleanup/native-learning-safety.json'])
    assert.equal(ciScope([file]).rust, true, file);
});

test('Python benchmark projects run only the benchmark audits', () => {
  assert.deepEqual(ciScope(['benchmarks/asr/python/audio-frontend/uv.lock']), {
    rust: false,
    browser: false,
    python: true,
    glib: false,
  });
  for (const file of [
    'uv.lock',
    'pyproject.toml',
    'benchmarks/asr/audio-frontend-requirements.txt',
  ])
    assert.equal(ciScope([file]).python, true, file);
});

test('shared or unknown paths run Rust and the browser suite', () => {
  for (const file of ['package-lock.json', 'scripts/desktop.mjs', 'something/new.txt'])
    assert.deepEqual(
      ciScope([file]),
      { rust: true, browser: true, python: false, glib: false },
      file,
    );
});

test('changes to the check workflows or the scope script run everything', () => {
  for (const file of [
    '.github/workflows/check.yml',
    '.github/workflows/security.yml',
    'scripts/ci-scope.mjs',
  ])
    assert.deepEqual(ciScope(['docs/a.md', file]), all, file);
});

test('files only other workflows or GitHub read skip Rust and the browser suite', () => {
  for (const file of [
    'Dockerfile',
    '.dockerignore',
    '.github/container/nginx.conf',
    '.github/workflows/pages.yml',
    '.github/workflows/nightly.yml',
    '.github/dependabot.yml',
    '.github/CODEOWNERS',
    'scripts/build-site.mjs',
    'scripts/build-site.test.mjs',
  ])
    assert.deepEqual(
      ciScope([file]),
      { rust: false, browser: false, python: false, glib: false },
      file,
    );
});

test('root Python tooling skips the browser suite but keeps Rust', () => {
  for (const file of [
    'pyproject.toml',
    'uv.lock',
    'benchmarks/asr/audio-frontend-requirements.txt',
  ])
    assert.deepEqual(
      ciScope([file]),
      { rust: true, browser: false, python: true, glib: false },
      file,
    );
});

const manifest = (devDependencies, extra = {}) =>
  JSON.stringify({ name: 'vesper-workspace', version: '0.1.2', devDependencies, ...extra });
const lock = (packages) =>
  JSON.stringify({ name: 'vesper-workspace', lockfileVersion: 3, packages });
const reader = (files) => (revision, path) => files[revision][path] ?? null;

test('npm changes run Rust only when a Tauri package or a non-dependency field changes', () => {
  const base = {
    'package.json': manifest({ '@tauri-apps/cli': '2.12.4', vite: '8.3.4' }),
    'package-lock.json': lock({
      '': { devDependencies: { '@tauri-apps/cli': '2.12.4', vite: '8.3.4' } },
      'node_modules/@tauri-apps/cli': { version: '2.12.4' },
      'node_modules/vite': { version: '8.3.4' },
      'node_modules/vite/node_modules/esbuild': { version: '1.0.0' },
    }),
  };
  const scope = (head) =>
    ciScope(Object.keys(head), { readFile: reader({ base, head: { ...base, ...head } }) });

  const devOnly = {
    'package.json': manifest({ '@tauri-apps/cli': '2.12.4', vite: '8.3.5' }),
    'package-lock.json': lock({
      '': { devDependencies: { '@tauri-apps/cli': '2.12.4', vite: '8.3.5' } },
      'node_modules/@tauri-apps/cli': { version: '2.12.4' },
      'node_modules/vite': { version: '8.3.5' },
      'node_modules/vite/node_modules/esbuild': { version: '1.0.1' },
    }),
  };
  assert.deepEqual(scope(devOnly), { rust: false, browser: true, python: false, glib: false });

  const tauri = {
    'package-lock.json': lock({
      '': { devDependencies: { '@tauri-apps/cli': '2.12.4', vite: '8.3.4' } },
      'node_modules/@tauri-apps/cli': { version: '2.12.5' },
      'node_modules/vite': { version: '8.3.4' },
      'node_modules/vite/node_modules/esbuild': { version: '1.0.0' },
    }),
  };
  assert.equal(scope(tauri).rust, true, 'Tauri lockfile entry');
  assert.equal(
    scope({ 'package.json': manifest({ '@tauri-apps/cli': '2.13.0', vite: '8.3.4' }) }).rust,
    true,
    'Tauri manifest entry',
  );
  assert.equal(
    scope({
      'package.json': manifest(
        { '@tauri-apps/cli': '2.12.4', vite: '8.3.4' },
        { scripts: { 'desktop:build': 'node scripts/desktop.mjs build' } },
      ),
    }).rust,
    true,
    'npm scripts',
  );
});

test('npm changes keep Rust when a revision is missing, unparsable or unreadable', () => {
  const files = ['apps/desktop/package.json'];
  const valid = manifest({ vite: '8.3.4' });
  assert.equal(
    ciScope(files, { readFile: reader({ base: {}, head: { [files[0]]: valid } }) }).rust,
    true,
  );
  assert.equal(
    ciScope(files, { readFile: reader({ base: { [files[0]]: '{' }, head: { [files[0]]: valid } }) })
      .rust,
    true,
  );
  assert.equal(ciScope(files).rust, true, 'no reader');
});

test('non-pull-request events and empty file lists run everything', () => {
  assert.deepEqual(ciScope(['docs/a.md'], { pullRequest: false }), all);
  assert.deepEqual(ciScope(['', ' ']), all);
});

test('only GLib, Cargo and audit-script changes run the GLib backport test', () => {
  for (const file of [
    'vendor/glib/src/value.rs',
    'tests/glib-backport/src/lib.rs',
    'scripts/audit-rust.py',
  ])
    assert.equal(ciScope([file]).glib, true, file);
  for (const file of ['crates/vesper-core/src/lib.rs', 'apps/desktop/src/App.svelte', 'docs/a.md'])
    assert.equal(ciScope([file]).glib, false, file);
});
