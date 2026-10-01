import { useEffect, useRef, useState } from 'react';
import { EditorContent, useEditor, useEditorState } from '@tiptap/react';
import { Bold, Italic, Strikethrough, List, ListOrdered, ListTodo, Quote, Code2, Link2, ImagePlus, Table2, Undo2, Redo2, X, Check } from 'lucide-react';
import { markdownExtensions } from './richMarkdown';

export function MarkdownEditor({ value, onChange, disabled }: { value: string; onChange: (value: string) => void; disabled: boolean }) {
  const emitted = useRef(value), change = useRef(onChange);
  change.current = onChange;
  const [insert, setInsert] = useState<'link' | 'image' | null>(null), [url, setUrl] = useState('');
  const [urlError, setUrlError] = useState('');
  const [extensions] = useState(markdownExtensions);
  const editor = useEditor({
    extensions, content: value, contentType: 'markdown', editable: !disabled,
    editorProps: {
      attributes: { class: 'markdown rich-body', role: 'textbox', 'aria-label': 'Body', 'aria-multiline': 'true' },
      handleKeyDown: (_view, event) => {
        // The form owns saving. Stop the editor's Mod-Enter hard-break command
        // first, while letting the event reach the form's save handler.
        if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') { event.preventDefault(); return true; }
        if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== 'k') return false;
        event.preventDefault(); openInsert('link'); return true;
      },
      handlePaste: (_view, event) => {
        if (!editor || event.clipboardData?.getData('text/html')) return false;
        const text = event.clipboardData?.getData('text/plain');
        if (!text) return false;
        editor.chain().focus().insertContent(text, { contentType: 'markdown' }).run();
        return true;
      },
    },
    onUpdate: ({ editor }) => { emitted.current = editor.getMarkdown(); change.current(emitted.current); },
  });
  useEffect(() => {
    if (!editor || value === emitted.current) return;
    editor.commands.setContent(value, { contentType: 'markdown', emitUpdate: false });
    emitted.current = value;
  }, [editor, value]);
  useEffect(() => { editor?.setEditable(!disabled); }, [editor, disabled]);
  const state = useEditorState({ editor, selector: ({ editor }) => editor ? {
    bold: editor.isActive('bold'), italic: editor.isActive('italic'), strike: editor.isActive('strike'),
    bullet: editor.isActive('bulletList'), ordered: editor.isActive('orderedList'), task: editor.isActive('taskList'),
    quote: editor.isActive('blockquote'), code: editor.isActive('codeBlock'), link: editor.isActive('link'),
    heading: editor.isActive('heading') ? String(editor.getAttributes('heading').level) : '0',
    undo: editor.can().undo(), redo: editor.can().redo(), table: editor.isActive('table'),
  } : null });
  if (!editor || !state) return null;
  function openInsert(kind: 'link' | 'image') {
    setInsert(kind); setUrl(kind === 'link' ? editor!.getAttributes('link').href || '' : ''); setUrlError('');
  }
  function applyUrl() {
    const href = url.trim();
    if (insert === 'link' && !href) { editor!.chain().focus().extendMarkRange('link').unsetLink().run(); setInsert(null); return; }
    if (insert === 'link' && !editor!.can().setLink({ href })) { setUrlError('This link address is not supported.'); return; }
    if (insert === 'image' && !href) { setUrlError('Enter an image address.'); return; }
    if (insert === 'image') editor!.chain().focus().setImage({ src: href }).run();
    else if (editor!.state.selection.empty && !state!.link) editor!.chain().focus().insertContent({ type: 'text', text: href, marks: [{ type: 'link', attrs: { href } }] }).run();
    else editor!.chain().focus().extendMarkRange('link').setLink({ href }).run();
    setInsert(null);
  }
  const tools = [
    { name: 'Bold', icon: Bold, active: state.bold, action: () => editor.chain().focus().toggleBold().run() },
    { name: 'Italic', icon: Italic, active: state.italic, action: () => editor.chain().focus().toggleItalic().run() },
    { name: 'Strikethrough', icon: Strikethrough, active: state.strike, action: () => editor.chain().focus().toggleStrike().run() },
    { name: 'Bullet list', icon: List, active: state.bullet, action: () => editor.chain().focus().toggleBulletList().run() },
    { name: 'Numbered list', icon: ListOrdered, active: state.ordered, action: () => editor.chain().focus().toggleOrderedList().run() },
    { name: 'Checklist', icon: ListTodo, active: state.task, action: () => editor.chain().focus().toggleTaskList().run() },
    { name: 'Quote', icon: Quote, active: state.quote, action: () => editor.chain().focus().toggleBlockquote().run() },
    { name: 'Code block', icon: Code2, active: state.code, action: () => editor.chain().focus().toggleCodeBlock().run() },
    { name: 'Link', icon: Link2, active: state.link, action: () => openInsert('link') },
    { name: 'Image', icon: ImagePlus, active: false, action: () => openInsert('image') },
    { name: 'Insert table', icon: Table2, active: state.table, action: () => editor.chain().focus().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run() },
  ];
  return <div className={`markdown-editor ${disabled ? 'disabled' : ''}`}>
    <div className="format-toolbar" role="toolbar" aria-label="Description formatting">
      <select aria-label="Text style" value={state.heading} disabled={disabled} onChange={event => {
        const level = Number(event.target.value);
        if (!level) editor.chain().focus().setParagraph().run();
        else editor.chain().focus().setHeading({ level: level as 1 | 2 | 3 | 4 | 5 | 6 }).run();
      }}><option value="0">Text</option><option value="1">Heading 1</option><option value="2">Heading 2</option><option value="3">Heading 3</option><option value="4">Heading 4</option><option value="5">Heading 5</option><option value="6">Heading 6</option></select>
      {tools.map(({ name, icon: Icon, active, action }) => <button key={name} type="button" aria-label={name} title={name} aria-pressed={active} disabled={disabled} onMouseDown={event => event.preventDefault()} onClick={action}><Icon size={16} /></button>)}
      <span className="format-divider" />
      <button type="button" aria-label="Undo" title="Undo" disabled={disabled || !state.undo} onClick={() => editor.chain().focus().undo().run()}><Undo2 size={16} /></button>
      <button type="button" aria-label="Redo" title="Redo" disabled={disabled || !state.redo} onClick={() => editor.chain().focus().redo().run()}><Redo2 size={16} /></button>
    </div>
    {insert && <div className="insert-url"><label><span>{insert === 'link' ? 'Link URL' : 'Image URL'}</span><input autoFocus type="text" inputMode="url" autoCapitalize="off" value={url} onChange={event => { setUrl(event.target.value); setUrlError(''); }} placeholder="https://… or a relative path" onKeyDown={event => {
      if (event.key === 'Enter') { event.preventDefault(); applyUrl(); }
      if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); setInsert(null); editor.commands.focus(); }
    }} /></label><button className="icon-button" type="button" aria-label="Apply URL" onClick={applyUrl}><Check size={17} /></button><button className="icon-button" type="button" aria-label="Cancel URL" onClick={() => { setInsert(null); editor.commands.focus(); }}><X size={17} /></button>{urlError && <span className="form-error" role="alert">{urlError}</span>}</div>}
    <EditorContent editor={editor} />
    {state.table && <div className="table-actions"><button type="button" onClick={() => editor.chain().focus().addRowAfter().run()}>Add row</button><button type="button" onClick={() => editor.chain().focus().addColumnAfter().run()}>Add column</button><button type="button" onClick={() => editor.chain().focus().deleteRow().run()}>Delete row</button><button type="button" onClick={() => editor.chain().focus().deleteColumn().run()}>Delete column</button><button type="button" onClick={() => editor.chain().focus().deleteTable().run()}>Remove table</button></div>}
  </div>;
}
