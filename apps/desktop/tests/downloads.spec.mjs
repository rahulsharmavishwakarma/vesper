import { expect, test } from '@playwright/test';

const nav = (page, name) =>
  page
    .getByRole('navigation', { name: 'Application', exact: true })
    .getByRole('button', { name, exact: true });
const section = (page, name) =>
  page
    .getByRole('navigation', { name: 'Settings sections' })
    .getByRole('button', { name, exact: true });

async function startDownload(page) {
  await page.goto('/tests/interaction-preview.html');
  await nav(page, 'Models').click();
  await page
    .getByRole('button', { name: /Verify files/ })
    .first()
    .click();
  await expect(
    page.getByRole('progressbar', { name: 'Model download progress' }).first(),
  ).toBeVisible();
}

test('a model download locks only what conflicts with it', async ({ page }) => {
  await startDownload(page);
  // The speech model downloads: its choice and the device wait, and say so.
  // The writing model is a different download, so it stays free (#313).
  await expect(page.locator('#models-asr-model')).toBeDisabled();
  await expect(page.locator('#compute-backend')).toBeDisabled();
  await expect(
    page.getByText('The speech model and processing device unlock when this download finishes.', {
      exact: false,
    }),
  ).toBeVisible();
  const writer = page.locator('#models-cleanup-model');
  await expect(writer).toBeEnabled();
  await writer.click();
  await page.getByRole('button', { name: 'Any language' }).click();
  await page.getByRole('option', { name: /SmolLM2 360M/ }).click();
  await expect(writer).toContainText('SmolLM2 360M');
  await expect(page.locator('.save-status')).toContainText(/Saved \d/, { timeout: 10_000 });
  await expect(page.getByRole('alert')).toHaveCount(0);

  // Dictate: recording waits, choosing a language and microphone does not.
  await nav(page, 'Dictate').click();
  await expect(page.getByRole('button', { name: 'Record', exact: true })).toBeDisabled();
  await expect(page.getByRole('button', { name: /Transcribe audio file/ })).toBeDisabled();
  await expect(page.locator('#dictate-language')).toBeEnabled();
  await expect(
    page.getByText('A model is downloading. Recording is back when it finishes.'),
  ).toBeVisible();

  // History stays readable and usable.
  await nav(page, 'History').click();
  await expect(page.getByRole('button', { name: 'Refresh recordings' })).toBeEnabled();
  await expect(page.getByRole('button', { name: 'Import audio' })).toBeEnabled();
  await expect(page.locator('#history-recordings button').first()).toBeEnabled();

  // Settings that do not touch models save while the download runs.
  await nav(page, 'Settings').click();
  const pill = page.locator('.save-status');
  const saved = await page.locator('#collect-training').isChecked();
  await page.locator('#collect-training').setChecked(!saved);
  await expect(pill).toContainText(/Saved \d/, { timeout: 10_000 });
  await expect(page.locator('#collect-training')).toBeChecked({ checked: !saved });
  await expect(page.getByRole('alert')).toHaveCount(0);
  await section(page, 'Shortcuts').click();
  await expect(page.locator('#hold-shortcut')).toBeEnabled();
  await section(page, 'Appearance').click();
  await expect(page.locator('#floating-bar')).toBeEnabled();
  await section(page, 'Recording').click();
  await expect(page.locator('.settings-defaults button')).toBeDisabled();

  // Everything unlocks when the download finishes.
  await page.evaluate(() => window.previewFinishDownload());
  await expect(page.locator('.settings-defaults button')).toBeEnabled();
  await nav(page, 'Dictate').click();
  await expect(page.getByRole('button', { name: 'Record', exact: true })).toBeEnabled();
  await nav(page, 'Models').click();
  await expect(page.locator('#models-asr-model')).toBeEnabled();
});
