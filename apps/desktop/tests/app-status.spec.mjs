import { expect, test } from '@playwright/test';

// Records every text a selector ever showed, from the first paint on.
const watch = (page, selector) =>
  page.addInitScript((target) => {
    window.seenTexts = new Set();
    const record = () =>
      document.querySelectorAll(target).forEach((element) => {
        const text = element.textContent?.trim();
        if (text) window.seenTexts.add(text);
      });
    new MutationObserver(record).observe(document, {
      subtree: true,
      childList: true,
      characterData: true,
    });
  }, selector);

test('startup does not flash loading text when settings arrive quickly', async ({ page }) => {
  await watch(page, '.recorder-label, .configured-models dd, .loading');
  await page.goto('/tests/interaction-preview.html');
  await expect(page.locator('.recorder-label')).toContainText('Ready to record');
  const seen = await page.evaluate(() => [...window.seenTexts]);
  for (const text of [
    'Loading preferences…',
    'Loading your settings…',
    'Checking your microphone…',
  ])
    expect(seen).not.toContain(text);
});

test('verifying one model leaves the other model rows on their status', async ({ page }) => {
  await page.goto('/tests/interaction-preview.html');
  await page
    .getByRole('navigation', { name: 'Application', exact: true })
    .getByRole('button', { name: /Models/ })
    .click();
  const writer = page.getByLabel('Writing model download');
  await expect(writer.locator('.model-status')).not.toContainText('Checking');
  await writer.evaluate((row) => {
    window.writerTexts = new Set();
    new MutationObserver(() => window.writerTexts.add(row.textContent)).observe(row, {
      subtree: true,
      childList: true,
      characterData: true,
    });
  });
  await page
    .getByLabel('Speech model download')
    .getByRole('button', { name: /^Verify files/ })
    .click();
  await page.evaluate(() => window.previewFinishDownload());
  await expect(
    page.getByLabel('Speech model download').getByRole('button', { name: /^Verify files/ }),
  ).toBeEnabled();
  const texts = await page.evaluate(() => [...window.writerTexts].join(' | '));
  expect(texts).not.toContain('Checking files');
});

test('the sidebar shows a running download with its progress on every page', async ({ page }) => {
  await page.goto('/tests/interaction-preview.html');
  const nav = page.getByRole('navigation', { name: 'Application', exact: true });
  await nav.getByRole('button', { name: /Models/ }).click();
  await page
    .getByLabel('Speech model download')
    .getByRole('button', { name: /^Verify files/ })
    .click();
  const activity = page.locator('.sidebar .activity-link');
  await expect(activity).toContainText(/(Downloading|Verifying) /);
  await expect(activity).toContainText('25%');
  // Still visible from another page, and it leads back to Models.
  await nav.getByRole('button', { name: /History/ }).click();
  await expect(activity).toBeVisible();
  await activity.click();
  await expect(page.getByRole('heading', { level: 1 })).toContainText('Two models');
  await page.evaluate(() => window.previewFinishDownload());
  await expect(activity).toHaveCount(0);
});
