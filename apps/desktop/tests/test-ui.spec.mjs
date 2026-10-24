import { test, expect } from '@playwright/test';

test('check UI responsiveness', async ({ page }) => {
  await page.goto('http://localhost:3000/tests/interaction-preview.html');
  await page.waitForTimeout(1000);

  // Navigate to Models page
  const navLocators = await page.locator('nav button, nav a').all();
  for (const loc of navLocators) {
      const text = await loc.innerText();
      if(text.includes('Models')) {
          await loc.click();
          break;
      }
  }

  await page.waitForTimeout(1000);
  await page.screenshot({ path: '/tmp/models.png' });
});
