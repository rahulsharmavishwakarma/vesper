import { expect, test } from '@playwright/test';

async function writing(page, name = 'Projects') {
  await page.goto('/tests/interaction-preview.html');
  await page
    .getByRole('navigation', { name: 'Application', exact: true })
    .getByRole('button', { name: 'Settings', exact: true })
    .click();
  await page
    .getByRole('navigation', { name: 'Settings sections' })
    .getByRole('button', { name, exact: true })
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

test('automatic workspaces rename, merge and remove', async ({ page }) => {
  // Reading other apps' text is opt-in, under Vocabulary → Learning.
  await writing(page, 'Vocabulary');
  await expect(
    page.getByRole('checkbox', { name: 'Use words already in the field' }),
  ).not.toBeChecked();
  await expect(
    page.getByRole('checkbox', { name: 'Learn from fixes in other apps' }),
  ).not.toBeChecked();
  await page
    .getByRole('navigation', { name: 'Settings sections' })
    .getByRole('button', { name: 'Projects', exact: true })
    .click();
  const toggle = page.getByRole('checkbox', { name: 'Detect workspaces automatically' });
  await expect(toggle).toBeChecked();
  const rows = page.locator('.workspace');
  await expect(rows).toHaveCount(2);
  await expect(rows.first()).toContainText('synthetic-repo');
  await expect(rows.first()).toContainText('Code · 12 dictations');
  await expect(rows.nth(1)).toContainText('Website · 6 dictations');
  // Rename: Escape cancels and returns focus; Enter saves.
  await page.getByRole('button', { name: 'Rename', exact: true }).first().click();
  const field = page.getByRole('textbox', { name: 'New name for synthetic-repo' });
  await expect(field).toBeFocused();
  await field.fill('Other name');
  await field.press('Escape');
  await expect(rows.first()).toContainText('synthetic-repo');
  await expect(page.getByRole('button', { name: 'Rename', exact: true }).first()).toBeFocused();
  await page.getByRole('button', { name: 'Rename', exact: true }).first().click();
  await field.fill('docs.example.org');
  await field.press('Enter');
  await expect(page.locator('.workspace .invalid')).toContainText('Use Merge instead');
  await field.fill('Synthetic app');
  await field.press('Enter');
  await expect(rows.first()).toContainText('Synthetic app');
  // There is no folder picker: Vesper never asks the user to choose folders.
  await expect(page.getByRole('button', { name: /Learn words/ })).toHaveCount(0);
  // Merge the website into the repo.
  await rows.nth(1).getByRole('button', { name: 'Merge…' }).click();
  await rows.nth(1).getByRole('button', { name: 'Merge', exact: true }).click();
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toContainText('18 dictations');
  await page.setViewportSize({ width: 320, height: 720 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.getByRole('button', { name: 'Remove workspace Synthetic app' }).click();
  await expect(rows).toHaveCount(0);
  await expect(page.getByText('None yet.', { exact: false })).toBeVisible();
  await toggle.uncheck();
  await expect(page.locator('.save-status')).toContainText(/Saved/, { timeout: 10_000 });
});
