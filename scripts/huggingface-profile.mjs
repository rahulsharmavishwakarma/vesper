#!/usr/bin/env node
// Public Hub metadata only; no repository code, credentials, or inference.
import { createHash } from 'node:crypto';
import { lstat, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const HOST = 'https://huggingface.co';
const SMALL_FILE_LIMIT = 1024 * 1024;
const HEX_COMMIT = /^[a-f0-9]{40}$/;
const HEX_SHA256 = /^[a-f0-9]{64}$/;
const HELP = `Generate a pinned public Hugging Face model profile (Node 20+; no Python).

Cleanup:
  node scripts/huggingface-profile.mjs --repo owner/model --runtime llama_cpp --weights model.gguf
Speech:
  node scripts/huggingface-profile.mjs --repo owner/model --runtime sherpa_onnx_transducer \\
    --encoder encoder.onnx --decoder decoder.onnx --joiner joiner.onnx --tokens tokens.txt --languages en

Options: --revision main|COMMIT|TAG (default main), --name NAME,
         --license LICENSE (otherwise use model card), --languages en,hi,
         --out FILE (otherwise JSON goes to stdout), --help.
Only metadata and files of at most 1 MiB lacking SHA-256 are downloaded.
Generated profiles do not attest runtime compatibility or model quality.`;

function requireValue(condition, message) {
  if (!condition) throw new Error(message);
}
const component = (value) => /^[A-Za-z0-9._-]+$/.test(value) && value !== '.' && value !== '..';
const relativePath = (value, max) =>
  typeof value === 'string' && value.length <= max && value.split('/').every(component);
const printable = (value) => typeof value === 'string' && value.trim() && !/\p{Cc}/u.test(value);

export function parseArguments(args) {
  const allowed = new Set([
    'repo',
    'revision',
    'runtime',
    'weights',
    'encoder',
    'decoder',
    'joiner',
    'tokens',
    'languages',
    'name',
    'license',
    'out',
  ]);
  const options = {};
  for (let index = 0; index < args.length; index += 1) {
    const key = args[index].replace(/^--/, '');
    if (args[index] === '--help') return { help: true };
    requireValue(
      args[index].startsWith('--') && allowed.has(key),
      `Unknown option: ${args[index]}. Use --help.`,
    );
    requireValue(!Object.hasOwn(options, key), `Duplicate option: --${key}`);
    const value = args[++index];
    requireValue(value && !value.startsWith('--'), `--${key} requires a value.`);
    options[key] = value;
  }
  return options;
}

async function boundedBytes(response, limit, label) {
  const declared = Number(response.headers.get('content-length'));
  if (declared > limit) {
    await response.body?.cancel();
    throw new Error(`${label} exceeds the ${limit}-byte download limit.`);
  }
  requireValue(response.body, `${label} returned an empty response.`);
  const reader = response.body.getReader();
  const chunks = [];
  let total = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      total += value.length;
      if (total > limit) {
        await reader.cancel();
        throw new Error(`${label} exceeds the ${limit}-byte download limit.`);
      }
      chunks.push(value);
    }
  } finally {
    reader.releaseLock();
  }
  return Buffer.concat(chunks, total);
}

