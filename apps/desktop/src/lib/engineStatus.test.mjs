import assert from 'node:assert/strict';
import test from 'node:test';
import {
  appendSample,
  validSample,
  resourcePath,
  resourceBars,
  durationLabel,
  createResourcePoller,
} from './engineStatus.ts';
const sample = (time, cpu = 25) => ({
  sampledAtMs: time,
  cpuPercent: cpu,
  memoryBytes: 2 ** 30,
  logicalCpus: 8,
  status: 'ready',
});

test('resource bars retain gaps and actual zero values, with bounded geometry', () => {
  const bars = resourceBars(
    [sample(0, 0), sample(1000, null), sample(2000, 100)],
    'cpuPercent',
    100,
  );
  assert.deepEqual(
    bars.map((bar) => bar.time),
    [0, 2000],
  );
  assert.equal(bars[0].height, 0);
  assert.equal(bars[1].height, 68);
  assert.ok(bars.every((bar) => bar.x >= 0 && bar.x + 4 <= 400 && bar.y >= 8));
  assert.deepEqual(resourceBars([sample(0, NaN)], 'cpuPercent', 100), []);
  assert.deepEqual(resourceBars([sample(0)], 'memoryBytes', 0), []);
});

test('resource history bounds time and count without filling missing CPU with zero', () => {
  let points = [];
  for (let i = 0; i < 200; i++) points = appendSample(points, sample(i * 1000));
  assert.equal(points.length, 60);
  assert.equal(appendSample(points, points.at(-1)), points);
  assert.equal(appendSample(points, sample(1)).length, 1);
  assert.throws(() => validSample(sample(0), 4000), /stale/);
  assert.equal(validSample(sample(1000, NaN), 1000).cpuPercent, null);
  assert.equal(validSample({ ...sample(1000), status: 'warming' }, 1000).cpuPercent, null);
  const unavailable = validSample({ ...sample(1000), status: 'unavailable', memoryBytes: 0 }, 1000);
  assert.equal(unavailable.cpuPercent, null);
  assert.equal(unavailable.memoryBytes, null);
  const path = resourcePath(
    [sample(0), sample(1000, null), sample(2000), sample(6000)],
    'cpuPercent',
    100,
  );
  assert.equal(path.split('M').length - 1, 3);
  assert.equal(durationLabel(null), '—');
  assert.equal(durationLabel(1234), '1.2 s');
});
test('polling suspends immediately, ignores old results and never overlaps requests', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  let resolve,
    calls = 0;
  const values = [],
    errors = [];
  const poller = createResourcePoller(
    () => {
      calls++;
      return new Promise((done) => {
        resolve = done;
      });
    },
    (value) => values.push(value),
    (error) => errors.push(error),
  );
  poller.setEnabled(true);
  poller.refresh();
  assert.equal(calls, 1);
  poller.setEnabled(false);
  poller.setEnabled(true);
  t.mock.timers.tick(3000);
  assert.equal(errors.length, 1, 'A resumed panel must report the outstanding request timeout');
  assert.equal(calls, 1);
  resolve('stale');
  await Promise.resolve();
  assert.deepEqual(values, []);
  t.mock.timers.tick(0);
  assert.equal(calls, 2);
  t.mock.timers.tick(3000);
  assert.equal(errors.length, 2);
  resolve('expired');
  await Promise.resolve();
  assert.deepEqual(values, []);
  poller.dispose();
  t.mock.timers.tick(10_000);
  assert.equal(calls, 2);
});
