import assert from 'node:assert/strict';
import test from 'node:test';
import { UpdateManager, busyLabel, compareVersions, selectCandidate } from './updates.ts';

const INFO = { version: '0.2.0', notes: 'Fixes recording on small windows.' };

function fakeBackend(overrides = {}) {
  const calls = [];
  return {
    calls,
    enabled: true,
    busy: { busy: false, stage: '' },
    options: null,
    async check(options) {
      calls.push('check');
      this.options = options;
      return INFO;
    },
    async download(_update, onProgress) {
      calls.push('download');
      onProgress(50, 100);
      onProgress(100, 100);
    },
    async install() {
      calls.push('install');
    },
    async relaunch() {
      calls.push('relaunch');
    },
    async operationState() {
      return this.busy;
    },
    ...overrides,
  };
}

const manager = (backend) => new UpdateManager(backend);

test('check records available, current and inert-error outcomes', async () => {
  const available = manager(fakeBackend());
  await available.check();
  assert.equal(available.state.status, 'available');
  assert.equal(available.state.available.version, '0.2.0');
  assert.ok(available.state.lastCheckedAt);

  const current = manager(
    fakeBackend({
      async check() {
        return null;
      },
    }),
  );
  await current.check();
  assert.equal(current.state.status, 'current');

  const inert = manager(fakeBackend({ enabled: false }));
  await inert.check();
  assert.equal(inert.state.status, 'error');
  assert.equal(inert.state.errorPhase, 'check');
});

test('checks never overlap and download reaches ready with progress', async () => {
  const backend = fakeBackend();
  const m = manager(backend);
  await Promise.all([m.check(), m.check()]);
  assert.equal(backend.calls.filter((c) => c === 'check').length, 1);

  await m.downloadAndInstall();
  assert.equal(m.state.status, 'ready');
  assert.equal(m.state.downloadedBytes, 100);
  assert.equal(m.state.totalBytes, 100);
  assert.deepEqual(backend.calls.slice(1), ['download', 'install']);
});

test('an active operation blocks install before and after downloading', async () => {
  const early = manager(fakeBackend());
  early.backend.busy = { busy: true, stage: 'recording' };
  await early.check();
  await early.downloadAndInstall();
  assert.equal(early.state.status, 'available');
  assert.match(early.state.blockedReason, /Recording is in progress/);
  assert.deepEqual(early.backend.calls, ['check']);

  const lateBackend = fakeBackend({
    async download(_u, onProgress) {
      this.busy = { busy: true, stage: 'installing' };
      onProgress(10, 10);
    },
  });
  const late = manager(lateBackend);
  await late.check();
  await late.downloadAndInstall();
  assert.equal(late.state.status, 'available');
  assert.match(late.state.blockedReason, /model download is in progress/);
  assert.equal(lateBackend.calls.filter((c) => c === 'install').length, 0);
});

test('download and install failures map to retryable phases', async () => {
  const badDownload = manager(
    fakeBackend({
      async download() {
        throw new Error('network unreachable');
      },
    }),
  );
  await badDownload.check();
  await badDownload.downloadAndInstall();
  assert.equal(badDownload.state.status, 'error');
  assert.equal(badDownload.state.errorPhase, 'download');
  assert.match(badDownload.state.errorMessage, /connection/);

  const badInstall = manager(
    fakeBackend({
      async download(_u, cb) {
        cb(10, 10);
      },
      async install() {
        throw new Error('signature verification failed');
      },
    }),
  );
  await badInstall.check();
  await badInstall.downloadAndInstall();
  assert.equal(badInstall.state.errorPhase, 'install');
  assert.match(badInstall.state.errorMessage, /could not be verified/);
});

test('restart runs only when ready and is blocked by active work', async () => {
  const backend = fakeBackend();
  const m = manager(backend);
  await m.restart();
  assert.equal(backend.calls.length, 0);

  await m.check();
  await m.downloadAndInstall();
  backend.busy = { busy: true, stage: 'transcribing' };
  await m.restart();
  assert.equal(m.state.status, 'ready');
  assert.match(m.state.blockedReason, /Transcription is in progress/);
  assert.equal(backend.calls.filter((c) => c === 'relaunch').length, 0);

  backend.busy = { busy: false, stage: '' };
  await m.restart();
  assert.equal(backend.calls.at(-1), 'relaunch');
});

