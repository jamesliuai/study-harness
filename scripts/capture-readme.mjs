// Capture the unchanged production UI, using the existing native-IPC test transport and real Rust/SQLite core.
// Prerequisites: npm run build; cargo build --manifest-path src-tauri/Cargo.toml --bin offline-harness --features dev-tools,offline-provider
// Run: node scripts/capture-readme.mjs
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { chromium, expect } from '@playwright/test';
import { createServer } from 'vite';

process.chdir(fileURLToPath(new URL('..', import.meta.url)));
const server = await createServer({
  mode: 'offline-test',
  server: { host: '127.0.0.1', port: 1422, strictPort: false },
});
let browser;
try {
  await server.listen();
  const address = server.httpServer.address();
  const origin = `http://127.0.0.1:${address.port}`;
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({
    viewport: { width: 1600, height: 1000 },
    deviceScaleFactor: 2,
    timezoneId: 'America/Los_Angeles',
    reducedMotion: 'reduce',
  });
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text());
  });
  // The test bridge accepts the standard test origin. Forward only this isolated server's IPC requests;
  // no workspace payloads or application state are fabricated, and no existing dev server is used.
  await page.route(`${origin}/__core`, async (route) => {
    const response = await route.fetch({
      headers: { ...route.request().headers(), origin: 'http://127.0.0.1:1420' },
    });
    await route.fulfill({ response });
  });
  await page.goto(`${origin}/__production/`);
  await expect(page).toHaveTitle('Study Harness');
  await page.locator('.sample-unit > summary').click();
  await page
    .locator('li')
    .filter({ hasText: 'Derive and justify the convergence interval' })
    .getByRole('button', { name: 'Start', exact: true })
    .click();
  const reveal = page.getByRole('button', {
    name: 'Show the iterations and explore the step size',
    exact: true,
  });
  await expect(reveal).toBeDisabled();
  await page.getByLabel('Your answer', { exact: true }).fill(
    String.raw`Since $f'(x)=2x$, the update is $x_{t+1}=(1-2\eta)x_t$. With $\eta=0.75$, the multiplier is $-\tfrac12$: $4 \to -2 \to 1 \to -\tfrac12$.

The signs alternate, but the distance to the minimum halves at every step. Convergence depends on $|1-2\eta|<1$, so $0<\eta<1$. Crossing the minimum is not the same as diverging.`,
  );
  await page.getByRole('button', { name: 'Save without feedback', exact: true }).click();
  await expect(reveal).toBeEnabled();
  await reveal.click();
  await page.getByRole('button', { name: 'Add the curvature control', exact: true }).click();
  const stepSize = page.getByRole('slider', { name: 'Step size η', exact: true });
  await expect(stepSize).toBeEnabled();
  await stepSize.focus();
  await page.keyboard.press('ArrowRight');
  await expect(stepSize).not.toHaveValue('0.75');
  await page.keyboard.press('ArrowLeft');
  await expect(stepSize).toHaveValue('0.75');
  await page.getByRole('button', { name: 'Help', exact: true }).click();
  await page.getByRole('menuitem', { name: 'Explain the idea', exact: true }).click();
  await expect(page.locator('.turn.tutor')).toContainText('The multiplier is');
  await expect(page.getByRole('button', { name: 'Help', exact: true })).toBeEnabled();
  await page.getByRole('textbox', { name: 'Go to n =', exact: true }).fill('8');
  await page.getByRole('button', { name: 'Go', exact: true }).click();
  await expect(page.getByRole('status').filter({ hasText: 'n = 8 of 1000' })).toBeVisible();
  await page.locator('.visual-panel').evaluate((element) => {
    element.scrollTop = 0;
  });
  await page
    .getByLabel('Your answer', { exact: true })
    .fill('What changes at the boundary where the step size is exactly 1?');
  await page.getByRole('radio', { name: 'Question', exact: true }).click();
  await page.getByRole('heading', { level: 1 }).click();
  await page.evaluate(() => document.fonts.ready);
  await page.mouse.move(12, 12);
  await expect(page.locator('vite-error-overlay')).toHaveCount(0);
  assert.deepEqual(errors, []);
  await mkdir('docs/images', { recursive: true });
  await page.screenshot({ path: 'docs/images/study-workspace.png' });

  // Verify that the photographed state survives reload and the independent check removes teaching/visuals.
  await page.reload();
  await expect(page.locator('.turn.learner')).toContainText('The signs alternate');
  await expect(page.getByRole('status').filter({ hasText: 'n = 8 of 1000' })).toBeVisible();
  await page.getByLabel('Your answer', { exact: true }).fill('');
  await page.getByRole('button', { name: 'Skip to a fresh check', exact: true }).click();
  await expect(page.locator('.visual-panel')).toHaveCount(0);
  await expect(page.locator('.turn.tutor')).toHaveCount(0);
  await page.setViewportSize({ width: 390, height: 844 });
  assert.equal(
    await page.evaluate(
      () => document.documentElement.scrollWidth > document.documentElement.clientWidth,
    ),
    false,
  );
  await page.screenshot({ path: '/tmp/study-readme-narrow-check.png', fullPage: true });
  assert.deepEqual(errors, []);
  console.log('Saved docs/images/study-workspace.png (3200 × 2000).');
  console.log(
    'Verified prediction gate, saved answer, reveal, keyboard controls, explanation, eight iterations, restart, independent-check hiding, and narrow layout. No browser errors.',
  );
} finally {
  await browser?.close();
  await server.close();
}
