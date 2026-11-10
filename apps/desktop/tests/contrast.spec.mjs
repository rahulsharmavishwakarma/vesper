import { expect, test } from '@playwright/test';

// #376: in the Light theme, small text is never drawn in the raw ember accent
// (3.81:1 on white). Small accent-coloured text uses --color-accent-text,
// which clears 4.5:1 on every light surface. The accent stays for fills,
// selection and icons, which are not text.
async function smallAccentText(page) {
  return page.evaluate(() => {
    const probe = (value) => {
      const el = document.createElement('span');
      el.style.color = value;
      document.body.append(el);
      const color = getComputedStyle(el).color;
      el.remove();
      return color;
    };
    const ember = new Set([probe('var(--color-accent)'), probe('var(--color-accent-text)')]);
    const rgb = (c) => (c.match(/[\d.]+/g) ?? []).map(Number);
    const lum = (c) => {
      const f = (v) => {
        v /= 255;
        return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
      };
      const [r, g, b] = rgb(c);
      return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
    };
    const background = (el) => {
      for (let node = el; node; node = node.parentElement) {
        const bg = getComputedStyle(node).backgroundColor;
        const parts = rgb(bg);
        if (parts.length >= 3 && (parts.length < 4 || parts[3] > 0.5)) return bg;
      }
      return 'rgb(255, 255, 255)';
    };
    const found = [];
    for (const el of document.querySelectorAll('.app-shell *')) {
      const ownText = [...el.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim());
      if (!ownText || el.closest('[hidden], [aria-hidden="true"]')) continue;
      const style = getComputedStyle(el);
      const box = el.getBoundingClientRect();
      if (!box.width || style.visibility === 'hidden' || Number(style.opacity) === 0) continue;
      if (!ember.has(style.color)) continue;
      const size = parseFloat(style.fontSize);
      const large = size >= 24 || (size >= 18.66 && Number(style.fontWeight) >= 700);
      if (large) continue;
      // Opacity anywhere above the text dilutes it; blend it into the background.
      let alpha = 1;
      for (let node = el; node; node = node.parentElement)
        alpha *= Number(getComputedStyle(node).opacity);
      const bg = background(el);
      const fg = rgb(style.color).map((v, i) => v * alpha + rgb(bg)[i] * (1 - alpha));
      const a = lum(`rgb(${fg.slice(0, 3).join(',')})`);
      const b = lum(bg);
      const ratio = (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
      if (ratio < 4.5)
        found.push(`${el.textContent.trim().slice(0, 30)} (${size}px, ${ratio.toFixed(2)}:1)`);
    }
    return found;
  });
}

test('Light theme draws small ember text at 4.5:1 or better', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto('/tests/interaction-preview.html');
  const app = page.getByRole('navigation', { name: 'Application', exact: true });
  await app.getByRole('button', { name: 'Settings', exact: true }).click();
  const sections = page.getByRole('navigation', { name: 'Settings sections' });
  await sections.getByRole('button', { name: 'Appearance', exact: true }).click();
  await page.getByRole('button', { name: 'Light', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');

  const failures = {};
  for (const screen of ['Dictate', 'History', 'Models']) {
    await app.getByRole('button', { name: screen, exact: true }).click();
    await page.waitForTimeout(400);
    const found = await smallAccentText(page);
    if (found.length) failures[screen] = found;
  }
  await app.getByRole('button', { name: 'Settings', exact: true }).click();
  for (const name of await sections.getByRole('button').allInnerTexts()) {
    await sections.getByRole('button', { name: name.trim(), exact: true }).click();
    await page.waitForTimeout(300);
    const found = await smallAccentText(page);
    if (found.length) failures[`Settings · ${name.trim()}`] = found;
  }
  expect(failures).toEqual({});
});
