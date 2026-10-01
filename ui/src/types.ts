export type Property = { key: string; value: string };
export type TaskInput = { task_type: string; subject: string; body: string; properties: Property[] };
export type Task = TaskInput & { id: number };
export type Summary = Omit<Task, 'body'> & { excerpt: string; revision: number };
export type Member = { name: string; email: string; role: string | null };
export type Board = { revision: number; header: string; team: Member[]; next: number[]; tasks: Summary[] };
export type Update = Omit<Board, 'tasks'> & { tasks: Summary[]; reset: boolean; removed: number[]; order: number[] | null };
export type Route = { task: number | 'new' | null; notify: number | null };
export const UNSET = '\u0000';
export const valueOf = (task: { properties: Property[] }, key: string) => task.properties.find(p => p.key === key)?.value ?? UNSET;
export const emptyTask = (): TaskInput => ({ task_type: 'Task', subject: '', body: '', properties: [{ key: 'Status', value: 'open' }] });
export const titleOf = (header: string) => header.split('\n').find(line => /^\s*#\s/.test(line))?.replace(/^\s*#\s+/, '').trim() || 'Task board';
export const columnLabel = (key: string, group: string) => key === UNSET ? `No ${group.toLowerCase()}` : key || 'Unscheduled';
export function sameInput(a: TaskInput, b: TaskInput) {
  return a.subject === b.subject && a.task_type === b.task_type && a.body === b.body && JSON.stringify(a.properties) === JSON.stringify(b.properties);
}
export function withProperty<T extends TaskInput>(task: T, key: string, value: string): T {
  const properties = task.properties.filter(p => p.key !== key);
  if (value !== UNSET && value !== '') properties.push({ key, value });
  return { ...task, properties };
}
export function mergeUpdate(board: Board | null, update: Update): Board {
  const tasks = new Map(update.reset ? [] : board?.tasks.map(task => [task.id, task]));
  for (const id of update.removed) tasks.delete(id);
  for (const task of update.tasks) tasks.set(task.id, task);
  const order = update.order ?? board?.tasks.map(task => task.id) ?? [];
  return { ...update, tasks: order.flatMap(id => { const task = tasks.get(id); return task ? [task] : []; }) };
}