export async function generateProfile(options, fetchImpl = globalThis.fetch) {
  const { repo, runtime } = options;
  requireValue(
    relativePath(repo, 200) && repo.split('/').length === 2,
    '--repo must be an owner/name repository, not a URL.',
  );
  const revision = options.revision ?? 'main';
  requireValue(
    relativePath(revision, 200),
    '--revision must be a branch, tag, or lowercase commit without traversal or URL escapes.',
  );
  requireValue(
    !/^[a-fA-F0-9]{40}$/.test(revision) || HEX_COMMIT.test(revision),
    'Commit hashes must be lowercase.',
  );
  requireValue(
    ['llama_cpp', 'sherpa_onnx_transducer'].includes(runtime),
    '--runtime must be llama_cpp or sherpa_onnx_transducer.',
  );
  const roles = runtime === 'llama_cpp' ? ['weights'] : ['encoder', 'decoder', 'joiner', 'tokens'];
  const languages = options.languages === undefined ? [] : options.languages.split(',');
  requireValue(
    languages.length <= 64 && languages.every((value) => /^[A-Za-z0-9-]{1,24}$/.test(value)),
    '--languages must contain comma-separated language codes, such as en,hi.',
  );
  requireValue(
    runtime !== 'sherpa_onnx_transducer' || languages.length,
    'Speech profiles require --languages declaring the model’s actual supported languages.',
  );
  const name = options.name ?? repo.split('/')[1].slice(0, 120);
  requireValue(
    printable(name) && [...name].length <= 120,
    '--name must be 1–120 printable characters.',
  );
  if (options.license !== undefined)
    requireValue(
      printable(options.license) && Buffer.byteLength(options.license) <= 500,
      '--license must be 1–500 printable bytes.',
    );
  for (const role of ['weights', 'encoder', 'decoder', 'joiner', 'tokens']) {
    if (!roles.includes(role))
      requireValue(options[role] === undefined, `--${role} does not apply to ${runtime}.`);
  }
  for (const role of roles) {
    const file = options[role];
    const extension = role === 'weights' ? '.gguf' : role === 'tokens' ? '.txt' : '.onnx';
    requireValue(
      relativePath(file, 512) && file.endsWith(extension),
      `--${role} requires a relative ${extension} repository path using ASCII letters, numbers, dots, underscores, hyphens and slashes.`,
    );
    requireValue(
      role !== 'weights' || !/-\d{5}-of-\d{5}\.gguf$/.test(file),
      'Split GGUF is unsupported; select one complete, unsplit GGUF.',
    );
  }
  requireValue(
    new Set(roles.map((role) => options[role])).size === roles.length,
    'Each file role must use a different repository path.',
  );
  async function request(url) {
    let response;
    try {
      response = await fetchImpl(url, { signal: AbortSignal.timeout(30_000) });
    } catch (error) {
      throw new Error(`Hugging Face request failed: ${error.message}`, { cause: error });
    }
    if (!response.ok) {
      await response.body?.cancel();
      if ([401, 403].includes(response.status))
        throw new Error(
          'Repository/file unavailable without credentials (private, gated, or missing). This utility supports public access only.',
        );
      if (response.status === 404)
        throw new Error('Hugging Face repository, revision, or file was not found.');
      if (response.status === 429) throw new Error('Hugging Face rate limit reached; retry later.');
      throw new Error(`Hugging Face returned HTTP ${response.status}.`);
    }
    return response;
  }
  async function metadata(ref, blobs = false) {
    const response = await request(
      `${HOST}/api/models/${repo}/revision/${encodeURIComponent(ref)}${blobs ? '?blobs=true' : ''}`,
    );
    const bytes = await boundedBytes(response, 16 * SMALL_FILE_LIMIT, 'Repository metadata');
    let data;
    try {
      data = JSON.parse(bytes.toString('utf8'));
    } catch {
      throw new Error('Hugging Face returned invalid metadata JSON.');
    }
    requireValue(
      data && typeof data === 'object' && !Array.isArray(data),
      'Hugging Face returned an invalid metadata object.',
    );
    return data;
  }
  const resolved = await metadata(revision);
  requireValue(
    HEX_COMMIT.test(resolved.sha),
    'Hub metadata did not return an immutable lowercase commit.',
  );
  requireValue(
    !HEX_COMMIT.test(revision) || resolved.sha === revision,
    'Hub metadata returned a different commit than requested.',
  );
  const pinned = await metadata(resolved.sha, true);
  requireValue(
    pinned.sha === resolved.sha && Array.isArray(pinned.siblings),
    'Pinned Hub metadata has an unexpected commit or file list.',
  );
  const license = options.license ?? pinned.cardData?.license;
  requireValue(
    printable(license) && Buffer.byteLength(license) <= 500,
    'Model card has no usable license; provide --license from the repository terms.',
  );
  const files = [];
  for (const role of roles) {
    const path = options[role];
    const matches = pinned.siblings.filter((entry) => entry.rfilename === path);
    requireValue(matches.length === 1, `File not found unambiguously at pinned revision: ${path}`);
    const entry = matches[0];
    const sizeBytes = entry.size ?? entry.lfs?.size;
    requireValue(
      Number.isSafeInteger(sizeBytes) && sizeBytes >= 1 && sizeBytes <= 50_000_000_000,
      `Invalid file size for ${path}.`,
    );
    requireValue(
      entry.lfs?.size === undefined || entry.lfs.size === sizeBytes,
      `Conflicting file sizes for ${path}.`,
    );
    let sha256 = entry.lfs?.sha256;
    if (sha256 !== undefined)
      requireValue(HEX_SHA256.test(sha256), `Invalid LFS SHA-256 for ${path}.`);
    else {
      requireValue(
        sizeBytes <= SMALL_FILE_LIMIT,
        `No SHA-256 for ${path}; refusing to download more than 1 MiB. Obtain its hash separately.`,
      );
      const response = await request(`${HOST}/${repo}/resolve/${resolved.sha}/${path}`);
      const bytes = await boundedBytes(response, Math.min(sizeBytes, SMALL_FILE_LIMIT), path);
      requireValue(
        bytes.length === sizeBytes,
        `Downloaded size differs from pinned metadata for ${path}.`,
      );
      sha256 = createHash('sha256').update(bytes).digest('hex');
    }
    files.push({ role, path, sha256, sizeBytes });
  }
  return {
    id: `hf-${createHash('sha256')
      .update(JSON.stringify([repo, resolved.sha, runtime, files]))
      .digest('hex')
      .slice(0, 16)}`,
    name,
    repository: repo,
    revision: resolved.sha,
    runtime,
    license,
    languages,
    files,
    ...(runtime === 'sherpa_onnx_transducer' ? { modelType: 'nemo_transducer' } : {}),
  };
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  if (options.help) return console.log(HELP);
  const profile = await generateProfile(options);
  const json = `${JSON.stringify(profile, null, 2)}\n`;
  if (options.out) {
    const existing = await lstat(options.out).catch((error) => {
      if (error.code !== 'ENOENT') throw error;
    });
    requireValue(
      !existing || existing.isFile(),
      '--out must point to a regular file, not a directory or symbolic link.',
    );
    await writeFile(options.out, json);
  } else process.stdout.write(json);
  console.error(
    'Profile generated from pinned metadata and any small files lacking hashes. Runtime compatibility and model quality have not been tested.',
  );
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch((error) => {
    console.error(`Profile error: ${error.message}`);
    process.exitCode = 1;
  });
}
