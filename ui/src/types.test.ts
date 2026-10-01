import { expect, it } from 'vitest';
import { mergeUpdate, type Update, type Summary } from './types';

const task = (id: number, revision = 1): Summary => ({ id, revision, subject: `Task ${id}`, task_type: 'Task', excerpt: '', properties: [] });
const update = (patch: Partial<Update> = {}): Update => ({ revision: 1, reset: true, header: '# Board', team: [], next: [], tasks: [task(1), task(2)], order: [1, 2], removed: [], ...patch });

it('merges a body-only delta without changing file order or unaffected task objects', () => {
  const initial = mergeUpdate(null, update());
  const merged = mergeUpdate(initial, update({ revision: 2, reset: false, tasks: [task(2, 2)], order: null }));
  expect(merged.tasks.map(task => task.id)).toEqual([1, 2]);
  expect(merged.tasks[0]).toBe(initial.tasks[0]);
  expect(merged.tasks[1].revision).toBe(2);
});
it('applies additions, removals and explicit reordering together', () => {
  const initial = mergeUpdate(null, update());
  const merged = mergeUpdate(initial, update({ revision: 2, reset: false, tasks: [task(3, 2)], removed: [1], order: [3, 2] }));
  expect(merged.tasks.map(task => task.id)).toEqual([3, 2]);
});
it('discards stale summaries when the server resets after missed updates or a restart', () => {
  const initial = mergeUpdate(null, update());
  const merged = mergeUpdate(initial, update({ revision: 5, tasks: [task(3, 5)], order: [3] }));
  expect(merged.tasks.map(task => task.id)).toEqual([3]);
});
