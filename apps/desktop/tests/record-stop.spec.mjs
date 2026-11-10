import { expect, test } from '@playwright/test';

// Regression for #393: Record -> Stop in the synthetic preview must render the
// result, as the app does, without a page error from the result panel.
test('Record then Stop renders the transcript without page errors', async ({ page }) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/tests/interaction-preview.html');
  await page
    .getByRole('navigation', { name: 'Application', exact: true })
    .getByRole('button', { name: 'Dictate', exact: true })
    .click();
  await page.getByRole('button', { name: 'Record', exact: true }).click();
  await page.getByRole('button', { name: 'Stop and transcribe', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Your latest transcription' })).toBeVisible();
  await expect(page.getByLabel('Your text')).toHaveValue(
    'Can you send the Arclight notes to Priya by Friday?',
  );
  expect(errors).toEqual([]);
});
