import { test, expect } from '@playwright/test';
import { resetBoard } from './fixture';
import { boardPath } from './fixture';
import { readFileSync, writeFileSync } from 'node:fs';

test.beforeEach(() => resetBoard());

test('closing after selecting two tasks returns to the board and stays closed', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Design the navigation', exact: true }).click();
  await page.getByRole('button', { name: 'Repair the task panel', exact: true }).click();
  await page.getByRole('button', { name: 'Close task view', exact: true }).first().click();
  await expect(page).not.toHaveURL(/#task/);
  await expect(page.getByRole('textbox', { name: 'Subject', exact: true })).toHaveCount(0);
  await page.waitForTimeout(2300);
  await expect(page.getByRole('textbox', { name: 'Subject', exact: true })).toHaveCount(0);
});

test('an unchanged multi-megabyte board returns no repeated document JSON', async ({ request }) => {
  resetBoard(true);
  const first = await request.get('/api/document');
  expect((await first.body()).length).toBeGreaterThan(2_000_000);
  expect(first.headers().etag).toBeTruthy();
  const idle = await request.get('/api/document', { headers: { 'If-None-Match': first.headers().etag } });
  expect(idle.status()).toBe(304);
  expect((await idle.body()).length).toBe(0);
});

test('large board browsing transfers summaries once and no task bodies while idle', async ({ page, request }) => {
  resetBoard(true);
  const transfers: { url: string; status: number; bytes: number }[] = [];
  const reads: Promise<void>[] = [];
  page.on('response', response => {
    if (!response.url().includes('/api/')) return;
    reads.push((async () => transfers.push({ url: response.url(), status: response.status(), bytes: response.status() === 304 ? 0 : (await response.body()).length }))().then(() => {}));
  });
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Design the navigation', exact: true })).toBeVisible();
  await page.waitForTimeout(4500);
  await Promise.all(reads);
  expect(transfers.filter(response => response.status === 304).length).toBeGreaterThanOrEqual(2);
  expect(transfers.reduce((bytes, response) => bytes + response.bytes, 0)).toBeLessThan(10_000);
  expect(transfers.some(response => /api\/document|api\/tasks\//.test(response.url))).toBe(false);
  const fullTask = await request.get('/api/tasks/1');
  expect((await fullTask.json()).body.length).toBeGreaterThan(2_000_000);
  console.log(`Large-board idle check: ${transfers.reduce((bytes, response) => bytes + response.bytes, 0)} JSON bytes over 4.5 seconds; ${transfers.filter(response => response.status === 304).length} empty refreshes.`);
});

test('a body-only edit sends one compact delta and preserves deep body search', async ({ request }) => {
  resetBoard(true);
  const initial = await (await request.get('/api/board')).json();
  const source = readFileSync(boardPath, 'utf8');
  writeFileSync(boardPath, source.replace('Second task body.', 'Second task body. ' + 'Supporting notes. '.repeat(1000) + 'deep-evidence-marker'));
  const response = await request.get(`/api/board?since=${initial.revision}`);
  const delta = await response.json();
  expect(delta.reset).toBe(false);
  expect(delta.tasks.map((task: { id: number }) => task.id)).toEqual([2]);
  expect(delta.order).toBeNull();
  expect((await response.body()).length).toBeLessThan(2000);
  expect(delta.tasks[0].body).toBeUndefined();
  expect(await (await request.get('/api/search?q=deep-evidence-marker')).json()).toEqual([2]);
});

test('drafts remain independent across task switches, closing and reloading', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Design the navigation', exact: true }).click();
  await page.getByRole('textbox', { name: 'Subject', exact: true }).fill('Navigation draft');
  await page.getByRole('button', { name: 'Repair the task panel', exact: true }).click();
  await page.getByRole('textbox', { name: 'Subject', exact: true }).fill('Panel draft');
  await page.getByRole('button', { name: 'Close task view', exact: true }).click();
  await page.getByRole('button', { name: 'Design the navigation', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Subject', exact: true })).toHaveValue('Navigation draft');
  await page.reload();
  await expect(page.getByRole('textbox', { name: 'Subject', exact: true })).toHaveValue('Navigation draft');
  await page.getByRole('button', { name: 'Repair the task panel', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Subject', exact: true })).toHaveValue('Panel draft');
  await page.getByRole('button', { name: 'Discard draft', exact: true }).click();
  await page.getByRole('button', { name: 'Repair the task panel', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Subject', exact: true })).toHaveValue('Repair the task panel');
});

test('back, forward, full-screen and Escape preserve a single task selection', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Design the navigation', exact: true }).click();
  await page.getByRole('button', { name: 'Repair the task panel', exact: true }).click();
  await page.goBack();
  await expect(page.getByRole('textbox', { name: 'Subject', exact: true })).toHaveCount(0);
  await page.goForward();
  await expect(page.getByRole('textbox', { name: 'Subject', exact: true })).toHaveValue('Repair the task panel');
  await page.getByRole('button', { name: 'Write', exact: true }).click();
  await page.getByRole('textbox', { name: 'Body', exact: true }).fill('A preserved draft.');
  await page.getByRole('button', { name: 'Expand task view' }).click();
  await expect(page.getByRole('dialog', { name: 'Task details' })).toBeVisible();
  await expect(page.getByRole('textbox', { name: 'Body', exact: true })).toHaveValue('A preserved draft.');
  await page.getByRole('button', { name: 'Dock task panel' }).click();
  await page.getByRole('textbox', { name: 'Body', exact: true }).focus();
  await page.keyboard.press('Escape');
  await expect(page).not.toHaveURL(/#task/);
});

test('external edits update clean views and leave dirty drafts intact', async ({ page }) => {
  await page.goto('/#task/2');
  await expect(page.getByRole('textbox', { name: 'Subject', exact: true })).toHaveValue('Repair the task panel');
  writeFileSync(boardPath, readFileSync(boardPath, 'utf8').replace('Repair the task panel', 'External task subject'));
  await expect(page.getByRole('textbox', { name: 'Subject', exact: true })).toHaveValue('External task subject', { timeout: 6000 });
  await page.getByRole('textbox', { name: 'Subject', exact: true }).fill('My draft');
  writeFileSync(boardPath, readFileSync(boardPath, 'utf8').replace('External task subject', 'Another external change'));
  await expect(page.getByText('This task changed outside this view. Your draft is untouched.')).toBeVisible({ timeout: 6000 });
  await expect(page.getByRole('textbox', { name: 'Subject', exact: true })).toHaveValue('My draft');
  await expect(page.getByRole('button', { name: 'Save task', exact: true })).toBeDisabled();
  await page.getByRole('button', { name: 'Keep my draft', exact: true }).click();
  await page.getByRole('button', { name: 'Save task', exact: true }).click();
  await expect(page.getByText('All changes saved', { exact: true })).toBeVisible({ timeout: 6000 });
  expect(readFileSync(boardPath, 'utf8')).toContain('My draft');
});

test('compare-and-save refuses a stale baseline without losing external text', async ({ request }) => {
  const task = await (await request.get('/api/tasks/2')).json();
  writeFileSync(boardPath, readFileSync(boardPath, 'utf8').replace('Second task body.', 'An external edit that must survive.'));
  const response = await request.put('/api/tasks/2', { data: { ...task, subject: 'Stale save', expected: task } });
  expect(response.status()).toBe(400);
  expect(await response.text()).toContain('Your draft is kept');
  expect(readFileSync(boardPath, 'utf8')).toContain('An external edit that must survive.');
  expect(readFileSync(boardPath, 'utf8')).not.toContain('Stale save');
});

test('large task bodies can be edited with their complete compare-and-save baseline', async ({ request }) => {
  resetBoard(true);
  const task = await (await request.get('/api/tasks/1')).json();
  const response = await request.put('/api/tasks/1', { data: { ...task, subject: 'Updated large task', expected: task } });
  expect(response.ok()).toBe(true);
  expect((await response.json()).body).toBe(task.body);
});

test('unrelated edits do not refetch an open task body', async ({ page }) => {
  let detailReads = 0;
  page.on('request', request => { if (new URL(request.url()).pathname === '/api/tasks/1') detailReads++; });
  await page.goto('/#task/1');
  await expect(page.getByRole('textbox', { name: 'Subject', exact: true })).toHaveValue('Design the navigation');
  writeFileSync(boardPath, readFileSync(boardPath, 'utf8').replace('Second task body.', 'An unrelated edit.'));
  await page.waitForTimeout(4300);
  expect(detailReads).toBe(1);
});

test('dragging between columns uses the shared board rules and keeps the complete body', async ({ page, request }) => {
  await page.goto('/');
  await page.locator('[data-task="1"]').dragTo(page.getByRole('region', { name: 'working column', exact: true }));
  await expect(page.getByRole('region', { name: 'working column', exact: true }).getByRole('button', { name: 'Design the navigation', exact: true })).toBeVisible();
  const task = await (await request.get('/api/tasks/1')).json();
  expect(task.body).toBe('Small document evidence.');
  expect(task.properties).toContainEqual({ key: 'Status', value: 'working' });
});

test('large columns render only visible cards and can reach the last task', async ({ page }) => {
  writeFileSync(boardPath, '# Large board\n\n## Tasks\n\n' + Array.from({ length: 2000 }, (_, index) => `### Task ${index + 1} - Item ${index + 1}\n\n${'Task evidence. '.repeat(40)}\n\nStatus: open\n\n`).join(''));
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Item 1', exact: true })).toBeVisible();
  expect(await page.locator('[data-task]').count()).toBeLessThan(30);
  await page.locator('.column-scroll').evaluate(element => { element.scrollTop = element.scrollHeight; });
  await expect(page.getByRole('button', { name: 'Item 2000', exact: true })).toBeVisible();
  expect(await page.locator('[data-task]').count()).toBeLessThan(30);
});

test('board and list filtering search the complete body without downloading it', async ({ page }) => {
  writeFileSync(boardPath, readFileSync(boardPath, 'utf8').replace('Second task body.', 'Ordinary opening. '.repeat(50) + 'unique-deep-search-term'));
  await page.goto('/');
  await page.getByRole('textbox', { name: 'Search tasks', exact: true }).fill('unique-deep-search-term');
  await expect(page.getByRole('button', { name: 'Repair the task panel', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Design the navigation', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'List', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Repair the task panel', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Clear filters', exact: true }).click();
  await page.getByRole('combobox', { name: 'Filter by type', exact: true }).selectOption('Task');
  await expect(page.getByRole('button', { name: 'Repair the task panel', exact: true })).toHaveCount(0);
  await page.getByRole('combobox', { name: 'Group by', exact: true }).selectOption('Priority');
  await page.reload();
  await expect(page.getByRole('combobox', { name: 'Group by', exact: true })).toHaveValue('Priority');
  await expect(page.getByRole('combobox', { name: 'Filter by type', exact: true })).toHaveValue('Task');
});

test('removed drafts survive a reload and can be saved as a new task', async ({ page, request }) => {
  await page.goto('/#task/3');
  await page.getByRole('textbox', { name: 'Subject', exact: true }).fill('Recovered work');
  await page.waitForTimeout(500);
  await request.delete('/api/tasks/3');
  await expect(page.getByText('This task was removed from the board. Your draft is still here.')).toBeVisible({ timeout: 6000 });
  await page.reload();
  await expect(page.getByRole('textbox', { name: 'Subject', exact: true })).toHaveValue('Recovered work');
  await page.getByRole('button', { name: 'Save as a new task', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Subject', exact: true })).toHaveValue('Recovered work');
  await expect(page.getByText('This task was removed from the board. Your draft is still here.')).toHaveCount(0);
});

test('mobile task creation, notifications and closing fit the viewport', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'New task', exact: false })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.screenshot({ path: '/tmp/frump-board-mobile.png' });
  await page.getByRole('button', { name: 'New task', exact: false }).click();
  await page.getByRole('textbox', { name: 'Subject', exact: true }).fill('A mobile task');
  await page.getByRole('textbox', { name: 'Body', exact: true }).fill('## A clear description\n\n- First step\n- Second step');
  await page.getByRole('button', { name: 'Save task', exact: true }).click();
  await expect(page).toHaveURL(/#task\/\d+$/);
  await page.route('**/api/tasks/*/notify', route => route.fulfill({ status: 204 }));
  await page.getByRole('button', { name: 'Notify', exact: true }).click();
  await page.getByRole('textbox', { name: 'Recipient', exact: true }).fill('test-recipient');
  await page.getByRole('textbox', { name: 'Message', exact: true }).fill('Ready to review.');
  await page.getByRole('button', { name: 'Send notification', exact: true }).click();
  await expect(page.getByRole('dialog', { name: 'Send a notification', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Close task view', exact: true }).click();
  await expect(page).not.toHaveURL(/#task/);
});

test('desktop layouts render without browser errors or external runtime dependencies', async ({ page }) => {
  const errors: string[] = [], external: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => { if (!request.url().startsWith('http://127.0.0.1:4317')) external.push(request.url()); });
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Design the navigation', exact: true })).toBeVisible();
  await page.screenshot({ path: '/tmp/frump-board-desktop.png' });
  await page.getByRole('button', { name: 'Repair the task panel', exact: true }).click();
  await expect(page.getByRole('textbox', { name: 'Subject', exact: true })).toBeVisible();
  await page.screenshot({ path: '/tmp/frump-board-editor.png' });
  await page.getByRole('button', { name: 'Change theme', exact: true }).click();
  await page.screenshot({ path: '/tmp/frump-board-dark.png' });
  expect(errors).toEqual([]);
  expect(external).toEqual([]);
});
