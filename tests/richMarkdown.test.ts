// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest'
import { Editor } from '@tiptap/core'
import { TextSelection } from '@tiptap/pm/state'
import { CellSelection } from '@tiptap/pm/tables'
import {
  clearRichMarkdownFormat,
  convertTypedMarkdownTable,
  indentRichMarkdown,
  insertMarkdownDivider,
  insertRichMarkdownTable,
  markdownCellContent,
  prepareRichMarkdown,
  richMarkdownExtensions,
  richMarkdownToolbarState,
  runMarkdownTableAction,
  selectedRichMarkdown,
} from '../src/lib/richMarkdown'

const editors: Editor[] = []
function makeEditor(content: string) {
  const element = document.createElement('div')
  document.body.append(element)
  const editor = new Editor({
    element,
    extensions: richMarkdownExtensions(),
    content: prepareRichMarkdown(content),
    contentType: 'markdown',
  })
  editors.push(editor)
  return editor
}
function selectText(editor: Editor, text: string, offset = 0, length = 0) {
  let position: number | undefined
  editor.state.doc.descendants((node, start) => {
    if (position === undefined && node.isText && node.text?.includes(text))
      position = start + node.text.indexOf(text) + offset
  })
  expect(position).toBeDefined()
  editor.commands.setTextSelection({ from: position!, to: position! + length })
}
function expectCanonicalTable(editor: Editor, rows: number, columns: number) {
  const table = editor.state.doc.firstChild!
  expect(table.type.name).toBe('table')
  expect(table.childCount).toBe(rows)
  table.forEach((row, _offset, index) => {
    expect(row.childCount).toBe(columns)
    row.forEach((cell) => expect(cell.type.name).toBe(index === 0 ? 'tableHeader' : 'tableCell'))
  })
  expect(() => editor.state.doc.check()).not.toThrow()
  expect(makeEditor(editor.getMarkdown()).getJSON()).toEqual(editor.getJSON())
}
afterEach(() => {
  editors.splice(0).forEach((editor) => editor.destroy())
  document.body.innerHTML = ''
})

