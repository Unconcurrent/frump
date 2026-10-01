import { memo, useEffect, useRef, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { ArrowLeft, ArrowRight, Bell, ChevronDown, ChevronRight, FileText, Plus, SearchX } from 'lucide-react';
import { readStored, store } from './hooks';
import { columnLabel, UNSET, valueOf, type Summary } from './types';

const tones = ['violet', 'blue', 'amber', 'mint', 'rose', 'slate'];
export function toneFor(label: string) {
  if (/done|complete/i.test(label)) return 'mint';
  if (/work|progress|active/i.test(label)) return 'blue';
  if (/block|bug|urgent/i.test(label)) return 'rose';
  if (/review|pending/i.test(label)) return 'amber';
  return tones[[...label].reduce((n, c) => n + c.charCodeAt(0), 0) % tones.length];
}
export function initials(name: string) { return name.split(/[\s,:]+/).filter(Boolean).slice(0, 2).map(part => part[0]).join('').toUpperCase(); }

type CardProps = {
  task: Summary; selected: boolean; compact: boolean;
  onOpen: (id: number) => void; onNotify: (id: number) => void;
  onMove: (id: number, direction: number) => void;
};
const Card = memo(function Card({ task, selected, compact, onOpen, onNotify, onMove }: CardProps) {
  const assignee = valueOf(task, 'Assigned To'), priority = valueOf(task, 'Priority');
  const prior = useRef(task.revision);
  const [updated, setUpdated] = useState(false);
  useEffect(() => {
    if (prior.current === task.revision) return;
    prior.current = task.revision;
    setUpdated(true);
    const timer = setTimeout(() => setUpdated(false), 1300);
    return () => clearTimeout(timer);
  }, [task.revision]);
  return <article className={`task-card ${selected ? 'selected' : ''} ${updated ? 'updated' : ''} ${compact ? 'compact' : ''}`}
    draggable onDragStart={event => { event.dataTransfer.setData('application/frump-task', String(task.id)); event.dataTransfer.effectAllowed = 'move'; }}
    onKeyDown={event => {
      if (event.altKey && ['ArrowLeft', 'ArrowRight'].includes(event.key)) {
        event.preventDefault(); onMove(task.id, event.key === 'ArrowLeft' ? -1 : 1);
      }
    }} data-task={task.id}>
    <div className="card-top"><span className={`type-badge tone-${toneFor(task.task_type)}`}>{task.task_type}</span><span className="task-number">#{task.id}</span>
      <button className="icon-button card-notify" title="Notify about this task" aria-label={`Notify about task ${task.id}`} onClick={() => onNotify(task.id)}><Bell size={14} /></button>
    </div>
    <button className="card-title" onClick={() => onOpen(task.id)}>{task.subject}</button>
    {!compact && task.excerpt && <p className="card-excerpt">{task.excerpt.replace(/(?:^|\n)#+\s*|[*`]/g, '')}</p>}
    <div className="card-bottom">
      {priority !== UNSET && <span className={`priority ${/high|urgent/i.test(priority) ? 'high' : ''}`}><span />{priority}</span>}
      {assignee !== UNSET && <span className="card-assignee" title={assignee}><span className="avatar">{initials(assignee)}</span><span>{assignee}</span></span>}
      {assignee === UNSET && <span className="unassigned">Unassigned</span>}
    </div>
  </article>;
});

type ColumnProps = Omit<CardProps, 'task' | 'selected'> & {
  label: string; columnKey: string; tasks: Summary[]; selected: number | 'new' | null;
  collapsed: boolean; storageKey: string; onCollapse: () => void;
  onAdd: () => void; onDrop: (id: number) => void;
  onReorder: (direction: number) => void; first: boolean; last: boolean;
};
function Column({ label, columnKey, tasks, selected, compact, collapsed, storageKey, onCollapse, onAdd, onDrop, onReorder, first, last, ...cardProps }: ColumnProps) {
  const scroller = useRef<HTMLDivElement>(null);
  const [over, setOver] = useState(false);
  const scrollKey = `${storageKey}:scroll:${columnKey}`;
  const virtual = useVirtualizer({
    count: collapsed ? 0 : tasks.length, getScrollElement: () => scroller.current,
    estimateSize: () => compact ? 124 : 194, overscan: 4,
    getItemKey: index => tasks[index].id, initialOffset: () => readStored(scrollKey, 0),
  });
  return <section aria-label={`${label} column`} className={`board-column ${collapsed ? 'collapsed' : ''} ${over ? 'drop-target' : ''}`} onDragOver={event => {
    if (!event.dataTransfer.types.includes('application/frump-task')) return;
    event.preventDefault(); event.dataTransfer.dropEffect = 'move'; setOver(true);
  }} onDragLeave={event => { if (!event.currentTarget.contains(event.relatedTarget as Node)) setOver(false); }} onDrop={event => {
    event.preventDefault(); setOver(false);
    const id = Number(event.dataTransfer.getData('application/frump-task'));
    if (id) onDrop(id);
  }}>
    <div className="column-header"><span className={`status-dot tone-${toneFor(label)}`} />
      <button className="column-title" onClick={onCollapse} aria-expanded={!collapsed}>{label}</button>
      <span className="column-count">{tasks.length}</span>
      <div className="column-tools">
        {!collapsed && <><button className="icon-button" disabled={first} onClick={() => onReorder(-1)} aria-label={`Move ${label} column left`}><ArrowLeft size={13} /></button>
          <button className="icon-button" disabled={last} onClick={() => onReorder(1)} aria-label={`Move ${label} column right`}><ArrowRight size={13} /></button>
          <button className="icon-button" onClick={onAdd} aria-label={`Add task to ${label}`}><Plus size={16} /></button></>}
        <button className="icon-button" onClick={onCollapse} aria-label={`${collapsed ? 'Expand' : 'Collapse'} ${label} column`}>{collapsed ? <ChevronRight size={15} /> : <ChevronDown size={15} />}</button>
      </div>
    </div>
    {!collapsed && <div className="column-scroll" ref={scroller} onScroll={event => store(scrollKey, event.currentTarget.scrollTop)}>
      {tasks.length === 0 ? <button className="column-empty" onClick={onAdd}><Plus size={18} /><span>Add a task</span></button> :
        <div className="virtual-cards" style={{ height: virtual.getTotalSize() }}>
          {virtual.getVirtualItems().map(item => <div key={item.key} ref={virtual.measureElement} data-index={item.index} className="virtual-card" style={{ transform: `translateY(${item.start}px)` }}>
            <Card {...cardProps} task={tasks[item.index]} selected={selected === tasks[item.index].id} compact={compact} />
          </div>)}
        </div>}
    </div>}
  </section>;
}

export type BoardViewProps = {
  tasks: Summary[]; allTasks: Summary[]; group: string; compact: boolean; selected: number | 'new' | null;
  order: string[]; collapsed: string[]; hideEmpty: boolean; storageKey: string;
  onOpen: (id: number) => void; onNotify: (id: number) => void;
  onNew: (key: string) => void; onMove: (id: number, key: string) => void;
  onLayout: (order: string[], collapsed: string[]) => void;
};
export function BoardView({ tasks, allTasks, group, compact, selected, order, collapsed, hideEmpty, storageKey, onLayout, onOpen, onNotify, onNew, onMove }: BoardViewProps) {
  const scroller = useRef<HTMLDivElement>(null);
  const groups = new Map<string, Summary[]>();
  for (const task of allTasks) groups.set(valueOf(task, group), []);
  if (!groups.size) groups.set(group === 'Status' ? 'open' : UNSET, []);
  for (const task of tasks) groups.get(valueOf(task, group))?.push(task);
  const known = order.filter(key => groups.has(key));
  const knownSet = new Set(known);
  const keys = [...known, ...[...groups.keys()].filter(key => !knownSet.has(key))];
  const visible = hideEmpty ? keys.filter(key => groups.get(key)!.length) : keys;
  useEffect(() => { if (scroller.current) scroller.current.scrollLeft = readStored(`${storageKey}:horizontal`, 0); }, [storageKey]);
  const moveDirection = (id: number, direction: number) => {
    const task = allTasks.find(task => task.id === id);
    if (!task) return;
    const index = keys.indexOf(valueOf(task, group)) + direction;
    if (index >= 0 && index < keys.length) onMove(id, keys[index]);
  };
  if (allTasks.length > 0 && !tasks.length) return <Empty icon="search" title="No tasks match your filters" description="Try a different search or clear the filters to see your board." />;
  return <div className="board" ref={scroller} onScroll={event => store(`${storageKey}:horizontal`, event.currentTarget.scrollLeft)}>
    {visible.map((key, index) => <Column key={key} columnKey={key} label={columnLabel(key, group)} tasks={groups.get(key)!} selected={selected}
      compact={compact} collapsed={collapsed.includes(key)} storageKey={storageKey}
      first={index === 0} last={index === visible.length - 1} onOpen={onOpen} onNotify={onNotify} onMove={moveDirection}
      onCollapse={() => onLayout(keys, collapsed.includes(key) ? collapsed.filter(k => k !== key) : [...collapsed, key])}
      onAdd={() => onNew(key)} onDrop={id => onMove(id, key)} onReorder={direction => {
        const copy = [...keys], at = copy.indexOf(key), to = copy.indexOf(visible[index + direction]);
        if (to < 0) return;
        copy.splice(at, 1); copy.splice(to, 0, key); onLayout(copy, collapsed);
      }} />)}
  </div>;
}

export function Empty({ icon = 'file', title, description }: { icon?: 'file' | 'search'; title: string; description: string }) {
  return <div className="empty-state"><span className="empty-icon">{icon === 'search' ? <SearchX size={28} /> : <FileText size={28} />}</span><h2>{title}</h2><p>{description}</p></div>;
}

export function ListView({ tasks, selected, onOpen, onNotify, storageKey }: { tasks: Summary[]; selected: number | 'new' | null; onOpen: (id: number) => void; onNotify: (id: number) => void; storageKey: string }) {
  const scroller = useRef<HTMLDivElement>(null);
  const virtual = useVirtualizer({ count: tasks.length, getScrollElement: () => scroller.current, estimateSize: () => 64, overscan: 8, getItemKey: index => tasks[index].id, initialOffset: () => readStored(`${storageKey}:list`, 0) });
  if (!tasks.length) return <Empty icon="search" title="No tasks to show" description="Create a task or adjust your filters." />;
  return <div className="task-list"><div className="list-header"><span>Task</span><span>Status</span><span>Assignee</span><span>Priority</span><span /></div>
    <div className="list-scroll" ref={scroller} onScroll={event => store(`${storageKey}:list`, event.currentTarget.scrollTop)}><div style={{ height: virtual.getTotalSize(), position: 'relative' }}>
      {virtual.getVirtualItems().map(item => {
        const task = tasks[item.index], status = valueOf(task, 'Status'), assignee = valueOf(task, 'Assigned To'), priority = valueOf(task, 'Priority');
        return <div key={item.key} className={`list-row ${selected === task.id ? 'selected' : ''}`} style={{ transform: `translateY(${item.start}px)` }}>
          <div className="list-subject"><span className="task-number">#{task.id}</span><button onClick={() => onOpen(task.id)}>{task.subject}</button></div>
          <span className={`status-pill tone-${toneFor(status)}`}>{columnLabel(status, 'Status')}</span>
          <span className="list-assignee">{assignee === UNSET ? 'Unassigned' : assignee}</span><span className="list-priority">{priority === UNSET ? '—' : priority}</span>
          <button className="icon-button" onClick={() => onNotify(task.id)} aria-label={`Notify about task ${task.id}`}><Bell size={15} /></button>
        </div>;
      })}
    </div></div></div>;
}
