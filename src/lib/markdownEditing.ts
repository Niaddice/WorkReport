export type MarkdownAction =
  | 'heading'
  | 'bold'
  | 'italic'
  | 'strike'
  | 'bulletList'
  | 'taskList'
  | 'orderedList'
  | 'quote'
  | 'inlineCode'
  | 'codeBlock'
  | 'link'
  | 'divider'
  | 'table'

export interface MarkdownEdit {
  content: string
  selectionStart: number
  selectionEnd: number
}

/** Build one edit so a toolbar action is saved as one undoable operation. */
export function markdownEdit(content: string, start: number, end: number, action: MarkdownAction): MarkdownEdit {
  start = Math.max(0, Math.min(content.length, start))
  end = Math.max(start, Math.min(content.length, end))
  const selected = content.slice(start, end)

  function replace(text: string, selectFrom: number, selectTo = selectFrom): MarkdownEdit {
    return {
      content: content.slice(0, start) + text + content.slice(end),
      selectionStart: start + selectFrom,
      selectionEnd: start + selectTo,
    }
  }
  function wrap(before: string, after: string, placeholder: string) {
    const text = selected || placeholder
    return replace(before + text + after, before.length, before.length + text.length)
  }
  function block(text: string, selectFrom: number, selectTo = selectFrom) {
    const previous = content.slice(0, start)
    const next = content.slice(end)
    const before = !previous || previous.endsWith('\n\n') ? '' : previous.endsWith('\n') ? '\n' : '\n\n'
    const after = !next || next.startsWith('\n\n') || text.endsWith('\n\n') ? '' : next.startsWith('\n') ? '\n' : '\n\n'
    return replace(before + text + after, before.length + selectFrom, before.length + selectTo)
  }

  if (action === 'bold') return wrap('**', '**', '加粗文字')
  if (action === 'italic') return wrap('*', '*', '斜体文字')
  if (action === 'strike') return wrap('~~', '~~', '删除线文字')
  if (action === 'inlineCode') {
    const text = selected || '代码'
    const longest = Math.max(0, ...Array.from(text.matchAll(/`+/g), (match) => match[0].length))
    const fence = '`'.repeat(longest + 1)
    const padding = text.startsWith('`') || text.endsWith('`') ? ' ' : ''
    return wrap(fence + padding, padding + fence, '代码')
  }
  if (action === 'link') {
    const text = selected || '链接文字'
    const before = `[${text}](`
    const url = 'https://'
    return replace(before + url + ')', before.length, before.length + url.length)
  }
  if (action === 'codeBlock') {
    const text = selected || '在此输入代码'
    const longest = Math.max(2, ...Array.from(text.matchAll(/`+/g), (match) => match[0].length))
    const fence = '`'.repeat(longest + 1)
    const before = fence + '\n'
    return block(before + text + (text.endsWith('\n') ? '' : '\n') + fence, before.length, before.length + text.length)
  }
  if (action === 'divider') return block('---\n\n', 5)
  if (action === 'table') {
    const text = '| 项目 | 内容 |\n| --- | --- |\n| 项目名称 | 填写内容 |'
    return block(text, 2, 4)
  }

  // Line commands include the whole current line; a selection ending at the next
  // line's beginning does not accidentally format that following line.
  const lineStart = start === 0 ? 0 : content.lastIndexOf('\n', start - 1) + 1
  const last = end > start && content[end - 1] === '\n' ? end - 1 : end
  const nextBreak = content.indexOf('\n', last)
  const lineEnd = nextBreak < 0 ? content.length : nextBreak
  const original = content.slice(lineStart, lineEnd)
  const placeholder =
    action === 'heading' ? '标题' : action === 'quote' ? '引用内容' : action === 'taskList' ? '待办事项' : '列表项'
  const lines = original.split('\n')
  const prefixes: number[] = []
  const formatted = lines.map((line, index) => {
    const indent = line.match(/^[ \t]*/)?.[0] ?? ''
    let body = line.slice(indent.length)
    const prefix =
      action === 'heading'
        ? '## '
        : action === 'quote'
          ? '> '
          : action === 'taskList'
            ? '- [ ] '
            : action === 'orderedList'
              ? `${index + 1}. `
              : '- '
    body =
      action === 'heading'
        ? body.replace(/^#{1,6}[ \t]+/, '')
        : action === 'quote'
          ? body.replace(/^>[ \t]?/, '')
          : body.replace(/^(?:[-+*]|\d+[.)])[ \t]+(?:\[[ xX]\][ \t]+)?/, '')
    prefixes.push(indent.length + prefix.length)
    return indent + prefix + (body || placeholder)
  })
  const replacement = formatted.join('\n')
  if (start === end && !original.trim()) {
    return {
      content: content.slice(0, lineStart) + replacement + content.slice(lineEnd),
      selectionStart: lineStart + prefixes[0]!,
      selectionEnd: lineStart + replacement.length,
    }
  }
  return {
    content: content.slice(0, lineStart) + replacement + content.slice(lineEnd),
    selectionStart: lineStart,
    selectionEnd: lineStart + replacement.length,
  }
}
