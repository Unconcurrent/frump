import { describe, expect, it } from 'vitest';
import { markdownKey } from './markdownKeys';

describe('Markdown keyboard editing', () => {
  it('continues ordered lists and quotes and exits an empty list item', () => {
    expect(markdownKey('9. Step', 7, 7, 'Enter')?.value).toBe('9. Step\n10. ');
    expect(markdownKey('> Quoted', 8, 8, 'Enter')?.value).toBe('> Quoted\n> ');
    expect(markdownKey('- First\n- ', 10, 10, 'Enter')?.value).toBe('- First\n');
  });
  it('continues a checked task as unchecked and preserves indentation', () => {
    expect(markdownKey('  - [x] Ready', 13, 13, 'Enter')?.value).toBe('  - [x] Ready\n  - [ ] ');
  });
  it('wraps the selected text while leaving that text selected', () => {
    expect(markdownKey('One two three', 4, 7, 'b', true)).toEqual({ value: 'One **two** three', start: 6, end: 9 });
  });
  it('indents and outdents every selected line without changing the surrounding lines', () => {
    expect(markdownKey('one\ntwo\nthree', 0, 7, 'Tab')?.value).toBe('  one\n  two\nthree');
    expect(markdownKey('  one\n  two\nthree', 2, 11, 'Tab', false, true)?.value).toBe('one\ntwo\nthree');
  });
  it('leaves Tab outside lists available for focus navigation and Shift Enter unmodified', () => {
    expect(markdownKey('Plain text', 4, 4, 'Tab')).toBeNull();
    expect(markdownKey('- List item', 11, 11, 'Enter', false, true)).toBeNull();
  });
});