describe('rich Markdown toolbar commands', () => {
  const source = '| Head A | Head B |\n| --- | --- |\n| Body A | Body B |\n| Last A | Last B |'

  it('reports formatting and stored marks without editing the document', () => {
    const editor = makeEditor('## **标题**\n\n普通正文')
    const updates = vi.fn()
    editor.on('update', updates)
    selectText(editor, '标题')
    let state = richMarkdownToolbarState(editor)
    expect(state.headingLevel).toBe(2)
    expect(state.active.heading).toBe(true)
    expect(state.active.bold).toBe(true)
    expect(state.enabled.clearFormat).toBe(true)
    selectText(editor, '普通正文', 2)
    editor.commands.toggleItalic()
    state = richMarkdownToolbarState(editor)
    expect(state.headingLevel).toBe(0)
    expect(state.active.italic).toBe(true)
    expect(state.enabled.indent).toBe(false)
    expect(state.enabled.outdent).toBe(false)
    expect(updates).not.toHaveBeenCalled()
    expect(Object.values(richMarkdownToolbarState(editor, true).enabled).every((value) => !value)).toBe(true)
  })

  it('disables block transformations inside cells and allows inline formatting', () => {
    const editor = makeEditor(source)
    selectText(editor, 'Body A')
    const state = richMarkdownToolbarState(editor)
    expect(state.inTable).toBe(true)
    for (const action of ['heading', 'bulletList', 'taskList', 'orderedList', 'quote', 'codeBlock'] as const)
      expect(state.enabled[action]).toBe(false)
    for (const action of ['bold', 'italic', 'strike', 'inlineCode', 'link', 'table'] as const)
      expect(state.enabled[action]).toBe(true)
    editor.setEditable(false)
    const readonlyState = richMarkdownToolbarState(editor)
    expect(Object.values(readonlyState.enabled).every((value) => !value)).toBe(true)
    expect(Object.values(readonlyState.tableEnabled).every((value) => !value)).toBe(true)
    expect(runMarkdownTableAction(editor, 'deleteTable')).toBe(false)
    expect(insertRichMarkdownTable(editor)).toBe(false)
    expect(clearRichMarkdownFormat(editor)).toBe(false)
    expect(indentRichMarkdown(editor)).toBe(false)
  })

  it.each(['addRowBefore', 'deleteRow'] as const)('keeps exactly one header after %s on the header row', (action) => {
    const editor = makeEditor(source)
    selectText(editor, 'Head A')
    const update = vi.fn()
    editor.on('update', update)
    expect(runMarkdownTableAction(editor, action)).toBe(true)
    expect(update).toHaveBeenCalledTimes(1)
    expectCanonicalTable(editor, action === 'addRowBefore' ? 4 : 2, 2)
    expect(editor.state.doc.firstChild!.textContent).toContain('Body A')
    if (action === 'deleteRow') expect(editor.state.doc.firstChild!.firstChild!.textContent).toBe('Body ABody B')
  })

  it('adds and removes rows and columns while retaining all original cells', () => {
    const editor = makeEditor(source)
    selectText(editor, 'Body B')
    expect(runMarkdownTableAction(editor, 'addRowAfter')).toBe(true)
    expectCanonicalTable(editor, 4, 2)
    selectText(editor, 'Body B')
    expect(runMarkdownTableAction(editor, 'addColumnBefore')).toBe(true)
    expectCanonicalTable(editor, 4, 3)
    selectText(editor, 'Body B')
    expect(runMarkdownTableAction(editor, 'addColumnAfter')).toBe(true)
    expectCanonicalTable(editor, 4, 4)
    selectText(editor, 'Body B')
    expect(runMarkdownTableAction(editor, 'deleteColumn')).toBe(true)
    expectCanonicalTable(editor, 4, 3)
    expect(editor.state.doc.textContent).not.toContain('Body B')
    expect(editor.state.doc.textContent).toContain('Body A')
  })

  it('aligns selected columns through header and body in one update', () => {
    const editor = makeEditor(source)
    selectText(editor, 'Body B')
    const update = vi.fn()
    editor.on('update', update)
    expect(runMarkdownTableAction(editor, 'alignCenter')).toBe(true)
    expect(update).toHaveBeenCalledTimes(1)
    editor.state.doc.firstChild!.forEach((row) => {
      expect(row.child(0).attrs.align).toBeNull()
      expect(row.child(1).attrs.align).toBe('center')
    })
    expect(richMarkdownToolbarState(editor).tableAlignment).toBe('center')
    expectCanonicalTable(editor, 3, 2)
    const cells: number[] = []
    editor.state.doc.descendants((node, position) => {
      if (node.type.name === 'tableCell') cells.push(position)
    })
    editor.view.dispatch(editor.state.tr.setSelection(CellSelection.create(editor.state.doc, cells[0]!, cells[1]!)))
    expect(richMarkdownToolbarState(editor).tableAlignment).toBeNull()
    expect(runMarkdownTableAction(editor, 'alignRight')).toBe(true)
    editor.state.doc.firstChild!.forEach((row) => row.forEach((cell) => expect(cell.attrs.align).toBe('right')))
    expectCanonicalTable(editor, 3, 2)
  })

  it('deletes a last remaining row or explicit table without invalid empty documents', () => {
    for (const action of ['deleteRow', 'deleteTable'] as const) {
      const editor = makeEditor('| Header |\n| --- |')
      expect(editor.state.doc.firstChild?.type.name).toBe('table')
      selectText(editor, 'Header')
      expect(runMarkdownTableAction(editor, action)).toBe(true)
      expect(editor.view.dom.querySelector('table')).toBeNull()
      expect(editor.state.doc.firstChild?.type.name).toBe('paragraph')
      expect(() => editor.state.doc.check()).not.toThrow()
    }
  })

  it('inserts bounded dimensions and refuses nested table insertion', () => {
    const editor = makeEditor('')
    expect(insertRichMarkdownTable(editor, 3, 4)).toBe(true)
    expectCanonicalTable(editor, 3, 4)
    expect(insertRichMarkdownTable(editor, 2, 2)).toBe(false)
    const bounded = makeEditor('')
    expect(insertRichMarkdownTable(bounded, 100, 100)).toBe(true)
    expectCanonicalTable(bounded, 20, 10)
    const empty = makeEditor('')
    expect(insertRichMarkdownTable(empty, Number.NaN, 2)).toBe(false)
  })

  it('indents and outdents selected regular and task items', () => {
    for (const markdown of ['- First\n- Second', '- [ ] First\n- [x] Second']) {
      const editor = makeEditor(markdown)
      selectText(editor, 'Second')
      expect(richMarkdownToolbarState(editor).enabled.indent).toBe(true)
      expect(indentRichMarkdown(editor)).toBe(true)
      expect(editor.state.selection.$from.depth).toBe(5)
      expect(richMarkdownToolbarState(editor).enabled.outdent).toBe(true)
      expect(indentRichMarkdown(editor, true)).toBe(true)
      expect(editor.state.selection.$from.depth).toBe(3)
      expect(makeEditor(editor.getMarkdown()).getJSON()).toEqual(editor.getJSON())
    }
  })

  it('clears selected marks and blocks while preserving tables, images and literal code', () => {
    const editor = makeEditor(
      '## **标题**\n\n| H |\n| --- |\n| **数据** |\n\n![图片](https://example.com/a.png)\n\n```md\n**literal**\n# heading\n```',
    )
    editor.commands.selectAll()
    expect(clearRichMarkdownFormat(editor)).toBe(true)
    expect(editor.view.dom.querySelector('h2,strong,pre code')).toBeNull()
    expect(editor.view.dom.querySelectorAll('table')).toHaveLength(1)
    expect(editor.view.dom.querySelector('.rich-markdown-image-placeholder')).not.toBeNull()
    expect(editor.state.doc.textContent).toContain('**literal**\n# heading')
    expect(makeEditor(editor.getMarkdown()).getJSON()).toEqual(editor.getJSON())
    selectText(editor, '数据', 0, 2)
    editor.commands.toggleBold()
    expect(clearRichMarkdownFormat(editor)).toBe(true)
    expect(editor.view.dom.querySelector('td strong')).toBeNull()
    expect(editor.view.dom.querySelectorAll('table')).toHaveLength(1)
  })

  it('keeps nested code fences and literal Markdown text intact after save and reopen', () => {
    for (const source of [
      '````markdown\n```js\nconst value = "**literal**"\n```\n````',
      '~~~foo`bar\nconst data = [1, 2] // **literal**\n~~~',
    ]) {
      const editor = makeEditor(source)
      expect(editor.state.doc.firstChild?.type.name).toBe('codeBlock')
      expect(richMarkdownToolbarState(editor).headingLevel).toBeNull()
      expect(makeEditor(editor.getMarkdown()).getJSON()).toEqual(editor.getJSON())
    }
  })

  it('clears list and quote containers around a selected table without lifting its cells', () => {
    const editor = makeEditor('- **First**\n- Second\n\n| H |\n| --- |\n| Data |\n\n> Quote')
    editor.commands.selectAll()
    expect(clearRichMarkdownFormat(editor)).toBe(true)
    expect(editor.view.dom.querySelector('ul,blockquote,strong')).toBeNull()
    expect(editor.view.dom.querySelectorAll('table')).toHaveLength(1)
    expect(editor.state.doc.textContent).toContain('FirstSecondHDataQuote')
    expect(makeEditor(editor.getMarkdown()).getJSON()).toEqual(editor.getJSON())
  })

  it.each(['# heading', '---', 'title\n===', '1. item', '    indented', '<div>literal</div>'])(
    'clears code formatting without interpreting literal block syntax: %s',
    (code) => {
      const editor = makeEditor(`\`\`\`\n${code}\n\`\`\``)
      editor.commands.selectAll()
      expect(clearRichMarkdownFormat(editor)).toBe(true)
      expect(editor.state.doc.firstChild?.type.name).toBe('paragraph')
      expect(editor.state.doc.textContent).toBe(code)
      expect(makeEditor(editor.getMarkdown()).getJSON()).toEqual(editor.getJSON())
    },
  )

  it('copies partial cell text without its open table ancestors', () => {
    const editor = makeEditor(source)
    selectText(editor, 'Body A', 1, 3)
    const selected = selectedRichMarkdown(editor)
    expect(selected).toBe('ody')
    editor.commands.insertContent(markdownCellContent(editor, selected))
    expect(editor.view.dom.querySelector('td br')).toBeNull()
    expect(editor.state.doc.firstChild!.child(1).child(0).textContent).toBe('Body A')
  })

  it.each([' foo ', ' foo', 'foo ', '   ', '`x`', 'foo `` bar'])(
    'round-trips significant inline-code whitespace and delimiters: %s',
    (text) => {
      for (const type of ['paragraph', 'heading']) {
        const editor = makeEditor('')
        editor.commands.setContent({
          type: 'doc',
          content: [
            {
              type,
              ...(type === 'heading' ? { attrs: { level: 3 } } : {}),
              content: [{ type: 'text', text, marks: [{ type: 'code' }] }],
            },
          ],
        })
        expect(makeEditor(editor.getMarkdown()).getJSON()).toEqual(editor.getJSON())
      }
    },
  )
})

