import { describe, expect, it } from 'vitest';
import { MarkdownManager } from '@tiptap/markdown';
import { markdownExtensions, markdownParser } from './richMarkdown';

function manager() {
  return new MarkdownManager({ extensions: markdownExtensions(), marked: markdownParser() });
}

describe('Markdown editing round trips', () => {
  it('preserves rich Markdown semantics when another block changes', () => {
    const parser = manager();
    const source = '# Heading\n\n**Bold**, *italic*, ~~strike~~ and `code`.\n\n> Quote\n\n- Item\n  - Nested\n\n1. Ordered\n\n- [x] Complete\n- [ ] Next\n\n```js\nconst x = 1;\n```\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n[Example](https://example.com)\n\n![Alt](https://example.com/image.png)';
    const parsed = parser.parse(source);
    parsed.content!.unshift({ type: 'paragraph', content: [{ type: 'text', text: 'New paragraph.' }] });
    const saved = parser.serialize(parsed);
    for (const text of ['# Heading', '**Bold**', '*italic*', '~~strike~~', '`code`', '> Quote', '- [x] Complete', 'const x = 1;', '[Example](https://example.com)', '![Alt](https://example.com/image.png)']) expect(saved).toContain(text);
    expect(parser.parse(saved)).toEqual(parsed);
  });

  it('retains comments and arbitrary HTML after editing prose', () => {
    const parser = manager();
    const source = 'Before <span data-note="yes">inline</span> after.\n\n<!-- private note -->\n\n<div data-note="yes">Raw block</div>';
    const parsed = parser.parse(source);
    const saved = parser.serialize(parsed);
    expect(saved).toContain('<span data-note="yes">inline</span>');
    expect(saved).toContain('<!-- private note -->');
    expect(saved).toContain('<div data-note="yes">Raw block</div>');
  });

  it('preserves literal delimiters, fenced HTML and reference-link meanings', () => {
    const parser = manager();
    const source = 'Escaped \\*literal\\* and [reference][site].\n\n[site]: https://example.com "A site"\n\n```html\n<div>Code, not HTML</div>\n```';
    const parsed = parser.parse(source);
    expect(parser.parse(parser.serialize(parsed))).toEqual(parsed);
  });

  it('keeps unused reference definitions instead of deleting source text', () => {
    const parser = manager();
    const source = 'A [used][site] reference.\n\n[site]: https://example.com "A site"\n[future]: ../notes.md "Future notes"';
    const saved = parser.serialize(parser.parse(source));
    expect(saved).toContain('[future]: ../notes.md "Future notes"');
    expect(saved).toContain('[used](https://example.com "A site")');
  });
});