test('operation stages get plain-language labels', () => {
  assert.equal(busyLabel('recording'), 'Recording is in progress');
  assert.equal(busyLabel('installing'), 'A model download is in progress');
  assert.equal(busyLabel('mystery'), 'Another operation is in progress');
});

const memoryStore = (initial = null) => {
  let saved = initial;
  return {
    saves: [],
    load: () => saved,
    save(preferences) {
      saved = preferences;
      this.saves.push(preferences);
    },
  };
};

test('preferences default to check on, install off, and normalize stored values', () => {
  const fresh = new UpdateManager(fakeBackend());
  assert.equal(fresh.state.autoCheck, true);
  assert.equal(fresh.state.autoInstall, false);
  const stored = new UpdateManager(
    fakeBackend(),
    memoryStore({ autoCheck: false, autoInstall: true }),
  );
  assert.equal(stored.state.autoCheck, false);
  assert.equal(stored.state.autoInstall, false, 'install needs automatic checks');
  const junk = new UpdateManager(fakeBackend(), memoryStore({ autoCheck: 'yes' }));
  assert.equal(junk.state.autoCheck, true);
});

test('setPreferences persists, notifies, and turning checks off turns install off', () => {
  const store = memoryStore();
  const updates = new UpdateManager(fakeBackend(), store);
  const seen = [];
  updates.subscribe((state) => seen.push([state.autoCheck, state.autoInstall]));
  updates.setPreferences({ autoInstall: true });
  assert.deepEqual(store.saves.at(-1), {
    autoCheck: true,
    autoInstall: true,
    includePrereleases: false,
  });
  updates.setPreferences({ autoCheck: false });
  assert.deepEqual(store.saves.at(-1), {
    autoCheck: false,
    autoInstall: false,
    includePrereleases: false,
  });
  assert.deepEqual(seen, [
    [true, true],
    [false, false],
  ]);
});

test('automatic check honors its preference while a manual check still works', async () => {
  const backend = fakeBackend();
  const updates = new UpdateManager(backend, memoryStore({ autoCheck: false }));
  await updates.automaticCheck();
  assert.deepEqual(backend.calls, []);
  assert.equal(updates.state.status, 'idle');
  await updates.check();
  assert.deepEqual(backend.calls, ['check']);
  assert.equal(updates.state.status, 'available');
  const on = fakeBackend();
  await new UpdateManager(on).automaticCheck();
  assert.deepEqual(on.calls, ['check']);
});

test('automatic install downloads and installs a found release but never restarts', async () => {
  const backend = fakeBackend();
  const updates = new UpdateManager(backend, memoryStore({ autoCheck: true, autoInstall: true }));
  await updates.automaticCheck();
  assert.deepEqual(backend.calls, ['check', 'download', 'install']);
  assert.equal(updates.state.status, 'ready');
  assert.ok(!backend.calls.includes('relaunch'));
});

test('automatic install waits for active work and is retried by the next check', async () => {
  const backend = fakeBackend({ busy: { busy: true, stage: 'recording' } });
  const updates = new UpdateManager(backend, memoryStore({ autoCheck: true, autoInstall: true }));
  await updates.automaticCheck();
  assert.deepEqual(backend.calls, ['check']);
  assert.equal(updates.state.status, 'available');
  assert.match(updates.state.blockedReason, /Recording is in progress/);
  backend.busy = { busy: false, stage: '' };
  await updates.automaticCheck();
  assert.equal(updates.state.status, 'ready');
});

test('enabling automatic install installs a release that is already waiting', async () => {
  const backend = fakeBackend();
  const updates = new UpdateManager(backend, memoryStore());
  await updates.check();
  assert.equal(updates.state.status, 'available');
  updates.setPreferences({ autoInstall: true });
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(updates.state.status, 'ready');
  assert.deepEqual(backend.calls, ['check', 'download', 'install']);
});

test('pre-release updates default to off and normalize stored values', () => {
  assert.equal(new UpdateManager(fakeBackend()).state.includePrereleases, false);
  const on = new UpdateManager(fakeBackend(), memoryStore({ includePrereleases: true }));
  assert.equal(on.state.includePrereleases, true);
  const junk = new UpdateManager(fakeBackend(), memoryStore({ includePrereleases: 'yes' }));
  assert.equal(junk.state.includePrereleases, false);
});

test('the backend is told whether pre-releases are wanted', async () => {
  const backend = fakeBackend();
  const updates = new UpdateManager(backend, memoryStore());
  await updates.check();
  assert.deepEqual(backend.options, { includePrereleases: false });
  const beta = fakeBackend();
  const testers = new UpdateManager(beta, memoryStore({ includePrereleases: true }));
  await testers.check();
  assert.deepEqual(beta.options, { includePrereleases: true });
});

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

