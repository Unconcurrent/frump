import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { AlertCircle, Columns3, Command, LayoutList, LoaderCircle, Moon, Plus, RefreshCw, Search, SlidersHorizontal, Sun, X } from 'lucide-react';
import { fetchTask, saveTask } from './api';
import { BoardView, ListView } from './Board';
import { Editor } from './Editor';
import { Notify } from './Notify';
import { parseRoute, store, useBoard, useRoute, useSearch, useStored } from './hooks';
import { migrateBrowserStorage } from './migrate';
import { titleOf, UNSET, valueOf, withProperty, type Board, type Summary } from './types';

type Preferences = {
  group: string; sort: string; type: string; assignee: string; query: string;
  theme: 'auto' | 'light' | 'dark'; compact: boolean; hideEmpty: boolean; view: 'board' | 'list';
  scope: 'all' | 'done'; order: Record<string, string[]>; collapsed: Record<string, string[]>;
};
const defaults: Preferences = { group: 'Status', sort: 'manual', type: '', assignee: '', query: '', theme: 'auto', compact: false, hideEmpty: false, view: 'board', scope: 'all', order: {}, collapsed: {} };

export function App() {
  const { board, error, refresh } = useBoard();
  if (!board) return <div className="boot-screen"><div className="brand-mark">f<span>.</span></div><h1>Frump</h1>{error ? <><p role="alert">{error}</p><button className="button primary" onClick={refresh}><RefreshCw size={16} />Try again</button></> : <><LoaderCircle size={22} className="spin" /><p>Getting your board ready…</p></>}</div>;
  return <BoardApp key={titleOf(board.header)} board={board} error={error} refresh={refresh} />;
}

