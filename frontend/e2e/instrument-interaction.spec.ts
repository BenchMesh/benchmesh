import { test, expect } from './fixtures/base';
import { PANELS } from './utils/api-mock';

test.describe('PSU setpoints', () => {
  test('editing a digit sends nothing until Enter, then POSTs the new voltage', async ({ mockPage, api }) => {
    const v = mockPage.getByRole('spinbutton', { name: 'Voltage setpoint' });
    await expect(v).toHaveAttribute('aria-valuenow', '5');

    await v.focus();
    // the units digit is selected first: 5.00 → 7.00
    await v.press('ArrowUp');
    await v.press('ArrowUp');
    await expect(v).toHaveAttribute('aria-valuenow', '7');
    await expect(v).toHaveClass(/kit-digits--pending/);
    expect(api.posts).toEqual([]);

    await v.press('Enter');
    await expect.poll(() => api.posts.map((p) => p.path)).toEqual(['/instruments/PSU/test-psu-1/1/voltage/7']);
    await expect(v).not.toHaveClass(/kit-digits--pending/);
    await expect(v).toHaveAttribute('aria-valuenow', '7');
  });

  test('moving the digit selection changes finer digits', async ({ mockPage, api }) => {
    const i = mockPage.getByRole('spinbutton', { name: 'Current limit' });
    await expect(i).toHaveAttribute('aria-valuenow', '1');
    await i.focus();
    await i.press('ArrowRight'); // tenths
    await i.press('ArrowDown');  // 1.000 → 0.900
    await i.press('Enter');
    await expect.poll(() => api.posts.map((p) => p.path)).toEqual(['/instruments/PSU/test-psu-1/1/current/0.9']);
  });

  test('Escape reverts an unsent edit', async ({ mockPage, api }) => {
    const v = mockPage.getByRole('spinbutton', { name: 'Voltage setpoint' });
    await expect(v).toHaveAttribute('aria-valuenow', '5');
    await v.focus();
    await v.press('ArrowDown');
    await expect(v).toHaveAttribute('aria-valuenow', '4');
    await v.press('Escape');
    await expect(v).toHaveAttribute('aria-valuenow', '5');
    await v.press('Enter');
    // let any request that Enter could have triggered reach the mock
    await mockPage.waitForLoadState('networkidle');
    expect(api.posts).toEqual([]);
  });

  test('a rejected setpoint shows the API error and keeps the draft', async ({ mockPage, api }) => {
    api.failures.set('/instruments/PSU/test-psu-1/1/voltage/6', { status: 500, detail: 'Instrument did not acknowledge' });
    const psu = mockPage.getByRole('article', { name: PANELS.psu });
    const v = psu.getByRole('spinbutton', { name: 'Voltage setpoint' });
    await expect(v).toHaveAttribute('aria-valuenow', '5');
    await v.focus();
    await v.press('ArrowUp');
    await v.press('Enter');

    await expect(psu.getByRole('alert')).toHaveText('Instrument did not acknowledge');
    await expect(v).toHaveClass(/kit-digits--error/);
    await expect(v).toHaveAttribute('aria-valuenow', '6');
  });
});

test.describe('PSU output switch', () => {
  test('needs two presses to switch on; the first only arms it', async ({ mockPage, api }) => {
    const out = mockPage.getByRole('article', { name: PANELS.psu }).getByRole('button', { name: /^Output/ });
    await expect(out).toHaveAttribute('data-state', 'off');

    await out.click();
    await expect(out).toHaveAttribute('data-state', 'armed');
    await expect(out).toContainText('press again to confirm');
    expect(api.posts).toEqual([]);

    await out.click();
    await expect.poll(() => api.posts.map((p) => p.path)).toEqual(['/instruments/PSU/test-psu-1/1/output/ON']);
  });

  test('disarms by itself when not confirmed within 3 s', async ({ mockPage, api }) => {
    const out = mockPage.getByRole('article', { name: PANELS.psu }).getByRole('button', { name: /^Output/ });
    await out.click();
    await expect(out).toHaveAttribute('data-state', 'armed');
    await expect(out).toHaveAttribute('data-state', 'off', { timeout: 5000 });
    expect(api.posts).toEqual([]);
  });

  test('switches off with a single press', async ({ mockPage, api }) => {
    await mockPage.evaluate(() => {
      const w = window as any;
      const r = structuredClone(w.__registry);
      r['test-psu-1'].PSU.status_ch1.OUTPUT = 'ON';
      w.__pushRegistry(r);
    });
    const out = mockPage.getByRole('article', { name: PANELS.psu }).getByRole('button', { name: /^Output/ });
    await expect(out).toHaveAttribute('data-state', 'on');
    await out.click();
    await expect.poll(() => api.posts.map((p) => p.path)).toEqual(['/instruments/PSU/test-psu-1/1/output/OFF']);
  });

  test('shows the API error when switching fails', async ({ mockPage, api }) => {
    api.failures.set('/instruments/PSU/test-psu-1/1/output/ON', { status: 503, detail: 'Device busy' });
    const out = mockPage.getByRole('article', { name: PANELS.psu }).getByRole('button', { name: /^Output/ });
    await out.click();
    await out.click();
    await expect(out.getByRole('alert')).toHaveText('Device busy');
  });
});

test.describe('DMM controls', () => {
  test('the function dial sends the next mode with the arrow keys', async ({ mockPage, api }) => {
    const dial = mockPage.getByRole('article', { name: PANELS.dmm }).getByRole('slider', { name: 'Function' });
    await dial.focus();
    await dial.press('ArrowRight');
    await expect.poll(() => api.posts.map((p) => p.path)).toEqual(['/instruments/DMM/test-dmm-1/1/mode/CURR']);
  });

  test('range chips send the chosen range', async ({ mockPage, api }) => {
    const dmm = mockPage.getByRole('article', { name: PANELS.dmm });
    await dmm.getByRole('group', { name: 'Range' }).getByRole('button', { name: '5 V' }).click();
    await expect.poll(() => api.posts.map((p) => p.path)).toEqual(['/instruments/DMM/test-dmm-1/1/range/5']);
  });

  test('a failed range change shows the API error', async ({ mockPage, api }) => {
    api.failures.set('/instruments/DMM/test-dmm-1/1/range/50', { status: 400, detail: 'Range not supported' });
    const dmm = mockPage.getByRole('article', { name: PANELS.dmm });
    await dmm.getByRole('group', { name: 'Range' }).getByRole('button', { name: '50 V' }).click();
    await expect(dmm.getByRole('alert')).toHaveText('Range not supported');
  });
});

test.describe('Request log', () => {
  test('instrument operations are recorded in Instruments - Log', async ({ mockPage, api }) => {
    const v = mockPage.getByRole('spinbutton', { name: 'Voltage setpoint' });
    await expect(v).toHaveAttribute('aria-valuenow', '5');
    await v.focus();
    await v.press('ArrowUp');
    await v.press('Enter');
    await expect.poll(() => api.posts.length).toBe(1);

    await mockPage.getByRole('button', { name: 'Instruments', exact: true }).click();
    await mockPage.getByTestId('settings-tab-log').click();
    const log = mockPage.getByRole('region', { name: 'Request log' });
    const entry = log.locator('.log-entry').filter({ hasText: '/instruments/PSU/test-psu-1/1/voltage/6' });
    await expect(entry).toBeVisible();
    await expect(entry).toContainText('POST');
    await expect(entry).toContainText('204');
  });
});
