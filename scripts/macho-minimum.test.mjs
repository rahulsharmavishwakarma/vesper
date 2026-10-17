import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  DEPLOYMENT_TARGET,
  loadableOn,
  minimumSystem,
  parseMinimumSystem,
} from './macho-minimum.mjs';

const commands = (minos) => `Load command 9
      cmd LC_BUILD_VERSION
  cmdsize 32
 platform 1
    minos ${minos}
      sdk 27.0`;

test('reads the minimum macOS from LC_BUILD_VERSION', () => {
  assert.equal(parseMinimumSystem(commands('27.0')), '27.0');
  assert.equal(parseMinimumSystem('no build version'), null);
});

test('libraries newer than the bundle minimum are not loadable', () => {
  assert.equal(DEPLOYMENT_TARGET, '14.0');
  assert.equal(loadableOn('27.0'), false);
  assert.equal(loadableOn('14.1'), false);
  assert.equal(loadableOn('14.0'), true);
  assert.equal(loadableOn('13.3'), true);
  assert.equal(loadableOn('14'), true);
});

test('non-Mach-O files and missing otool yield no minimum', () => {
  const failing = () => {
    throw new Error('not an object file');
  };
  assert.equal(minimumSystem('/fixture', failing), null);
  assert.equal(
    minimumSystem('/lib.dylib', () => commands('14.0')),
    '14.0',
  );
});
