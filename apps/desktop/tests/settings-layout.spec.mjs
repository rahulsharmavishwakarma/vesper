import { expect, test } from '@playwright/test';

const section = (page, name) =>
  page
    .getByRole('navigation', { name: 'Settings sections' })
    .getByRole('button', { name, exact: true });
async function openSettings(page, path = '/tests/interaction-preview.html') {
  await page.goto(path);
  await page
    .getByRole('navigation', { name: 'Application', exact: true })
    .getByRole('button', { name: 'Settings', exact: true })
    .click();
}

test('Settings sections preserve independent application and vocabulary drafts', async ({
  page,
}) => {
  await openSettings(page);
  await expect(section(page, 'Recording')).toHaveAttribute('aria-current', 'page');
  const saved = await page.locator('#collect-training').isChecked();
  await page.locator('#collect-training').setChecked(!saved);
  await section(page, 'Writing').click();
  await page.locator('#personal-project').fill('Synthetic writing project');
  await section(page, 'Shortcuts').click();
  await expect(page.locator('#hold-shortcut')).toBeVisible();
  await section(page, 'Recording').click();
  await expect(page.locator('#collect-training')).toBeChecked({ checked: !saved });
  await page.locator('.savebar').getByRole('button', { name: 'Discard changes' }).click();
  await expect(page.locator('#collect-training')).toBeChecked({ checked: saved });
  await section(page, 'Writing').click();
  await expect(page.locator('#personal-project')).toHaveValue('Synthetic writing project');
  await page
    .locator('.personalization')
    .getByRole('button', { name: 'Save preferences', exact: true })
    .click();
  await expect(page.locator('.personalization')).toContainText('Saved for your next recording.');
  await expect(
    page.locator('.savebar').getByRole('button', { name: 'Save changes', exact: true }),
  ).toBeDisabled();
  await section(page, 'Appearance').click();
  await page.locator('#floating-bar').uncheck();
  await page.getByRole('button', { name: 'Save changes', exact: true }).click();
  await section(page, 'Recording').click();
  await section(page, 'Appearance').click();
  await expect(page.locator('#floating-bar')).not.toBeChecked();
  await section(page, 'About').click();
  await expect(page.locator('.about-inline .version')).toContainText('0.1.0');
  await page.getByRole('button', { name: 'Copy app info', exact: true }).click();
  await expect.poll(() => page.evaluate(() => window.previewClipboard)).toContain('Version: 0.1.0');
});

test('Models view offers no Model folder override', async ({ page }) => {
  await openSettings(page);
  await page
    .getByRole('navigation', { name: 'Application', exact: true })
    .getByRole('button', { name: 'Models', exact: true })
    .click();
  await expect(page.getByRole('heading', { name: 'Device and storage' })).toBeVisible();
  await expect(page.getByLabel('Model folder')).toHaveCount(0);
  await expect(page.locator('#model-directory')).toHaveCount(0);
});

test('App sidebar collapses to an icon rail on Settings only', async ({ page }) => {
  await openSettings(page);
  await expect(page.locator('.app-shell')).toHaveClass(/settings-rail/);
  await expect(page.locator('.brand svg')).toHaveAttribute('viewBox', '0 0 80 60');
  await expect(page.locator('.brand .ember-word')).toHaveCount(0);
  const nav = page.getByRole('navigation', { name: 'Application', exact: true });
  await expect(nav.getByRole('button', { name: 'Dictate', exact: true })).toHaveAttribute(
    'title',
    'Dictate',
  );
  await nav.getByRole('button', { name: 'Dictate', exact: true }).click();
  await expect(page.locator('.app-shell')).not.toHaveClass(/settings-rail/);
});

test('Appearance persists, follows system changes and recovers from unavailable storage', async ({
  page,
}) => {
  await openSettings(page, '/');
  await section(page, 'Appearance').click();
  await page.getByRole('button', { name: 'Light', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page
    .getByRole('navigation', { name: 'Application', exact: true })
    .getByRole('button', { name: 'Settings', exact: true })
    .click();
  await section(page, 'Appearance').click();
  await page.getByRole('button', { name: 'System', exact: true }).click();
  await page.emulateMedia({ colorScheme: 'dark', reducedMotion: 'reduce' });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await expect(page.locator('.appearance-motion')).toContainText('On · follows system');
  await page.emulateMedia({ colorScheme: 'light' });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.evaluate(() => {
    const original = Storage.prototype.setItem;
    Storage.prototype.setItem = function (key, value) {
      if (key === 'vesper-appearance-v1') throw new Error('Synthetic storage denial');
      return original.call(this, key, value);
    };
  });
  await page.getByRole('button', { name: 'Dark', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('Could not save appearance');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(page.getByRole('button', { name: 'System', exact: true })).toHaveAttribute(
    'aria-pressed',
    'true',
  );
});

test('All five Settings sections fit narrow and desktop layouts in both themes', async ({
  page,
}) => {
  await openSettings(page);
  await page.emulateMedia({ reducedMotion: 'reduce' });
  for (const theme of ['Light', 'Dark']) {
    await section(page, 'Appearance').click();
    await page.getByRole('button', { name: theme, exact: true }).click();
    for (const width of [320, 360, 375, 414, 768, 1280]) {
      await page.setViewportSize({ width, height: width === 360 ? 480 : 900 });
      for (const name of ['Recording', 'Writing', 'Shortcuts', 'Appearance', 'About']) {
        await section(page, name).click();
        await expect(section(page, name)).toHaveAttribute('aria-current', 'page');
        const layout = await page.evaluate(() => ({
          overflow: document.documentElement.scrollWidth > innerWidth,
          clipped: [
            ...document.querySelectorAll(
              '.settings-content button, .settings-content input, .settings-content textarea, .settings-nav button',
            ),
          ]
            .filter((e) => e.checkVisibility())
            .filter((e) => {
              const r = e.getBoundingClientRect();
              return r.left < -1 || r.right > innerWidth + 1;
            })
            .map((e) => e.id || e.textContent.trim()),
          underline: getComputedStyle(
            document.querySelector('.settings-nav [aria-current="page"]'),
            '::after',
          ).content,
        }));
        expect(layout, `${theme} ${name} at ${width}`).toEqual({
          overflow: false,
          clipped: [],
          underline: 'none',
        });
      }
    }
  }
});
