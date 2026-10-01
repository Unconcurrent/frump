import { Node } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import { Markdown } from '@tiptap/markdown';
import { TableKit } from '@tiptap/extension-table';
import TaskList from '@tiptap/extension-task-list';
import TaskItem from '@tiptap/extension-task-item';
import Image from '@tiptap/extension-image';
import Placeholder from '@tiptap/extension-placeholder';
import { Lexer, Marked, Tokenizer, type marked } from 'marked';

export function markdownParser() {
  const parser = new Marked(), tokenizer = new Tokenizer();
  new Lexer({ tokenizer, gfm: true });
  // Marked normally consumes link definitions without emitting a document
  // node. Retain their source while still resolving reference links normally.
  parser.use({ extensions: [{
    name: 'sourceBlock', level: 'block',
    tokenizer(source) {
      const definition = tokenizer.def(source);
      if (!definition) return;
      const links = this.lexer.tokens.links;
      if (!links[definition.tag]) links[definition.tag] = { href: definition.href, title: definition.title };
      return { type: 'sourceBlock', raw: definition.raw };
    },
  }] });
  // Tiptap types this option as the global function but uses the instance API.
  return parser as unknown as typeof marked;
}

// Keep HTML and comments as editable source nodes: a rich-text schema cannot represent
// arbitrary HTML attributes. Editing nearby prose must not silently delete them.
function sourceNode(inline: boolean) {
  const name = inline ? 'sourceInline' : 'sourceBlock';
  const tokenizer = new Tokenizer();
  new Lexer({ tokenizer, gfm: true });
  return Node.create({
    name, inline, group: inline ? 'inline' : 'block', content: 'text*', marks: '', code: true,
    parseHTML: () => [{ tag: `${inline ? 'span' : 'pre'}[data-markdown-source]`, preserveWhitespace: 'full' }],
    renderHTML: () => [inline ? 'span' : 'pre', { 'data-markdown-source': '', class: 'markdown-source', title: 'Markdown source' }, 0],
    markdownTokenizer: {
      name, level: inline ? 'inline' : 'block',
      start: inline ? source => source.indexOf('<') : () => -1,
      tokenize: source => {
        const token = inline ? tokenizer.tag(source) : tokenizer.html(source);
        return token ? { ...token, type: name } : undefined;
      },
    },
    parseMarkdown: token => {
      const text = (token.raw ?? '').replace(/\n+$/, '');
      return { type: name, content: text ? [{ type: 'text', text }] : [] };
    },
    renderMarkdown: node => (node.content ?? []).map(child => child.text ?? '').join(''),
  });
}

export function markdownExtensions() {
  return [
    StarterKit.configure({ underline: false, trailingNode: false, link: { openOnClick: false, autolink: false } }),
    // Each editor owns its parser; registering custom tokens on the global
    // marked singleton accumulates handlers every time a task is opened.
    Markdown.configure({ marked: markdownParser() }),
    TableKit, TaskList, TaskItem.configure({ nested: true }),
    Image.configure({ allowBase64: true }),
    Placeholder.configure({ placeholder: 'Add context, a plan, or the details that matter…' }),
    sourceNode(false), sourceNode(true),
  ];
}
