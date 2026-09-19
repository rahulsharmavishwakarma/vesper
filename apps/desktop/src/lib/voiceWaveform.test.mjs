import assert from 'node:assert/strict';
import test from 'node:test';
import {
  followSignal,
  pitchCycles,
  reedPath,
  waveformEnergy,
  waveformCycles,
  ReedMotion,
  followLevel,
} from './voiceWaveform.ts';
import { PitchDisplay } from './pitchDisplay.ts';

test('quiet ambient input stays small without hiding unvoiced speech or imposing a gain gate', () => {
  assert.ok(waveformEnergy(10 ** (-50 / 20)) < 0.05);
  assert.ok(waveformEnergy(10 ** (-40 / 20)) < 0.18);
  assert.ok(waveformEnergy(10 ** (-20 / 20)) > 0.65);
  let previous = 0;
  for (let db = -60; db <= 0; db += 0.1) {
    const value = waveformEnergy(10 ** (db / 20));
    assert.ok(value >= previous && value <= 1);
    assert.ok(value - previous < 0.005, 'Gain steps must not become an on/off threshold');
    previous = value;
  }
  for (const level of [NaN, Infinity, -1, 0]) assert.equal(waveformEnergy(level), 0);
  assert.equal(waveformEnergy(1), 1);
});

test('long unvoiced gaps do not reshape a steady signal; silence clears visual pitch memory', () => {
  for (const frameMs of [1000 / 30, 1000 / 60, 1000 / 120]) {
    const display = new PitchDisplay();
    let target = waveformCycles(3, display.update(145, 0), false);
    let current = target;
    const before = reedPath(waveformEnergy(0.06), current, 0);
    for (let time = 0; time < 900; time += frameMs) {
      const measured = display.update(time < 120 || time > 650 ? 145 : null, time);
      target = waveformCycles(target, measured, false);
      current = followSignal(current, target, frameMs, 35);
      assert.equal(reedPath(waveformEnergy(0.06), current, 0), before);
      if (time > 300 && time < 650)
        assert.equal(measured, null, 'Visual retention must not fabricate pitch telemetry');
    }
    target = waveformCycles(target, null, true);
    assert.equal(target, 3, 'Silence must clear the previous voice');
    assert.equal(waveformCycles(target, null, false), 3, 'A fresh unvoiced input starts neutral');
    assert.notEqual(waveformCycles(target, 330, false), 3, 'New valid pitch remains responsive');
  }
});

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

test('pitch transitions morph between the same Reed shapes without unrelated peaks', () => {
  for (const compact of [false, true]) {
    const motion = new ReedMotion(compact);
    const from = pitchCycles(145),
      to = pitchCycles(440),
      level = 0.75;
    motion.path(0, from, 0);
    const heights = (path) => strokes(path).map((bar) => bar.bottom - bar.top);
    const start = heights(reedPath(level, from, 0, compact));
    const end = heights(reedPath(level, to, 0, compact));
    for (let frame = 0; frame < 40; frame++) {
      const current = heights(motion.path(level, to, 1000 / 60));
      current.forEach((height, i) => {
        assert.ok(height >= Math.min(start[i], end[i]) - 0.02);
        assert.ok(
          height <= Math.max(start[i], end[i]) + 0.02,
          'An intermediate pitch created an unrelated sine peak',
        );
      });
    }
    assert.equal(motion.path(level, to, 16), reedPath(level, to, 0, compact));
    assert.equal(motion.path(0, 3, 16), reedPath(0, 3, 0, compact));
  }
});

test('level response is frame-rate independent and completes a voice onset promptly', () => {
  for (const dt of [1000 / 30, 1000 / 60, 1000 / 120]) {
    let level = 0;
    for (let t = 0; t < 100; t += dt) level = followLevel(level, 1, dt);
    assert.ok(level > 0.94 && level <= 1);
    for (let t = 0; t < 500; t += dt) level = followLevel(level, 0, dt);
    assert.ok(level < 0.002);
  }
});

test('pitch steps respond promptly at different frame rates and settle without drifting', () => {
  for (const frameMs of [1000 / 120, 1000 / 60, 1000 / 30]) {
    const motion = new ReedMotion();
    const from = pitchCycles(110);
    const target = pitchCycles(330);
    motion.path(0, from, 0);
    const heights = (path) => strokes(path).map((bar) => bar.bottom - bar.top);
    const start = heights(reedPath(0.8, from, 0));
    const end = heights(reedPath(0.8, target, 0));
    const span = Math.max(...end.map((height, i) => Math.abs(height - start[i])));
    let path;
    for (let time = 0; time < 140; time += frameMs) path = motion.path(0.8, target, frameMs);
    const error = Math.max(...heights(path).map((height, i) => Math.abs(height - end[i])));
    assert.ok(
      error / span < 0.025,
      'Stroke morph should cover 97.5% of a step within 140 ms of UI smoothing',
    );
    for (let time = 0; time < 250; time += frameMs) path = motion.path(0.8, target, frameMs);
    assert.equal(path, reedPath(0.8, target, 0), 'A steady pitch should stop moving');
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
