import { expect, test } from '@playwright/test';

async function writing(page) {
  await page.goto('/tests/interaction-preview.html');
  await page
    .getByRole('navigation', { name: 'Application', exact: true })
    .getByRole('button', { name: 'Settings', exact: true })
    .click();
  await page
    .getByRole('navigation', { name: 'Settings sections' })
    .getByRole('button', { name: 'Writing', exact: true })
    .click();
}

test('project rules save, keep overrides, cancel pickers and fit a narrow screen', async ({
  page,
}) => {
  await writing(page);
  await page.locator('#project-mode').click();
  await page.getByRole('option', { name: 'Use application rules', exact: true }).click();
  await page.evaluate(() => {
    window.previewDialogCancelled = true;
  });
  await page.getByRole('button', { name: 'Add application rule', exact: true }).click();
  await expect(page.locator('.project-rule')).toHaveCount(0);
  await expect(
    page.getByRole('button', { name: 'Add application rule', exact: true }),
  ).toBeEnabled();
  await page.evaluate(() => {
    window.previewDialogCancelled = false;
  });
  await page.getByRole('button', { name: 'Add application rule', exact: true }).click();
  const project = page.getByRole('textbox', { name: 'Project for TextEdit', exact: true });
  await expect(project).toBeFocused();
  await project.fill('Project A');
  await page.getByRole('button', { name: 'Choose folder', exact: true }).click();
  await expect(page.locator('.project-folder')).toContainText('/Projects/Synthetic');
  await page.locator('#personal-project').fill('Manual override');
  await expect(page.locator('.save-status')).toHaveAttribute('data-state', 'saving');
  await expect(page.locator('.save-status')).toContainText(/Saved/, { timeout: 10_000 });
  await expect(page.locator('.save-status')).toHaveCount(0, { timeout: 10_000 });
  await expect(page.locator('.preferences-footer')).toBeHidden();
  await expect(project).toHaveValue('Project A');
  await expect(page.locator('#personal-project')).toHaveValue('Manual override');
  await page.setViewportSize({ width: 320, height: 720 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.getByRole('button', { name: 'Use entire app', exact: true }).click();
  await expect(page.locator('.project-folder')).toHaveCount(0);
  await page.getByRole('button', { name: 'Remove TextEdit project rule', exact: true }).click();
  await expect(page.locator('.project-rule')).toHaveCount(0);
});
