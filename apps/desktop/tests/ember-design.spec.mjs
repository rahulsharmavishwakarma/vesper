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
  expect(light.bg).toBe('#ffffff');
  expect(light.card).toBe('#ffffff');
  expect(light.accent).toBe('#e84d17');
  expect(light.muted).toBe('#6b6962');

  await dictate.click();
  await expect(page.locator('.dictate-device .field-label [data-icon="globe"]')).toBeVisible();
  await expect(page.locator('.dictate-device .field-label [data-icon="mic"]')).toBeVisible();
  await expect(page.locator('.recorder-heading [data-icon="mic"]')).toBeVisible();
  await expect(page.locator('.recording-timer')).toHaveText('00:00');
  await expect(page.locator('.record-button [data-icon="record"]')).toBeVisible();
  const wave = page.locator('.ember-wave');
  await expect(wave).toBeVisible();
  expect(await wave.locator('i').count()).toBe(44);
  expect(
    await wave
      .locator('i')
      .first()
      .evaluate((bar) => bar.style.height),
  ).toBe('4px');
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

test('Dictate field icons center with their label text', async ({ page }) => {
  await goToDictate(page);
  for (const target of ['dictate-language', 'microphone']) {
    const geometry = await page.evaluate((id) => {
      const label = document.querySelector(`label[for="${id}"]`);
      const icon = label.querySelector('svg');
      const labelBox = label.getBoundingClientRect();
      const iconBox = icon.getBoundingClientRect();
      return {
        display: getComputedStyle(label).display,
        delta: Math.abs(labelBox.top + labelBox.height / 2 - (iconBox.top + iconBox.height / 2)),
      };
    }, target);
    expect(geometry.display).toBe('flex');
    expect(geometry.delta).toBeLessThan(1);
  }
});

test('Sidebar nav answers a press with a deeper wash', async ({ page }) => {
  await goToDictate(page);
  const history = page
    .getByRole('navigation', { name: 'Application', exact: true })
    .getByRole('button', { name: 'History', exact: true });
  await history.hover();
  await expect
    .poll(() => history.evaluate((button) => getComputedStyle(button).backgroundColor))
    .toBe('rgb(21, 21, 21)');
  const box = await history.boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await expect
    .poll(() => history.evaluate((button) => getComputedStyle(button).backgroundColor))
    .toBe('rgb(39, 39, 39)');
  await page.mouse.up();
});

test('Autosave views never describe saving as a user action', async ({ page }) => {
  await goToDictate(page);
  const nav = page.getByRole('navigation', { name: 'Application', exact: true });
  for (const view of ['Dictate', 'Models', 'Settings']) {
    await nav.getByRole('button', { name: view, exact: true }).click();
    await expect(page.locator('.studio-main')).not.toContainText(/Save your (changes|preferences)/);
    await expect(page.locator('.studio-main')).not.toContainText(/unsaved changes/i);
  }
});
