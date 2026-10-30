import { test, expect } from '@playwright/test';
test('comparison registration, exclusion and held-out responses remain separate from tutoring', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'New session' }).click();
  await page.getByRole('button', { name: 'Begin learning' }).click();
  await page.getByRole('button', { name: 'Learning comparison', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Personal learning comparison' });
  await expect(dialog).toBeVisible();
  await dialog.getByText('Register a matched topic pair', { exact: true }).click();
  for (const label of ['Topic A', 'Topic B']) {
    const topic = dialog.getByRole('group', { name: label, exact: true });
    await topic.getByLabel('Topic name').fill(`Synthetic ${label}`);
    await topic
      .getByLabel('Prior knowledge')
      .fill('Synthetic engineering fixture, no actual prior-knowledge evidence');
    for (const itemLabel of ['Familiar-form held-out item', 'Changed-context held-out item']) {
      const item = topic.getByRole('group', { name: itemLabel, exact: true });
      await item
        .getByLabel('Question', { exact: true })
        .fill(`Held-out fixture ${label} ${itemLabel}`);
      await item
        .getByLabel('Scoring rubric', { exact: true })
        .fill('Five points for the complete synthetic response');
      await item
        .getByLabel('Checked solution', { exact: true })
        .fill('Synthetic reference only; no human review');
    }
  }
  await dialog.getByLabel('Strong baseline study method').fill('Current best method');
  await dialog
    .getByLabel('Baseline provider and model (or no model)')
    .fill('No model in this synthetic baseline');
  await dialog
    .getByLabel('Why these topics are comparable and distinct')
    .fill('Separate synthetic topics, never counted as an efficacy result');
  await dialog
    .getByLabel('Personal decision criterion before collecting outcomes')
    .fill('No effect is expected or inferred from this engineering fixture');
  await dialog
    .getByLabel('These are distinct matched topics, not multiple objectives from one lesson')
    .check();
  await dialog.getByRole('button', { name: 'Freeze and assign pair' }).click();
  const topic = dialog
    .locator('.comparison-topic')
    .filter({ has: page.getByRole('heading', { name: /Study Harness$/ }) });
  await topic.getByRole('button', { name: 'Link current study session' }).click();
  await topic.getByRole('button', { name: 'Start timer', exact: true }).click();
  await topic.getByRole('button', { name: 'Stop timer', exact: true }).click();
  await topic.getByRole('button', { name: 'Complete initial lesson', exact: true }).click();
  await expect(topic.getByRole('button', { name: 'Begin held-out assessment' })).toBeDisabled();
  await topic.getByText('Simulated comparison time controls', { exact: true }).click();
  await topic.getByRole('button', { name: 'Simulate start of review exclusion' }).click();
  await expect(topic.getByText(/The preceding 48-hour review exclusion is active/)).toBeVisible();
  await topic.getByLabel('Time category').selectOption('review');
  await expect(topic.getByRole('button', { name: 'Start timer', exact: true })).toBeDisabled();
  await topic.getByRole('button', { name: 'Simulate seven-day assessment time' }).click();
  await topic.getByRole('button', { name: 'Begin held-out assessment' }).click();
  await expect(topic.getByText(/Synthetic reference only/)).toHaveCount(0);
  const responses = topic.getByLabel('Your held-out response');
  await expect(responses).toHaveCount(2);
  await responses.nth(0).fill('Synthetic familiar answer');
  await responses.nth(1).fill('Synthetic changed-context answer');
  await topic.getByRole('button', { name: 'Save held-out responses' }).click();
  await topic.getByText('Saved held-out responses and separate scoring', { exact: true }).click();
  await expect(topic.getByText('Reference: Synthetic reference only; no human review')).toHaveCount(
    2,
  );
  const rationales = topic.getByLabel('Scoring rationale');
  await rationales.nth(0).fill('Synthetic test score');
  await rationales.nth(1).fill('Synthetic test score');
  await topic.getByRole('button', { name: 'Append manual score' }).click();
  await expect(topic.getByText(/user-entered review, unverified by the app/)).toBeVisible();
  await dialog.getByRole('button', { name: 'Close comparison' }).click();
  await expect(page.getByText('Source citations', { exact: true })).toHaveCount(0);
});
