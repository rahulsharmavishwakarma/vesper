import { expect, test } from '@playwright/test';

test('existing UI regression suite passes with mocked native commands', async ({
  page,
}, testInfo) => {
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/tests/recovery.html');
  await page.waitForFunction(() =>
    ['passed', 'failed'].includes(document.documentElement.dataset.testStatus),
  );
  const results = await page.evaluate(() => window.regressionResults);
  await testInfo.attach('regression-results', {
    body: JSON.stringify(results, null, 2),
    contentType: 'application/json',
  });
  expect(results.length, 'The regression page must execute tests').toBeGreaterThan(0);
  expect(
    results.filter((result) => !result.passed),
    'Failed UI scenarios',
  ).toEqual([]);
  expect(errors, 'Uncaught browser errors').toEqual([]);
});
