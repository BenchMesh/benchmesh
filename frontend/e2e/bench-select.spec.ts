/**
 * The bench feels like an app, not a web page: pressing and dragging across it selects no text,
 * while fields stay editable and selectable.
 */
import { test, expect } from './fixtures/base';
import { PANELS } from './utils/api-mock';

test('dragging across the bench selects no text', async ({ mockPage }) => {
  const box = (await mockPage.getByRole('article', { name: PANELS.psu }).boundingBox())!;
  await mockPage.mouse.move(box.x + 8, box.y + 8);
  await mockPage.mouse.down();
  await mockPage.mouse.move(box.x + box.width - 8, box.y + box.height - 8, { steps: 10 });
  await mockPage.mouse.up();
  expect(await mockPage.evaluate(() => window.getSelection()?.toString() ?? '')).toBe('');
  await mockPage.keyboard.press('Control+a');
  expect(await mockPage.evaluate(() => window.getSelection()?.toString() ?? '')).toBe('');
});

test('fields stay selectable', async ({ mockPage }) => {
  expect(await mockPage.evaluate(() => {
    const i = document.body.appendChild(document.createElement('input'));
    const s = getComputedStyle(i).userSelect;
    i.remove();
    return s;
  })).not.toBe('none');
});
