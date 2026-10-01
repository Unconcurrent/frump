import { readStored, store } from './hooks';
import { emptyTask, type TaskInput } from './types';

// Read the previous UI's browser storage once and move it into per-board/task slots.
// These values are viewer conveniences; Markdown remains the task authority.
export function migrateBrowserStorage(title: string, boardKey: string) {
  const prefs = readStored<Record<string, unknown> | null>('frump.board.prefs', null);
  if (prefs && !readStored(`${boardKey}:view`, null)) {
    const saved = (key: string) => (prefs[key] as Record<string, Record<string, string[]>> | undefined)?.[title] ?? {};
    store(`${boardKey}:view`, {
      group: prefs.groupBy ?? 'Status', sort: prefs.sort ?? 'manual', type: prefs.type ?? '',
      query: prefs.query ?? '', theme: prefs.theme ?? 'auto', compact: prefs.density === 'compact',
      hideEmpty: prefs.hideEmpty ?? false, view: 'board', scope: 'all', assignee: '',
      order: saved('order'), collapsed: saved('collapsed'),
    });
  }
  const draft = readStored<{ docKey: string; id: number | null; form: TaskInput & { status: string } } | null>('frump.board.draft', null);
  if (draft?.docKey === title && draft.form) {
    const key = `${boardKey}:draft:${draft.id ?? 'new'}`;
    if (!readStored(key, null)) {
      const { status, ...input } = draft.form;
      const properties = input.properties.filter(property => property.key !== 'Status');
      if (status) properties.push({ key: 'Status', value: status });
      const form = { ...input, properties };
      store(key, { form, base: emptyTask() });
    }
    if (readStored(key, null)) store('frump.board.draft', null);
  }
  const notify = readStored<{ docKey: string; id: number; recipient: string; message: string } | null>('frump.board.notify', null);
  if (notify?.docKey === title) {
    const key = `${boardKey}:notify:${notify.id}`;
    if (!readStored(key, null)) store(key, { recipient: notify.recipient, message: notify.message });
    if (readStored(key, null)) store('frump.board.notify', null);
  }
}
