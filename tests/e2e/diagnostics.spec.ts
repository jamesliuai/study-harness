import { test, expect } from '@playwright/test';
import fs from 'node:fs/promises';
test('all 40 versioned cases traverse the learner UI, provider parser and actual SQLite core', async ({
  page,
}) => {
  test.setTimeout(180_000);
  const cases = JSON.parse(await fs.readFile('content/diagnostics/cases-v1.json', 'utf8'))
    .cases as { id: string; expected_verdict: string | null; expected_status: string }[];
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto('/');
  await page.getByText('Deterministic scenarios and simulated time', { exact: true }).click();
  const results = [];
  for (const c of cases) {
    await page.getByLabel('Versioned diagnostic case').selectOption(c.id);
    await page.getByRole('button', { name: 'Load diagnostic case', exact: true }).click();
    await expect(
      page.getByRole('heading', { name: `Diagnostic: ${c.id}`, exact: true }),
    ).toBeVisible();
    await page.getByRole('button', { name: 'Run simulated feedback', exact: true }).click();
    const w = await page.evaluate(async () => {
      const r = await fetch('/__core', {
        method: 'POST',
        body: JSON.stringify({ command: 'workspace', args: {} }),
      });
      return (await r.json()).result;
    });
    expect(w.requests.at(-1)?.status, c.id).toBe(c.expected_status);
    expect(w.learning.assessments.at(-1)?.verdict ?? null, c.id).toBe(c.expected_verdict);
    expect(w.learning.summary?.independent_count ?? 0, c.id).toBe(0);
    results.push({
      id: c.id,
      status: w.requests.at(-1)?.status,
      verdict: w.learning.assessments.at(-1)?.verdict ?? null,
      simulated: w.offline,
    });
  }
  await fs.mkdir('/tmp/study-harness-evidence', { recursive: true });
  await fs.writeFile(
    '/tmp/study-harness-evidence/diagnostic-ui.json',
    JSON.stringify(
      {
        version: 'diagnostics-v1',
        claim: 'Offline contract and integrity verification; no real tutor-quality claim',
        cases: results,
      },
      null,
      2,
    ),
  );
  expect(errors).toEqual([]);
});
