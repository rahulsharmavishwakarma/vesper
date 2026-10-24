import { test, expect } from '@playwright/test';

test('first-run flow UI', async ({ page }) => {
  await page.goto('http://localhost:3000/tests/interaction-preview.html?first-run');
  await page.waitForTimeout(1000);
});
