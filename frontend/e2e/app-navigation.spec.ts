import { test, expect } from './fixtures/base';
import { setupApiMocks, setupWebSocketMock } from './utils/api-mock';

test.describe('Title bar', () => {
  test('shows the brand, the version on its baseline and Arrange; no window controls in a browser', async ({ mockPage }) => {
    const title = mockPage.locator('.bench-title');
    await expect(title.getByText('BenchMesh', { exact: true })).toBeVisible();
    await expect(title.getByRole('button', { name: 'Arrange' })).toBeVisible();
    await expect(title.getByRole('group', { name: 'Window' })).toHaveCount(0);
    const version = title.locator('.bench-title__version');
    if (await version.count()) {
      const baseline = (sel: string) => title.locator(sel).evaluate((el) => {
        const r = document.createRange(); r.selectNodeContents(el); const b = r.getBoundingClientRect()
        return b.bottom - parseFloat(getComputedStyle(el).fontSize) * 0.2
      });
      // same text line: bottoms within a couple of pixels after allowing for the font size
      expect(Math.abs((await baseline('.bench-title__name')) - (await baseline('.bench-title__version')))).toBeLessThan(6);
    }
  });
});

test.describe('Scrolling', () => {
  test('only the content scrolls: the title bar and the top bar never move and the scrollbar starts below them', async ({ mockPage }) => {
    await mockPage.setViewportSize({ width: 900, height: 360 });
    const body = mockPage.locator('.bench-body');
    await expect(mockPage.locator('.bench-panel').first()).toBeVisible();
    const m = await mockPage.evaluate(() => {
      const head = document.querySelector('.bench-head')!.getBoundingClientRect();
      const b = document.querySelector('.bench-body')!;
      const doc = document.scrollingElement!;
      return { headBottom: head.bottom, bodyTop: b.getBoundingClientRect().top, overflow: b.scrollHeight > b.clientHeight, docScrolls: doc.scrollHeight > doc.clientHeight };
    });
    expect(m.overflow).toBe(true);
    expect(m.docScrolls).toBe(false);
    expect(Math.abs(m.bodyTop - m.headBottom)).toBeLessThan(1);
    await body.evaluate((e) => e.scrollTo(0, e.scrollHeight));
    expect(await body.evaluate((e) => e.scrollTop)).toBeGreaterThan(0);
    expect(await mockPage.locator('.bench-head').evaluate((e) => e.getBoundingClientRect().top)).toBe(0);
    expect(await mockPage.evaluate(() => window.scrollY)).toBe(0);
  });
});

test.describe('Top bar', () => {
  test('shows healthy API / Readings status pills', async ({ mockPage }) => {
    const top = mockPage.locator('header.bench-top');
    const pills = top.locator('.bench-pill');
    await expect(pills).toHaveText(['API', 'Readings']);
    await expect(pills.nth(0)).toHaveAttribute('data-ok', 'true');
    await expect(pills.nth(1)).toHaveAttribute('data-ok', 'true');
  });

  test('shows all navigation buttons; the skin switch lives in Instruments - Miscellaneous', async ({ mockPage }) => {
    const nav = mockPage.getByRole('navigation', { name: 'Main' });
    for (const name of ['Instruments', 'Recording', 'Automation', 'Log', 'API', 'Feedback']) {
      await expect(nav.getByRole('button', { name, exact: true })).toBeVisible();
    }
    await expect(mockPage.locator('header.bench-top').getByRole('group', { name: 'Skin' })).toHaveCount(0);
  });
});

