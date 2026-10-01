// Textarea editing remains plain Markdown. These transformations preserve caret selection.
export type TextEdit = { value: string; start: number; end: number };
export function markdownKey(value: string, start: number, end: number, key: string, modifier = false, shift = false): TextEdit | null {
  const replace = (from: number, to: number, text: string, selectionStart: number, selectionEnd = selectionStart) => ({ value: value.slice(0, from) + text + value.slice(to), start: selectionStart, end: selectionEnd });
  if (modifier && ['b', 'i', 'k'].includes(key.toLowerCase())) {
    const selected = value.slice(start, end), symbol = key.toLowerCase() === 'b' ? '**' : '*';
    if (key.toLowerCase() === 'k') {
      const text = `[${selected || 'link text'}](https://)`;
      return replace(start, end, text, start + text.length - 9, start + text.length - 1);
    }
    return replace(start, end, `${symbol}${selected}${symbol}`, start + symbol.length, end + symbol.length);
  }
  const lineStart = value.lastIndexOf('\n', start - 1) + 1;
  if (key === 'Tab') {
    const lineEnd = value.indexOf('\n', end), to = lineEnd < 0 ? value.length : lineEnd;
    const block = value.slice(lineStart, to), lines = block.split('\n');
    if (lines.length === 1 && !/^(\s*)([-*+] |\d+[.)] |>)/.test(block)) return null;
    const result = lines.map(line => shift ? line.replace(/^(?:  |\t)/, '') : `  ${line}`).join('\n');
    const deltaFirst = result.split('\n')[0].length - lines[0].length;
    return replace(lineStart, to, result, Math.max(lineStart, start + deltaFirst), Math.max(lineStart, end + result.length - block.length));
  }
  if (key === 'Enter' && !modifier && !shift && start === end) {
    const line = value.slice(lineStart, start), match = line.match(/^(\s*)([-*+] |\d+[.)] |>[ ]?)(.*)$/);
    if (!match) return null;
    if (!match[3].trim()) return replace(lineStart, start, '', lineStart);
    let marker = match[2];
    const number = marker.match(/^(\d+)([.)] )$/);
    if (number) marker = `${Number(number[1]) + 1}${number[2]}`;
    else if (/^[-*+] $/.test(marker)) marker += match[3].match(/^\[[ xX]\] /) ? '[ ] ' : '';
    const continuation = `\n${match[1]}${marker}`;
    return replace(start, end, continuation, start + continuation.length);
  }
  return null;
}
