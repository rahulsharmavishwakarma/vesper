import assert from 'node:assert/strict';
import test from 'node:test';
import { followSignal, pitchCycles, reedPath } from './voiceWaveform.ts';

test('higher measured pitch produces shorter wavelengths, with a stable unvoiced fallback', () => {
  const values = [60, 110, 220, 440, 700].map(pitchCycles);
  assert.ok(values.every((value, index) => index === 0 || value > values[index - 1]));
  for (const value of [null, NaN, Infinity, 0, -10, 900])
    assert.equal(pitchCycles(value), pitchCycles(null));
});

const strokes = (path) =>
  [...path.matchAll(/M ([\d.]+) ([\d.]+) V ([\d.]+)/g)].map(([, x, top, bottom]) => ({
    x: +x,
    top: +top,
    bottom: +bottom,
  }));

test('pitch steps respond promptly at different frame rates and settle without drifting', () => {
  for (const frameMs of [1000 / 120, 1000 / 60, 1000 / 30]) {
    let value = pitchCycles(110);
    const target = pitchCycles(330);
    for (let time = 0; time < 140; time += frameMs)
      value = followSignal(value, target, frameMs, 35);
    assert.ok(
      Math.abs(value - target) / Math.abs(pitchCycles(110) - target) < 0.025,
      'Pitch should cover 97.5% of a step within 140 ms of UI smoothing',
    );
    for (let time = 0; time < 250; time += frameMs)
      value = followSignal(value, target, frameMs, 35);
    assert.equal(value, target, 'A steady pitch should stop moving');
  }
});

test('volume scales the same pitch pattern while higher pitch adds visible lobes', () => {
  const normalized = (level, hz) =>
    strokes(reedPath(level, pitchCycles(hz), 0)).map((bar) => (bar.bottom - bar.top - 2) / level);
  const loud = normalized(0.8, 220),
    quiet = normalized(0.2, 220);
  loud.forEach((height, i) => assert.ok(Math.abs(height - quiet[i]) < 0.1));
  const peaks = (hz) => {
    const heights = normalized(0.8, hz);
    return heights.filter(
      (height, i) =>
        i > 0 && i < heights.length - 1 && height > heights[i - 1] && height > heights[i + 1],
    ).length;
  };
  assert.ok(peaks(330) > peaks(110), 'A higher voice pitch should visibly tighten the lobes');
});

test('Reed silence never acquires motion and measured energy bounds both sizes', () => {
  for (const compact of [false, true]) {
    const quiet = reedPath(0, 3, 0, compact);
    for (const phase of [0, 1, Math.PI]) {
      assert.equal(reedPath(0, pitchCycles(440), phase, compact), quiet);
      for (const hz of [60, 110, 440, 700]) {
        const low = strokes(reedPath(0.2, pitchCycles(hz), phase, compact));
        const high = strokes(reedPath(1, pitchCycles(hz), phase, compact));
        assert.equal(high.length, compact ? 11 : 45);
        high.forEach((bar, index) => {
          assert.ok(bar.x >= 0 && bar.x <= 320);
          assert.ok(bar.top >= 0 && bar.bottom <= 100);
          assert.ok(bar.bottom - bar.top >= low[index].bottom - low[index].top);
          assert.ok(Math.abs(bar.top + bar.bottom - 100) < 0.02);
        });
      }
    }
    assert.equal(reedPath(NaN, 3, 1, compact), quiet);
    assert.equal(reedPath(-1, 3, 1, compact), quiet);
    assert.equal(reedPath(2, 3, 1, compact), reedPath(1, 3, 1, compact));
    assert.ok(!/NaN|Infinity/.test(reedPath(1, NaN, Infinity, compact)));
  }
});
