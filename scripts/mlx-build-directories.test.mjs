import assert from 'node:assert/strict';
import {
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  readlinkSync,
  renameSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { prepareMlxBuildDirectories } from './mlx-build-directories.mjs';

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'mlx-directories-test-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, '.tooling'));
  return root;
}

test('creates missing directories and preserves existing generated files on repeated preparation', (t) => {
  const root = fixture(t);
  const paths = prepareMlxBuildDirectories(root);
  for (const path of Object.values(paths)) {
    assert.ok(lstatSync(path).isDirectory());
    writeFileSync(join(path, 'keep'), 'existing data');
  }
  assert.deepEqual(prepareMlxBuildDirectories(root), paths);
  for (const path of Object.values(paths))
    assert.equal(readFileSync(join(path, 'keep'), 'utf8'), 'existing data');
});

test('replaces dangling links with local directories and preserves the links without recreating targets', (t) => {
  const root = fixture(t);
  const names = ['mlx-build', 'swift-cache', 'mlx-runtime'];
  const target = join(root, 'removed-checkout');
  for (const name of names) symlinkSync(join(target, name), join(root, '.tooling', name), 'dir');
  const warnings = [];
  const paths = prepareMlxBuildDirectories(root, (message) => warnings.push(message));
  assert.equal(warnings.length, 1);
  const backups = readdirSync(join(root, '.tooling')).filter((name) =>
    name.startsWith('mlx-directory-recovery-'),
  );
  assert.equal(backups.length, 1);
  for (const name of names) {
    assert.ok(lstatSync(join(root, '.tooling', name)).isDirectory());
    assert.equal(readlinkSync(join(root, '.tooling', backups[0], name)), join(target, name));
  }
  assert.equal(existsSync(target), false);
  assert.deepEqual(
    prepareMlxBuildDirectories(root, (message) => warnings.push(message)),
    paths,
  );
  assert.equal(warnings.length, 1);
});

test('preserves valid directory symlinks and their contents', (t) => {
  const root = fixture(t);
  const target = join(root, 'shared-cache');
  mkdirSync(target);
  writeFileSync(join(target, 'keep'), 'cached package');
  const cache = join(root, '.tooling/swift-cache');
  symlinkSync(target, cache, 'dir');
  prepareMlxBuildDirectories(root);
  assert.ok(lstatSync(cache).isSymbolicLink());
  assert.equal(readlinkSync(cache), target);
  assert.equal(readFileSync(join(cache, 'keep'), 'utf8'), 'cached package');
});

test('refuses files and symlinks to files without overwriting them', (t) => {
  const root = fixture(t);
  const cache = join(root, '.tooling/swift-cache');
  writeFileSync(cache, 'not a directory');
  assert.throws(() => prepareMlxBuildDirectories(root), /Expected a directory/);
  assert.equal(readFileSync(cache, 'utf8'), 'not a directory');
  const target = join(root, 'not-a-directory');
  renameSync(cache, target);
  symlinkSync(target, cache, 'file');
  assert.throws(() => prepareMlxBuildDirectories(root), /Expected a directory/);
  assert.ok(lstatSync(cache).isSymbolicLink());
  assert.equal(readFileSync(target, 'utf8'), 'not a directory');
});

test('does not treat other filesystem errors as missing cache directories', (t) => {
  const root = fixture(t);
  const cache = join(root, '.tooling/swift-cache');
  symlinkSync(cache, cache, 'dir');
  assert.throws(() => prepareMlxBuildDirectories(root), { code: 'ELOOP' });
  assert.equal(readlinkSync(cache), cache);
  assert.deepEqual(readdirSync(join(root, '.tooling')), ['swift-cache']);
});

test('resolves a linked .tooling folder to its canonical path', (t) => {
  const shared = fixture(t);
  const worktree = mkdtempSync(join(tmpdir(), 'mlx-directories-worktree-'));
  t.after(() => rmSync(worktree, { recursive: true, force: true }));
  symlinkSync(join(shared, '.tooling'), join(worktree, '.tooling'), 'dir');
  assert.deepEqual(prepareMlxBuildDirectories(worktree), prepareMlxBuildDirectories(shared));
});
