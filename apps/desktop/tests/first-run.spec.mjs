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

async function passVoice(page) {
  await page.getByRole('button', { name: 'Allow microphone' }).click();
  await expect(page.getByText('Heard you')).toBeVisible({ timeout: 8000 });
  await page.getByRole('button', { name: 'Continue' }).click();
}

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
  // Continue waits for the microphone, then for the model download to start.
  await expect(page.getByRole('button', { name: 'Continue' })).toBeDisabled();
  await passVoice(page);
  await expect(page.getByRole('heading', { name: 'Models for this Mac.' })).toBeVisible();
  await expect(page.getByText('Apple M3')).toBeVisible();
  await expect(page.getByText('Already on this Mac. Nothing to download.')).toBeVisible();
  await page.getByRole('button', { name: 'Download and set up' }).click();
  for (const title of [
    'Let Vesper type for you.',
    'Your shortcut.',
    'Make it yours.',
    'Say something.',
  ]) {
    await expect(page.getByRole('heading', { name: title })).toBeVisible();
    await page.getByRole('button', { name: 'Continue' }).click();
  }
  await expect(page.getByRole('heading', { name: 'Vesper is ready.' })).toBeVisible();
  await page.getByRole('button', { name: 'Start dictating' }).click();
  await expect(page.getByRole('heading', { name: 'Vesper is ready.' })).toHaveCount(0);
  await expect(page.getByRole('navigation', { name: 'Application', exact: true })).toBeVisible();
  expect(await commands(page, 'complete_onboarding')).toBe(1);
  expect(await commands(page, 'stop_microphone_check')).toBeGreaterThan(0);
});

test('models download in the background while setup carries on', async ({ page }) => {
  await page.goto('/tests/interaction-preview.html?first-run&first-run-step=2&first-run-download');
  await expect(page.getByRole('heading', { name: 'Models for this Mac.' })).toBeVisible();
  await expect(page.getByText('Download 4.3 GB')).toBeVisible();
  await expect(page.getByText(/Needs 5\.4 GB free/)).toBeVisible();
  await page.getByRole('button', { name: 'Download and set up' }).click();
  await expect(page.getByRole('heading', { name: 'Let Vesper type for you.' })).toBeVisible();
  await expect(page.getByText('Setting up models')).toBeVisible();
  await expect(page.getByRole('progressbar', { name: 'Model download' })).toBeVisible();
  await page.evaluate(() => window.previewFinishDownload());
  await page.waitForFunction(
    () => window.previewCommands.filter((c) => c === 'install_model').length >= 2,
  );
  await page.evaluate(() => window.previewFinishDownload());
  await expect(page.getByText('Models ready')).toBeVisible();
});

test('too little free space blocks the download with the reason', async ({ page }) => {
  await page.goto(
    '/tests/interaction-preview.html?first-run&first-run-step=2&first-run-download&first-run-disk=low',
  );
  await expect(page.getByText('Not enough free space.')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Download and set up' })).toBeDisabled();
  await expect(page.getByText('Free up space to continue.')).toBeVisible();
});

test('choosing Hindi switches to a speech model that can hear it', async ({ page }) => {
  await page.goto(
    '/tests/interaction-preview.html?first-run&first-run-step=1&first-run-mic=allowed',
  );
  await page.getByRole('radio', { name: 'Hindi' }).click();
  await expect(page.getByText('Hindi needs a different speech model.')).toBeVisible();
  await expect.poll(() => commands(page, 'save_settings')).toBeGreaterThan(0);
  await expect(page.getByText('Heard you')).toBeVisible({ timeout: 8000 });
  await page.getByRole('button', { name: 'Continue' }).click();
  await expect(page.locator('.first-run .row b').first()).toHaveText('Qwen3-ASR 0.6B');
});

test('a blocked microphone explains where to turn it on', async ({ page }) => {
  await page.goto(
    '/tests/interaction-preview.html?first-run&first-run-step=1&first-run-mic=denied',
  );
  await expect(page.getByText('Microphone is blocked.')).toBeVisible();
  await page.getByRole('button', { name: 'Open System Settings' }).click();
  expect(await commands(page, 'open_privacy_settings')).toBe(1);
  await expect(page.getByRole('button', { name: 'Continue' })).toBeDisabled();
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
