import { expect, test } from '@playwright/test';

async function goToDictate(page) {
  await page.goto('/');
  await page
    .getByRole('navigation', { name: 'Application', exact: true })
    .getByRole('button', { name: 'Dictate', exact: true })
    .click();
}

test('Ember theme, single family, logo and icons apply with bundled fonts', async ({ page }) => {
  const remoteFonts = [];
  page.on('request', (request) => {
    if (/fonts\.(googleapis|gstatic)\.com/.test(request.url())) remoteFonts.push(request.url());
  });
  await goToDictate(page);

  const tokens = await page.evaluate(() => {
    const root = getComputedStyle(document.documentElement);
    const pick = (name) => root.getPropertyValue(name).trim();
    return {
      bg: pick('--color-bg'),
      card: pick('--color-raised'),
      accent: pick('--color-accent'),
      muted: pick('--color-muted'),
      body: getComputedStyle(document.body).fontFamily,
      h1: getComputedStyle(document.querySelector('.studio-main h1')).fontFamily,
      wordmark: getComputedStyle(document.querySelector('.ember-word')).fontFamily,
    };
  });
  expect(tokens.bg).toBe('#0a0a0a');
  expect(tokens.card).toBe('#151515');
  expect(tokens.accent).toBe('#ff6b3d');
  expect(tokens.muted).toBe('#9a9088');
  expect(tokens.body).toMatch(/Bricolage/);
  expect(tokens.h1).toMatch(/Bricolage/);
  expect(tokens.wordmark).toMatch(/Bricolage/);

  await expect(page.locator('.sidebar .brand-logo svg')).toBeVisible();
  await expect(page.locator('.sidebar .ember-word')).toHaveText('vesper');
  expect(await page.locator('.sidebar nav [data-icon="mic"] .nav-detail').count()).toBeGreaterThan(
    0,
  );

  const dictate = page
    .getByRole('navigation', { name: 'Application', exact: true })
    .getByRole('button', { name: 'Dictate', exact: true });
  await dictate.hover();
  await expect
    .poll(() =>
      dictate.evaluate(
        (button) =>
          [...button.querySelectorAll('.nav-detail')].flatMap((part) => part.getAnimations())
            .length,
      ),
    )
    .toBeGreaterThan(0);

  await page
    .getByRole('navigation', { name: 'Application', exact: true })
    .getByRole('button', { name: 'Settings', exact: true })
    .click();
  await page
    .getByRole('navigation', { name: 'Settings sections' })
    .getByRole('button', { name: 'Appearance', exact: true })
    .click();
  await page.getByRole('button', { name: 'Light', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  const light = await page.evaluate(() => {
    const root = getComputedStyle(document.documentElement);
    const pick = (name) => root.getPropertyValue(name).trim();
    return {
      bg: pick('--color-bg'),
      card: pick('--color-raised'),
      accent: pick('--color-accent'),
      muted: pick('--color-muted'),
    };
  });
  expect(light.bg).toBe('#fff8f2');
  expect(light.card).toBe('#ffffff');
  expect(light.accent).toBe('#e8501f');
  expect(light.muted).toBe('#85715f');

  await dictate.click();
  await expect(page.locator('.transcription-language .icon')).toBeVisible();
  await expect(page.locator('.recorder-heading [data-icon="mic"]')).toBeVisible();
  await expect(page.locator('.record-button [data-icon="record"]')).toBeVisible();
  const wave = page.locator('.ember-wave');
  await expect(wave).toBeVisible();
  expect(await wave.locator('i').count()).toBe(56);
  expect(
    await wave
      .locator('i')
      .first()
      .evaluate((bar) => bar.style.height),
  ).toBe('3px');
  expect(remoteFonts).toEqual([]);
});

test('Ember entrance motion rests under reduced motion', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await goToDictate(page);
  const rise = await page.evaluate(
    () => getComputedStyle(document.querySelector('.studio-recorder')).animationName,
  );
  expect(rise).toBe('none');
});