function BoardApp({ board, error, refresh }: { board: Board; error: string; refresh: () => void }) {
  const title = titleOf(board.header), boardKey = `frump:${location.origin}:${title}`;
  useState(() => { migrateBrowserStorage(title, boardKey); });
  const [prefs, setPrefs] = useStored<Preferences>(`${boardKey}:view`, defaults);
  const [effectiveTheme, setEffectiveTheme] = useState<'light' | 'dark'>(() => prefs.theme === 'auto' ? window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light' : prefs.theme);
  const { route, navigate } = useRoute();
  const [notice, setNotice] = useState<{ text: string; error: boolean } | null>(null);
  const [preset, setPreset] = useState<{ key: string; value: string } | null>(null);
  const [moving, setMoving] = useState<number | null>(null);
  const searchRef = useRef<HTMLInputElement>(null), draggingGroup = useRef(false);
  useEffect(() => {
    const start = () => { draggingGroup.current = true; }, end = () => { draggingGroup.current = false; };
    document.addEventListener('frump:group-drag-start', start); document.addEventListener('frump:group-drag-end', end);
    return () => { document.removeEventListener('frump:group-drag-start', start); document.removeEventListener('frump:group-drag-end', end); };
  }, []);
  const { ids, pending, error: searchError } = useSearch(prefs.query, board.revision);
  const filter = (patch: Partial<Preferences>) => setPrefs(current => ({ ...defaults, ...current, ...patch }));
  const announce = useCallback((text: string, error = false) => setNotice({ text, error }), []);
  const open = useCallback((id: number) => navigate({ task: id, notify: null }, route.task !== null), [navigate, route.task]);
  const close = useCallback(() => navigate({ task: null, notify: null }, true), [navigate]);
  const notify = useCallback((id: number) => navigate({ task: route.task === null ? null : id, notify: id }), [navigate, route.task]);
  function create(key?: string) {
    setPreset(key === undefined ? null : { key: prefs.group, value: key }); navigate({ task: 'new', notify: null }, route.task !== null);
  }
  useEffect(() => { document.title = `${title} · Frump`; }, [title]);
  useLayoutEffect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    const apply = () => {
      const theme = prefs.theme === 'auto' ? media.matches ? 'dark' : 'light' : prefs.theme;
      document.documentElement.dataset.theme = theme; setEffectiveTheme(theme);
      store('frump:theme', prefs.theme);
      document.querySelector('meta[name="theme-color"]')?.setAttribute('content', theme === 'dark' ? '#15181f' : '#f7f8fa');
    };
    apply(); media.addEventListener('change', apply);
    return () => media.removeEventListener('change', apply);
  }, [prefs.theme]);
  useEffect(() => {
    const keys = (event: globalThis.KeyboardEvent) => {
      const target = event.target as HTMLElement;
      if (['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName) || target.isContentEditable || event.ctrlKey || event.metaKey || event.altKey) return;
      if (route.notify !== null || draggingGroup.current) return;
      if (event.key === '/') { event.preventDefault(); searchRef.current?.focus(); }
      if (event.key === 'n' && !error) { event.preventDefault(); create(); }
      if (event.key === 'Escape' && route.task !== null) close();
    };
    document.addEventListener('keydown', keys);
    return () => document.removeEventListener('keydown', keys);
  }, [route, error, prefs.group]);

  const { keys, types, assignees } = useMemo(() => {
    const keys = new Set(['Status']), types = new Set<string>(), assignees = new Set<string>();
    for (const task of board.tasks) {
      types.add(task.task_type);
      for (const p of task.properties) { keys.add(p.key); if (p.key === 'Assigned To') assignees.add(p.value); }
    }
    return { keys: [...keys], types: [...types].sort(), assignees: [...assignees].sort() };
  }, [board.tasks]);
  const group = keys.includes(prefs.group) ? prefs.group : 'Status';
  const tasks = useMemo(() => {
    const matched = ids === null ? null : new Set(ids);
    const tasks = board.tasks.filter(task => (!prefs.type || task.task_type === prefs.type)
      && (!prefs.assignee || valueOf(task, 'Assigned To') === prefs.assignee)
      && (prefs.scope !== 'done' || valueOf(task, 'Status') === 'done')
      && (!prefs.query.trim() || matched?.has(task.id)));
    const comparators: Record<string, (a: Summary, b: Summary) => number> = {
      id: (a, b) => a.id - b.id, 'id-desc': (a, b) => b.id - a.id,
      subject: (a, b) => a.subject.localeCompare(b.subject), type: (a, b) => a.task_type.localeCompare(b.task_type) || a.id - b.id,
      updated: (a, b) => {
        const x = valueOf(a, 'Last Updated'), y = valueOf(b, 'Last Updated');
        if (x === UNSET) return y === UNSET ? a.id - b.id : 1;
        if (y === UNSET) return -1;
        return y.localeCompare(x) || a.id - b.id;
      },
    };
    if (comparators[prefs.sort]) tasks.sort(comparators[prefs.sort]);
    return tasks;
  }, [board.tasks, ids, prefs.type, prefs.assignee, prefs.scope, prefs.query, prefs.sort]);
  const done = board.tasks.filter(task => valueOf(task, 'Status') === 'done').length;
  const progress = board.tasks.length ? Math.round(done / board.tasks.length * 100) : 0;
  async function move(id: number, key: string) {
    const summary = board.tasks.find(task => task.id === id);
    if (!summary || valueOf(summary, group) === key || error || moving !== null) return;
    setMoving(id);
    try {
      const task = await fetchTask(id);
      const result = await saveTask(id, withProperty(task, group, key), task);
      if (result.warning) announce(result.warning);
      refresh();
    } catch (error) { announce(error instanceof Error ? error.message : String(error), true); }
    finally { setMoving(null); }
  }
  const storageKey = `${boardKey}:layout:${group}`;
  return <div className="app-shell">
    <div className="main-layout">
      <section className="page-heading"><div className="heading-content"><div className="heading-eyebrow">YOUR WORK, IN VIEW</div>
        <div className="heading-title-row"><h1>{prefs.scope === 'done' ? 'Completed tasks' : 'Project board'}<span>{board.tasks.length}</span></h1>
          <div className="board-progress"><div className="progress-meter"><span>Board progress</span><div className="progress-track" role="progressbar" aria-label="Board progress" aria-valuemin={0} aria-valuemax={100} aria-valuenow={progress} aria-valuetext={`${progress}%, ${done} of ${board.tasks.length} tasks complete`}><span style={{ width: `${progress}%` }} /></div><strong>{progress}%</strong></div><span className="progress-detail">{done} of {board.tasks.length} tasks complete</span></div>
        </div><p>{title}</p>
      </div><button className="button primary new-task" disabled={!!error} onClick={() => create()}><Plus size={17} />New task<kbd>N</kbd></button></section>
      <div className="viewbar"><div className="view-tabs" role="group" aria-label="View"><button aria-pressed={prefs.view === 'board'} className={prefs.view === 'board' ? 'active' : ''} onClick={() => filter({ view: 'board' })}><Columns3 size={16} />Board</button><button aria-pressed={prefs.view === 'list'} className={prefs.view === 'list' ? 'active' : ''} onClick={() => filter({ view: 'list' })}><LayoutList size={17} />List</button></div><span className="viewbar-hint">{tasks.length} task{tasks.length === 1 ? '' : 's'} in view</span></div>
      <div className="toolbar"><div className="search-field"><Search size={17} /><input ref={searchRef} aria-label="Search tasks" placeholder="Search tasks…" value={prefs.query} onChange={event => filter({ query: event.target.value })} />{pending ? <LoaderCircle size={14} className="spin" /> : prefs.query ? <button className="icon-button" aria-label="Clear search" onClick={() => filter({ query: '' })}><X size={14} /></button> : <kbd>/</kbd>}</div>
        <label className="filter-select"><span>Type</span><select aria-label="Filter by type" value={prefs.type} onChange={event => filter({ type: event.target.value })}><option value="">All types</option>{types.map(type => <option key={type}>{type}</option>)}</select></label>
        <label className="filter-select assignee-filter"><span>Assignee</span><select aria-label="Filter by assignee" value={prefs.assignee} onChange={event => filter({ assignee: event.target.value })}><option value="">Anyone</option>{assignees.map(name => <option key={name}>{name}</option>)}</select></label>
        <span className="toolbar-spacer" /><label className="filter-select"><span>Group</span><select aria-label="Group by" value={group} onChange={event => filter({ group: event.target.value })}>{keys.map(key => <option key={key}>{key}</option>)}</select></label>
        <label className="filter-select"><span>Sort</span><select aria-label="Sort tasks" value={prefs.sort} onChange={event => filter({ sort: event.target.value })}><option value="manual">File order</option><option value="id">Number ↑</option><option value="id-desc">Number ↓</option><option value="subject">Subject A–Z</option><option value="type">Type</option><option value="updated">Last updated</option></select></label>
        <details className="view-options"><summary className="button secondary"><SlidersHorizontal size={16} /><span>Display</span></summary><div className="options-popover"><strong>Board appearance</strong><label><input type="checkbox" checked={prefs.scope === 'done'} onChange={event => filter({ scope: event.target.checked ? 'done' : 'all' })} />Completed tasks only</label><label><input type="checkbox" checked={prefs.compact} onChange={event => filter({ compact: event.target.checked })} />Compact cards</label><label><input type="checkbox" checked={prefs.hideEmpty} onChange={event => filter({ hideEmpty: event.target.checked })} />Hide empty columns</label><label className="theme-option"><span>Theme</span><select aria-label="Color theme" value={prefs.theme} onChange={event => filter({ theme: event.target.value as Preferences['theme'] })}><option value="auto">System</option><option value="light">Light</option><option value="dark">Dark</option></select></label><button onClick={() => filter({ order: {}, collapsed: {} })}>Reset column layout</button></div></details>
      </div>
      {(prefs.type || prefs.assignee || prefs.query || prefs.scope !== 'all') && <div className="active-filters"><span>Showing {tasks.length} of {board.tasks.length} tasks</span><button onClick={() => filter({ type: '', assignee: '', query: '', scope: 'all' })}>Clear filters<X size={12} /></button></div>}
      {(error || searchError || notice) && <div className={`notice ${(error || searchError || notice?.error) ? 'error' : ''}`} role={(error || searchError || notice?.error) ? 'alert' : 'status'}><AlertCircle size={17} /><span>{error || searchError || notice?.text}</span>{notice && !error && !searchError && <button className="icon-button" aria-label="Dismiss notice" onClick={() => setNotice(null)}><X size={15} /></button>}</div>}
      <div className="content-area"><main className={`board-area ${moving !== null ? 'saving-move' : ''}`} aria-label="Task board">
        {prefs.view === 'board' ? <BoardView tasks={tasks} allTasks={board.tasks} group={group} compact={prefs.compact} selected={route.task} hideEmpty={prefs.hideEmpty} order={prefs.order[group] || []} collapsed={prefs.collapsed[group] || []} storageKey={storageKey} onOpen={open} onNotify={notify} onNew={create} onMove={(id, key) => void move(id, key)} onLayout={(order, collapsed) => filter({ order: { ...prefs.order, [group]: order }, collapsed: { ...prefs.collapsed, [group]: collapsed } })} /> : <ListView tasks={tasks} selected={route.task} onOpen={open} onNotify={notify} storageKey={storageKey} />}
      </main>{route.task !== null && <Editor key={route.task} id={route.task} board={board} boardKey={boardKey} preset={preset} available={!error} onClose={close} onOpen={open} onNotify={notify} onNotice={announce} onSaved={id => {
        refresh();
        const current = parseRoute(location.hash);
        if (current.task === route.task && (current.task === 'new' || current.task !== id)) navigate({ task: id, notify: null }, true);
      }} />}</div>
      <footer className="board-footer"><span className="footer-context"><strong title={title}>{title}</strong><span className="footer-dot">·</span><span className={`footer-connection ${error ? 'offline' : ''}`}><span />{error ? 'Disconnected' : 'Local board'}</span></span><span className="footer-shortcuts"><Command size={12} />/ to search<span className="footer-dot">·</span>N to create<span className="footer-dot">·</span>Alt + ← → to move tasks</span><div className="footer-controls"><button className="icon-button" title="Refresh board" aria-label="Refresh board" onClick={refresh}><RefreshCw size={16} className={moving !== null ? 'spin' : ''} /></button><button className="icon-button" title={`Theme: ${prefs.theme}. Switch to ${effectiveTheme === 'dark' ? 'light' : 'dark'}.`} aria-label="Change theme" onClick={() => filter({ theme: effectiveTheme === 'dark' ? 'light' : 'dark' })}>{effectiveTheme === 'dark' ? <Moon size={17} /> : <Sun size={17} />}</button></div></footer>

    </div>
    {route.notify !== null && <Notify key={route.notify} id={route.notify} board={board} boardKey={boardKey} onClose={() => navigate({ task: route.task, notify: null }, true)} onNotice={announce} />}
  </div>;
}