test.describe('Drawers', () => {
  test('Instruments drawer opens and closes with the close button', async ({ mockPage }) => {
    await mockPage.getByRole('button', { name: 'Instruments', exact: true }).click();
    const drawer = mockPage.getByRole('dialog', { name: 'Instruments' });
    await expect(drawer).toBeVisible();
    await expect(drawer.getByTestId('settings-view')).toBeVisible();

    await drawer.getByRole('button', { name: 'Close' }).click();
    await expect(drawer).toBeHidden();
  });

  test('Instruments drawer closes with Escape', async ({ mockPage }) => {
    await mockPage.getByRole('button', { name: 'Instruments', exact: true }).click();
    const drawer = mockPage.getByRole('dialog', { name: 'Instruments' });
    await expect(drawer).toBeVisible();
    await mockPage.keyboard.press('Escape');
    await expect(drawer).toBeHidden();
  });

  test('drawers open below the title bar and menu, which stay usable and switch drawers', async ({ mockPage }) => {
    await mockPage.getByRole('button', { name: 'Instruments', exact: true }).click();
    const instruments = mockPage.getByRole('dialog', { name: 'Instruments' });
    await expect(instruments).toBeVisible();
    const head = await mockPage.locator('.bench-head').boundingBox();
    const sheet = await instruments.locator('.bench-drawer__sheet').boundingBox();
    expect(Math.abs(sheet!.y - (head!.y + head!.height))).toBeLessThan(1.5);
    const menu = mockPage.getByRole('navigation', { name: 'Main' });
    await expect(menu.getByRole('button', { name: 'Instruments', exact: true })).toHaveAttribute('aria-pressed', 'true');

    // the menu is not covered: Recording replaces the Instruments drawer
    await menu.getByRole('button', { name: 'Recording', exact: true }).click();
    await expect(mockPage.getByRole('dialog', { name: 'Recordings' })).toBeVisible();
    await expect(instruments).toBeHidden();
    await expect(menu.getByRole('button', { name: 'Recording', exact: true })).toHaveAttribute('aria-pressed', 'true');
  });

  test('Log and API are Instruments tabs, not menu buttons; Log shows the request log and closes on the scrim', async ({ mockPage }) => {
    const menu = mockPage.getByRole('navigation', { name: 'Main' });
    await expect(menu.getByRole('button', { name: 'Log', exact: true })).toHaveCount(0);
    await expect(menu.getByRole('button', { name: 'API', exact: true })).toHaveCount(0);
    await menu.getByRole('button', { name: 'Instruments', exact: true }).click();
    const drawer = mockPage.getByRole('dialog', { name: 'Instruments' });
    await expect(drawer.getByTestId('settings-tab-api')).toBeVisible();
    await drawer.getByTestId('settings-tab-log').click();
    await expect(drawer.getByRole('region', { name: 'Request log' }).locator('.logs-panel')).toBeVisible();

    // the scrim covers the page left of the sheet
    await drawer.locator('.bench-drawer__scrim').click({ position: { x: 10, y: 200 } });
    await expect(drawer).toBeHidden();
  });

  test('Recording opens the recordings drawer', async ({ mockPage }) => {
    await mockPage.getByRole('button', { name: 'Recording', exact: true }).click();
    const drawer = mockPage.getByRole('dialog', { name: 'Recordings' });
    await expect(drawer).toBeVisible();
    await expect(drawer).toContainText('No recordings yet');
    await drawer.getByRole('button', { name: 'Close' }).click();
    await expect(drawer).toBeHidden();
  });
});

test.describe('Modals', () => {
  test('Metrics are a tab of the Instruments drawer, per instrument', async ({ mockPage }) => {
    await expect(mockPage.getByRole('button', { name: 'Metrics', exact: true })).toHaveCount(0);
    await mockPage.getByRole('button', { name: 'Instruments', exact: true }).click();
    const drawer = mockPage.getByRole('dialog', { name: 'Instruments' });
    await drawer.getByTestId('settings-tab-metrics').click();
    const view = drawer.getByTestId('metrics-view');
    await expect(view.getByText('live - updates every 30 s')).toBeVisible();
    const psu = view.getByRole('group', { name: /Test PSU|test-psu-1/ });
    await expect(psu.getByText('Link busy')).toBeVisible();
    await expect(psu.getByText('12.5 %')).toHaveAttribute('data-state', 'ok');
    await expect(psu.getByText('11.2 ms')).toBeVisible();
  });

});

test.describe('Safety disclaimer', () => {
  test('shows on first load and is remembered when "don\'t show again" is ticked', async ({ page }) => {
    await setupApiMocks(page);
    await setupWebSocketMock(page);
    await page.goto('/ui/');

    const title = page.getByRole('heading', { name: /Important Safety Information/ });
    await expect(title).toBeVisible();
    const welcome = page.getByRole('dialog', { name: /Thank you for trying BenchMesh/ });
    await expect(welcome).toBeVisible();
    await expect(welcome.getByRole('link', { name: 'support@benchmesh.online' })).toBeVisible();
    await page.getByRole('checkbox', { name: "Don't show this message again" }).check();
    await page.getByRole('button', { name: 'I Agree and Continue' }).click();
    await expect(title).toBeHidden();
    expect(await page.evaluate(() => localStorage.getItem('benchmesh:disclaimer-accepted'))).toBe('true');

    await page.reload();
    await expect(page.getByRole('article').first()).toBeVisible();
    await expect(title).toBeHidden();

    // Instruments - Miscellaneous brings it back
    await page.getByRole('navigation', { name: 'Main' }).getByRole('button', { name: 'Instruments' }).click();
    const drawer = page.getByRole('dialog', { name: 'Instruments' });
    await drawer.getByTestId('settings-tab-miscellaneous').click();
    const show = drawer.getByRole('checkbox', { name: /Show at start/ });
    await expect(show).not.toBeChecked();
    await show.check();
    await page.reload();
    await expect(title).toBeVisible();
  });
});
