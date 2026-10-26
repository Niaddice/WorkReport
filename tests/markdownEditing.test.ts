import { describe, expect, it } from 'vitest'
import { markdownEdit } from '../src/lib/markdownEditing'

describe('Markdown toolbar edits', () => {
  it('wraps just the selected text and preserves the surrounding content', () => {
    const result = markdownEdit('今天完成接口联调。', 4, 8, 'bold')
    expect(result.content).toBe('今天完成**接口联调**。')
    expect(result.content.slice(result.selectionStart, result.selectionEnd)).toBe('接口联调')
  })

  it.each([
    ['bold', '**加粗文字**', '加粗文字'],
    ['italic', '*斜体文字*', '斜体文字'],
    ['strike', '~~删除线文字~~', '删除线文字'],
    ['inlineCode', '`代码`', '代码'],
  ] as const)('selects a useful placeholder for empty %s insertion', (action, expected, placeholder) => {
    const result = markdownEdit('', 0, 0, action)
    expect(result.content).toBe(expected)
    expect(result.content.slice(result.selectionStart, result.selectionEnd)).toBe(placeholder)
  })

  it('formats the current line when the caret is in the middle of a paragraph', () => {
    expect(markdownEdit('上一行\n需要加标题\n下一行', 9, 9, 'heading').content).toBe('上一行\n## 需要加标题\n下一行')
    expect(markdownEdit('### 已有标题', 5, 5, 'heading').content).toBe('## 已有标题')
  })

  it('adds list markers to all selected lines without including the next line', () => {
    const original = '接口联调\n回归测试\n发布准备'
    const result = markdownEdit(original, 2, original.indexOf('发布准备'), 'orderedList')
    expect(result.content).toBe('1. 接口联调\n2. 回归测试\n发布准备')
    expect(result.content.slice(result.selectionStart, result.selectionEnd)).toBe('1. 接口联调\n2. 回归测试')
  })

  it('converts existing bullet and numbered lists to tasks without duplicating markers', () => {
    expect(markdownEdit('- 已有一项\n2. 已有二项\n  - [x] 完成项', 0, 31, 'taskList').content).toBe(
      '- [ ] 已有一项\n- [ ] 已有二项\n  - [ ] 完成项',
    )
  })

  it('quotes every selected line and selects a placeholder on an empty line', () => {
    expect(markdownEdit('第一段\n第二段', 0, 7, 'quote').content).toBe('> 第一段\n> 第二段')
    const result = markdownEdit('\n已有正文', 0, 0, 'quote')
    expect(result.content).toBe('> 引用内容\n已有正文')
    expect(result.content.slice(result.selectionStart, result.selectionEnd)).toBe('引用内容')
  })

  it('creates an editable URL for selected link text', () => {
    const result = markdownEdit('查看接口文档即可', 2, 6, 'link')
    expect(result.content).toBe('查看[接口文档](https://)即可')
    expect(result.content.slice(result.selectionStart, result.selectionEnd)).toBe('https://')
  })

  it('uses longer backtick delimiters when inline code already contains backticks', () => {
    const result = markdownEdit('`value`', 0, 7, 'inlineCode')
    expect(result.content).toBe('`` `value` ``')
    expect(result.content.slice(result.selectionStart, result.selectionEnd)).toBe('`value`')
  })

  it('inserts a safely fenced code block between surrounding paragraphs', () => {
    const text = '说明const code = "```";结束'
    const result = markdownEdit(text, 2, text.length - 2, 'codeBlock')
    expect(result.content).toBe('说明\n\n````\nconst code = "```";\n````\n\n结束')
    expect(result.content.slice(result.selectionStart, result.selectionEnd)).toBe('const code = "```";')
  })

  it('inserts a divider without accumulating extra blank lines', () => {
    expect(markdownEdit('前文后文', 2, 2, 'divider').content).toBe('前文\n\n---\n\n后文')
    expect(markdownEdit('前文\n\n后文', 4, 4, 'divider').content).toBe('前文\n\n---\n\n后文')
  })

  it('inserts a table as a separate block with its first heading selected', () => {
    const result = markdownEdit('说明', 2, 2, 'table')
    expect(result.content).toBe('说明\n\n| 项目 | 内容 |\n| --- | --- |\n| 项目名称 | 填写内容 |')
    expect(result.content.slice(result.selectionStart, result.selectionEnd)).toBe('项目')
  })

  it('clamps a stale selection to the current document boundaries', () => {
    expect(markdownEdit('当前', -10, 100, 'bold').content).toBe('**当前**')
  })
})
