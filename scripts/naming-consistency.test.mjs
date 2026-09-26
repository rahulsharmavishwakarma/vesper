import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const root = fileURLToPath(new URL('../', import.meta.url));
const files = execFileSync('git', ['ls-files', '-z'], { cwd: root, encoding: 'utf8' })
  .split('\0')
  .filter(Boolean);
const historical = (path) =>
  path === 'docs/validation.md' ||
  (path.startsWith('docs/validation/') && !path.endsWith('native-device-checklist.md')) ||
  /^benchmarks\/asr\/(?:\d{4}-|results\/|manifests\/)/.test(path);

test('current project namespaces and paths use Vesper, without rewriting historical evidence', () => {
  const stale =
    /\btranscribe-(?:core|audio|data|desktop|eval|inference|mlx|models|nemo|workspace|tooling|browser-preview|macos|export|microphone|menu-bar|menu-status|live-audio)\b|@transcribe\/|\bTranscribeMLX|\btranscribe_mlx_|\btranscribe\.(?:decode_report|whisper_scores|whisper_language|input_truncated)|\btranscribe(?:CompletionReason|WhisperScores)|\b(?:k)?Transcribe(?:Begin|Report|ValidLogits|WhisperBudget)/;
  const violations = [];
  for (const file of files) {
    assert.doesNotMatch(
      file,
      /^(?:crates\/transcribe-|native\/mlx\/(?:Sources|Tests)\/Transcribe)/,
    );
    if (
      historical(file) ||
      file.startsWith('vendor/') ||
      file === 'scripts/naming-consistency.test.mjs'
    )
      continue;
    if (
      !/\.(?:rs|ts|mjs|js|svelte|swift|cpp|h|m|sh|py|md|json|toml|yaml|yml|lock|patch)$/.test(
        file,
      ) &&
      !['Dockerfile', 'Makefile'].includes(file)
    )
      continue;
    const source = readFileSync(new URL(`../${file}`, import.meta.url), 'utf8');
    source.split('\n').forEach((line, index) => {
      if (stale.test(line) || /std::env::var(?:_os)?\("TRANSCRIBE_/.test(line))
        violations.push(`${file}:${index + 1}: ${line.trim()}`);
    });
  }
  assert.deepEqual(violations, []);
});

test('persisted learning identities deliberately retain their original hash domain', () => {
  const source = readFileSync(
    new URL('../crates/vesper-data/src/learning.rs', import.meta.url),
    'utf8',
  );
  assert.match(source, /b"transcribe-learning-pcm16k-v1"/);
  assert.doesNotMatch(source, /b"vesper-learning-pcm16k-v1"/);
});
