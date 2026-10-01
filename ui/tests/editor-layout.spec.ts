import { test, expect } from '@playwright/test';
import { readFileSync, writeFileSync } from 'node:fs';
import { boardPath, resetBoard } from './fixture';

test.beforeEach(() => resetBoard());

test('one rendered editor formats text and saves Markdown without a preview switch', async ({ page, request }) => {
  await page.goto('/#task/2');
  const body = page.getByRole('textbox', { name: 'Body', exact: true });
  await expect(body).toHaveAttribute('contenteditable', 'true');
  await expect(page.getByRole('button', { name: 'Preview', exact: true })).toHaveCount(0);
  await body.fill('A useful description');
  await body.press('ControlOrMeta+a');
  await page.getByRole('button', { name: 'Bold', exact: true }).click();
  await expect(body.locator('strong')).toHaveText('A useful description');
  await page.getByRole('button', { name: 'Save task', exact: true }).click();
  await expect(page.getByText('Saved', { exact: true })).toBeVisible();
  expect((await (await request.get('/api/tasks/2')).json()).body).toBe('**A useful description**');
});

test('dark mode covers the page, persists, and follows the system when selected', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'light' });
  await page.goto('/');
  await page.getByRole('button', { name: 'Change theme', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  expect(await page.locator('html').evaluate(element => getComputedStyle(element).backgroundColor)).toBe('rgb(21, 24, 31)');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.getByText('Display', { exact: true }).click();
  await page.getByRole('combobox', { name: 'Color theme' }).selectOption('auto');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.emulateMedia({ colorScheme: 'dark' });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.mouse.move(0, 0);
  await expect.poll(() => page.getByRole('button', { name: /New task/ }).evaluate(element => getComputedStyle(element).backgroundColor)).toBe('rgb(160, 150, 251)');
  await expect.poll(() => page.locator('.view-options summary').evaluate(element => getComputedStyle(element).backgroundColor)).toBe('rgb(29, 34, 43)');
  await page.getByText('Display', { exact: true }).click();
  await page.getByRole('button', { name: 'Repair the task panel', exact: true }).click();
  expect(await page.getByRole('textbox', { name: 'Body', exact: true }).evaluate(element => getComputedStyle(element).color)).toBe('rgb(229, 232, 239)');
  await page.getByRole('button', { name: 'Notify', exact: true }).click();
  expect(await page.getByRole('dialog', { name: 'Send a notification', exact: true }).evaluate(element => getComputedStyle(element).backgroundColor)).toBe('rgb(29, 34, 43)');
});

test('persisted dark mode applies before a delayed board has loaded', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'light' });
  await page.goto('/');
  await page.getByRole('button', { name: 'Change theme', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  let release!: () => void;
  const pending = new Promise<void>(resolve => { release = resolve; });
  await page.route('**/api/board', async route => { await pending; await route.continue(); });
  try {
    await page.reload({ waitUntil: 'domcontentloaded' });
    await expect(page.getByText('Getting your board ready…', { exact: true })).toBeVisible();
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
    expect(await page.locator('html').evaluate(element => getComputedStyle(element).backgroundColor)).toBe('rgb(21, 24, 31)');
  } finally { release(); }
  await expect(page.getByRole('heading', { name: /Project board/ })).toBeVisible();
});

