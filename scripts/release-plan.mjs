#!/usr/bin/env node
import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-rc\.(0|[1-9]\d*))?$/;
export const BUMPS = ['patch', 'minor', 'major', 'prerelease', 'release'];

function parse(version) {
  const match = SEMVER.exec(version);
  if (!match) {
    throw new Error(`Version ${version} must be X.Y.Z or X.Y.Z-rc.N for automated releases.`);
  }
  const [major, minor, patch] = match.slice(1, 4).map(Number);
  return { major, minor, patch, rc: match[4] === undefined ? undefined : Number(match[4]) };
}

// Computes the next application version. `prerelease` starts X.Y.(Z+1)-rc.1
// from a stable version and increments the candidate number otherwise;
// `release` promotes a candidate to its stable version. patch/minor/major
// always produce a stable version, and refuse to run from a candidate so a
// pending rc is promoted or abandoned deliberately.
export function nextVersion(current, bump) {
  const { major, minor, patch, rc } = parse(current);
  if (!BUMPS.includes(bump)) throw new Error(`Bump must be one of ${BUMPS.join(', ')}.`);
  if (bump === 'release') {
    if (rc === undefined) throw new Error(`${current} is already a stable version.`);
    return `${major}.${minor}.${patch}`;
  }
  if (bump === 'prerelease') {
    return rc === undefined
      ? `${major}.${minor}.${patch + 1}-rc.1`
      : `${major}.${minor}.${patch}-rc.${rc + 1}`;
  }
  if (rc !== undefined) {
    throw new Error(`${current} is a release candidate; use "release" or "prerelease".`);
  }
  if (bump === 'major') return `${major + 1}.0.0`;
  if (bump === 'minor') return `${major}.${minor + 1}.0`;
  return `${major}.${minor}.${patch + 1}`;
}

// Dates the [Unreleased] section of CHANGELOG.md as a stable release: its
// entries move under "## [X.Y.Z] - date", an empty [Unreleased] stays on top,
// and the compare/tag links follow. The entries themselves are written by
// hand as changes land; an empty section fails so a release never ships
// without notes.
export function stampChangelog(text, version, date) {
  const { major, minor, patch, rc } = parse(version);
  if (rc !== undefined)
    throw new Error(`${version} is a candidate; only stable releases are dated.`);
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) throw new Error(`Date ${date} must be YYYY-MM-DD.`);
  if (text.includes(`## [${version}]`)) throw new Error(`CHANGELOG.md already has ${version}.`);
  const heading = '## [Unreleased]\n';
  const at = text.indexOf(heading);
  if (at < 0) throw new Error('CHANGELOG.md has no [Unreleased] section.');
  const bodyStart = at + heading.length;
  const next = text.slice(bodyStart).search(/^## \[|^\[Unreleased\]: /m);
  const bodyEnd = next < 0 ? text.length : bodyStart + next;
  const body = text.slice(bodyStart, bodyEnd).trim();
  if (!body) throw new Error('CHANGELOG.md [Unreleased] is empty; add notes before a release.');
  const link = /^\[Unreleased\]: (\S+)\/compare\/v\S+\.\.\.HEAD$/m.exec(text);
  if (!link) throw new Error('CHANGELOG.md has no [Unreleased] compare link.');
  const tag = `v${major}.${minor}.${patch}`;
  return (
    text.slice(0, at) +
    `${heading}\n## [${version}] - ${date}\n\n${body}\n\n` +
    text.slice(bodyEnd)
  ).replace(
    link[0],
    `[Unreleased]: ${link[1]}/compare/${tag}...HEAD\n[${version}]: ${link[1]}/releases/tag/${tag}`,
  );
}

function usage() {
  return 'Usage: node scripts/release-plan.mjs next <current> <bump> | stamp <version> <YYYY-MM-DD> [file]';
}

async function main() {
  const [command, first, second, extra] = process.argv.slice(2);
  if (command === 'next' && first && second && !extra) {
    process.stdout.write(`${nextVersion(first, second)}\n`);
  } else if (command === 'stamp' && first && second) {
    const file = extra ?? 'CHANGELOG.md';
    await writeFile(file, stampChangelog(await readFile(file, 'utf8'), first, second));
  } else {
    throw new Error(usage());
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
}
