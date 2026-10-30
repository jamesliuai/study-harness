import { test, expect } from '@playwright/test';
import { pdf } from '../pdf-fixture';
import fs from 'node:fs/promises';
test('exact production assets compute, render math and preview PDF under the native CSP', async ({
  page,
}) => {
  const errors: string[] = [];
  const violations: unknown[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => {
    if (m.type() === 'error') errors.push(m.text());
  });
  await page.addInitScript(() => {
    const list: unknown[] = [];
    (window as unknown as { csp: unknown[] }).csp = list;
    document.addEventListener('securitypolicyviolation', (e) =>
      list.push({
        directive: e.effectiveDirective,
        blocked: e.blockedURI,
        source: e.sourceFile,
        line: e.lineNumber,
      }),
    );
  });
  await page.goto('/__production/');
  await page.getByRole('button', { name: 'New session' }).click();
  await page.getByRole('button', { name: 'Begin learning' }).click();
  await page
    .getByLabel('Your reasoning', { exact: true })
    .fill('The multiplier -.5 alternates and shrinks to zero.');
  await page.getByRole('button', { name: 'Save attempt', exact: true }).click();
  const visual = page.getByRole('complementary', { name: 'Interactive visual activity' });
  await expect(visual.getByText('Visual saved locally', { exact: true })).toBeVisible();
  await visual
    .getByRole('button', { name: 'Show the iterations and explore the step size' })
    .click();
  await expect(visual.getByRole('button', { name: 'Add the curvature control' })).toBeVisible();
  await expect(visual.getByRole('slider').first()).toBeEnabled();
  await page.getByRole('button', { name: 'Sources', exact: true }).click();
  const source = page.getByRole('complementary', { name: 'Sources' });
  await source.getByLabel('Import selected text, Markdown or text PDF').setInputFiles({
    name: 'csp-fixture.pdf',
    mimeType: 'application/pdf',
    buffer: pdf('Local PDF text and a checked assumption.'),
  });
  await expect(source.getByRole('heading', { name: 'csp-fixture.pdf' })).toBeVisible();
  await expect
    .poll(() =>
      source.locator('canvas').evaluate((canvas: HTMLCanvasElement) => {
        const c = canvas.getContext('2d')!;
        if (!canvas.width || !canvas.height) return 0;
        const pixels = c.getImageData(0, 0, canvas.width, canvas.height).data;
        let dark = 0;
        for (let i = 0; i < pixels.length; i += 4)
          if (pixels[i + 3] && pixels[i] < 150 && pixels[i + 1] < 150 && pixels[i + 2] < 150)
            dark++;
        return dark;
      }),
    )
    .toBeGreaterThan(20);
  violations.push(...(await page.evaluate(() => (window as unknown as { csp: unknown[] }).csp)));
  await fs.mkdir('/tmp/study-harness-evidence', { recursive: true });
  await fs.writeFile(
    '/tmp/study-harness-evidence/production-csp.json',
    JSON.stringify(
      {
        kind: 'Exact production frontend assets with test-only transport to actual Rust store; Chromium, not native pixel proof',
        errors,
        violations,
      },
      null,
      2,
    ),
  );
  expect(errors).toEqual([]);
  expect(violations).toEqual([]);
});