test('opening and saving preserves source and editing retains embedded HTML', async ({ page, request }) => {
  const source = '## Details\n\nAn **important** detail.\n\n<!-- keep this comment -->\n\n<div data-example="yes">Keep the HTML</div>\n\n- [x] Finished\n- [ ] Next';
  writeFileSync(boardPath, readFileSync(boardPath, 'utf8').replace('Second task body.', source));
  await page.goto('/#task/2');
  await expect(page.getByText('All changes saved', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Save task', exact: true }).click();
  await expect(page.getByText('Saved', { exact: true })).toBeVisible();
  expect((await (await request.get('/api/tasks/2')).json()).body).toBe(source);
  const body = page.getByRole('textbox', { name: 'Body', exact: true });
  await body.click();
  await body.press('ControlOrMeta+Home');
  await body.press('End');
  await body.press('!');
  await page.getByRole('button', { name: 'Save task', exact: true }).click();
  await expect(page.getByText('Saved', { exact: true })).toBeVisible();
  await expect.poll(async () => (await (await request.get('/api/tasks/2')).json()).body).toContain('Details!');
  const saved = (await (await request.get('/api/tasks/2')).json()).body;
  expect(saved).toContain('<!-- keep this comment -->');
  expect(saved).toContain('<div data-example="yes">Keep the HTML</div>');
  expect(saved).toContain('- [x] Finished');
  await body.locator('pre[data-markdown-source]').filter({ hasText: 'keep this comment' }).fill('<!-- edited comment -->');
  await page.getByRole('button', { name: 'Save task', exact: true }).click();
  await expect.poll(async () => (await (await request.get('/api/tasks/2')).json()).body).toContain('<!-- edited comment -->');
});

test('sidebar hides completely, restores from the heading and remembers its visibility', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Hide sidebar', exact: true }).click();
  await expect(page.getByRole('navigation', { name: 'Task views' })).toHaveCount(0);
  await page.reload();
  await expect(page.getByRole('navigation', { name: 'Task views' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Show sidebar', exact: true }).click();
  await expect(page.getByRole('navigation', { name: 'Task views' })).toBeVisible();
  await expect(page.getByText('Local workspace', { exact: true })).toHaveCount(0);
});

test('group headers reorder by pointer and keyboard and remember their order', async ({ page }) => {
  await page.goto('/');
  const handle = page.getByRole('button', { name: 'Drag open group', exact: true });
  const start = (await handle.boundingBox())!, target = (await page.getByRole('button', { name: 'Drag done group', exact: true }).boundingBox())!;
  await page.mouse.move(start.x + start.width / 2, start.y + start.height / 2);
  await page.mouse.down();
  await page.mouse.move(target.x + target.width / 2, target.y + target.height / 2, { steps: 20 });
  await page.mouse.up();
  await expect.poll(() => page.locator('.board-column').evaluateAll(columns => columns.map(column => column.getAttribute('aria-label')))).toEqual(['working column', 'done column', 'open column']);
  await page.reload();
  await expect(page.locator('.board-column').first()).toHaveAttribute('aria-label', 'working column');
  const keyboard = page.getByRole('button', { name: 'Drag open group', exact: true });
  await keyboard.focus();
  await keyboard.press('Space');
  await expect(page.locator('.group-dragging')).toBeVisible();
  await keyboard.press('ArrowLeft');
  await expect(page.locator('.board-column').last()).toHaveCSS('transform', /matrix\(1, 0, 0, 1, -/);
  await keyboard.press('Space');
  await expect.poll(() => page.locator('.board-column').evaluateAll(columns => columns.map(column => column.getAttribute('aria-label')))).toEqual(['working column', 'open column', 'done column']);
});

test('Markdown paste, lists, links, undo and drafts work inside the unified editor', async ({ page, request }) => {
  await page.goto('/#task/2');
  const body = page.getByRole('textbox', { name: 'Body', exact: true });
  await body.fill('');
  await body.evaluate(element => {
    const data = new DataTransfer();
    data.setData('text/plain', '## Plan\n\n- First\n- Second\n\nA link');
    element.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }));
  });
  await expect(body.locator('h2')).toHaveText('Plan');
  await expect(body.locator('li')).toHaveCount(2);
  await body.press('ControlOrMeta+End');
  await body.press('ControlOrMeta+Shift+ArrowLeft');
  await body.press('ControlOrMeta+k');
  await page.getByRole('textbox', { name: 'Link URL', exact: true }).fill('https://example.com');
  await page.getByRole('button', { name: 'Apply URL', exact: true }).click();
  await expect(body.locator('a')).toHaveAttribute('href', 'https://example.com');
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect(body.locator('a')).toHaveCount(0);
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  await expect(body.locator('a')).toHaveCount(1);
  await page.getByRole('button', { name: 'Repair the task panel', exact: true }).click();
  await page.getByRole('button', { name: 'Close task view', exact: true }).click();
  await page.getByRole('button', { name: 'Repair the task panel', exact: true }).click();
  await expect(body.locator('h2')).toHaveText('Plan');
  await expect(body.locator('a')).toHaveAttribute('href', 'https://example.com');
  await body.press('ControlOrMeta+Enter');
  await expect(page.getByText('Saved', { exact: true })).toBeVisible();
  const saved = (await (await request.get('/api/tasks/2')).json()).body;
  expect(saved).toContain('## Plan');
  expect(saved).toContain('- First');
  expect(saved).toContain('[link](https://example.com)');
});

test('list continuation, nesting and normal Tab focus remain usable', async ({ page }) => {
  await page.goto('/#task/2');
  const body = page.getByRole('textbox', { name: 'Body', exact: true });
  await body.fill('First');
  await page.getByRole('button', { name: 'Bullet list', exact: true }).click();
  await body.press('ControlOrMeta+End');
  await body.press('Enter');
  await body.press('Tab');
  await page.keyboard.type('Nested');
  await expect(body.locator('ul ul li')).toHaveText('Nested');
  await body.press('Shift+Tab');
  await expect(body.locator('ul ul')).toHaveCount(0);
  await body.fill('Plain paragraph');
  await page.getByRole('button', { name: 'Bullet list', exact: true }).click();
  await body.press('Tab');
  await expect(body).not.toBeFocused();
});

test('mobile navigation opens as a drawer and dismisses with Escape', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  await page.getByRole('button', { name: 'Show sidebar', exact: true }).click();
  await expect(page.getByRole('dialog', { name: 'Task navigation', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Completed', exact: false }).click();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog', { name: 'Task navigation', exact: true })).toHaveCount(0);
  await expect(page.getByRole('heading', { name: /Completed tasks/ })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});

test('cancelling a group drag keeps the open task and resumes board refreshes', async ({ page }) => {
  await page.goto('/#task/1');
  const handle = page.getByRole('button', { name: 'Drag open group', exact: true });
  await handle.focus();
  await handle.press('Space');
  await expect(page.locator('.group-dragging')).toBeVisible();
  await handle.press('Escape');
  await expect(page.locator('.group-dragging')).toHaveCount(0);
  await expect(page).toHaveURL(/#task\/1$/);
  writeFileSync(boardPath, readFileSync(boardPath, 'utf8').replace('Small document evidence.', 'Updated after cancelling the drag.'));
  await expect(page.getByRole('textbox', { name: 'Body', exact: true })).toHaveText('Updated after cancelling the drag.', { timeout: 6000 });
});

test('relative links and checklist edits save correctly in the rich editor', async ({ page, request }) => {
  writeFileSync(boardPath, readFileSync(boardPath, 'utf8').replace('Second task body.', '## Release notes\n\nA focused description with **clear priorities**.\n\n- [ ] Review the layout\n- [x] Keep the task body\n\n| Area | State |\n| --- | --- |\n| Editor | Ready |\n\n[future]: ../notes.md\n\nMore notes.'));
  await page.goto('/#task/2');
  const body = page.getByRole('textbox', { name: 'Body', exact: true });
  await expect(body.locator('h2')).toHaveText('Release notes');
  await body.getByRole('checkbox').first().check();
  await body.press('ControlOrMeta+End');
  await body.press('ArrowDown');
  await page.getByRole('button', { name: 'Link', exact: true }).click();
  await page.getByRole('textbox', { name: 'Link URL', exact: true }).fill('../notes.md');
  await page.getByRole('button', { name: 'Apply URL', exact: true }).click();
  await expect(body.locator('a')).toHaveAttribute('href', '../notes.md');
  await page.getByRole('button', { name: 'Save task', exact: true }).click();
  await expect(page.getByText('Saved', { exact: true })).toBeVisible();
  const saved = (await (await request.get('/api/tasks/2')).json()).body;
  expect(saved).toContain('- [x] Review the layout');
  expect(saved).toContain('[future]: ../notes.md');
  expect(saved).toContain('[../notes.md](../notes.md)');
  await page.getByRole('button', { name: 'Change theme', exact: true }).click();
  await page.getByRole('button', { name: 'Expand task view', exact: true }).click();
  await page.screenshot({ path: '/tmp/frump-board-rich-editor.png', animations: 'disabled' });
});
