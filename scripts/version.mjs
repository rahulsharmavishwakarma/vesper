#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;
const WORKSPACE_PACKAGES = [
  'vesper-audio',
  'vesper-core',
  'vesper-data',
  'vesper-desktop',
  'vesper-inference',
  'vesper-models',
  'vesper-nemo',
];

function readJson(root, relative) {
  return JSON.parse(fs.readFileSync(path.join(root, relative), 'utf8'));
}

function writeJson(root, relative, value) {
  fs.writeFileSync(path.join(root, relative), `${JSON.stringify(value, null, 2)}\n`);
}

function requireSemver(version, label = 'version') {
  if (!SEMVER.test(version))
    throw new Error(`${label} must be SemVer in X.Y.Z or X.Y.Z-prerelease form.`);
  return version;
}

function cargoWorkspaceVersion(text) {
  const section = text.match(/\[workspace\.package\]([\s\S]*?)(?=\n\[|$)/);
  const version = section?.[1].match(/^version = "([^"]+)"$/m)?.[1];
  if (!version) throw new Error('Cargo.toml is missing [workspace.package].version.');
  return version;
}

function cargoLockVersions(text) {
  return WORKSPACE_PACKAGES.map((name) => {
    const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const match = text.match(
      new RegExp(`\\[\\[package\\]\\]\\nname = "${escaped}"\\nversion = "([^"]+)"`),
    );
    if (!match) throw new Error(`Cargo.lock is missing workspace package ${name}.`);
    return [`Cargo.lock (${name})`, match[1]];
  });
}

export function collectVersions(root) {
  const rootPackage = readJson(root, 'package.json');
  const desktopPackage = readJson(root, 'apps/desktop/package.json');
  const tauri = readJson(root, 'apps/desktop/src-tauri/tauri.conf.json');
  const packageLock = readJson(root, 'package-lock.json');
  const cargo = fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8');
  const cargoLock = fs.readFileSync(path.join(root, 'Cargo.lock'), 'utf8');
  return [
    ['Cargo.toml workspace', cargoWorkspaceVersion(cargo)],
    ['package.json', rootPackage.version],
    ['apps/desktop/package.json', desktopPackage.version],
    ['apps/desktop/src-tauri/tauri.conf.json', tauri.version],
    ['package-lock.json root', packageLock.version],
    ['package-lock.json packages[""]', packageLock.packages?.['']?.version],
    ['package-lock.json packages["apps/desktop"]', packageLock.packages?.['apps/desktop']?.version],
    ...cargoLockVersions(cargoLock),
  ];
}

export function checkVersions(root, expectedTag) {
  const versions = collectVersions(root);
  const canonical = requireSemver(versions[0][1], versions[0][0]);
  const mismatches = versions.filter(([, version]) => version !== canonical);
  if (mismatches.length) {
    throw new Error(
      `Version ${canonical} does not match:\n${mismatches.map(([file, version]) => `- ${file}: ${version ?? 'missing'}`).join('\n')}`,
    );
  }
  if (expectedTag !== undefined) {
    const tagVersion = expectedTag.startsWith('v') ? expectedTag.slice(1) : expectedTag;
    requireSemver(tagVersion, 'release tag');
    if (tagVersion !== canonical)
      throw new Error(
        `Release tag ${expectedTag} does not match application version ${canonical}.`,
      );
  }
  return canonical;
}

function replaceCargoVersion(text, version) {
  const section = /\[workspace\.package\]([\s\S]*?)(?=\n\[|$)/;
  if (!section.test(text)) throw new Error('Cargo.toml is missing [workspace.package].');
  return text.replace(section, (value) =>
    value.replace(/^version = "[^"]+"$/m, `version = "${version}"`),
  );
}

function replaceCargoLockVersions(text, version) {
  for (const name of WORKSPACE_PACKAGES) {
    const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const expression = new RegExp(
      `(\\[\\[package\\]\\]\\nname = "${escaped}"\\nversion = ")[^"]+(")`,
    );
    if (!expression.test(text)) throw new Error(`Cargo.lock is missing workspace package ${name}.`);
    text = text.replace(expression, `$1${version}$2`);
  }
  return text;
}

export function setVersion(root, version) {
  requireSemver(version);
  const rootPackage = readJson(root, 'package.json');
  const desktopPackage = readJson(root, 'apps/desktop/package.json');
  const tauri = readJson(root, 'apps/desktop/src-tauri/tauri.conf.json');
  const packageLock = readJson(root, 'package-lock.json');
  rootPackage.version = version;
  desktopPackage.version = version;
  tauri.version = version;
  packageLock.version = version;
  packageLock.packages[''].version = version;
  packageLock.packages['apps/desktop'].version = version;
  writeJson(root, 'package.json', rootPackage);
  writeJson(root, 'apps/desktop/package.json', desktopPackage);
  writeJson(root, 'apps/desktop/src-tauri/tauri.conf.json', tauri);
  writeJson(root, 'package-lock.json', packageLock);
  const cargoPath = path.join(root, 'Cargo.toml');
  fs.writeFileSync(cargoPath, replaceCargoVersion(fs.readFileSync(cargoPath, 'utf8'), version));
  const lockPath = path.join(root, 'Cargo.lock');
  fs.writeFileSync(lockPath, replaceCargoLockVersions(fs.readFileSync(lockPath, 'utf8'), version));
  checkVersions(root);
}

function usage() {
  return 'Usage: node scripts/version.mjs check [--tag vX.Y.Z] | show | set X.Y.Z';
}

function main() {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
  const [command = 'check', argument, extra] = process.argv.slice(2);
  if (extra) throw new Error(usage());
  if (command === 'show' && argument === undefined) {
    process.stdout.write(`${checkVersions(root)}\n`);
  } else if (command === 'check' && (argument === undefined || argument === '--tag')) {
    if (argument === '--tag') throw new Error('Missing release tag.');
    process.stdout.write(`Versions match: ${checkVersions(root)}\n`);
  } else if (command === 'check' && argument?.startsWith('--tag=')) {
    process.stdout.write(`Versions and tag match: ${checkVersions(root, argument.slice(6))}\n`);
  } else if (command === 'set' && argument) {
    setVersion(root, argument);
    process.stdout.write(`Set application version to ${argument}.\n`);
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
