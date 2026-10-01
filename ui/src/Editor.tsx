import { useEffect, useId, useRef, useState, type KeyboardEvent } from 'react';
import * as Dialog from '@radix-ui/react-dialog';
import { AlertCircle, Bell, Check, ChevronLeft, ChevronRight, LoaderCircle, Maximize2, Minimize2, Plus, Save, Trash2, X } from 'lucide-react';
import { fetchTask, request, RequestError, saveTask } from './api';
import { readStored, store } from './hooks';
import { Markdown } from './Markdown';
import { markdownKey } from './markdownKeys';
import { emptyTask, sameInput, UNSET, valueOf, withProperty, type Board, type Task, type TaskInput } from './types';

type Draft = { form: TaskInput; base: TaskInput };
type Props = {
  id: number | 'new'; board: Board; boardKey: string; preset: { key: string; value: string } | null;
  available: boolean; onClose: () => void; onSaved: (id: number) => void; onNotify: (id: number) => void;
  onOpen: (id: number) => void; onNotice: (message: string, error?: boolean) => void;
};
export function Editor({ id, board, boardKey, preset, available, onClose, onSaved, onNotify, onOpen, onNotice }: Props) {
  const [form, setForm] = useState<TaskInput | null>(null), [base, setBase] = useState<TaskInput | null>(null);
  const [current, setCurrent] = useState<Task | null>(null), [error, setError] = useState('');
  const [busy, setBusy] = useState(false), [conflict, setConflict] = useState<'changed' | 'removed' | 'restored' | null>(null);
  const [tab, setTab] = useState<'write' | 'preview'>(id === 'new' ? 'write' : 'preview');
  const [full, setFull] = useState(false), [narrow, setNarrow] = useState(() => window.matchMedia('(max-width: 900px)').matches);
  const [width, setWidth] = useState(() => readStored(`${boardKey}:panel-width`, 520));
  const [saved, setSaved] = useState(false);
  const formRef = useRef(form), baseRef = useRef(base), hasLoaded = useRef(false), suppressDraft = useRef(false);
  const subjectRef = useRef<HTMLInputElement>(null), saveButton = useRef<HTMLButtonElement>(null);
  const draftKey = `${boardKey}:draft:${id}`;
  const listId = useId();
  const summary = id === 'new' ? undefined : board.tasks.find(task => task.id === id);
  const revision = summary?.revision;
  formRef.current = form; baseRef.current = base;
  const dirty = !!form && !!base && !sameInput(form, base);

  useEffect(() => {
    const media = window.matchMedia('(max-width: 900px)');
    const changed = () => setNarrow(media.matches);
    media.addEventListener('change', changed);
    return () => media.removeEventListener('change', changed);
  }, []);
  useEffect(() => {
    const controller = new AbortController();
    async function load() {
      if (id !== 'new' && !summary && hasLoaded.current) { setConflict('removed'); return; }
      try {
        let task: TaskInput;
        if (id === 'new') {
          task = emptyTask();
          if (preset) task = withProperty(task, preset.key, preset.value);
        } else {
          task = await fetchTask(id, controller.signal);
        }
        if (controller.signal.aborted) return;
        if (id !== 'new') setCurrent(task as Task);
        if (!hasLoaded.current) {
          hasLoaded.current = true;
          const draft = readStored<Draft | null>(draftKey, null);
          setBase(task); baseRef.current = task;
          const restored = draft && !sameInput(draft.form, task);
          setForm(restored ? draft.form : task); formRef.current = restored ? draft.form : task;
          if (restored) setConflict('restored');
          if (id === 'new') setTimeout(() => subjectRef.current?.focus(), 0);
        } else if (!sameInput(task, baseRef.current!)) {
          if (sameInput(formRef.current!, baseRef.current!) || sameInput(formRef.current!, task)) {
            setForm(task); setBase(task); setConflict(null);
          } else setConflict('changed');
        }
        setError('');
      } catch (error) {
        if (controller.signal.aborted) return;
        const draft = readStored<Draft | null>(draftKey, null);
        if (error instanceof RequestError && error.status === 404 && draft) {
          hasLoaded.current = true; setForm(draft.form); setBase(draft.base); setConflict('removed');
        } else setError(error instanceof Error ? error.message : String(error));
      }
    }
    void load();
    return () => controller.abort();
  }, [id, revision]);

  // Closing or switching tasks flushes the last draft; each task has an independent slot.
  useEffect(() => {
    const persist = () => {
      if (suppressDraft.current) return;
      const draft = formRef.current, original = baseRef.current;
      if (draft && original) store(draftKey, sameInput(draft, original) ? null : { form: draft, base: original });
    };
    window.addEventListener('pagehide', persist);
    return () => { persist(); window.removeEventListener('pagehide', persist); };
  }, [draftKey]);
  useEffect(() => {
    const timer = setTimeout(() => {
      if (!suppressDraft.current && form && base) store(draftKey, dirty ? { form, base } : null);
    }, 350);
    return () => clearTimeout(timer);
  }, [form, base, dirty, draftKey]);

  async function save(asNew = false) {
    if (!form || busy || !available) return;
    if (!form.subject.trim()) { subjectRef.current?.focus(); setError('Give this task a subject before saving.'); return; }
    setBusy(true); setError('');
    try {
      const result = await saveTask(asNew ? 'new' : id, { ...form, subject: form.subject.trim(), task_type: form.task_type.trim() || 'Task', properties: form.properties.filter(p => p.key.trim()).map(p => ({ key: p.key.trim(), value: p.value })) }, asNew || id === 'new' ? undefined : base!);
      const { warning, ...task } = result;
      setForm(task); setBase(task); setCurrent(task); setConflict(null); setSaved(true);
      formRef.current = task; baseRef.current = task; store(draftKey, null);
      if (id === 'new' || asNew) suppressDraft.current = true;
      if (warning) onNotice(warning);
      onSaved(task.id);
      setTimeout(() => setSaved(false), 2500);
    } catch (error) { setError(error instanceof Error ? error.message : String(error)); }
    finally { setBusy(false); }
  }
  async function closeTask() {
    if (id === 'new' || busy) return;
    if (!confirm('Close this completed task and remove it from the board? Git history keeps committed task text.')) return;
    setBusy(true); setError('');
    try {
      const result = await request<{ warning?: string }>(`/api/tasks/${id}`, { method: 'DELETE' });
      suppressDraft.current = true; store(draftKey, null);
      onNotice(result.warning || 'Task closed.'); onSaved(id); onClose();
    } catch (error) { setError(error instanceof Error ? error.message : String(error)); }
    finally { setBusy(false); }
  }
  function textKey(event: KeyboardEvent<HTMLTextAreaElement>) {
    const input = event.currentTarget;
    const edit = markdownKey(input.value, input.selectionStart, input.selectionEnd, event.key, event.ctrlKey || event.metaKey, event.shiftKey);
    if (!edit) return;
    event.preventDefault(); setForm(form => form ? { ...form, body: edit.value } : form);
    requestAnimationFrame(() => { input.selectionStart = edit.start; input.selectionEnd = edit.end; });
  }
  const keys = [...new Set(['Priority', 'Assigned To', 'Tags', 'Due Date', 'Depends On', ...board.tasks.flatMap(task => task.properties.map(p => p.key))])].filter(key => !['Status', 'Last Updated'].includes(key));
  const values = (key: string) => [...new Set(board.tasks.flatMap(task => task.properties.filter(p => p.key === key).map(p => p.value)))];
  const index = board.tasks.findIndex(task => task.id === id);
  const expanded = full || narrow;
  const content = <>
    <header className="editor-header">
      <div className="editor-heading"><span className="eyebrow">{id === 'new' ? 'CREATE SOMETHING GOOD' : `TASK #${id}`}</span><Dialog.Title asChild><h2>{id === 'new' ? 'New task' : 'Task details'}</h2></Dialog.Title></div>
      <div className="editor-controls">
        {id !== 'new' && <><button className="icon-button" disabled={index <= 0 || busy} onClick={() => onOpen(board.tasks[index - 1].id)} aria-label="Previous task"><ChevronLeft size={17} /></button><button className="icon-button" disabled={index < 0 || index === board.tasks.length - 1 || busy} onClick={() => onOpen(board.tasks[index + 1].id)} aria-label="Next task"><ChevronRight size={17} /></button></>}
        {!narrow && <button className="icon-button" aria-label={full ? 'Dock task panel' : 'Expand task view'} onClick={() => setFull(!full)}>{full ? <Minimize2 size={17} /> : <Maximize2 size={17} />}</button>}
        <button className="icon-button" aria-label="Close task view" disabled={busy} onClick={onClose}><X size={19} /></button>
      </div>
    </header>
    <Dialog.Description className="sr-only">View and edit a task. Unsaved changes are kept as a draft when you close this view.</Dialog.Description>
    {form ? <form className="editor-form" onSubmit={event => { event.preventDefault(); void save(); }} onKeyDown={event => {
      if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') { event.preventDefault(); saveButton.current?.click(); }
      if (event.key === 'Escape' && !busy) { event.preventDefault(); onClose(); }
    }}>
      <div className="editor-scroll">
        {!available && <div className="conflict-banner"><AlertCircle size={18} /><div>The board cannot be read. Saving is paused; your draft is kept.</div></div>}
        {conflict && <div className="conflict-banner"><AlertCircle size={18} /><div><p>{conflict === 'removed' ? 'This task was removed from the board. Your draft is still here.' : conflict === 'restored' ? 'Your unsaved draft was restored.' : 'This task changed outside this view. Your draft is untouched.'}</p>
          <div className="banner-actions">{current && conflict !== 'removed' && <><button type="button" onClick={() => { setForm(current); setBase(current); setConflict(null); store(draftKey, null); }}>Reload from file</button><button type="button" onClick={() => { setBase(current); setConflict(null); }}>Keep my draft</button></>}
            {conflict === 'removed' && <button type="button" onClick={() => void save(true)}>Save as a new task</button>}</div></div></div>}
        <label className="field subject-field"><span>Subject</span><input ref={subjectRef} className="subject-input" required value={form.subject} onChange={event => setForm({ ...form, subject: event.target.value })} placeholder="What needs to happen?" /></label>
        <div className="task-metadata">
          <label className="field"><span>Type</span><input list={`${listId}-types`} value={form.task_type} onChange={event => setForm({ ...form, task_type: event.target.value })} /><datalist id={`${listId}-types`}>{[...new Set(['Task', 'Bug', 'Feature', ...board.tasks.map(task => task.task_type)])].map(type => <option key={type} value={type} />)}</datalist></label>
          <label className="field"><span>Status</span><input list={`${listId}-statuses`} value={valueOf(form, 'Status') === UNSET ? '' : valueOf(form, 'Status')} onChange={event => setForm(withProperty(form, 'Status', event.target.value))} placeholder="No status" /><datalist id={`${listId}-statuses`}>{values('Status').map(value => <option key={value} value={value} />)}</datalist></label>
        </div>
        <div className="properties-section"><div className="section-label"><span>PROPERTIES</span><button className="text-button" type="button" onClick={() => setForm({ ...form, properties: [...form.properties, { key: '', value: '' }] })}><Plus size={14} />Add property</button></div>
          <datalist id={`${listId}-keys`}>{keys.map(key => <option key={key} value={key} />)}</datalist>
          {form.properties.map((property, at) => ['Status', 'Last Updated'].includes(property.key) ? null : <div className="property-row" key={at}>
            <input aria-label="Property name" list={`${listId}-keys`} placeholder="Property name" value={property.key} onChange={event => setForm({ ...form, properties: form.properties.map((p, i) => i === at ? { ...p, key: event.target.value } : p) })} />
            <input aria-label={`${property.key || 'Property'} value`} list={`${listId}-values-${at}`} placeholder="Value" value={property.value} onChange={event => setForm({ ...form, properties: form.properties.map((p, i) => i === at ? { ...p, value: event.target.value } : p) })} />
            <datalist id={`${listId}-values-${at}`}>{values(property.key).map(value => <option key={value} value={value} />)}</datalist>
            <button type="button" className="icon-button" aria-label={`Remove ${property.key || 'property'}`} onClick={() => setForm({ ...form, properties: form.properties.filter((_, i) => i !== at) })}><X size={15} /></button>
          </div>)}
        </div>
        <div className="description-section"><div className="description-heading"><span className="section-label">DESCRIPTION</span><div className="segmented"><button type="button" aria-pressed={tab === 'write'} onClick={() => setTab('write')}>Write</button><button type="button" aria-pressed={tab === 'preview'} onClick={() => setTab('preview')}>Preview</button></div></div>
          {tab === 'write' ? <><label className="sr-only" htmlFor={`${listId}-body`}>Body</label><textarea id={`${listId}-body`} className="body-input" value={form.body} onChange={event => setForm({ ...form, body: event.target.value })} onKeyDown={textKey} placeholder="Add context, a plan, or the details that matter…" /><p className="editor-tip">Markdown supported · ⌘ / Ctrl + Enter to save</p></> : form.body ? <Markdown text={form.body} /> : <button type="button" className="description-empty" onClick={() => setTab('write')}>Add a description…</button>}
        </div>
        {valueOf(form, 'Last Updated') !== UNSET && <p className="updated-at">Last updated {valueOf(form, 'Last Updated').replace('T', ' ').replace('Z', ' UTC')}</p>}
      </div>
      <footer className="editor-footer">{error && <p className="form-error" role="alert">{error}</p>}
        <div className="editor-save-row"><span className={`draft-status ${saved ? 'saved' : ''}`}>{saved ? <><Check size={14} />Saved</> : dirty ? 'Unsaved draft' : 'All changes saved'}</span>
          <button className="button primary" ref={saveButton} disabled={busy || !available || conflict === 'changed' || conflict === 'removed'}>{busy ? <LoaderCircle className="spin" size={16} /> : <Save size={16} />}Save task</button></div>
        <div className="editor-secondary-actions">{id !== 'new' && <button className="text-button" type="button" disabled={busy} onClick={() => onNotify(id)}><Bell size={14} />Notify</button>}
          {dirty && <button className="text-button" type="button" disabled={busy} onClick={() => { suppressDraft.current = true; store(draftKey, null); onClose(); }}>Discard draft</button>}
          {id !== 'new' && <button className="text-button danger-text" type="button" disabled={busy || !available || dirty} onClick={() => void closeTask()}><Trash2 size={14} />Close completed task</button>}</div>
      </footer>
    </form> : <div className="editor-loading">{error ? <><AlertCircle size={24} /><p role="alert">{error}</p><button className="button" onClick={onClose}>Return to board</button></> : <><LoaderCircle size={24} className="spin" /><p>Opening task…</p></>}</div>}
  </>;
  return <Dialog.Root open onOpenChange={open => { if (!open && !busy) onClose(); }} modal={expanded}>
    {expanded ? <Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className={`editor-panel expanded ${full && !narrow ? 'fullscreen' : ''}`} onEscapeKeyDown={event => { if (busy) event.preventDefault(); }}>{content}</Dialog.Content></Dialog.Portal> :
      <aside className="editor-panel docked" aria-label="Task details" style={{ width: Math.min(width, window.innerWidth * .6) }}>
        <div className="panel-resizer" role="separator" aria-label="Resize task panel" aria-orientation="vertical" aria-valuenow={width} tabIndex={0}
          onDoubleClick={() => { setWidth(520); store(`${boardKey}:panel-width`, 520); }}
          onKeyDown={event => { if (!['ArrowLeft', 'ArrowRight'].includes(event.key)) return; event.preventDefault(); const next = Math.max(380, Math.min(900, width + (event.key === 'ArrowLeft' ? 24 : -24))); setWidth(next); store(`${boardKey}:panel-width`, next); }}
          onPointerDown={event => event.currentTarget.setPointerCapture(event.pointerId)}
          onPointerMove={event => { if (!event.currentTarget.hasPointerCapture(event.pointerId)) return; const next = Math.max(380, Math.min(900, window.innerWidth - event.clientX)); setWidth(next); store(`${boardKey}:panel-width`, next); }}
          onPointerUp={event => event.currentTarget.releasePointerCapture(event.pointerId)} />
        {content}
      </aside>}
  </Dialog.Root>;
}
