import { expect, test } from '@playwright/test';

const commands = (page, name) =>
  page.evaluate(
    (command) => window.previewCommands.filter((entry) => entry === command).length,
    name,
  );

test('a finished or existing profile never sees setup', async ({ page }) => {
  await page.goto('/tests/interaction-preview.html');
  await expect(page.getByRole('navigation', { name: 'Application', exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Welcome to Vesper' })).toHaveCount(0);
  // The flow is still off for users, so the app does not even ask.
  expect(await commands(page, 'get_onboarding')).toBe(0);
});

test('setup welcomes a new profile, saves progress and finishes once', async ({ page }) => {
  await page.goto('/tests/interaction-preview.html?first-run');
  await expect(page.getByRole('heading', { name: 'Welcome to Vesper' })).toBeVisible();
  const start = page.getByRole('button', { name: 'Get started' });
  await expect(start).toHaveCSS('opacity', '1', { timeout: 5000 });
  await start.click();
  await expect(page.getByRole('heading', { name: 'What will you speak?' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Welcome to Vesper' })).toHaveCount(0, {
    timeout: 3000,
  });
  await expect(page.getByRole('button', { name: 'Go back to Welcome' })).toBeVisible();
  await expect.poll(() => commands(page, 'save_onboarding_step')).toBeGreaterThan(0);
  for (const title of [
    'Models for this Mac.',
    'Let Vesper type for you.',
    'Your shortcut.',
    'Make it yours.',
    'Say something.',
  ]) {
    await page.getByRole('button', { name: 'Continue' }).click();
    await expect(page.getByRole('heading', { name: title })).toBeVisible();
  }
  await page.getByRole('button', { name: 'Back', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Make it yours.' })).toBeVisible();
  await page.getByRole('button', { name: 'Continue' }).click();
  await page.getByRole('button', { name: 'Continue' }).click();
  await expect(page.getByRole('heading', { name: 'Vesper is ready.' })).toBeVisible();
  await page.getByRole('button', { name: 'Start dictating' }).click();
  await expect(page.getByRole('heading', { name: 'Vesper is ready.' })).toHaveCount(0);
  await expect(page.getByRole('navigation', { name: 'Application', exact: true })).toBeVisible();
  expect(await commands(page, 'complete_onboarding')).toBe(1);
});

test('setup resumes where it stopped, without replaying Welcome', async ({ page }) => {
  await page.goto('/tests/interaction-preview.html?first-run&first-run-step=3');
  await expect(page.getByRole('heading', { name: 'Let Vesper type for you.' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Welcome to Vesper' })).toHaveCount(0);
  await expect(page.getByText('Step 4 of 7')).toBeVisible();
});

test('reduced motion shows Welcome at once and changes step without the flight', async ({
  browser,
}) => {
  const context = await browser.newContext({ reducedMotion: 'reduce' });
  const page = await context.newPage();
  await page.goto('/tests/interaction-preview.html?first-run');
  await expect(page.getByRole('button', { name: 'Get started' })).toHaveCSS('opacity', '1');
  await page.getByRole('button', { name: 'Get started' }).click();
  await expect(page.getByRole('heading', { name: 'What will you speak?' })).toBeVisible();
  await expect(page.locator('.fr-flyer')).toHaveCount(0);
  await context.close();
});
