import { useCallback, useEffect, useRef, useState } from 'react';
import { fetchBoard, request } from './api';
import { mergeUpdate, type Board, type Route } from './types';

export function useBoard() {
  const [board, setBoard] = useState<Board | null>(null);
  const [error, setError] = useState('');
  const revision = useRef<number>(undefined);
  const refreshRef = useRef<() => void>(() => {});
  useEffect(() => {
    let disposed = false, timer: ReturnType<typeof setTimeout>, active: AbortController | null = null, queued = false, dragging = false;
    async function refresh() {
      clearTimeout(timer);
      if (disposed || document.hidden) return;
      if (dragging) { timer = setTimeout(refresh, 2000); return; }
      if (active) { queued = true; return; }
      const controller = new AbortController();
      active = controller;
      try {
        const update = await fetchBoard(revision.current, controller.signal);
        if (disposed) return;
        if (update) { revision.current = update.revision; setBoard(current => mergeUpdate(current, update)); }
        setError('');
      } catch (error) {
        if (!controller.signal.aborted) setError(error instanceof Error ? error.message : String(error));
      } finally {
        active = null;
        if (!disposed) { timer = setTimeout(refresh, queued ? 0 : 2000); queued = false; }
      }
    }
    refreshRef.current = () => { void refresh(); };
    const visibility = () => {
      if (document.hidden) clearTimeout(timer);
      else void refresh();
    };
    document.addEventListener('visibilitychange', visibility);
    const dragStarted = () => { dragging = true; };
    const dragEnded = () => { dragging = false; void refresh(); };
    document.addEventListener('dragstart', dragStarted);
    document.addEventListener('dragend', dragEnded);
    document.addEventListener('frump:group-drag-start', dragStarted);
    document.addEventListener('frump:group-drag-end', dragEnded);
    void refresh();
    return () => {
      disposed = true; clearTimeout(timer); active?.abort();
      document.removeEventListener('visibilitychange', visibility);
      document.removeEventListener('dragstart', dragStarted); document.removeEventListener('dragend', dragEnded);
      document.removeEventListener('frump:group-drag-start', dragStarted); document.removeEventListener('frump:group-drag-end', dragEnded);
    };
  }, []);
  return { board, error, refresh: useCallback(() => refreshRef.current(), []) };
}

export function parseRoute(hash: string): Route {
  const task = hash.match(/^#task\/(new|\d+)(\/notify)?$/);
  if (task) return { task: task[1] === 'new' ? 'new' : Number(task[1]), notify: task[2] && task[1] !== 'new' ? Number(task[1]) : null };
  const notify = hash.match(/^#notify\/(\d+)$/);
  return { task: null, notify: notify ? Number(notify[1]) : null };
}
export function routeHash(route: Route) {
  if (route.task !== null) return `#task/${route.task}${route.notify === null ? '' : '/notify'}`;
  return route.notify === null ? '' : `#notify/${route.notify}`;
}
export function useRoute() {
  const [route, setRoute] = useState(() => parseRoute(location.hash));
  useEffect(() => {
    const changed = () => setRoute(parseRoute(location.hash));
    window.addEventListener('popstate', changed);
    window.addEventListener('hashchange', changed);
    return () => { window.removeEventListener('popstate', changed); window.removeEventListener('hashchange', changed); };
  }, []);
  const navigate = useCallback((next: Route, replace = false) => {
    const hash = routeHash(next);
    if (hash !== location.hash) history[replace ? 'replaceState' : 'pushState'](null, '', `${location.pathname}${location.search}${hash}`);
    setRoute(next);
  }, []);
  return { route, navigate };
}

export function readStored<T>(key: string, fallback: T): T {
  try { return JSON.parse(localStorage.getItem(key) ?? 'null') ?? fallback; } catch { return fallback; }
}
export function store(key: string, value: unknown) {
  try { value === null ? localStorage.removeItem(key) : localStorage.setItem(key, JSON.stringify(value)); } catch { /* Private browser storage can be unavailable. */ }
}
export function useStored<T>(key: string, fallback: T) {
  const [entry, setEntry] = useState(() => ({ key, value: readStored(key, fallback) }));
  const value = entry.key === key ? entry.value : readStored(key, fallback);
  useEffect(() => { if (entry.key === key) store(key, entry.value); }, [key, entry]);
  const setValue = (update: T | ((previous: T) => T)) => setEntry(previous => ({
    key, value: typeof update === 'function' ? (update as (previous: T) => T)(previous.key === key ? previous.value : readStored(key, fallback)) : update,
  }));
  return [value, setValue] as const;
}
export function useSearch(query: string, revision: number) {
  const [result, setResult] = useState<{ query: string; ids: number[] } | null>(null);
  const [error, setError] = useState('');
  useEffect(() => {
    setError('');
    if (!query.trim()) { setResult(null); return; }
    const controller = new AbortController();
    const timer = setTimeout(() => {
      request<number[]>(`/api/search?q=${encodeURIComponent(query)}`, { signal: controller.signal }).then(ids => {
        setResult({ query, ids }); setError('');
      }).catch(error => { if (!controller.signal.aborted) setError(error.message); });
    }, 180);
    return () => { clearTimeout(timer); controller.abort(); };
  }, [query, revision]);
  return { ids: result?.query === query ? result.ids : null, pending: !!query.trim() && result?.query !== query, error };
}
