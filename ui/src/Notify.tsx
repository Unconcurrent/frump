import { useId, useRef, useState } from 'react';
import * as Dialog from '@radix-ui/react-dialog';
import { Bell, LoaderCircle, Send, X } from 'lucide-react';
import { request } from './api';
import { readStored, store } from './hooks';
import { UNSET, valueOf, type Board } from './types';

export function Notify({ id, board, boardKey, onClose, onNotice }: { id: number; board: Board; boardKey: string; onClose: () => void; onNotice: (message: string, error?: boolean) => void }) {
  const task = board.tasks.find(task => task.id === id);
  const assignee = task ? valueOf(task, 'Assigned To') : UNSET;
  const key = `${boardKey}:notify:${id}`;
  const [form, setForm] = useState(() => readStored(key, { recipient: assignee === UNSET ? board.team[0]?.name || '' : assignee, message: '' }));
  const [busy, setBusy] = useState(false), [error, setError] = useState('');
  const ids = useId(), recipientRef = useRef<HTMLInputElement>(null);
  const update = (next: typeof form) => { setForm(next); store(key, next); };
  return <Dialog.Root open modal onOpenChange={open => { if (!open && !busy) onClose(); }}><Dialog.Portal><Dialog.Overlay className="dialog-overlay" /><Dialog.Content className="notify-dialog" onOpenAutoFocus={event => { event.preventDefault(); recipientRef.current?.focus(); }} onEscapeKeyDown={event => { if (busy) event.preventDefault(); }} onInteractOutside={event => { if (busy) event.preventDefault(); }}>
    <header className="notify-header"><span className="notify-icon"><Bell size={20} /></span><div><Dialog.Title>Send a notification</Dialog.Title><Dialog.Description>#{id} · {task?.subject || 'Task'}</Dialog.Description></div><button className="icon-button" disabled={busy} aria-label="Close notification" onClick={onClose}><X size={18} /></button></header>
    <form onSubmit={async event => {
      event.preventDefault(); setBusy(true); setError('');
      try {
        await request(`/api/tasks/${id}/notify`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(form) });
        store(key, null); onNotice('Notification sent.'); onClose();
      } catch (error) { setError(error instanceof Error ? error.message : String(error)); }
      finally { setBusy(false); }
    }}>
      <div className="notify-body"><label className="field"><span id={`${ids}-recipient-label`}>Recipient</span><input ref={recipientRef} aria-labelledby={`${ids}-recipient-label`} aria-describedby={`${ids}-recipient-hint`} required value={form.recipient} onChange={event => update({ ...form, recipient: event.target.value })} placeholder="Crew member or team" autoComplete="off" /><small id={`${ids}-recipient-hint`}>A Metateam target, such as a crew member or all.</small></label>
        <label className="field"><span id={`${ids}-message-label`}>Message</span><textarea aria-labelledby={`${ids}-message-label`} aria-describedby={`${ids}-message-hint`} required value={form.message} onChange={event => update({ ...form, message: event.target.value })} placeholder="What would you like them to know?" /><small id={`${ids}-message-hint`}>The task number and subject are included automatically.</small></label>
        {error && <p className="form-error" role="alert">{error}</p>}</div>
      <footer className="notify-footer"><button type="button" className="button secondary" disabled={busy} onClick={onClose}>Cancel</button><button className="button primary" disabled={busy || !task}>{busy ? <LoaderCircle size={16} className="spin" /> : <Send size={16} />}Send notification</button></footer>
    </form>
  </Dialog.Content></Dialog.Portal></Dialog.Root>;
}