describe('rich Markdown document', () => {
  it('loads headings, editable tables, nested tasks and formatting from persisted Markdown', () => {
    const source = '## 今日工作\n\n| 项目 | 进展 |\n| --- | --- |\n| 接口 | **完成** |\n\n- [x] 联调\n  - [ ] 回归'
    const editor = makeEditor(source)
    expect(editor.getHTML()).toContain('<h2>今日工作</h2>')
    expect(editor.view.dom.querySelector('table th')?.textContent).toBe('项目')
    expect(editor.view.dom.querySelector('table td strong')?.textContent).toBe('完成')
    expect(editor.view.dom.querySelectorAll('ul[data-type="taskList"]')).toHaveLength(2)
    expect(editor.view.dom.querySelector<HTMLInputElement>('input[type="checkbox"]')?.checked).toBe(true)
    const roundTrip = makeEditor(editor.getMarkdown())
    expect(roundTrip.getJSON()).toEqual(editor.getJSON())
  })

  it('converts the Markdown heading shortcut while typing', () => {
    const editor = makeEditor('')
    editor.commands.setContent({
      type: 'doc',
      content: [{ type: 'paragraph', content: [{ type: 'text', text: '##' }] }],
    })
    editor.commands.setTextSelection(3)
    let handled = false
    editor.view.someProp('handleTextInput', (handler) => {
      handled = handler(editor.view, 3, 3, ' ', () => editor.state.tr) === true
      return handled
    })
    expect(handled).toBe(true)
    expect(editor.getHTML()).toContain('<h2></h2>')
  })

  it('converts typed pipe header and separator paragraphs into a table on Enter', () => {
    const editor = makeEditor('')
    editor.commands.setContent({
      type: 'doc',
      content: [
        { type: 'paragraph', content: [{ type: 'text', text: '| 项目 | 进展 |' }] },
        { type: 'paragraph', content: [{ type: 'text', text: '| --- | --- |' }] },
      ],
    })
    editor.commands.setTextSelection(editor.state.doc.content.size - 1)
    expect(convertTypedMarkdownTable(editor)).toBe(true)
    expect(editor.state.doc.firstChild?.type.name).toBe('table')
    expect(editor.view.dom.querySelectorAll('table th')).toHaveLength(2)
    expect(editor.view.dom.querySelectorAll('table td')).toHaveLength(2)
    expect(editor.isActive('tableCell')).toBe(true)
    expect(makeEditor(editor.getMarkdown()).view.dom.querySelectorAll('table')).toHaveLength(1)
  })

  it('does not convert partial separators or text inside a code block', () => {
    const editor = makeEditor('| 项目 | 进展 |\n\n| -- | -- |')
    editor.commands.setTextSelection(editor.state.doc.content.size - 1)
    expect(convertTypedMarkdownTable(editor)).toBe(false)
    editor.commands.toggleCodeBlock()
    expect(convertTypedMarkdownTable(editor)).toBe(false)
  })

  it('keeps images as safe placeholders and preserves reference definitions', () => {
    const source = '![日报截图][shot]\n\n[shot]: https://example.com/private.png "截图"'
    const editor = makeEditor(source)
    expect(() => editor.state.doc.check()).not.toThrow()
    expect(editor.view.dom.querySelector('img[src]')).toBeNull()
    expect(editor.view.dom.querySelector('.rich-markdown-image-placeholder')?.textContent).toContain('日报截图')
    expect(editor.getMarkdown()).toContain('![日报截图][shot]')
    expect(editor.getMarkdown()).toContain('[shot]: https://example.com/private.png "截图"')
    expect(makeEditor(editor.getMarkdown()).view.dom.querySelector('.rich-markdown-image-placeholder')).not.toBeNull()
    expect(() => editor.commands.selectAll()).not.toThrow()
    editor.commands.insertContent('已替换')
    expect(editor.getMarkdown()).toContain('已替换')
  })

  it('keeps standalone images valid and preserves links wrapped around images', () => {
    const editor = makeEditor('![截图](https://example.com/x.png)')
    expect(() => editor.state.doc.check()).not.toThrow()
    expect(editor.state.doc.firstChild?.type.name).toBe('paragraph')
    expect(() => editor.commands.selectAll()).not.toThrow()
    const linked = '[![图](image.png)](https://example.com/link)'
    expect(makeEditor(linked).getMarkdown()).toContain(linked)
  })

  it.each(['>', '1.', '- ', '- [ ] '])('keeps empty Markdown containers valid: %s', (source) => {
    const editor = makeEditor(source)
    expect(() => editor.state.doc.check()).not.toThrow()
    expect(() => editor.commands.selectAll()).not.toThrow()
  })

  it.each(['https://example.com/a)b', 'https://example.com/a(b', 'https://example.com/a\\b'])(
    'round-trips escaped link destinations and titles: %s',
    (href) => {
      const editor = makeEditor('')
      const title = '说明 "引用" \\ 结尾'
      editor.commands.setContent({
        type: 'doc',
        content: [
          {
            type: 'paragraph',
            content: [{ type: 'text', text: '链接', marks: [{ type: 'link', attrs: { href, title } }] }],
          },
        ],
      })
      const reopened = makeEditor(editor.getMarkdown())
      expect(reopened.getJSON()).toEqual(editor.getJSON())
      expect(reopened.view.dom.textContent).toBe('链接')
    },
  )

  it('keeps multiline cell pastes inside their original table and reopens hard breaks', () => {
    const editor = makeEditor('| A | B |\n| --- | --- |\n| C | D |')
    let cellPosition = 0
    editor.state.doc.descendants((node, position) => {
      if (node.isText && node.text === 'C') cellPosition = position
    })
    editor.commands.setTextSelection({ from: cellPosition, to: cellPosition + 1 })
    editor.commands.insertContent(markdownCellContent(editor, 'line1\n\n**line2**'))
    expect(editor.view.dom.querySelectorAll('table')).toHaveLength(1)
    expect(editor.view.dom.querySelectorAll('table tr')).toHaveLength(2)
    expect(editor.view.dom.querySelector('table td br')).not.toBeNull()
    expect(editor.view.dom.querySelector('table td strong')?.textContent).toBe('line2')
    expect(() => editor.state.doc.check()).not.toThrow()
    const reopened = makeEditor(editor.getMarkdown())
    expect(reopened.getJSON()).toEqual(editor.getJSON())
    expect(reopened.view.dom.querySelectorAll('table')).toHaveLength(1)
  })

  it('preserves angle-delimited destinations containing spaces', () => {
    const editor = makeEditor('[x](<https://example.com/a b>)')
    const reopened = makeEditor(editor.getMarkdown())
    expect(editor.getMarkdown()).toContain('(<https://example.com/a b>)')
    expect(reopened.getJSON()).toEqual(editor.getJSON())
  })

  it('inserts a divider after a table instead of splitting its current row', () => {
    const editor = makeEditor('| A | B |\n| --- | --- |\n| C | D |')
    expect(insertMarkdownDivider(editor)).toBe(true)
    expect(editor.view.dom.querySelectorAll('table')).toHaveLength(1)
    expect(editor.view.dom.querySelectorAll('table tr')).toHaveLength(2)
    expect(editor.view.dom.querySelector('hr')).not.toBeNull()
    expect(() => editor.state.doc.check()).not.toThrow()
  })

  it('retains comments, raw HTML and front matter as inert source through later edits', () => {
    const source =
      '---\ntitle: 日报\n---\n\n<!-- private note -->\n\n<div><img src="https://example.com/a"><script>bad()</script></div>\n\n正文'
    const editor = makeEditor(source)
    expect(editor.view.dom.querySelector('img, script')).toBeNull()
    expect(editor.view.dom.querySelectorAll('.rich-markdown-preserved')).toHaveLength(3)
    editor.commands.setTextSelection(editor.state.doc.content.size - 1)
    editor.commands.insertContent('补充')
    const markdown = editor.getMarkdown()
    expect(markdown).toContain('title: 日报')
    expect(markdown).toContain('<!-- private note -->')
    expect(markdown).toContain('<div><img src="https://example.com/a"><script>bad()</script></div>')
    expect(markdown).toContain('正文补充')
    expect(makeEditor(markdown).view.dom.querySelector('img, script')).toBeNull()
  })

  it('keeps inline comments and HTML within their original block rather than dropping source', () => {
    const source = '段落 <!-- detail --> 后半段\n\n- 列表 <kbd>Enter</kbd>'
    const editor = makeEditor(source)
    expect(editor.getMarkdown()).toContain('段落 <!-- detail --> 后半段')
    expect(editor.getMarkdown()).toContain('- 列表 <kbd>Enter</kbd>')
  })

  it('makes table cells single-paragraph containers', () => {
    const editor = makeEditor('| A | B |\n| --- | --- |\n| C | D |')
    const cell = editor.state.doc.firstChild!.child(1).child(0)
    expect(cell.type.validContent(cell.content)).toBe(true)
    expect(cell.type.spec.content).toBe('paragraph')
    editor.view.dispatch(editor.state.tr.setSelection(TextSelection.near(editor.state.doc.resolve(12))))
    expect(editor.extensionManager.extensions.some((extension) => extension.name === 'undoRedo')).toBe(false)
  })
})
