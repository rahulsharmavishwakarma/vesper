import { expect, test } from '@playwright/test';

// Regression: a long paragraph must not leave the Polished editor clipped
// mid-line while the Heard pane grows full height (Refs #126).
test('long transcript keeps heard and polished panes in height sync', async ({
  page,
}, testInfo) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/tests/interaction-preview.html');
  await page
    .getByRole('navigation', { name: 'Application', exact: true })
    .getByRole('button', { name: 'History', exact: true })
    .click();
  await page.getByPlaceholder(/Search words or filenames/).fill('Sunlit paragraphs');
  await page
    .getByRole('button', { name: /Sunlit paragraphs stretch/ })
    .first()
    .click();
  const card = page.getByRole('article', { name: 'Recording transcript card' });
  await expect(card.getByText('Polished · edit to teach')).toBeVisible();
  const editor = card.getByLabel('Polished · edit to teach');
  await expect(editor).toBeVisible();

  const overflow = await editor.evaluate((el) => el.scrollHeight - el.clientHeight);
  expect(overflow, 'polished editor clips long text mid-line').toBeLessThanOrEqual(1);

  const gap = await card
    .locator('.cols')
    .first()
    .evaluate((cols) => {
      const panes = cols.querySelectorAll(':scope > .col > p, :scope > .col > textarea');
      if (panes.length !== 2) return -1;
      const heights = [...panes].map((el) => el.getBoundingClientRect().height);
      return Math.abs(heights[0] - heights[1]);
    });
  expect(gap, 'heard/polished pane height gap').toBeLessThanOrEqual(48);

  await card
    .screenshot({ animations: 'disabled' })
    .then((body) => testInfo.attach('pane-sync-long', { body, contentType: 'image/png' }));
  expect(errors, 'Uncaught browser errors').toEqual([]);
});
