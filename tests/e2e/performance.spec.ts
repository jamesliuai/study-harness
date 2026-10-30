import { test, expect } from '@playwright/test';
import fs from 'node:fs/promises';
test('realistic history, input acknowledgement, local scene updates and context bounds', async ({
  page,
}) => {
  test.setTimeout(120_000);
  await page.goto('/');
  const seed = await page.evaluate(async () => {
    const r = await fetch('/__core', {
      method: 'POST',
      body: JSON.stringify({ command: 'test.seedHistory', args: {} }),
    });
    return await r.json();
  });
  expect(seed.ok).toBe(true);
  const t0 = performance.now();
  await page.reload();
  await expect(
    page.getByRole('heading', {
      name: 'Performance fixture: predict quadratic convergence',
      exact: true,
    }),
  ).toBeVisible();
  const browserReloadMs = performance.now() - t0;
  const input = page.getByLabel('Your reasoning', { exact: true });
  await input.evaluate((el) => {
    const sample: number[] = [];
    (window as unknown as { inputSamples: number[] }).inputSamples = sample;
    el.addEventListener('input', (e) => {
      const at = e.timeStamp;
      requestAnimationFrame(() => sample.push(performance.now() - at));
    });
  });
  await input.fill('The multiplier -.5 alternates and shrinks.');
  await input.press('End');
  await page.keyboard.type(' The magnitude halves on each update.');
  await page.getByRole('button', { name: 'Save attempt', exact: true }).click();
  const v = page.getByRole('complementary', { name: 'Interactive visual activity' });
  await expect(v.getByText('Visual saved locally', { exact: true })).toBeVisible();
  await v.getByRole('button', { name: 'Show the iterations and explore the step size' }).click();
  await expect(v.getByRole('button', { name: 'Add the curvature control' })).toBeVisible();
  const slider = v.getByRole('slider').first();
  await slider.focus();
  await page.evaluate(() => {
    const target = document.querySelector<HTMLElement>('.real-visual [data-frame-id]')!;
    let at = 0;
    const samples: number[] = [];
    (window as unknown as { scientificSamples: number[] }).scientificSamples = samples;
    document.addEventListener('input', () => {
      at = performance.now();
    });
    new MutationObserver((changes) => {
      if (at && changes.some((c) => c.oldValue !== target.dataset.frameId)) {
        samples.push(performance.now() - at);
        at = 0;
      }
    }).observe(target, {
      attributes: true,
      attributeFilter: ['data-frame-id'],
      attributeOldValue: true,
    });
  });
  const host = v.locator('[data-frame-id]');
  const redrawMs: number[] = [];
  for (let n = 0; n < 20; n++) {
    const before = await host.getAttribute('data-frame-id');
    const at = performance.now();
    await page.keyboard.press(n % 2 === 0 ? 'ArrowRight' : 'ArrowLeft');
    await expect.poll(() => host.getAttribute('data-frame-id')).not.toBe(before);
    redrawMs.push(performance.now() - at);
  }
  const inputMs = await page.evaluate(
    () => (window as unknown as { inputSamples: number[] }).inputSamples,
  );
  const quantile = (a: number[], p: number) =>
    [...a].sort((a, b) => a - b)[Math.min(a.length - 1, Math.floor(p * a.length))];
  const scientific = await page.evaluate(
    () => (window as unknown as { scientificSamples: number[] }).scientificSamples,
  );
  const context = await page.evaluate(async (sid) => {
    const r = await fetch('/__core', {
      method: 'POST',
      body: JSON.stringify({ command: 'test.contextMetrics', args: { sessionId: sid } }),
    });
    return (await r.json()).result;
  }, seed.result.active_session_id);
  const report = {
    kind: 'Playwright Chromium over Vite development server and real offline Rust/SQLite; native startup/CPU/memory measured separately',
    history: { sessions_added: 51, attempts_added: 1000, sources: 3, source_bytes_approx: 258000 },
    browserReloadToActiveHeadingMs: browserReloadMs,
    inputEventToNextAnimationFrame: {
      samples: inputMs.length,
      median: quantile(inputMs, 0.5),
      p95: quantile(inputMs, 0.95),
      max: Math.max(...inputMs),
    },
    inputToLearnerScientificFrame: {
      samples: scientific.length,
      median: quantile(scientific, 0.5),
      p95: quantile(scientific, 0.95),
      max: Math.max(...scientific),
      method:
        'DOM input event to learner projection frame change; local worker, native authorization and subscription, before browser paint; excludes automation polling.',
    },
    keyboardToCommittedScientificFrameMs: {
      samples: redrawMs.length,
      median: quantile(redrawMs, 0.5),
      p95: quantile(redrawMs, 0.95),
      max: Math.max(...redrawMs),
      method:
        'Automation-inclusive wall latency; input event, worker compute, native durable exposure and redraw. Not an isolated GPU frame rate.',
    },
    context,
    providerLatency: 'none; no provider calls',
  };
  await fs.mkdir('/tmp/study-harness-evidence', { recursive: true });
  await fs.writeFile(
    '/tmp/study-harness-evidence/performance-browser.json',
    JSON.stringify(report, null, 2),
  );
  expect(report.inputEventToNextAnimationFrame.p95).toBeLessThan(100);
  expect(scientific.length).toBeGreaterThanOrEqual(18);
  expect(context.encoded_payload_bytes).toBeLessThanOrEqual(32000);
});
