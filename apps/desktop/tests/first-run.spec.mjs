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
  // The app asks once and the finished profile stays out of setup.
  expect(await commands(page, 'get_onboarding')).toBe(1);
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
  // The meaning-checked writer leads; it is already installed here.
  await expect(page.locator('#setup-writer-model')).toContainText('Qwen3.5 0.8B');
  await expect(page.getByText('Best writing model for English')).toBeVisible();
  await expect(page.getByText('Already on this Mac. Nothing to download.')).toBeVisible();
  await page.getByRole('button', { name: 'Download and set up' }).click();
  // Access: typing into apps turns on once Accessibility is allowed.
  await expect(page.getByRole('heading', { name: 'Let Vesper type for you.' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Continue' })).toBeDisabled();
  await page.getByRole('button', { name: 'Open System Settings' }).click();
  await page.evaluate(() => {
    window.previewPermissions = {
      supported: true,
      accessibility: true,
      eventPosting: true,
      clipboard: true,
    };
  });
  await expect(page.locator('.first-run .pill.ok')).toHaveCount(2, { timeout: 5000 });
  await page.getByRole('button', { name: 'Continue' }).click();
  // Shortcut: the same recorder as Settings.
  await expect(page.getByRole('heading', { name: 'Your shortcut.' })).toBeVisible();
  await expect(page.locator('#setup-hold-shortcut')).toBeVisible();
  await page.getByRole('button', { name: 'Continue' }).click();
  // Your words: a taught word lands in the sample sentence and is saved.
  await expect(page.getByRole('heading', { name: 'Make it yours.' })).toBeVisible();
  await page.getByRole('button', { name: 'Try “Arclight”' }).click();
  await expect(page.locator('.first-run mark[data-word="Arclight"]')).toBeVisible();
  await expect.poll(() => commands(page, 'save_personalization')).toBeGreaterThan(0);
  await page.getByRole('radio', { name: /Polished/ }).click();
  await expect(page.locator('.first-run .written')).toHaveText(
    'Can you send the Arclight notes to Priya by Friday?',
  );
  await page.getByRole('button', { name: 'Continue' }).click();
  // Try it: a real dictation lands in the practice box, then Finish unlocks.
  await expect(page.getByRole('heading', { name: 'Say something.' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Finish' })).toBeDisabled();
  // Click to start: live words stream in while speaking; click again to stop.
  const record = page.locator('.first-run').getByRole('button', { name: 'Record', exact: true });
  await expect(record).toBeEnabled();
  await record.click();
  await expect(page.locator('.first-run').getByText('Listening…', { exact: true })).toBeVisible();
  await expect(page.locator('.first-run .live-words')).toContainText(
    'Can you send the Arclight notes',
  );
  await page.locator('.first-run').getByRole('button', { name: 'Stop and transcribe' }).click();
  await expect(page.getByLabel('Practice text')).toHaveValue(
    'Can you send the Arclight notes to Priya by Friday?',
  );
  await expect(page.getByText('It works')).toBeVisible();
  await page.getByRole('button', { name: 'Finish' }).click();
  await expect(page.getByRole('heading', { name: 'Vesper is ready.' })).toBeVisible();
  await expect(page.locator('.first-run .summary')).toContainText('Polished · Arclight');
  await expect(page.locator('.first-run .summary')).toContainText(/Typing into apps\s*On/);
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
  // Published figures pick the models; the total covers both downloads.
  await expect(page.locator('#setup-speech-model')).toContainText('Parakeet TDT 0.6B v3');
  await expect(page.locator('#setup-writer-model')).toContainText('Qwen3.5 0.8B');
  await expect(page.getByText('Download 4.3 GB')).toBeVisible();
  await expect(page.getByText(/Needs 5\.4 GB free/)).toBeVisible();
  await page.getByRole('button', { name: 'Download and set up' }).click();
  await expect(page.getByRole('heading', { name: 'Let Vesper type for you.' })).toBeVisible();
  await expect(page.getByText('Setting up models')).toBeVisible();
  await expect(
    page.getByRole('progressbar', { name: 'Model download', exact: true }),
  ).toBeVisible();
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
  await page.locator('#setup-language').click();
  await page.getByRole('option', { name: /^Hindi/ }).click();
  await expect(page.locator('#setup-language')).toContainText('Hindi');
  await expect.poll(() => commands(page, 'save_settings')).toBeGreaterThan(0);
  await expect(page.getByText('Heard you')).toBeVisible({ timeout: 8000 });
  await page.getByRole('button', { name: 'Continue' }).click();
  await expect(page.locator('#setup-speech-model')).toContainText('Nemotron');
  await expect(page.getByText('Best for Hindi', { exact: true })).toBeVisible();
  // The picker opens on Hindi, best first, with result rings on each row.
  await page.locator('#setup-speech-model').click();
  await expect(page.getByRole('button', { name: 'Hindi', pressed: true })).toBeVisible();
  const first = page.getByRole('option').first();
  await expect(first).toContainText('Nemotron');
  await expect(first.locator('.gauge')).toHaveCount(3);
  // Models that cannot hear Hindi are listed under Any language, locked with the reason.
  await page.getByRole('button', { name: 'Any language' }).click();
  const cohere = page.getByRole('option', { name: /Cohere Transcribe/ });
  await expect(cohere).toBeDisabled();
  await expect(cohere).toContainText('Doesn’t support Hindi');
});

test('setup offers every language its speech models declare', async ({ page }) => {
  await page.goto(
    '/tests/interaction-preview.html?first-run&first-run-step=1&first-run-mic=allowed',
  );
  await page.locator('#setup-language').click();
  for (const name of ['English', 'German', 'Japanese', 'Thai', 'Norwegian (Bokmål)'])
    await expect(
      page.getByRole('option', { name: new RegExp(`^${name.replace(/[()]/g, '\\$&')}`) }),
    ).toBeVisible();
  await expect(page.getByRole('option', { name: /^Detect automatically/ })).toHaveCount(0);
  await page.getByRole('option', { name: /^German/ }).click();
  await expect(page.locator('#setup-microphone')).toContainText('Synthetic microphone');
  // The recording waveform from Dictate, not a separate meter.
  await expect(page.locator('.first-run .meter .voice-indicator')).toBeVisible();
});

test('Back from Voice and Welcome in the rail return to the real Welcome', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto(
    '/tests/interaction-preview.html?first-run&first-run-step=1&first-run-mic=allowed',
  );
  await page.locator('.first-run').getByRole('button', { name: 'Back', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Welcome to Vesper' })).toBeVisible();
  await expect(page.getByText('Step 1 of 7')).toHaveCount(0);
  await page.getByRole('button', { name: 'Get started' }).click();
  await expect(page.getByRole('heading', { name: 'What will you speak?' })).toBeVisible();
  await page.getByRole('button', { name: 'Go back to Welcome' }).click();
  await expect(page.getByRole('heading', { name: 'Welcome to Vesper' })).toBeVisible();
  await expect(page.getByText('Step 1 of 7')).toHaveCount(0);
});

test('Try it also records while Record is held, like push-to-talk', async ({ page }) => {
  await page.goto(
    '/tests/interaction-preview.html?first-run&first-run-step=6&first-run-mic=allowed',
  );
  const record = page.locator('.first-run').getByRole('button', { name: 'Record', exact: true });
  const box = await record.boundingBox();
  await page.mouse.move(box.x + 20, box.y + 20);
  await page.mouse.down();
  await expect(
    page.locator('.first-run').getByRole('button', { name: 'Release to finish' }),
  ).toBeVisible();
  await page.waitForTimeout(450);
  await page.mouse.up();
  await expect(page.getByLabel('Practice text')).toHaveValue(
    'Can you send the Arclight notes to Priya by Friday?',
  );
  await expect(page.locator('.first-run').getByText('It works', { exact: true })).toBeVisible();
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

test('a person who cannot turn on Accessibility can keep text in Vesper', async ({ page }) => {
  await page.goto('/tests/interaction-preview.html?first-run&first-run-step=3');
  await page.getByRole('button', { name: 'Open System Settings' }).click();
  await expect(page.getByText('Still waiting?')).toBeVisible({ timeout: 12_000 });
  await page.getByRole('button', { name: 'Keep text in Vesper' }).click();
  await expect(page.getByText('Your words appear in Vesper, ready to copy.')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Continue' })).toBeEnabled();
});

test('a word already added is not added twice', async ({ page }) => {
  await page.goto('/tests/interaction-preview.html?first-run&first-run-step=5');
  await page.getByLabel('Word to add').fill('Priya');
  await page.getByLabel('Word to add').press('Enter');
  await page.getByLabel('Word to add').fill('priya');
  await page.getByLabel('Word to add').press('Enter');
  await expect(page.getByRole('alert')).toHaveText('“priya” is already in your words.');
  await expect(page.locator('.first-run .chip:not(.try)')).toHaveCount(1);
});

test('the practice waits for the models and resumes their download after a restart', async ({
  page,
}) => {
  await page.goto(
    '/tests/interaction-preview.html?first-run&first-run-step=6&first-run-models-missing',
  );
  await expect(page.getByText('Finishing your models')).toBeVisible();
  await expect(
    page.locator('.first-run').getByRole('button', { name: 'Record', exact: true }),
  ).toBeDisabled();
  await expect.poll(() => commands(page, 'install_model')).toBeGreaterThan(0);
  await page.evaluate(() => window.previewFinishDownload());
  await page.waitForFunction(
    () => window.previewCommands.filter((c) => c === 'install_model').length >= 2,
  );
  await page.evaluate(() => window.previewFinishDownload());
  await expect(
    page.locator('.first-run').getByRole('button', { name: 'Record', exact: true }),
  ).toBeEnabled();
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
