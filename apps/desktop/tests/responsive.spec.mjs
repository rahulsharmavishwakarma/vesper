import { expect, test } from '@playwright/test';

test.use({ reducedMotion: 'reduce', actionTimeout: 10_000 });

async function navigate(page, name) {
  await page
    .getByRole('navigation', { name: 'Application', exact: true })
    .getByRole('button', { name, exact: true })
    .click();
}

async function expectInsideWindow(page, locator) {
  const rect = await locator.boundingBox();
  const viewport = page.viewportSize();
  expect(rect).not.toBeNull();
  expect(rect.x).toBeGreaterThanOrEqual(0);
  expect(rect.y).toBeGreaterThanOrEqual(0);
  expect(rect.x + rect.width).toBeLessThanOrEqual(viewport.width + 1);
  expect(rect.y + rect.height).toBeLessThanOrEqual(viewport.height + 1);
}

test('minimum window shows Settings controls and preserves drafts, menus and dialogs during resize', async ({
  page,
}) => {
  await page.setViewportSize({ width: 360, height: 480 });
  await page.goto('/tests/interaction-preview.html');
  await navigate(page, 'Models');
  await page.locator('#models-asr-model').scrollIntoViewIfNeeded();
  await page.evaluate(
    () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))),
  );
  await page.locator('#models-asr-model').click();
  await page.getByRole('option', { name: /NVIDIA Nemotron 3\.5/ }).click();
  await page.getByRole('button', { name: 'Save changes', exact: true }).click();
  await expect(page.locator('.form-feedback')).toContainText('Settings saved on this device.');
  await navigate(page, 'Settings');

  const summary = page.locator('.page-model-summary');
  await expect(summary).toContainText('NVIDIA Nemotron 3.5 ASR Streaming 0.6B');
  await expect(summary).toContainText('Qwen3.5 0.8B');
  await expectInsideWindow(page, page.locator('#settings-language'));
  await page.locator('#settings-language').click();
  await expectInsideWindow(page, page.getByRole('listbox'));
  await page.setViewportSize({ width: 701, height: 480 });
  await expect(page.getByRole('listbox')).toHaveCount(0);

  const collection = page.locator('#collect-training');
  const original = await collection.isChecked();
  await collection.setChecked(!original);
  await page.setViewportSize({ width: 1180, height: 840 });
  await expect(collection).toBeChecked({ checked: !original });
  await page.setViewportSize({ width: 360, height: 480 });
  await expect(collection).toBeChecked({ checked: !original });
  const save = page.getByRole('button', { name: 'Save changes', exact: true });
  await expect(save).toBeEnabled();
  await save.scrollIntoViewIfNeeded();
  await expectInsideWindow(page, save);
  await save.click();
  await expect(page.locator('.form-feedback')).toContainText('Settings saved on this device.');

  const about = page.locator('.settings-about-row');
  await about.scrollIntoViewIfNeeded();
  await expectInsideWindow(page, about);
  await about.click();
  await expect(page.locator('.about-sheet .version')).toContainText('0.1.0');
  for (const width of [700, 320, 360]) {
    await page.setViewportSize({ width, height: 480 });
    await expectInsideWindow(page, page.getByRole('dialog'));
    await expectInsideWindow(page, page.getByRole('button', { name: 'Close About' }));
  }
  await page.keyboard.press('Escape');
  await expect(about).toBeFocused();
});

test('pages and expanded History keep controls reachable across narrow and short windows', async ({
  page,
}) => {
  await page.goto('/tests/interaction-preview.html');
  for (const [width, height] of [
    [320, 480],
    [360, 480],
    [414, 640],
    [700, 480],
    [701, 480],
    [768, 480],
    [1180, 480],
    [1280, 900],
  ]) {
    await page.setViewportSize({ width, height });
    for (const view of ['Dictate', 'History', 'Models', 'Settings']) {
      await navigate(page, view);
      if (view === 'History') {
        const recording = page.locator('.dataset-panel[data-view="full"] .session-button').first();
        if ((await recording.getAttribute('aria-expanded')) === 'false') await recording.click();
      }
      const clipped = await page.evaluate(() =>
        [...document.querySelectorAll('button, input, textarea, select, summary')]
          .filter((element) => {
            const rect = element.getBoundingClientRect();
            return rect.width > 0 && (rect.left < -1 || rect.right > innerWidth + 1);
          })
          .map((element) => element.id || element.textContent.trim()),
      );
      expect(clipped, `${view} controls at ${width}×${height}`).toEqual([]);
      const lastControl = page.locator('main button:visible').last();
      await lastControl.scrollIntoViewIfNeeded();
      await expectInsideWindow(page, lastControl);
    }
  }
});
