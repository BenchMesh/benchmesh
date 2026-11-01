/**
 * Request log persistence and retention: logs live in localStorage, are shown in the Log
 * drawer, are pruned to the retention period on load and when the setting changes, and
 * are capped at 1000 entries.
 *
 * Panels log their own requests while loading (setpoint queries), so assertions look at the
 * seeded entries by id rather than at total counts.
 */
import { Page } from '@playwright/test';
import { test, expect, gotoBench } from './fixtures/base';

const DAY = 24 * 60 * 60 * 1000;

interface Seed { id: string; ageMs: number; method?: string; url?: string; status?: number }

/** Replace the stored logs (and optionally the retention), then reload the app. */
async function seed(page: Page, logs: Seed[], retentionDays?: number) {
  // let the panels' own (logged) setpoint queries finish so they cannot overwrite the seed
  await expect(page.getByRole('spinbutton', { name: 'Voltage setpoint' })).toHaveAttribute('aria-valuenow', '5');
  await expect(page.getByRole('spinbutton', { name: 'Current limit' })).toHaveAttribute('aria-valuenow', '1');
  await page.evaluate(([entries, days]) => {
    const now = Date.now();
    localStorage.setItem('benchmesh:requestLogs', JSON.stringify(entries.map((e) => ({
      id: e.id,
      timestamp: new Date(now - e.ageMs).toISOString(),
      method: e.method ?? 'GET',
      url: e.url ?? `/instruments/PSU/test-psu-1/1/${e.id}`,
      status: e.status ?? 200,
      duration: 12,
    }))));
    if (days) localStorage.setItem('benchmesh:historyRetentionDays', String(days));
  }, [logs, retentionDays] as const);
  await gotoBench(page, 3);
}

const storedIds = (page: Page) => page.evaluate(() =>
  (JSON.parse(localStorage.getItem('benchmesh:requestLogs') ?? '[]') as { id: string }[]).map((l) => l.id));

test.describe('Request log persistence', () => {
  test('stored logs are shown in Instruments - Log after a reload', async ({ mockPage }) => {
    await seed(mockPage, [
      { id: 'seed-get', ageMs: 1000, method: 'GET', url: 'http://localhost/instruments/PSU/test-1/1/voltage' },
      { id: 'seed-post', ageMs: 2000, method: 'POST', url: 'http://localhost/instruments/PSU/test-1/1/current/2.5' },
    ]);
    await mockPage.getByRole('button', { name: 'Instruments', exact: true }).click();
    await mockPage.getByTestId('settings-tab-log').click();
    const log = mockPage.getByRole('region', { name: 'Request log' });
    await expect(log.getByText('/instruments/PSU/test-1/1/voltage', { exact: true })).toBeVisible();
    await expect(log.locator('.log-entry').filter({ hasText: '/instruments/PSU/test-1/1/current/2.5' })).toContainText('POST');
  });

  test('new requests are persisted automatically', async ({ mockPage }) => {
    // the PSU panel queries its setpoints on load
    await expect.poll(async () => (await mockPage.evaluate(() => localStorage.getItem('benchmesh:requestLogs') ?? ''))).toContain('/instruments/PSU/test-psu-1/1/voltage');
  });

  test('Clear empties the log and its storage', async ({ mockPage }) => {
    await seed(mockPage, [{ id: 'seed-1', ageMs: 1000 }]);
    await mockPage.getByRole('button', { name: 'Instruments', exact: true }).click();
    await mockPage.getByTestId('settings-tab-log').click();
    const log = mockPage.getByRole('region', { name: 'Request log' });
    await expect(log.locator('.log-entry').first()).toBeVisible();
    await log.getByRole('button', { name: 'Clear' }).click();
    await expect(log.getByText('No API requests logged yet')).toBeVisible();
    await expect.poll(() => storedIds(mockPage)).toEqual([]);
  });

  test('the method filter hides other methods', async ({ mockPage }) => {
    await seed(mockPage, [
      { id: 'seed-get', ageMs: 1000, method: 'GET', url: 'http://localhost/seeded/get' },
      { id: 'seed-post', ageMs: 2000, method: 'POST', url: 'http://localhost/seeded/post' },
    ]);
    await mockPage.getByRole('button', { name: 'Instruments', exact: true }).click();
    await mockPage.getByTestId('settings-tab-log').click();
    const log = mockPage.getByRole('region', { name: 'Request log' });
    await log.locator('select').first().selectOption('POST');
    await expect(log.getByText('/seeded/post')).toBeVisible();
    await expect(log.getByText('/seeded/get')).toHaveCount(0);
  });
});

test.describe('Retention', () => {
  test('logs older than the retention period are removed on load', async ({ mockPage }) => {
    await seed(mockPage, [{ id: 'recent', ageMs: 1000 }, { id: 'old', ageMs: 3 * DAY }], 1);
    const ids = await storedIds(mockPage);
    expect(ids).toContain('recent');
    expect(ids).not.toContain('old');
  });

  test('logs within the retention period are kept', async ({ mockPage }) => {
    await seed(mockPage, [{ id: 'five-days', ageMs: 5 * DAY }], 7);
    expect(await storedIds(mockPage)).toContain('five-days');
  });

  test('lowering the retention in the Instruments drawer prunes older logs', async ({ mockPage }) => {
    await seed(mockPage, [{ id: 'half-day', ageMs: DAY / 2 }, { id: 'three-days', ageMs: 3 * DAY }], 7);
    expect(await storedIds(mockPage)).toEqual(expect.arrayContaining(['half-day', 'three-days']));

    await mockPage.getByRole('button', { name: 'Instruments', exact: true }).click();
    const drawer = mockPage.getByRole('dialog', { name: 'Instruments' });
    await drawer.getByTestId('settings-tab-miscellaneous').click();
    await drawer.getByLabel(/History Retention/).selectOption('1');

    await expect.poll(() => storedIds(mockPage)).not.toContain('three-days');
    expect(await storedIds(mockPage)).toContain('half-day');
  });

  test('the log is capped at 1000 entries', async ({ mockPage }) => {
    await seed(mockPage, Array.from({ length: 1005 }, (_, i) => ({ id: `log-${i}`, ageMs: i * 1000 })));
    // new requests from the panels push the log over the cap, which trims the oldest
    await expect.poll(async () => (await storedIds(mockPage)).length).toBeLessThanOrEqual(1000);
  });
});
