import { test, expect } from '@playwright/test';
import fs from 'node:fs/promises';
test('all HostPort hooks against the actual SQLite core and real process terminations', async ({
  page,
}) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => {
    if (m.type() === 'error') errors.push(m.text());
  });
  await page.goto('/tests/conformance.html');
  await page.waitForFunction(() => 'hostConformance' in window);
  const run = await page.evaluate(async () => await window.hostConformance);
  await fs.mkdir('/tmp/study-harness-evidence', { recursive: true });
  await fs.writeFile(
    '/tmp/study-harness-evidence/host-conformance.json',
    JSON.stringify(run, null, 2),
  );
  console.log(
    JSON.stringify(
      {
        result: run.result.result,
        problems: run.problems,
        failed: run.checks.filter((c) => c.outcome !== 'pass'),
      },
      null,
      2,
    ),
  );
  expect(errors).toEqual([]);
  expect(run.problems).toEqual([]);
  expect(run.result.result).toBe('pass');
});

test('the learner app saves a prediction, reveals the real player and restores its checkpoint', async ({
  page,
}) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => {
    if (m.type() === 'error') errors.push(m.text());
  });
  await page.goto('/');
  await expect(page).toHaveTitle('Study Harness');
  await page.getByRole('button', { name: 'New session' }).click();
  await page.getByRole('button', { name: 'Begin learning' }).click();
  await page
    .getByLabel('Your reasoning', { exact: true })
    .fill('q=-0.5; the signs alternate while the magnitude halves, so x tends to zero.');
  await page.getByRole('button', { name: 'Save attempt', exact: true }).click();
  const visual = page.getByRole('complementary', { name: 'Interactive visual activity' });
  await expect(visual.getByText('Visual saved locally', { exact: true })).toBeVisible();
  await visual
    .getByRole('button', { name: 'Show the iterations and explore the step size' })
    .click();
  await expect(visual.getByRole('button', { name: 'Add the curvature control' })).toBeVisible();
  await expect(visual.getByRole('slider').first()).toBeEnabled();
  await visual.getByRole('slider').first().focus();
  await page.keyboard.press('ArrowRight');
  await visual.getByRole('button', { name: 'Save visual checkpoint' }).click();
  await expect(visual.getByText('Visual saved locally', { exact: true })).toBeVisible();
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({
    path: '/tmp/study-harness-evidence/integrated-desktop.png',
    fullPage: true,
  });
  const overflowing = await page.evaluate(
    () => document.documentElement.scrollWidth > document.documentElement.clientWidth,
  );
  expect(overflowing).toBe(false);
  await page.reload();
  await expect(visual.getByRole('button', { name: 'Add the curvature control' })).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({
    path: '/tmp/study-harness-evidence/integrated-narrow.png',
    fullPage: true,
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth > document.documentElement.clientWidth,
    ),
  ).toBe(false);
  expect(errors).toEqual([]);
});
