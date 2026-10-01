import type { Task, TaskInput, Update } from './types';

export class RequestError extends Error {
  constructor(message: string, public status: number) { super(message); }
}
export async function request<T>(path: string, options?: RequestInit): Promise<T> {
  const response = await fetch(path, options);
  if (!response.ok) throw new RequestError((await response.text()).trim() || 'The request could not be completed.', response.status);
  return response.status === 204 ? undefined as T : response.json();
}
export async function fetchBoard(since: number | undefined, signal: AbortSignal): Promise<Update | null> {
  const response = await fetch(`/api/board${since === undefined ? '' : `?since=${since}`}`, { signal, cache: 'no-store' });
  if (response.status === 304) return null;
  if (!response.ok) throw new Error((await response.text()).trim());
  return response.json();
}
export const fetchTask = (id: number, signal?: AbortSignal) => request<Task>(`/api/tasks/${id}`, { signal, cache: 'no-store' });
export const saveTask = (id: number | 'new', task: TaskInput, expected?: TaskInput) => request<Task & { warning?: string }>(id === 'new' ? '/api/tasks' : `/api/tasks/${id}`, {
  method: id === 'new' ? 'POST' : 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ ...task, expected }),
});
