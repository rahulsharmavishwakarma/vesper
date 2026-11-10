import { expect, test } from '@playwright/test';

async function openPendingCard(page) {
  await page.goto('/tests/interaction-preview.html');
  await page
    .getByRole('navigation', { name: 'Application', exact: true })
    .getByRole('button', { name: 'History', exact: true })
    .click();
  await page
    .getByRole('button', { name: /The waveform should follow my voice/ })
    .first()
    .click();
  const card = page.getByRole('article', { name: 'Recording transcript card' });
  await expect(card.getByText('Is the polished text right?')).toBeVisible();
  return card;
}

test('transcript card votes, correction learning, undo and details', async ({ page }, testInfo) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  const card = await openPendingCard(page);

  await expect(card.getByText('Heard', { exact: true })).toBeVisible();
  await expect(card.getByText('Polished · edit to teach')).toBeVisible();
  await expect(card.getByRole('button', { name: 'Looks right', exact: true })).toBeVisible();
  await expect(card.getByRole('button', { name: 'Needs a fix', exact: true })).toBeVisible();
  await card
    .screenshot({ animations: 'disabled' })
    .then((body) => testInfo.attach('card-needs-review', { body, contentType: 'image/png' }));

  await card.getByRole('button', { name: 'Looks right', exact: true }).click();
  await expect(card.getByText('Saved as a verified label')).toBeVisible();
  await expect(card.getByText(/Verified label/)).toBeVisible();
  await expect(card.getByRole('button', { name: 'Looks right', exact: true })).toBeHidden();

  await card.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(card.getByText('Review undone')).toBeVisible();
  await expect(card.getByText('Is the polished text right?')).toBeVisible();

  await card.getByRole('button', { name: 'Needs a fix', exact: true }).click();
  await expect(card.getByText('Fix the wording, then save.')).toBeVisible();
  const editor = card.getByLabel('Polished · edit to teach');
  await expect(editor).toBeVisible();
  await editor.fill('The waveform should follow my voice, twice as nice.');
  await card.getByRole('button', { name: 'Save correction', exact: true }).click();
  await expect(
    card.getByText('Learned 1 spelling correction from the preview fixture.'),
  ).toBeVisible();
  await expect(card.getByText(/Learned from your edit/)).toBeVisible();
  await card
    .screenshot({ animations: 'disabled' })
    .then((body) => testInfo.attach('card-learned', { body, contentType: 'image/png' }));

  await card.getByRole('button', { name: 'Details', exact: true }).click();
  await expect(card.getByText('Original file', { exact: true })).toBeVisible();
  await expect(card.getByText('Archived file', { exact: true })).toBeVisible();
  await expect(card.getByText('Project', { exact: true })).toBeVisible();

  await card.getByRole('button', { name: 'Play recording', exact: true }).click();
  await expect(card.getByRole('button', { name: 'Pause recording', exact: true })).toBeVisible({
    timeout: 10000,
  });

  expect(errors, 'Uncaught browser errors').toEqual([]);
});

// #394: tapping a heard word stays a pointer shortcut for seeking, while the
// labelled slider is the keyboard and screen-reader seek control. Words are
// plain text in the accessibility tree, not a Tab stop each.
test('heard words seek by pointer and the slider seeks by keyboard', async ({ page }) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  const card = await openPendingCard(page);
  const slider = card.getByRole('slider', { name: 'Seek recording' });
  await expect(slider).toHaveAttribute('aria-valuenow', '0');

  const words = card.locator('.raw .w');
  const count = await words.count();
  expect(count).toBeGreaterThan(4);
  await expect(words.first()).toHaveAttribute('role', 'presentation');
  await expect(words.first()).not.toHaveAttribute('tabindex', /.*/);
  await words.nth(Math.floor(count * 0.75)).click();
  await expect
    .poll(async () => Number(await slider.getAttribute('aria-valuenow')))
    .toBeGreaterThan(0);

  // The slider takes focus and keys move it: Home returns from mid-track to the start.
  await slider.focus();
  await expect(slider).toBeFocused();
  await page.keyboard.press('Home');
  await expect(slider).toHaveAttribute('aria-valuenow', '0');
  expect(errors).toEqual([]);
});
