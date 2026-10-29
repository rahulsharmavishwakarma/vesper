#!/usr/bin/env node
// Prints a version's CHANGELOG.md section: the notes in the update feed, which
// the app shows as "What's new", and the top of the GitHub release body. A
// version without its own section, such as a release candidate, uses
// [Unreleased]. Empty notes fail so a release never ships without them.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export function releaseNotes(changelog, version) {
  const exact = version.replace(/^v/, '');
  const sections = new Map();
  let lines = null;
  for (const line of changelog.split(/\r?\n/)) {
    const heading = /^## \[([^\]]+)\]/.exec(line);
    if (heading) {
      lines = [];
      sections.set(heading[1], lines);
    } else if (lines && !/^\[[^\]]+\]:\s/.test(line)) {
      lines.push(line);
    }
  }
  const section =
    sections.get(exact) ?? sections.get(exact.split('-')[0]) ?? sections.get('Unreleased');
  const notes = (section ?? []).join('\n').trim();
  if (!notes) throw new Error(`CHANGELOG.md has no notes for ${version} or [Unreleased].`);
  return notes;
}

export function releaseNotesFromFile(version, file = 'CHANGELOG.md') {
  return releaseNotes(fs.readFileSync(file, 'utf8'), version);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const version = process.argv[2];
  if (!version) {
    console.error('Usage: node scripts/release-notes.mjs <version>');
    process.exit(2);
  }
  process.stdout.write(`${releaseNotesFromFile(version)}\n`);
}
