import assert from 'node:assert/strict';
import test from 'node:test';
import { ciScope } from './ci-scope.mjs';

const all = { rust: true, browser: true, python: true };

test('documentation-only pull requests skip the heavy jobs', () => {
  assert.deepEqual(ciScope(['docs/user-guide.md', 'README.md', 'notebooks/a.ipynb']), {
    rust: false,
    browser: false,
    python: false,
  });
});

test('frontend changes run the browser suite only', () => {
  assert.deepEqual(ciScope(['apps/desktop/src/lib/updates.ts', 'apps/desktop/tests/a.spec.mjs']), {
    rust: false,
    browser: true,
    python: false,
  });
});

test('native changes run Rust only', () => {
  assert.deepEqual(
    ciScope(['crates/vesper-core/src/lib.rs', 'native/mlx/Package.resolved', 'Cargo.lock']),
    { rust: true, browser: false, python: false },
  );
});

test('files Rust embeds keep the Rust job', () => {
  for (const file of ['models/catalog.json', 'benchmarks/cleanup/native-learning-safety.json'])
    assert.equal(ciScope([file]).rust, true, file);
});

test('Python benchmark projects run only the benchmark audits', () => {
  assert.deepEqual(ciScope(['benchmarks/asr/python/vibevoice/uv.lock']), {
    rust: false,
    browser: false,
    python: true,
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
    assert.deepEqual(ciScope([file]), { rust: true, browser: true, python: false }, file);
});

test('workflow or scope changes run everything', () => {
  for (const file of ['.github/workflows/check.yml', 'scripts/ci-scope.mjs'])
    assert.deepEqual(ciScope(['docs/a.md', file]), all, file);
});

test('non-pull-request events and empty file lists run everything', () => {
  assert.deepEqual(ciScope(['docs/a.md'], { pullRequest: false }), all);
  assert.deepEqual(ciScope(['', ' ']), all);
});
