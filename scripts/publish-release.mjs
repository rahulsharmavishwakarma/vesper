#!/usr/bin/env node
// Called only after release build/checks. Never overwrite a published asset.
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { releaseNotesFromFile } from './release-notes.mjs';

export const ASSET_NAMES = ['vesper-macos-arm64.dmg', 'SHA256SUMS'];

export function publishRelease({
  repository,
  tag,
  directory = 'release',
  run,
  publish = true,
  notes = '',
}) {
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository ?? '')) throw new Error('Invalid repository');
  if (
    !/^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/.test(
      tag ?? '',
    )
  ) {
    throw new Error('Release tag must be vX.Y.Z or vX.Y.Z-prerelease');
  }
  const files = ASSET_NAMES.map((name) => path.join(directory, name));
  for (const file of files) {
    if (!fs.statSync(file).isFile() || fs.statSync(file).size === 0)
      throw new Error(`Empty or missing asset: ${file}`);
  }
  const digest = (file) => createHash('sha256').update(fs.readFileSync(file)).digest('hex');
  const expectedChecksum = `${digest(files[0])}  ${ASSET_NAMES[0]}`;
  if (fs.readFileSync(files[1], 'utf8').trim() !== expectedChecksum)
    throw new Error('SHA256SUMS does not match the desktop DMG');

  const call = (args, allowFailure = false) => {
    const result = run(args);
    if (!allowFailure && result.status !== 0)
      throw new Error(`GitHub command failed: ${result.stderr || args.join(' ')}`);
    return result;
  };
  const endpoint = `repos/${repository}/releases/tags/${tag}`;
  const lookup = call(['api', endpoint], true);
  let release;
  if (lookup.status === 0) {
    release = JSON.parse(lookup.stdout);
  } else {
    // A denied request or temporary API failure must not be treated as absence.
    let error;
    try {
      error = JSON.parse(lookup.stdout);
    } catch {
      /* reported below */
    }
    if (Number(error?.status) !== 404) throw new Error('Could not read release state');
    release = { draft: true, assets: [] };
    const options = tag.includes('-') ? ['--prerelease', '--latest=false'] : [];
    call([
      'release',
      'create',
      tag,
      '--repo',
      repository,
      '--draft',
      '--verify-tag',
      // The changelog section leads; GitHub appends its pull-request list.
      ...(notes ? ['--notes', notes] : []),
      '--generate-notes',
      '--title',
      `Vesper ${tag}`,
      ...options,
    ]);
  }

  const existing = new Map((release.assets ?? []).map((asset) => [asset.name, asset]));
  if (!release.draft) {
    const missing = files.filter((file) => !existing.has(path.basename(file)));
    if (!missing.length) return 'already-published';
    // A retry may complete a partial upload only when existing bytes match.
    for (const file of files) {
      const asset = existing.get(path.basename(file));
      if (asset && asset.digest !== `sha256:${digest(file)}`) {
        throw new Error('Published asset differs from this build; use a new version tag');
      }
    }
    call(['release', 'upload', tag, ...missing, '--repo', repository]);
    return 'assets-attached';
  }

  call(['release', 'upload', tag, ...files, '--repo', repository, '--clobber']);
  // Draft-only mode stages the candidate without publishing it. Promotion to
  // published happens separately through the approval-gated promote job.
  if (!publish) return 'draft-updated';
  const prerelease = tag.includes('-') || release.prerelease === true;
  call([
    'release',
    'edit',
    tag,
    '--repo',
    repository,
    '--draft=false',
    `--prerelease=${prerelease}`,
    `--latest=${!prerelease}`,
  ]);
  return 'published';
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const result = publishRelease({
    repository: process.env.GITHUB_REPOSITORY,
    tag: process.env.GITHUB_REF_NAME,
    publish: process.env.RELEASE_DRAFT_ONLY !== '1',
    notes: releaseNotesFromFile(process.env.GITHUB_REF_NAME ?? ''),
    run: (args) => spawnSync('gh', args, { encoding: 'utf8' }),
  });
  console.log(`Desktop release: ${result}`);
}
