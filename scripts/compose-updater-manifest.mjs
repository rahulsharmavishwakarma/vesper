#!/usr/bin/env node
// Composes the Tauri v2 updater manifest (latest.json / beta.json) for one
// platform. Called only when the signed updater archive and its .sig exist;
// without a signing key there is no manifest to publish. Never invents a
// signature: the manifest embeds the literal .sig file content.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const PLATFORM = 'darwin-aarch64';

export function composeManifest({ version, notes, pubDate, signature, url }) {
  if (
    !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/.test(
      version ?? '',
    )
  )
    throw new Error('Manifest version must be X.Y.Z or X.Y.Z-prerelease');
  if (!signature || !signature.trim()) throw new Error('Manifest signature must not be empty');
  if (!/^https:\/\//.test(url ?? '')) throw new Error('Manifest URL must be an HTTPS URL');
  if (Number.isNaN(Date.parse(pubDate ?? ''))) throw new Error('Manifest pub_date must be a date');
  return {
    version,
    notes: notes ?? '',
    pub_date: new Date(pubDate).toISOString(),
    platforms: {
      [PLATFORM]: {
        signature: signature.trim(),
        url,
      },
    },
  };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = Object.fromEntries(
    process.argv.slice(2).map((arg) => {
      const match = /^--([^=]+)=(.*)$/s.exec(arg);
      if (!match) throw new Error(`Expected --key=value, got: ${arg}`);
      return [match[1], match[2]];
    }),
  );
  for (const key of ['version', 'archive', 'sig', 'url', 'out']) {
    if (!args[key]) throw new Error(`Missing required --${key}=`);
  }
  const manifest = composeManifest({
    version: args.version,
    notes: args.notes ?? '',
    pubDate: args.date ?? new Date().toISOString(),
    signature: fs.readFileSync(args.sig, 'utf8'),
    url: args.url,
  });
  fs.writeFileSync(args.out, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(`Updater manifest: ${args.out}`);
}
