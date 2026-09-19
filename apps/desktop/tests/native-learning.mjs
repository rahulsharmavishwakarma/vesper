// Test-only page: production panel and real native runtime, public PCM in place of capture.
import { mount, unmount, tick } from 'svelte';
import { invoke, isTauri } from '@tauri-apps/api/core';
import TranscriptionPanel from '../src/lib/components/TranscriptionPanel.svelte';
import '../src/app.css';

const target = document.querySelector('#probe-panel');
const report = document.querySelector('#report');
let component;
const delay = () => new Promise((resolve) => setTimeout(resolve, 5));
async function until(test, message) {
  const deadline = performance.now() + 120_000;
  while (!test()) {
    if (performance.now() >= deadline) throw new Error(message);
    await delay();
  }
}
const frame = () =>
  new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      cancelAnimationFrame(request);
      reject(
        new Error(
          `No rendering opportunity within two seconds (visibility=${document.visibilityState}, focused=${document.hasFocus()})`,
        ),
      );
    }, 2000);
    const request = requestAnimationFrame(() => {
      clearTimeout(timer);
      resolve();
    });
  });

async function replay(descriptor) {
  let completed = null;
  let previousResultId = null;
  let runtime = null;
  let busy = true;
  let hidden = document.visibilityState !== 'visible';
  const visibility = () => {
    hidden ||= document.visibilityState !== 'visible';
  };
  document.addEventListener('visibilitychange', visibility);
  const metrics = {
    operationId: descriptor.operationId,
    resultId: null,
    stopToDomMs: null,
    stopToRenderOpportunityMs: null,
    editSubmitMs: null,
    visible: false,
    renderedText: null,
    error: null,
  };
  try {
    component = mount(TranscriptionPanel, {
      target,
      props: {
        environment: 'desktop',
        view: 'dictate',
        onNavigate: () => {},
        settingsLoaded: true,
        settingsDirty: false,
        settingsBusy: false,
        settingsRevision: descriptor.operationId,
        cleanupEnabled: true,
        modelSettings: descriptor.config,
        maxRecordingSeconds: 0,
        onBusyChange: (value) => {
          busy = value;
        },
        onRuntimeChange: (value) => {
          runtime = value;
        },
        onDownloadState: () => {},
        onResult: (value) => {
          if (value.resultId !== previousResultId) completed = value;
        },
      },
    });
    const button = () => target.querySelector('.record-button');
    await until(() => button() && !button().disabled, 'Recorder did not become ready');
    previousResultId = completed?.resultId ?? null;
    completed = null;
    button().click();
    await until(() => runtime?.stage === 'recording' && !button().disabled, 'Replay did not start');
    const start = performance.now();
    button().click();
    await until(() => (completed && !busy) || runtime?.stage === 'error', 'No final result');
    if (!completed || runtime?.stage === 'error')
      throw new Error(runtime?.message || 'No final result');
    await tick();
    const field = target.querySelector('#final-draft');
    const expected =
      completed.draftTranscript ??
      completed.personalizedTranscript ??
      completed.cleanedTranscript ??
      completed.rawTranscript;
    if (!field || field.disabled || field.value !== expected)
      throw new Error('Final DOM does not match the completed operation');
    metrics.stopToDomMs = performance.now() - start;
    // Two frame callbacks allow a rendering opportunity; this is not a pixel-present timestamp.
    field.scrollIntoView({ block: 'nearest', behavior: 'instant' });
    await frame();
    await frame();
    const bounds = field.getBoundingClientRect();
    metrics.visible =
      !hidden &&
      bounds.width > 0 &&
      bounds.height > 0 &&
      bounds.top >= 0 &&
      bounds.bottom <= innerHeight;
    if (!metrics.visible) throw new Error('Result window/field was hidden or outside the viewport');
    metrics.renderedText = field.value;
    metrics.resultId = completed.resultId;
    metrics.stopToRenderOpportunityMs = performance.now() - start;
    if (descriptor.feedback !== null && descriptor.feedback !== field.value) {
      const revision = completed.draftRevision ?? 0;
      field.focus();
      const editStart = performance.now();
      field.value = descriptor.feedback;
      field.dispatchEvent(new Event('input', { bubbles: true }));
      await tick();
      field.blur();
      await until(
        () => (completed.draftRevision ?? 0) > revision || target.querySelector('.error'),
        'Feedback did not save',
      );
      if ((completed.draftRevision ?? 0) <= revision) throw new Error('Scripted correction failed');
      metrics.editSubmitMs = performance.now() - editStart;
    }
  } catch (error) {
    metrics.error = String(error);
  } finally {
    document.removeEventListener('visibilitychange', visibility);
    if (component) await unmount(component);
    component = null;
  }
  await invoke('finish_case', { metrics });
}

async function run() {
  if (!isTauri()) {
    report.textContent =
      'Run the learning_webview native example. Browser preview does not execute inference.';
    return;
  }
  try {
    // A hidden/occluded host cannot measure presentation; fail before inference.
    await frame();
    await frame();
    if (document.visibilityState !== 'visible') throw new Error('Replay page is not visible');
    for (;;) {
      const descriptor = await invoke('prepare_case');
      if (!descriptor) break;
      report.textContent = `${descriptor.sampleId} · ${descriptor.arm === 0 ? 'frozen memory' : 'automatic learning'} · microphone off`;
      await replay(descriptor);
    }
    await invoke('finish_probe', { error: null });
  } catch (error) {
    report.textContent = String(error);
    await invoke('finish_probe', { error: String(error) });
  }
}
void run();
