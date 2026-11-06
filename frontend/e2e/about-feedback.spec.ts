/**
 * About and feedback: the version in the top bar (a community build: "v<version> community", nothing
 * else; the official build also in the drawer) and the drawer with the GitHub issues links;
 * and the drawer page standard - no drawer's text or controls touch the drawer's edges.
 */
import { Page } from '@playwright/test';
import { test, expect } from './fixtures/base';

/** Inset (px) every drawer's content keeps from the sheet's left and right edges. */
const MIN_INSET = 8;

/** Visible text and controls inside the open drawer body closer to the sheet's sides than MIN_INSET. */
async function touchingEdges(page: Page): Promise<string[]> {
  return page.evaluate((min) => {
    const sheet = document.querySelector('.bench-drawer__sheet') as HTMLElement;
    const body = sheet.querySelector('.bench-drawer__body') as HTMLElement;
    const s = sheet.getBoundingClientRect();
    const right = s.left + body.clientWidth; // without the scrollbar
    const bad: string[] = [];
    for (const el of Array.from(body.querySelectorAll<HTMLElement>('p, span, a, button, label, h3, h4, code, input, select, li'))) {
      const b = el.getBoundingClientRect();
      const cs = getComputedStyle(el);
      if (!b.width || !b.height || cs.visibility === 'hidden') continue;
      // controls by their outer box, text by where it starts (its content box)
      const control = /^(BUTTON|INPUT|SELECT)$/.test(el.tagName) || el.classList.contains('bench-btn');
      const px = (v: string) => parseFloat(v) || 0;
      const r = control ? b : {
        left: b.left + px(cs.borderLeftWidth) + px(cs.paddingLeft),
        right: b.right - px(cs.borderRightWidth) - px(cs.paddingRight),
      };
      // full-width tab strips are part of the drawer frame, not content
      if (el.closest('[role="tablist"], .settings-view__tabs')) continue;
      if (r.left - s.left < min || right - r.right < min) bad.push(`${el.tagName.toLowerCase()} "${(el.textContent ?? '').trim().slice(0, 30)}" ${Math.round(r.left - s.left)}px / ${Math.round(right - r.right)}px`);
    }
    return bad;
  }, MIN_INSET);
}

test.describe('About and feedback', () => {
  test('official build: the version in the top bar and in the drawer', async ({ mockPage }) => {
    await mockPage.route('**/version', (route) => route.fulfill({ json: { releaseVersion: '0.1.7', version: '0.1.7', displayVersion: 'BenchMesh 0.1.7', distribution: 'official' } }));
    await mockPage.reload();
    const version = mockPage.getByRole('button', { name: 'v0.1.7', exact: true });
    await expect(version).toBeVisible();
    await version.click();
    const drawer = mockPage.getByRole('dialog', { name: 'About and feedback' });
    await expect(drawer.getByTestId('app-version')).toHaveText('BenchMesh 0.1.7');
    await expect(drawer.getByRole('button', { name: 'Copy version' })).toBeVisible();
  });

  test('community build: "community" and the version only in the top bar; the drawer links to GitHub issues', async ({ mockPage }) => {
    const version = mockPage.getByRole('button', { name: 'v0.1.7 community' });
    await expect(version).toBeVisible();
    await version.click();
    const drawer = mockPage.getByRole('dialog', { name: 'About and feedback' });
    await expect(drawer.getByText('Please mention that you use a community build.', { exact: false })).toBeVisible();
    await expect(drawer.getByTestId('app-version')).toHaveCount(0);
    await expect(drawer.getByRole('button', { name: 'Copy version' })).toHaveCount(0);
    await expect(drawer.getByText(/0\.1\.7|newer release/)).toHaveCount(0);
    await expect(drawer.getByRole('link', { name: 'Report an app issue' })).toHaveAttribute('href', 'https://github.com/BenchMesh/benchmesh/issues/new');
    await expect(drawer.getByRole('link', { name: 'See open issues' })).toHaveAttribute('href', 'https://github.com/BenchMesh/benchmesh/issues');
    await expect(drawer.getByRole('link', { name: 'support@benchmesh.online' })).toHaveAttribute('href', 'mailto:support@benchmesh.online');
    await drawer.getByRole('button', { name: 'Close' }).click();
    await mockPage.getByRole('button', { name: 'Feedback', exact: true }).click();
    await expect(mockPage.getByRole('dialog', { name: 'About and feedback' })).toBeVisible();
  });

  test('drawer pages keep their content off the edges', async ({ mockPage }) => {
    const open = async (button: string, dialog: string, tab?: string) => {
      await mockPage.getByRole('button', { name: button, exact: true }).click();
      const d = mockPage.getByRole('dialog', { name: dialog });
      await expect(d).toBeVisible();
      if (tab) await d.getByTestId(tab).click();
      await mockPage.waitForTimeout(200);
      const bad = await touchingEdges(mockPage);
      await d.getByRole('button', { name: 'Close' }).first().click();
      return bad;
    };
    expect(await open('Feedback', 'About and feedback'), 'About and feedback').toEqual([]);
    expect(await open('Instruments', 'Instruments', 'settings-tab-remote'), 'Remote access').toEqual([]);
    expect(await open('Instruments', 'Instruments', 'settings-tab-metrics'), 'Metrics').toEqual([]);
    expect(await open('Instruments', 'Instruments', 'settings-tab-connectors'), 'Connectors').toEqual([]);
    expect(await open('Instruments', 'Instruments', 'settings-tab-system'), 'System').toEqual([]);
    expect(await open('Recording', 'Recordings'), 'Recordings').toEqual([]);
  });
});
