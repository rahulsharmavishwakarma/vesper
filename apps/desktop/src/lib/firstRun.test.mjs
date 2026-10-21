import assert from 'node:assert/strict';
import test from 'node:test';
import {
  FIRST_RUN_ENABLED,
  FIRST_RUN_STEPS,
  READY_STEP,
  firstRunRequested,
  resumeStep,
  shouldShowFirstRun,
} from './firstRun.ts';

test('setup shows only for a pending profile', () => {
  assert.equal(shouldShowFirstRun({ status: 'pending' }, true), true);
  assert.equal(shouldShowFirstRun({ status: 'completed' }, true), false);
  // A completed profile never sees it again, even when a preview asks.
  assert.equal(shouldShowFirstRun({ status: 'completed' }, false, true), false);
});

test('setup stays off for real users until it is enabled', () => {
  assert.equal(FIRST_RUN_ENABLED, false);
  assert.equal(shouldShowFirstRun({ status: 'pending' }), false);
  assert.equal(shouldShowFirstRun({ status: 'pending' }, false, true), true);
});

test('a pending flow resumes at a valid saved step', () => {
  assert.equal(FIRST_RUN_STEPS.length, 7);
  assert.equal(READY_STEP, 7);
  assert.equal(resumeStep(3), 3);
  assert.equal(resumeStep(READY_STEP), READY_STEP);
  for (const bad of [-1, 8, 2.5, '3', null, undefined]) assert.equal(resumeStep(bad), 0);
});

test('previews open setup with ?first-run', () => {
  assert.equal(firstRunRequested('?first-run'), true);
  assert.equal(firstRunRequested('?first-run=1&x=2'), true);
  assert.equal(firstRunRequested('?other'), false);
});
