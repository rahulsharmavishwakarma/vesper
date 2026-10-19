#!/usr/bin/env node
// Refuses private signing-key material anywhere in the repository.
//
// The enrolled updater PUBLIC key (minisign public format, tauri.conf.json)
// is safe to ship and is explicitly exempt. These rules target the private
// counterparts only: minisign secret keys, PEM/OpenSSH private keys, and the
// key/password filenames our tooling uses (Refs #65). Generic secret shapes
// stay covered by gitleaks, trivy, and the detect-private-key hook; this
// guard closes the formats those miss.
//
// Usage: node scripts/check-no-private-keys.mjs [files...]
// With no arguments every tracked file is scanned (CI). The pre-commit hook
// passes the changed files instead.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.join(HERE, '..');
// This scanner and its test quote the forbidden patterns as literals, so
// they must never scan themselves.
const SELF_PATHS = new Set([
  path.join(HERE, 'check-no-private-keys.mjs'),
  path.join(HERE, 'no-private-keys.test.mjs'),
]);

// Private-key content shapes. The public counterpart reads
// "minisign public key" and never matches these.
const SECRET_MARKERS = [
  'minisign encrypted secret key',
  'BEGIN PRIVATE KEY',
  'BEGIN RSA PRIVATE KEY',
  'BEGIN EC PRIVATE KEY',
  'BEGIN OPENSSH PRIVATE KEY',
];

const FORBIDDEN_BASENAMES = [
  // Private key files of any kind. Published public halves (*.key.pub) never
  // match this pattern since they end in .pub.
  { test: (name) => /\.key$/i.test(name), reason: 'private key filename' },
  // Password sidecars such as the updater key password backup name.
  { test: (name) => /\.password$/i.test(name), reason: 'password filename' },
  // Conventional SSH private key names; the .pub halves stay allowed.
  {
    test: (name) => /^(id_rsa|id_dsa|id_ecdsa|id_ed25519)$/.test(name),
    reason: 'SSH private key filename',
  },
];

// Returns a violation reason, or null when the entry is allowed.
export function classifyEntry(relativePath, content) {
  const base = path.basename(relativePath);
  for (const { test, reason } of FORBIDDEN_BASENAMES) {
    if (test(base)) return `${reason}: ${relativePath}`;
  }
  if (content === null) return null;
  for (const marker of SECRET_MARKERS) {
    if (content.includes(marker)) return `private key material (${marker}): ${relativePath}`;
  }
  return null;
}

export function findViolations(entries) {
  return entries.flatMap(({ file, content }) => {
    const absolute = path.resolve(file);
    if (SELF_PATHS.has(absolute)) return [];
    const reason = classifyEntry(path.relative(process.cwd(), absolute), content);
    return reason === null ? [] : [reason];
  });
}

function readTree(files) {
  return files.flatMap((file) => {
    let buffer;
    try {
      buffer = fs.readFileSync(file);
    } catch {
      return [];
    }
    // Binary files carry no scannable text; their names are still checked.
    const content = buffer.includes(0) ? null : buffer.toString('utf8');
    return [{ file, content }];
  });
}

function trackedFiles() {
  const out = execFileSync('git', ['ls-files', '-z'], { cwd: ROOT, encoding: 'utf8' });
  return out
    .split('\0')
    .filter(Boolean)
    .map((file) => path.join(ROOT, file));
}

function main() {
  const files = process.argv.length > 2 ? process.argv.slice(2) : trackedFiles();
  const violations = findViolations(readTree(files));
  for (const violation of violations) {
    console.error(`no-private-keys: ${violation}`);
  }
  if (violations.length > 0) {
    console.error(
      'no-private-keys: private signing-key material must never be committed (Refs #65).',
    );
    process.exitCode = 1;
  }
}

main();
