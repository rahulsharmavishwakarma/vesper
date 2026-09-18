import assert from 'node:assert/strict';
import test from 'node:test';
import { PitchDisplay } from './pitchDisplay.ts';
import { reedPath, pitchCycles } from './voiceWaveform.ts';

test('unvoiced consonants retain spacing without inventing a new pitch measurement', () => {
  const display = new PitchDisplay();
  const original = reedPath(0.4, pitchCycles(display.update(220, 0)), 0);
  for (const time of [40, 80, 120]) {
    assert.equal(reedPath(0.4, pitchCycles(display.update(null, time)), 0), original);
  }
  assert.equal(display.update(220, 140), 220);
  assert.equal(
    display.update(null, 300),
    null,
    'A long unvoiced interval releases the held spacing',
  );
});

test('single octave outliers are ignored but sustained real steps are accepted', () => {
  for (const interval of [20, 40, 55]) {
    const display = new PitchDisplay();
    assert.equal(display.update(150, 0), 150);
    assert.equal(display.update(300, interval), 150);
    assert.equal(display.update(150, interval * 2), 150);
    const start = interval * 3;
    assert.equal(display.update(300, start), 150);
    let result;
    let time = start;
    do {
      time += interval;
      result = display.update(300, time);
    } while (time - start < 60);
    assert.equal(result, 300);
    assert.ok(time - start <= 115, 'Confirmation is duration based, not tied to browser frames');
  }
});

test('normal pitch inflection and measured amplitude remain responsive', () => {
  const display = new PitchDisplay();
  display.update(220, 0);
  assert.equal(display.update(240, 40), 240);
  const pitch = display.update(null, 80);
  assert.notEqual(reedPath(0.15, pitchCycles(pitch), 0), reedPath(0.6, pitchCycles(pitch), 0));
  assert.equal(display.update(250, 120), 250);
});

test('invalid readings expire and recording/bridge resets cannot inherit pitch', () => {
  const display = new PitchDisplay();
  display.update(220, 0);
  for (const [i, value] of [NaN, Infinity, -1, 900].entries()) display.update(value, 40 * (i + 1));
  assert.equal(display.update(null, 200), null);
  assert.equal(display.update(330, 220), 330);
  display.reset();
  assert.equal(display.update(null, 230), null);
  assert.equal(display.update(110, 240), 110);
});
