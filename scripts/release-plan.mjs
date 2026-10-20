#!/usr/bin/env node
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

// Builds release notes from commit subjects since the previous tag. Version
// bump commits and merge noise are dropped; the rest keep their order.
export function changelog(subjects, version) {
  const entries = subjects
    .map((subject) => subject.trim())
    .filter((subject) => subject && !/^Set application version to /.test(subject))
    .filter((subject) => !/^Merge (branch|remote-tracking branch|pull request)/.test(subject));
  const body = entries.length ? entries.map((entry) => `- ${entry}`).join('\n') : '- No changes.';
  return `## Vesper ${version}\n\n${body}\n`;
}

function usage() {
  return 'Usage: node scripts/release-plan.mjs next <current> <bump> | notes <version> (subjects on stdin)';
}

async function readStdin() {
  let text = '';
  for await (const chunk of process.stdin) text += chunk;
  return text;
}

async function main() {
  const [command, first, second, extra] = process.argv.slice(2);
  if (command === 'next' && first && second && !extra) {
    process.stdout.write(`${nextVersion(first, second)}\n`);
  } else if (command === 'notes' && first && !second) {
    process.stdout.write(changelog((await readStdin()).split('\n'), first));
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
