#!/usr/bin/env node
// Decides which heavy CI jobs a pull request needs from its changed paths
// (Refs #298). Required checks must still report, so callers skip a job with a
// job-level `if:`, which GitHub reports as success, never with workflow `paths:`.
// Anything not listed as irrelevant runs the job, so unknown paths stay covered.
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { isDeepStrictEqual } from 'node:util';

// Nothing compiled, tested or bundled reads these.
const inert = [
  /^docs\//,
  /\.md$/,
  /^notebooks\//,
  /\.ipynb$/,
  /^\.github\/ISSUE_TEMPLATE\//,
  /^LICENSE$/,
  /^site\//,
];

const python = [/^benchmarks\/asr\/python\//];

// Read only by other workflows (container image, Pages, nightly, release) or by
// GitHub itself. The always-on hygiene job still lints the workflow files, and
// check.yml/security.yml changes are caught by `everything` before these apply.
const elsewhere = [
  /^Dockerfile$/,
  /^\.dockerignore$/,
  /^\.github\/container\//,
  /^\.github\/workflows\//,
  /^\.github\/(CODEOWNERS|dependabot\.yml|release\.yml)$/,
  /^scripts\/build-site(\.test)?\.mjs$/,
];

// The root Python environment serves contributor tools and the bundle's speaker
// packaging step, never the browser suite.
const rootPython = [/^pyproject\.toml$/, /^uv\.lock$/, /^benchmarks\/asr\/[^/]*requirements\.txt$/];

// npm manifests reach the Rust job only through its bundle step, which runs the
// Tauri CLI and rejects a Tauri npm package whose version does not match its
// crate. Other dependency bumps are covered by the frontend and browser jobs.
const npmManifests = [/^(apps\/desktop\/)?package\.json$/, /^package-lock\.json$/];

// Rust embeds files from models/ and benchmarks/, so only the browser-only
// frontend and its tests are excluded beyond the inert set.
const rustIrrelevant = [
  ...inert,
  ...python,
  ...elsewhere,
  /^apps\/desktop\/src\//,
  /^apps\/desktop\/tests\//,
  /^playwright\.config\.mjs$/,
  /^eslint\.config\.mjs$/,
];

// The browser suite runs the Vite frontend against a mocked native layer.
const browserIrrelevant = [
  ...inert,
  ...python,
  ...elsewhere,
  ...rootPython,
  /^crates\//,
  /^native\//,
  /^patches\//,
  /^vendor\//,
  /^apps\/desktop\/src-tauri\//,
  /^Cargo\.(toml|lock)$/,
  /^rust-toolchain\.toml$/,
  /^deny\.toml$/,
  /^tests\/glib-backport\//,
  /^benchmarks\/asr\/hash-probe\//,
];

// The benchmark audits read only their own locks plus the root audit tooling.
// Pushes and the weekly schedule still audit them for newly published advisories.
const pythonAuditInputs = [
  ...python,
  /^benchmarks\/asr\/[^/]*requirements\.txt$/,
  /^pyproject\.toml$/,
  /^uv\.lock$/,
  /^Makefile$/,
];

// The GLib backport regression builds tests/glib-backport against system GLib
// with the patched vendored crate; installing GLib's development files from the
// Ubuntu mirror is the slow, unreliable part (#298).
const glibInputs = [
  /^vendor\/glib\//,
  /^tests\/glib-backport\//,
  /^Cargo\.(toml|lock)$/,
  /^rust-toolchain\.toml$/,
  /^scripts\/(audit-rust|test_rust_audit)\.py$/,
  /^Makefile$/,
];

// Changing how scope is decided or how these jobs run re-runs everything.
const everything = [
  /^\.github\/workflows\/(check|security)\.yml$/,
  /^scripts\/ci-scope(\.test)?\.mjs$/,
];

const needs = (files, irrelevant) =>
  files.some((file) => !irrelevant.some((pattern) => pattern.test(file)));

const dependencyFields = [
  'dependencies',
  'devDependencies',
  'optionalDependencies',
  'peerDependencies',
];
const isTauri = (name) => name.startsWith('@tauri-apps/');

// A manifest with every non-Tauri dependency removed; scripts, version and
// workspace settings stay, so changing them still runs the Rust job.
function tauriManifest(manifest) {
  const kept = { ...manifest };
  for (const field of dependencyFields)
    if (kept[field])
      kept[field] = Object.fromEntries(
        Object.entries(kept[field]).filter(([name]) => isTauri(name)),
      );
  return kept;
}

function rustView(path, text) {
  const json = JSON.parse(text);
  if (path !== 'package-lock.json') return tauriManifest(json);
  const packages = Object.entries(json.packages ?? {}).flatMap(([key, entry]) => {
    const at = key.lastIndexOf('node_modules/');
    if (at === -1) return [[key, tauriManifest(entry)]];
    return isTauri(key.slice(at + 'node_modules/'.length)) ? [[key, entry]] : [];
  });
  return { ...json, packages: Object.fromEntries(packages) };
}

// True when the change touches only non-Tauri dependencies. A missing file or a
// parse failure keeps the Rust job.
function npmChangeSkipsRust(file, readFile) {
  try {
    const before = readFile('base', file);
    const after = readFile('head', file);
    if (before == null || after == null) return false;
    return isDeepStrictEqual(rustView(file, before), rustView(file, after));
  } catch {
    return false;
  }
}

function needsRust(files, readFile) {
  return files
    .filter((file) => !rustIrrelevant.some((pattern) => pattern.test(file)))
    .some(
      (file) =>
        !(
          readFile &&
          npmManifests.some((pattern) => pattern.test(file)) &&
          npmChangeSkipsRust(file, readFile)
        ),
    );
}

// `readFile(revision, path)` returns a file at 'base' or 'head', or null when it
// does not exist there. Without it, npm manifest changes always run Rust.
export function ciScope(files, { pullRequest = true, readFile } = {}) {
  const changed = files.map((file) => file.trim()).filter(Boolean);
  // Pushes, tags, merge queues, schedules and manual runs always check everything,
  // as does a pull request whose changed paths could not be listed.
  if (
    !pullRequest ||
    !changed.length ||
    changed.some((file) => everything.some((pattern) => pattern.test(file)))
  )
    return { rust: true, browser: true, python: true, glib: true };
  return {
    rust: needsRust(changed, readFile),
    browser: needs(changed, browserIrrelevant),
    python: changed.some((file) => pythonAuditInputs.some((pattern) => pattern.test(file))),
    glib: changed.some((file) => glibInputs.some((pattern) => pattern.test(file))),
  };
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  const pullRequest = process.env.GITHUB_EVENT_NAME === 'pull_request';
  const files = pullRequest ? readFileSync(0, 'utf8').split('\n') : [];
  // The workflows diff the pull request's merge commit against its first parent.
  const revisions = { base: 'HEAD^1', head: 'HEAD' };
  const readFile = (revision, path) => {
    try {
      return execFileSync('git', ['show', `${revisions[revision]}:${path}`], {
        encoding: 'utf8',
        maxBuffer: 64 * 1024 * 1024,
        stdio: ['ignore', 'pipe', 'ignore'],
      });
    } catch {
      return null;
    }
  };
  for (const [job, run] of Object.entries(ciScope(files, { pullRequest, readFile })))
    console.log(`${job}=${run}`);
}
