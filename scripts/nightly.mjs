#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { collectVersions, setVersion } from './version.mjs';

// A nightly installs beside stable: Tauri derives the app data and config
// directories from the identifier, and the legacy-store fallback in
// storage.rs only applies to com.vesper.desktop, so a distinct identifier
// gives nightly its own settings and models.
export const NIGHTLY_PRODUCT_NAME = 'Vesper Nightly';
export const NIGHTLY_IDENTIFIER = 'com.vesper.desktop.nightly';
// In-app nightly update feed, published by pages.yml from the newest nightly
// release (issue #262). It lives on GitHub Pages because immutable releases give
// every nightly its own tag, so no fixed release URL can name "the newest".
export const NIGHTLY_FEED = 'https://rahulsharmavishwakarma.github.io/vesper/nightly.json';
// Public half of the nightly-only updater key pair. The private key and its
// password are secrets of the main-only `nightly` environment; stable builds
// keep the stable key, so they never trust a nightly signature and nightly
// builds never trust a stable one. Rotate by replacing this value and both
// secrets together.
export const NIGHTLY_PUBKEY =
  'dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDc5QTlFMEE4RDE1Qjg4NkYKUldSdmlGdlJxT0NwZVl2ckZvT3cvdEVQNDY1RndUamFCVXdvWU1yajFhUnU2NjJwODZCazVTcncK';

const SEMVER_BASE = /^(\d+)\.(\d+)\.(\d+)(-.+)?$/;

// X.Y.Z-nightly.YYYYMMDD, ordered after the previous release and before the
// release it leads to: from a stable X.Y.Z it targets the next patch; from a
// candidate X.Y.Z-rc.N it targets X.Y.Z.
export function nightlyVersion(current, date) {
  if (!/^[12]\d{7}$/.test(date)) throw new Error('Nightly date must be YYYYMMDD.');
  const match = SEMVER_BASE.exec(current);
  if (!match) throw new Error(`Cannot derive a nightly from version ${current}.`);
  const [, major, minor, patch, prerelease] = match;
  const base = prerelease ? `${major}.${minor}.${patch}` : `${major}.${minor}.${Number(patch) + 1}`;
  return `${base}-nightly.${date}`;
}

// Rewrites the checked-out workspace for one nightly build. These edits are
// never committed: the version files on main are never touched by a nightly.
export function applyNightly(root, version) {
  if (!/^\d+\.\d+\.\d+-nightly\.[12]\d{7}$/.test(version)) {
    throw new Error(`${version} is not a nightly version.`);
  }
  setVersion(root, version);
  const file = path.join(root, 'apps/desktop/src-tauri/tauri.conf.json');
  const config = JSON.parse(fs.readFileSync(file, 'utf8'));
  config.productName = NIGHTLY_PRODUCT_NAME;
  config.identifier = NIGHTLY_IDENTIFIER;
  config.plugins.updater.endpoints = [NIGHTLY_FEED];
  config.plugins.updater.pubkey = NIGHTLY_PUBKEY;
  fs.writeFileSync(file, `${JSON.stringify(config, null, 2)}\n`);
}

function usage() {
  return 'Usage: node scripts/nightly.mjs version YYYYMMDD | apply X.Y.Z-nightly.YYYYMMDD';
}

function main() {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
  const [command, argument, extra] = process.argv.slice(2);
  if (!argument || extra) throw new Error(usage());
  if (command === 'version') {
    process.stdout.write(`${nightlyVersion(collectVersions(root)[0][1], argument)}\n`);
  } else if (command === 'apply') {
    applyNightly(root, argument);
    process.stdout.write(`Prepared nightly ${argument}.\n`);
  } else {
    throw new Error(usage());
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  }
}