test('changing the pre-release switch drops the stale offer and re-checks', async () => {
  const backend = fakeBackend({
    async check(options) {
      return options.includePrereleases ? { version: '0.2.0-rc.1', notes: '' } : null;
    },
  });
  const updates = new UpdateManager(backend, memoryStore({ includePrereleases: true }));
  await updates.check();
  assert.equal(updates.state.status, 'available');
  assert.equal(updates.state.available.version, '0.2.0-rc.1');
  updates.setPreferences({ includePrereleases: false });
  assert.equal(updates.state.available, null, 'the rc is not offered once the switch is off');
  await flush();
  assert.equal(updates.state.status, 'current');
  updates.setPreferences({ includePrereleases: true });
  await flush();
  assert.equal(updates.state.available.version, '0.2.0-rc.1');
});

test('with automatic checks off the switch only resets the offer', async () => {
  const backend = fakeBackend();
  const updates = new UpdateManager(backend, memoryStore({ autoCheck: false }));
  await updates.check();
  const before = backend.calls.length;
  updates.setPreferences({ includePrereleases: true });
  await flush();
  assert.equal(backend.calls.length, before, 'no check without automatic checks');
  assert.equal(updates.state.status, 'idle');
  assert.equal(updates.state.available, null);
});

test('the switch leaves a downloaded update alone', async () => {
  const updates = new UpdateManager(fakeBackend(), memoryStore());
  await updates.check();
  await updates.downloadAndInstall();
  assert.equal(updates.state.status, 'ready');
  updates.setPreferences({ includePrereleases: true });
  await flush();
  assert.equal(updates.state.status, 'ready');
  assert.equal(updates.state.includePrereleases, true);
});

test('versions order by SemVer precedence', () => {
  const order = [
    '0.1.0',
    '0.1.1-nightly.20261021',
    '0.1.1-nightly.20261022',
    '0.1.1-rc.2',
    '0.1.1-rc.3',
    '0.1.1-rc.10',
    '0.1.1',
    '0.2.0-rc.1',
    '0.2.0',
    '1.0.0',
  ];
  for (let i = 0; i < order.length; i += 1) {
    assert.equal(compareVersions(order[i], order[i]), 0);
    for (let j = i + 1; j < order.length; j += 1) {
      assert.equal(compareVersions(order[i], order[j]), -1, `${order[i]} < ${order[j]}`);
      assert.equal(compareVersions(order[j], order[i]), 1, `${order[j]} > ${order[i]}`);
    }
  }
  assert.equal(compareVersions('v0.1.1+build.5', '0.1.1'), 0, 'a leading v and build metadata');
});

test('the higher of the stable and pre-release answers wins', () => {
  const ok = (version) => ({ value: version ? { version } : null, error: null });
  const failed = (message) => ({ value: null, error: new Error(message) });
  // Stable only, because pre-releases are off.
  assert.equal(selectCandidate(ok('0.2.0'), null).chosen.version, '0.2.0');
  assert.equal(selectCandidate(ok(null), null).chosen, null);
  // A newer rc beats an older stable; a stable that overtakes the rc beats it.
  assert.equal(selectCandidate(ok('0.1.1'), ok('0.1.2-rc.1')).chosen.version, '0.1.2-rc.1');
  assert.equal(selectCandidate(ok('0.1.2'), ok('0.1.2-rc.3')).chosen.version, '0.1.2');
  assert.equal(selectCandidate(ok(null), ok('0.1.2-rc.1')).chosen.version, '0.1.2-rc.1');
  // A failing pre-release feed never hides a working stable answer, and vice versa.
  assert.equal(selectCandidate(ok('0.1.2'), failed('beta feed down')).chosen.version, '0.1.2');
  assert.equal(selectCandidate(failed('stable'), ok('0.1.2-rc.1')).chosen.version, '0.1.2-rc.1');
  assert.equal(selectCandidate(ok(null), failed('beta feed down')).error, null);
  // Only when every consulted feed fails is it an error, reported from the stable feed.
  const both = selectCandidate(failed('stable'), failed('beta'));
  assert.equal(both.chosen, null);
  assert.equal(both.error.message, 'stable');
  assert.equal(selectCandidate(failed('stable'), null).error.message, 'stable');
});
