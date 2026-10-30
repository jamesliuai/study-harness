import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import fs from 'node:fs/promises';
test('keyboard, source navigation, enlarged text, reduced motion and integrated 2D/3D routes', async ({
  page,
}) => {
  test.setTimeout(120_000);
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  const audit: unknown[] = [];
  const check = async (label: string) => {
    const result = await new AxeBuilder({ page })
      .withTags(['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa'])
      .analyze();
    audit.push({ label, violations: result.violations });
    expect(result.violations, label).toEqual([]);
  };
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto('/');
  await check('home');
  await page.keyboard.press('Tab');
  await expect(page.getByRole('link', { name: 'Skip to study activity' })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.locator('#study-work')).toBeFocused();
  await page.getByRole('button', { name: 'New session' }).click();
  await page.getByLabel('Capability in the initial unit').selectOption('quadratic-derivative');
  await page.getByRole('button', { name: 'Begin learning' }).click();
  await page
    .getByLabel('Your reasoning', { exact: true })
    .fill(
      'For a quadratic, the finite secant approaches the tangent slope as the increment tends to zero.',
    );
  await page.getByRole('button', { name: 'Save attempt', exact: true }).click();
  const visual = page.getByRole('complementary', { name: 'Interactive visual activity' });
  await expect(visual.getByText('Visual saved locally', { exact: true })).toBeVisible();
  await visual
    .getByRole('button', { name: 'Show the tangent, the slope and the secant', exact: true })
    .click();
  await expect(
    visual.getByRole('button', { name: 'Show the derivative graph', exact: true }),
  ).toBeVisible();
  await expect(visual.getByText('Visual saved locally', { exact: true })).toBeVisible();
  await check('derivative visual');
  await page.screenshot({
    path: '/tmp/study-harness-evidence/derivative-desktop.png',
    fullPage: true,
  });
  await page.getByRole('button', { name: 'Sources', exact: true }).click();
  const sources = page.getByRole('complementary', { name: 'Sources' });
  await sources.getByLabel('Import selected text, Markdown or text PDF').setInputFiles({
    name: 'keyboard-notes.md',
    mimeType: 'text/markdown',
    buffer: Buffer.from(
      '# Tangent\nThe derivative is the limit of secant slopes.\n# Conditions\nThe increment must be nonzero before taking its limit.\n',
    ),
  });
  await expect(sources.getByRole('heading', { name: 'keyboard-notes.md' })).toBeVisible();
  await sources.getByLabel('Find a passage').fill('increment');
  await sources.getByRole('button', { name: 'Search sources' }).press('Enter');
  await expect(sources.getByRole('button', { name: /keyboard-notes.md · section:/ })).toBeVisible();
  await check('source navigation');
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const settings = page.getByRole('dialog');
  await settings.getByLabel('Text size').selectOption('200');
  await expect
    .poll(() => page.evaluate(() => getComputedStyle(document.documentElement).fontSize))
    .toBe('32px');
  await check('settings at 200 percent text');
  await settings.getByRole('button', { name: 'Close settings' }).click();
  await page.setViewportSize({ width: 390, height: 844 });
  await expect
    .poll(() =>
      page.evaluate(
        () => document.documentElement.scrollWidth <= document.documentElement.clientWidth,
      ),
    )
    .toBe(true);
  await check('narrow sources with enlarged text');
  await page.screenshot({
    path: '/tmp/study-harness-evidence/source-narrow-enlarged.png',
    fullPage: true,
  });
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('dialog').getByLabel('Text size').selectOption('100');
  await page.getByRole('button', { name: 'Close settings' }).click();
  await page.setViewportSize({ width: 1280, height: 860 });
  await page.getByRole('button', { name: 'New session' }).click();
  await page.getByLabel('Capability in the initial unit').selectOption('surface-gradient');
  await page.getByRole('button', { name: 'Begin learning' }).click();
  await page
    .getByLabel('Your reasoning', { exact: true })
    .fill('A gradient is normal to a contour; a tangent direction has zero dot product with it.');
  await page.getByRole('button', { name: 'Save attempt', exact: true }).click();
  await expect(visual.getByText('Visual saved locally', { exact: true })).toBeVisible();
  await visual
    .getByRole('button', { name: 'Show the gradient at the point and explore', exact: true })
    .click();
  await expect(
    visual.getByRole('button', {
      name: 'Show the gradient field and the coefficient controls',
      exact: true,
    }),
  ).toBeVisible();
  await expect(visual.getByText('Visual saved locally', { exact: true })).toBeVisible();
  await check('surface gradient visual and alternatives');
  await page.screenshot({
    path: '/tmp/study-harness-evidence/surface-desktop.png',
    fullPage: true,
  });
  await page.getByRole('button', { name: 'Learning comparison', exact: true }).click();
  await check('comparison dialog');
  await page
    .getByRole('dialog')
    .getByText('Register a matched topic pair', { exact: true })
    .click();
  await check('comparison registration form');
  await page.getByRole('button', { name: 'Close comparison' }).click();
  await fs.mkdir('/tmp/study-harness-evidence', { recursive: true });
  await fs.writeFile(
    '/tmp/study-harness-evidence/accessibility.json',
    JSON.stringify(
      {
        kind: 'Automated axe/keyboard/browser evidence; manual assistive-technology testing remains unrun',
        checks: audit,
      },
      null,
      2,
    ),
  );
  expect(errors).toEqual([]);
});
