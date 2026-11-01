/**
 * API reference, Instruments - API (frontend/src/ui/apidocs): the server's /openapi.json grouped
 * by tag, filterable, each operation sent from the drawer, which is wide while the tab is open.
 */
import { test, expect } from './fixtures/base';

const openReference = async (page: import('@playwright/test').Page) => {
  await page.getByRole('navigation', { name: 'Main' }).getByRole('button', { name: 'Instruments', exact: true }).click();
  await page.getByTestId('settings-tab-api').click();
  await expect(page.locator('.bench-drawer__sheet--wide')).toBeVisible();
  const drawer = page.getByRole('region', { name: 'API reference' });
  await expect(drawer).toBeVisible();
  return drawer;
};

test.describe('API reference', () => {
  test('opens as a drawer with the endpoints grouped by tag, and filters them', async ({ mockPage }) => {
    const drawer = await openReference(mockPage);
    await expect(drawer.getByText('BenchMesh API')).toBeVisible();
    for (const tag of ['system', 'instrument-control', 'recordings', 'websockets']) {
      await expect(drawer.getByRole('region', { name: tag })).toBeVisible();
    }
    await expect(drawer.getByText('5 endpoints')).toBeVisible();
    await drawer.getByRole('searchbox', { name: 'Filter endpoints' }).fill('registry');
    await expect(drawer.getByText('1 endpoint', { exact: true })).toBeVisible();
    await expect(drawer.getByRole('region', { name: 'system' })).toHaveCount(0);
    // the reference is a tab of the Instruments drawer: closing the drawer closes it
    await mockPage.getByRole('dialog', { name: 'Instruments' }).getByRole('button', { name: 'Close', exact: true }).click();
    await expect(drawer).toBeHidden();
  });

  test('sends a GET and shows status, time, the formatted body and a curl line', async ({ mockPage }) => {
    const drawer = await openReference(mockPage);
    await drawer.getByRole('button', { name: /GET \/status/ }).click();
    await expect(drawer.getByText(/^curl '.*\/status'$/)).toBeVisible();
    await drawer.getByRole('button', { name: 'Send', exact: true }).click();
    const response = drawer.getByLabel('Response');
    await expect(response.getByText('200 OK')).toBeVisible();
    await expect(response.locator('pre')).toContainText('"connected": 2');
  });

  test('needs every path parameter before sending', async ({ mockPage }) => {
    const drawer = await openReference(mockPage);
    await drawer.getByRole('button', { name: /GET \/instruments\/\{klass\}/ }).click();
    await expect(drawer.getByRole('button', { name: 'Send', exact: true })).toBeDisabled();
    await expect(drawer.getByText('Fill in klass, id, ch, op.')).toBeVisible();
    await drawer.getByLabel('klass').fill('PSU');
    await drawer.getByLabel('id', { exact: true }).fill('test-psu-1');
    await drawer.getByLabel('ch', { exact: true }).fill('1');
    await drawer.getByLabel('op', { exact: true }).fill('status');
    await expect(drawer.getByRole('button', { name: 'Send', exact: true })).toBeEnabled();
    await expect(drawer.getByText(/curl '.*\/instruments\/PSU\/test-psu-1\/1\/status'/)).toBeVisible();
  });

  test('a POST takes a second press, validates its JSON body and reaches the server', async ({ mockPage, api }) => {
    const drawer = await openReference(mockPage);
    await drawer.getByRole('button', { name: /POST \/recordings/ }).click();
    const body = drawer.getByLabel('Body (JSON)');
    await body.fill('{"name": ');
    await expect(drawer.getByText(/Not valid JSON/)).toBeVisible();
    await expect(drawer.getByRole('button', { name: 'Send', exact: true })).toBeDisabled();
    await body.fill('{"name": "From the reference", "interval_s": 1}');

    await drawer.getByRole('button', { name: 'Send', exact: true }).click();
    expect(api.recordingRequests).toHaveLength(0); // armed only
    await drawer.getByRole('button', { name: 'Press again: POST' }).click();
    await expect(drawer.getByLabel('Response').getByText(/^20\d/)).toBeVisible();
    expect(api.recordingRequests).toContainEqual(expect.objectContaining({ method: 'POST', path: '/recordings' }));
  });

  test('says why an operation is not sent from here, and offers the WebSocket address', async ({ mockPage }) => {
    const drawer = await openReference(mockPage);
    await drawer.getByRole('button', { name: /GET \/ Redirect/ }).click();
    await expect(drawer.getByText('Not sent from here: Redirects a browser to the UI.')).toBeVisible();
    await drawer.getByRole('button', { name: /WS \/ws\/registry/ }).click();
    await expect(drawer.getByText(/^ws:\/\/.*\/ws\/registry$/)).toBeVisible();
    await expect(drawer.getByRole('button', { name: 'Show the first message' })).toBeVisible();
  });

  test('/docs (?openDocs=true) opens the reference directly', async ({ page, api: _api }) => {
    await page.goto('/ui/?openDocs=true');
    await expect(page.getByRole('dialog', { name: 'Instruments' }).getByRole('region', { name: 'API reference' })).toBeVisible();
  });
});
