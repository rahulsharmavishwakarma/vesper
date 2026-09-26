import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { generateProfile, parseArguments } from './huggingface-profile.mjs';

const commit = 'a'.repeat(40);
const digest = 'b'.repeat(64);
const base = {
  repo: 'owner/model',
  runtime: 'llama_cpp',
  weights: 'weights/model.gguf',
  languages: 'en',
};
const weight = {
  rfilename: base.weights,
  size: 563036064,
  lfs: { size: 563036064, sha256: digest },
};
function mockHub(files = [weight], overrides = {}, body = 'hello') {
  const calls = [];
  const fetchImpl = async (url) => {
    calls.push(url);
    if (url.includes('/resolve/')) return new Response(body);
    return Response.json(
      url.includes('?blobs=true')
        ? { sha: commit, cardData: { license: 'apache-2.0' }, siblings: files, ...overrides }
        : { sha: commit },
    );
  };
  return { calls, fetchImpl };
}

test('resolves a mutable revision, reads pinned metadata, and never fetches large LFS weights', async () => {
  const hub = mockHub();
  const profile = await generateProfile(base, hub.fetchImpl);
  assert.deepEqual(hub.calls, [
    'https://huggingface.co/api/models/owner/model/revision/main',
    `https://huggingface.co/api/models/owner/model/revision/${commit}?blobs=true`,
  ]);
  assert.equal(profile.revision, commit);
  assert.deepEqual(profile.files, [
    { role: 'weights', path: base.weights, sizeBytes: weight.size, sha256: digest },
  ]);
  assert.equal(profile.license, 'apache-2.0');
  assert.equal(profile.modelType, undefined);
});

test('speech profile hashes bounded token bytes instead of treating Git blobId as SHA256', async () => {
  const options = {
    repo: base.repo,
    runtime: 'sherpa_onnx_transducer',
    encoder: 'e.onnx',
    decoder: 'd.onnx',
    joiner: 'j.onnx',
    tokens: 'tokens.txt',
    languages: 'en,hi',
  };
  const files = ['e.onnx', 'd.onnx', 'j.onnx'].map((rfilename) => ({ ...weight, rfilename }));
  files.push({ rfilename: 'tokens.txt', size: 5, blobId: 'c'.repeat(40) });
  const hub = mockHub(files);
  const profile = await generateProfile(options, hub.fetchImpl);
  assert.deepEqual(profile.languages, ['en', 'hi']);
  assert.equal(profile.modelType, 'nemo_transducer');
  assert.equal(profile.files[3].sha256, createHash('sha256').update('hello').digest('hex'));
  assert.equal(hub.calls[2], `https://huggingface.co/owner/model/resolve/${commit}/tokens.txt`);
});

test('rejects malformed input before requesting the network', async () => {
  for (const override of [
    { repo: 'https://huggingface.co/a/b' },
    { repo: '../model' },
    { repo: 'a/b/c' },
    { revision: '../main' },
    { revision: 'main?x=1' },
    { revision: 'A'.repeat(40) },
    { weights: '../model.gguf' },
    { weights: 'x\\model.gguf' },
    { weights: '/model.gguf' },
    { weights: 'x/%2e%2e/model.gguf' },
    { weights: 'x//model.gguf' },
    { weights: 'model-00001-of-00002.gguf' },
    { weights: 'model.onnx' },
    { name: '\u0000' },
    { languages: 'en,,hi' },
    { languages: undefined },
    { languages: '' },
    { license: '' },
    { encoder: 'wrong.onnx' },
  ]) {
    await assert.rejects(
      generateProfile({ ...base, ...override }, () => assert.fail('network called')),
    );
  }
  await assert.rejects(
    generateProfile({ repo: 'a/b', runtime: 'sherpa_onnx_transducer' }, () =>
      assert.fail('network called'),
    ),
    /languages/,
  );
});

test('rejects missing, malformed or mismatched pinned metadata', async () => {
  for (const hub of [
    mockHub([], {}),
    mockHub([weight], { sha: 'c'.repeat(40) }),
    mockHub([{ ...weight, lfs: { ...weight.lfs, sha256: 'bad' } }]),
    mockHub([{ ...weight, size: 0 }]),
    mockHub([{ ...weight, size: 99 }]),
    mockHub([weight], { cardData: {} }),
  ]) {
    await assert.rejects(generateProfile(base, hub.fetchImpl));
  }
  const hub = mockHub([weight], { cardData: {} });
  assert.equal((await generateProfile({ ...base, license: 'MIT' }, hub.fetchImpl)).license, 'MIT');
  await assert.rejects(
    generateProfile({ ...base, revision: 'd'.repeat(40) }, mockHub().fetchImpl),
    /different commit/,
  );
});

test('does not download a large non-LFS file without SHA256', async () => {
  const hub = mockHub([{ rfilename: base.weights, size: 1024 * 1024 + 1, blobId: 'c'.repeat(40) }]);
  await assert.rejects(generateProfile(base, hub.fetchImpl), /refusing to download/);
  assert.equal(hub.calls.length, 2);
});

test('rejects truncated and oversized small-file responses, including absent Content-Length', async () => {
  for (const body of ['tiny', 'too long']) {
    const hub = mockHub([{ rfilename: base.weights, size: 5 }], {}, body);
    await assert.rejects(generateProfile(base, hub.fetchImpl), /size differs|download limit/);
  }
});

test('reports public access and HTTP errors without requiring tokens', async () => {
  for (const [status, pattern] of [
    [401, /public access/],
    [403, /public access/],
    [404, /not found/],
    [429, /rate limit/],
    [500, /HTTP 500/],
  ]) {
    await assert.rejects(
      generateProfile(base, async () => new Response('error', { status })),
      pattern,
    );
  }
  await assert.rejects(
    generateProfile(base, async () => Response.json(null)),
    /invalid metadata object/,
  );
  await assert.rejects(
    generateProfile(base, async () => new Response('broken JSON')),
    /invalid metadata JSON/,
  );
});

test('CLI rejects unknown, duplicate and missing options', () => {
  for (const args of [
    ['--unknown'],
    ['--repo'],
    ['--repo', '--runtime'],
    ['--repo', 'a/b', '--repo', 'c/d'],
    ['repo', 'a/b'],
  ]) {
    assert.throws(() => parseArguments(args));
  }
  assert.deepEqual(parseArguments(['--help']), { help: true });
  assert.deepEqual(parseArguments(['--repo', 'a/b', '--runtime', 'llama_cpp']), {
    repo: 'a/b',
    runtime: 'llama_cpp',
  });
});
