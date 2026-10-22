#!/usr/bin/env node
// Decides which heavy CI jobs a pull request needs from its changed paths
// (Refs #298). Required checks must still report, so callers skip a job with a
// job-level `if:`, which GitHub reports as success, never with workflow `paths:`.
// Anything not listed as irrelevant runs the job, so unknown paths stay covered.
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

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

// Rust embeds files from models/ and benchmarks/, so only the browser-only
// frontend and its tests are excluded beyond the inert set.
const rustIrrelevant = [
  ...inert,
  ...python,
  /^apps\/desktop\/src\//,
  /^apps\/desktop\/tests\//,
  /^playwright\.config\.mjs$/,
  /^eslint\.config\.mjs$/,
];

// The browser suite runs the Vite frontend against a mocked native layer.
const browserIrrelevant = [
  ...inert,
  ...python,
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

// Changing how scope is decided or how the jobs run re-runs everything.
const everything = [/^\.github\/workflows\//, /^scripts\/ci-scope(\.test)?\.mjs$/];

const needs = (files, irrelevant) =>
  files.some((file) => !irrelevant.some((pattern) => pattern.test(file)));

export function ciScope(files, { pullRequest = true } = {}) {
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
    rust: needs(changed, rustIrrelevant),
    browser: needs(changed, browserIrrelevant),
    python: changed.some((file) => pythonAuditInputs.some((pattern) => pattern.test(file))),
    glib: changed.some((file) => glibInputs.some((pattern) => pattern.test(file))),
  };
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  const pullRequest = process.env.GITHUB_EVENT_NAME === 'pull_request';
  const files = pullRequest ? readFileSync(0, 'utf8').split('\n') : [];
  for (const [job, run] of Object.entries(ciScope(files, { pullRequest })))
    console.log(`${job}=${run}`);
}
