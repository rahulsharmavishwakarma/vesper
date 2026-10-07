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

test('Settings sections autosave application and vocabulary drafts', async ({ page }) => {
  await openSettings(page);
  await expect(section(page, 'Recording')).toHaveAttribute('aria-current', 'page');
  const pill = page.locator('.save-status');
  const saves = (command) =>
    page.evaluate(
      (name) => window.previewCommands.filter((entry) => entry === name).length,
      command,
    );
  await expect(pill).toHaveCount(0);
  const saved = await page.locator('#collect-training').isChecked();
  await page.locator('#collect-training').setChecked(!saved);
  await expect(pill).toHaveAttribute('data-state', 'saving');
  await expect(pill).toContainText(/Saved \d/, { timeout: 10_000 });
  await expect.poll(() => saves('save_settings'), { timeout: 10_000 }).toBeGreaterThan(0);
  await expect(pill).toHaveCount(0, { timeout: 10_000 });
  await section(page, 'Writing').click();
  await page.locator('#personal-project').fill('Synthetic writing project');
  await expect.poll(() => saves('save_personalization'), { timeout: 10_000 }).toBeGreaterThan(0);
  await expect(pill).toContainText(/Saved \d/, { timeout: 10_000 });
  await expect(pill).toHaveCount(0, { timeout: 10_000 });
  await section(page, 'Shortcuts').click();
  await expect(page.locator('#hold-shortcut')).toBeVisible();
  await section(page, 'Recording').click();
  await expect(page.locator('#collect-training')).toBeChecked({ checked: !saved });
  await section(page, 'Writing').click();
  await expect(page.locator('#personal-project')).toHaveValue('Synthetic writing project');
  await expect(page.locator('.preferences-footer')).toBeHidden();
  await section(page, 'Appearance').click();
  await page.locator('#floating-bar').uncheck();
  await expect.poll(() => saves('save_settings'), { timeout: 10_000 }).toBeGreaterThan(1);
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

test('Denied paste access explains the restart requirement', async ({ page }) => {
  await openSettings(page);
  await section(page, 'Shortcuts').click();
  await page.getByRole('combobox', { name: 'After shortcut dictation' }).click();
  await page.getByRole('option', { name: 'Paste into the original text field' }).click();
  const state = page.locator('.permission-state');
  await expect(state).toContainText('Allow access to paste');
  await expect(state).toContainText('quit and reopen Vesper');
  await expect(
    state.getByRole('button', { name: 'Allow access' }).locator('[data-icon="check"]'),
  ).toBeVisible();
  await expect(
    state.getByRole('button', { name: 'Check access' }).locator('[data-icon="refresh"]'),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Check access' }).click();
  await expect(state).toContainText('quit and reopen Vesper');
  await expect
    .poll(() =>
      page.evaluate(
        () => window.previewCommands.filter((entry) => entry === 'dictation_permissions').length,
      ),
    )
    .toBeGreaterThan(0);
});
