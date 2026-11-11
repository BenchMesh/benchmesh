/**
 * Settings in the Instruments drawer: tab switching and the history retention setting
 * (Miscellaneous tab).
 */
import { Page } from '@playwright/test';
import { test, expect } from './fixtures/base';

async function openMiscellaneous(page: Page) {
  await page.getByRole('button', { name: 'Instruments', exact: true }).click();
  const drawer = page.getByRole('dialog', { name: 'Instruments' });
  await drawer.getByTestId('settings-tab-miscellaneous').click();
  await expect(drawer.getByTestId('settings-tab-miscellaneous')).toHaveClass(/settings-view__tab--active/);
  return drawer;
}

test.describe('Settings tabs', () => {
  test('switches between instrument configuration and Miscellaneous', async ({ mockPage }) => {
    await mockPage.getByRole('button', { name: 'Instruments', exact: true }).click();
    const drawer = mockPage.getByRole('dialog', { name: 'Instruments' });
    await expect(drawer.getByTestId('instrument-config-view')).toBeVisible();

    await drawer.getByTestId('settings-tab-miscellaneous').click();
    await expect(drawer.getByRole('heading', { name: 'History & Logs' })).toBeVisible();
    await expect(drawer.getByTestId('instrument-config-view')).toBeHidden(); // kept mounted: unsaved edits survive

    await drawer.getByTestId('settings-tab-instruments').click();
    await expect(drawer.getByTestId('settings-tab-instruments')).toHaveClass(/settings-view__tab--active/);
    await expect(drawer.getByTestId('instrument-config-view')).toBeVisible();
  });
});

test.describe('History retention setting', () => {
  test('offers 1, 3, 5 and 7 days and defaults to 3', async ({ mockPage }) => {
    const drawer = await openMiscellaneous(mockPage);
    const select = drawer.getByLabel(/History Retention/);
    await expect(select.locator('option')).toHaveText(['1 day', '3 days', '5 days', '7 days']);
    await expect(select).toHaveValue('3');
  });

  test('changing it updates the description and persists to localStorage', async ({ mockPage }) => {
    const drawer = await openMiscellaneous(mockPage);
    const select = drawer.getByLabel(/History Retention/);
    await select.selectOption('7');
    await expect(drawer.getByText(/older than 7 days will be automatically removed/)).toBeVisible();
    await expect.poll(() => mockPage.evaluate(() => localStorage.getItem('benchmesh:historyRetentionDays'))).toBe('7');

    await select.selectOption('1');
    await expect(drawer.getByText(/older than 1 day will be automatically removed/)).toBeVisible();
  });

  test('a press opens the dropdown list in the skin, not the system one, and stays in the drawer', async ({ mockPage }) => {
    const drawer = await openMiscellaneous(mockPage);
    const select = drawer.getByTestId('history-retention-dropdown');
    await select.click();
    const list = mockPage.getByRole('listbox', { name: /History Retention/ });
    await expect(list).toBeVisible();
    await expect(list).toHaveClass(/kit-menu/);
    await expect(list.getByRole('option', { name: /3 days/ })).toHaveAttribute('aria-selected', 'true');
    await mockPage.keyboard.press('Escape');
    await expect(list).toBeHidden();
    await expect(drawer).toBeVisible(); // Escape closed the list only

    await select.click();
    await list.getByRole('option', { name: /5 days/ }).click();
    await expect(list).toBeHidden();
    await expect(select).toHaveValue('5');
    await expect(drawer.getByText(/older than 5 days will be automatically removed/)).toBeVisible();
  });

  test('is restored after a reload', async ({ mockPage }) => {
    await mockPage.evaluate(() => localStorage.setItem('benchmesh:historyRetentionDays', '5'));
    await mockPage.reload();
    const drawer = await openMiscellaneous(mockPage);
    await expect(drawer.getByLabel(/History Retention/)).toHaveValue('5');
  });
});

test.describe('Skin setting', () => {
  test('switches the skin from Miscellaneous and persists it', async ({ mockPage }) => {
    const drawer = await openMiscellaneous(mockPage);
    const skins = drawer.getByRole('group', { name: 'Skin' });
    await expect(skins.getByRole('button')).toHaveText(['Reflective LCD', 'LED segment', 'Cockpit glass']);
    await skins.getByRole('button', { name: 'LED segment' }).click();
    await expect(mockPage.locator('html')).toHaveAttribute('data-skin', 'led');
    expect(await mockPage.evaluate(() => localStorage.getItem('benchmesh:skin'))).toBe('led');
  });
});
