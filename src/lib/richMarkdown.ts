import { Node, type Editor, type Extensions, type JSONContent } from '@tiptap/core'
import { Selection, TextSelection, type Transaction } from '@tiptap/pm/state'
import type { Slice } from '@tiptap/pm/model'
import { isInTable, selectedRect } from '@tiptap/pm/tables'
import { liftTarget } from '@tiptap/pm/transform'
import StarterKit from '@tiptap/starter-kit'
import { Markdown } from '@tiptap/markdown'
import { Table, TableCell, TableHeader, TableRow } from '@tiptap/extension-table'
import { TaskItem, TaskList } from '@tiptap/extension-list'
import { marked } from 'marked'
import type { HeadingLevel, MarkdownTableAction, MarkdownToolbarAction, MarkdownToolbarState } from './markdownToolbar'

const preservedPrefix = '\uE000workreport-preserved:'
const preservedSuffix = '\uE001'

/** Code spans keep significant spaces inside their delimiters. */
function renderInlineMarkdown(content: JSONContent[] = [], render: (nodes: JSONContent[]) => string): string {
  const parts: string[] = []
  let ordinary: JSONContent[] = []
  const flush = () => {
    if (ordinary.length) parts.push(render(ordinary))
    ordinary = []
  }
  const isCode = (node: JSONContent) =>
    node.type === 'text' && node.marks?.length === 1 && node.marks[0]?.type === 'code'
  for (let index = 0; index < content.length; index++) {
    const node = content[index]!
    if (!isCode(node)) {
      ordinary.push(node)
      continue
    }
    flush()
    let text = node.text ?? ''
    while (index + 1 < content.length && isCode(content[index + 1]!)) text += content[++index]!.text ?? ''
    const longestRun = Math.max(0, ...(text.match(/`+/g) ?? []).map((run) => run.length))
    const fence = '`'.repeat(longestRun + 1)
    const padding =
      text.startsWith('`') ||
      text.endsWith('`') ||
      (text.startsWith(' ') && text.endsWith(' ') && text.trim().length > 0)
        ? ' '
        : ''
    parts.push(`${fence}${padding}${text}${padding}${fence}`)
  }
  flush()
  return parts.join('')
}

function preserveBlock(source: string) {
  return `${preservedPrefix}${encodeURIComponent(source.replace(/\n+$/, ''))}${preservedSuffix}\n\n`
}

function hasHtml(token: unknown): boolean {
  if (!token || typeof token !== 'object') return false
  const item = token as Record<string, unknown>
  if (item.type === 'html') return !/^<br\s*\/?>$/i.test(String(item.raw ?? ''))
  return Object.values(item).some((value) => Array.isArray(value) && value.some(hasHtml))
}

function hasMarkedImage(token: unknown, marked = false): boolean {
  if (!token || typeof token !== 'object') return false
  const item = token as Record<string, unknown>
  if (item.type === 'image') return marked
  const insideMark = marked || ['link', 'strong', 'em', 'del'].includes(String(item.type))
  return Object.values(item).some(
    (value) => Array.isArray(value) && value.some((child) => hasMarkedImage(child, insideMark)),
  )
}

/** Keep unsupported source in inert, removable blocks instead of silently dropping it. */
export function prepareRichMarkdown(source: string): string {
  // Front matter is not a Markdown heading or divider. Keep the complete block.
  const frontMatter = source.match(/^---\r?\n[\s\S]*?\r?\n(?:---|\.\.\.)(?:\r?\n|$)/)?.[0]
  const body = frontMatter ? source.slice(frontMatter.length) : source
  const blocks = marked.lexer(body)
  return (
    (frontMatter ? preserveBlock(frontMatter) : '') +
    blocks
      .map((token) => {
        if (token.type === 'def') {
          // Leave the definition available to Marked's reference resolver, then
          // retain it as an opaque node after the lexer consumes the definition.
          return token.raw + '\n\n' + preserveBlock(token.raw)
        }
        // Upstream Markdown mark handlers currently only apply marks to text.
        // Keep linked/formatted image source intact instead of losing its link.
        if (hasHtml(token) || hasMarkedImage(token)) return preserveBlock(token.raw)
        return token.raw
      })
      .join('')
  )
}

const PreservedMarkdown = Node.create({
  name: 'preservedMarkdown',
  group: 'block',
  atom: true,
  selectable: true,
  addAttributes() {
    return { source: { default: '' } }
  },
  parseHTML() {
    return []
  },
  renderHTML({ node }) {
    // DOMOutputSpec text children are escaped. Never render arbitrary source as HTML.
    return [
      'div',
      { class: 'rich-markdown-preserved', contenteditable: 'false', role: 'note' },
      ['span', { class: 'rich-markdown-preserved-label' }, '原文内容已保留'],
      ['pre', {}, String(node.attrs.source)],
    ]
  },
  parseMarkdown(token, helpers) {
    return helpers.createNode('preservedMarkdown', { source: token.source })
  },
  renderMarkdown(node) {
    return String(node.attrs?.source ?? '')
  },
  markdownTokenizer: {
    name: 'preservedMarkdown',
    level: 'block',
    start: (source) => source.indexOf(preservedPrefix),
    tokenize(source) {
      if (!source.startsWith(preservedPrefix)) return undefined
      const end = source.indexOf(preservedSuffix, preservedPrefix.length)
      if (end < 0) return undefined
      try {
        return {
          type: 'preservedMarkdown',
          raw: source.slice(0, end + preservedSuffix.length),
          source: decodeURIComponent(source.slice(preservedPrefix.length, end)),
        }
      } catch {
        return undefined
      }
    },
  },
})

const MarkdownStarterKit = StarterKit.extend({
  addExtensions() {
    return (this.parent?.() ?? []).map((extension) => {
      if (extension.name === 'paragraph') {
        return extension.extend({
          renderMarkdown(node, helpers) {
            // Literal text at a paragraph boundary must not reopen as another
            // block type after clearing a heading/list/code-block format.
            return renderInlineMarkdown(node.content, (nodes) => helpers.renderChildren(nodes))
              .replace(/(^|\n)( {0,3})(#{1,6}(?=\s|$)|>|[-+]+(?=\s|$)|=+(?=\s|$))/g, '$1$2\\$3')
              .replace(/(^|\n)( {0,3})(\d{1,9})([.)])(?=\s|$)/g, '$1$2$3\\$4')
              .replace(
                /(^|\n)( {4,})/g,
                (_match, boundary: string, spaces: string) => `${boundary}&#32;${spaces.slice(1)}`,
              )
          },
          parseMarkdown(token, helpers) {
            // Upstream unwraps a sole image as a block image. Our inert image
            // placeholder is inline and must remain inside a paragraph.
            if (token.tokens?.length === 1 && token.tokens[0].type === 'image') {
              return helpers.createNode('paragraph', undefined, helpers.parseInline(token.tokens))
            }
            return this.parent?.(token, helpers) ?? []
          },
        })
      }
      if (extension.name === 'heading') {
        return extension.extend({
          renderMarkdown(node, helpers) {
            return `${'#'.repeat(Number(node.attrs?.level ?? 1))} ${renderInlineMarkdown(node.content, (nodes) => helpers.renderChildren(nodes))}`
          },
        })
      }
      if (extension.name === 'link') {
        return extension.extend({
          renderMarkdown(node, helpers) {
            const destination = String(node.attrs?.href ?? '')
            const href = /[\s<>]/.test(destination)
              ? `<${destination.replace(/[\\<>]/g, '\\$&')}>`
              : destination.replace(/[\\()]/g, '\\$&')
            const title = String(node.attrs?.title ?? '').replace(/[\\"]/g, '\\$&')
            return `[${helpers.renderChildren(node)}](${href}${title ? ` "${title}"` : ''})`
          },
        })
      }
      if (extension.name === 'codeBlock') {
        return extension.extend({
          renderMarkdown(node) {
            const code = (node.content ?? []).map((child) => child.text ?? '').join('')
            const language = String(node.attrs?.language ?? '')
            const marker = language.includes('`') ? '~' : '`'
            const runs = code.match(marker === '`' ? /`+/g : /~+/g) ?? []
            const longestFence = Math.max(2, ...runs.map((run) => run.length))
            const fence = marker.repeat(longestFence + 1)
            return `${fence}${language}\n${code}\n${fence}`
          },
        })
      }
      if (['blockquote', 'listItem', 'orderedList', 'bulletList'].includes(extension.name)) {
        return extension.extend({
          parseMarkdown(token, helpers) {
            const parsed = this.parent?.(token, helpers)
            if (!parsed || Array.isArray(parsed) || 'mark' in parsed) return parsed ?? []
            const fillContainers = (node: JSONContent): JSONContent => {
              const content = node.content?.map(fillContainers)
              return {
                ...node,
                ...(content ? { content } : {}),
                ...(['blockquote', 'listItem', 'taskItem'].includes(node.type ?? '') && !content?.length
                  ? { content: [{ type: 'paragraph' }] }
                  : {}),
              }
            }
            return fillContainers(parsed)
          },
        })
      }
      return extension
    })
  },
})

const PreservedImage = Node.create({
  name: 'preservedImage',
  markdownTokenName: 'image',
  inline: true,
  group: 'inline',
  atom: true,
  selectable: true,
  addAttributes() {
    return { source: { default: '' }, alt: { default: '' } }
  },
  parseHTML() {
    return []
  },
  renderHTML({ node }) {
    return [
      'span',
      {
        class: 'rich-markdown-image-placeholder',
        contenteditable: 'false',
        title: String(node.attrs.source),
        role: 'img',
        'aria-label': `图片：${node.attrs.alt || '未命名图片'}（已保留，未加载）`,
      },
      `▧ ${node.attrs.alt || '图片'}（未加载）`,
    ]
  },
  parseMarkdown(token, helpers) {
    return helpers.createNode('preservedImage', {
      source: token.raw ?? `![${token.text ?? ''}](${token.href ?? ''})`,
      alt: token.text ?? '',
    })
  },
  renderMarkdown(node) {
    return String(node.attrs?.source ?? '')
  },
})

export function richMarkdownExtensions(): Extensions {
  return [
    MarkdownStarterKit.configure({
      undoRedo: false,
      underline: false,
      trailingNode: false,
      link: { openOnClick: false },
    }),
    Markdown,
    Table.configure({ renderWrapper: true }),
    TableRow,
    // Markdown tables cannot retain block lists, nested tables or multiple
    // paragraphs inside a cell. Keep the editable schema equally restrictive.
    TableCell.extend({ content: 'paragraph' }),
    TableHeader.extend({ content: 'paragraph' }),
    TaskList,
    TaskItem.configure({
      nested: true,
      a11y: { checkboxLabel: (node) => `待办事项：${node.textContent || '未命名待办'}` },
    }),
    PreservedMarkdown,
    PreservedImage,
  ]
}

export function selectedRichMarkdown(editor: Editor): string {
  if (editor.state.selection.empty) return ''
  return richMarkdownSlice(editor, editor.state.selection.content())
}

export function richMarkdownSlice(editor: Editor, slice: Slice): string {
  // A partial text selection includes its open ancestors (including table,
  // row and cell). Strip those only when they lead to one inline textblock;
  // a copied table or multi-cell selection must keep its table structure.
  let content = slice.content
  for (let depth = Math.min(slice.openStart, slice.openEnd); depth > 0 && content.childCount === 1; depth--) {
    const first = content.firstChild!
    if (first.isTextblock) {
      return editor.markdown!.serialize({
        type: 'doc',
        content: [{ type: 'paragraph', content: first.content.toJSON() ?? [] }],
      })
    }
    content = first.content
  }
  return editor.markdown!.serialize({
    type: 'doc',
    content: slice.content.toJSON() ?? [],
  })
}

const toolbarActions: MarkdownToolbarAction[] = [
  'heading',
  'bold',
  'italic',
  'strike',
  'bulletList',
  'taskList',
  'orderedList',
  'quote',
  'inlineCode',
  'codeBlock',
  'link',
  'divider',
  'table',
  'clearFormat',
  'indent',
  'outdent',
]
const tableActions: MarkdownTableAction[] = [
  'addRowBefore',
  'addRowAfter',
  'deleteRow',
  'addColumnBefore',
  'addColumnAfter',
  'deleteColumn',
  'deleteTable',
  'alignLeft',
  'alignCenter',
  'alignRight',
]

function currentListItem(editor: Editor): 'listItem' | 'taskItem' | undefined {
  const { $from } = editor.state.selection
  for (let depth = $from.depth; depth > 0; depth--) {
    const name = $from.node(depth).type.name
    if (name === 'listItem' || name === 'taskItem') return name
  }
}

function currentTable(editor: Editor) {
  return isInTable(editor.state) ? selectedRect(editor.state) : undefined
}

function selectedHeading(editor: Editor): HeadingLevel | 0 | null {
  const { from, to, empty, $from } = editor.state.selection
  if (empty)
    return $from.parent.type.name === 'heading'
      ? ($from.parent.attrs.level as HeadingLevel)
      : $from.parent.type.name === 'paragraph'
        ? 0
        : null
  const levels = new Set<HeadingLevel | 0 | null>()
  editor.state.doc.nodesBetween(from, to, (node, position) => {
    if (!node.isTextblock) return
    const start = position + 1
    if (node.content.size && (start >= to || start + node.content.size <= from)) return
    levels.add(
      node.type.name === 'heading' ? (node.attrs.level as HeadingLevel) : node.type.name === 'paragraph' ? 0 : null,
    )
  })
  return levels.size === 1 ? [...levels][0]! : null
}

function selectionHasFormatting(editor: Editor): boolean {
  const { from, to, empty, $from } = editor.state.selection
  if (empty) {
    if ((editor.state.storedMarks ?? $from.marks()).length) return true
    return ['heading', 'bulletList', 'orderedList', 'taskList', 'blockquote', 'codeBlock'].some((name) =>
      editor.isActive(name),
    )
  }
  let formatted = false
  editor.state.doc.nodesBetween(from, to, (node) => {
    if (
      node.marks.length ||
      ['heading', 'bulletList', 'orderedList', 'taskList', 'blockquote', 'codeBlock'].includes(node.type.name)
    )
      formatted = true
  })
  return formatted
}

export function richMarkdownToolbarState(editor: Editor | null, disabled = false): MarkdownToolbarState {
  const state: MarkdownToolbarState = {
    active: Object.fromEntries(toolbarActions.map((action) => [action, false])),
    enabled: Object.fromEntries(toolbarActions.map((action) => [action, false])),
    headingLevel: 0,
    inTable: false,
    tableEnabled: Object.fromEntries(tableActions.map((action) => [action, false])),
    tableAlignment: null,
  }
  if (!editor || editor.isDestroyed) return state
  const table = currentTable(editor)
  state.inTable = !!table
  state.headingLevel = selectedHeading(editor)
  const nodes: Partial<Record<MarkdownToolbarAction, string>> = {
    heading: 'heading',
    bold: 'bold',
    italic: 'italic',
    strike: 'strike',
    bulletList: 'bulletList',
    taskList: 'taskList',
    orderedList: 'orderedList',
    quote: 'blockquote',
    inlineCode: 'code',
    codeBlock: 'codeBlock',
    link: 'link',
    table: 'table',
  }
  for (const action of toolbarActions) state.active[action] = !!nodes[action] && editor.isActive(nodes[action]!)
  if (table) {
    const alignments = new Set<string | null>()
    for (let row = 0; row < table.map.height; row++) {
      for (let column = table.left; column < table.right; column++) {
        const cell = table.table.nodeAt(table.map.map[row * table.map.width + column]!)
        alignments.add(cell?.attrs.align ?? null)
      }
    }
    if (alignments.size === 1) state.tableAlignment = [...alignments][0] as MarkdownToolbarState['tableAlignment']
  }
  if (disabled || !editor.isEditable) return state
  const canRun = (run: (can: ReturnType<Editor['can']>) => boolean) => {
    try {
      return run(editor.can())
    } catch {
      // Some upstream list conversions cannot represent task-item children.
      // State queries must remain pure and must never break editing.
      return false
    }
  }
  const item = currentListItem(editor)
  const hasTextSelection =
    editor.state.selection.empty ||
    !!editor.state.doc.textBetween(editor.state.selection.from, editor.state.selection.to, ' ')
  Object.assign(state.enabled, {
    heading: !table && (canRun((can) => can.setParagraph()) || canRun((can) => can.setHeading({ level: 2 }))),
    bold: canRun((can) => can.toggleBold()),
    italic: canRun((can) => can.toggleItalic()),
    strike: canRun((can) => can.toggleStrike()),
    bulletList: !table && canRun((can) => can.toggleBulletList()),
    taskList: !table && canRun((can) => can.toggleTaskList()),
    orderedList: !table && canRun((can) => can.toggleOrderedList()),
    quote: !table && canRun((can) => can.toggleBlockquote()),
    inlineCode: canRun((can) => can.toggleCode()),
    codeBlock: !table && canRun((can) => can.toggleCodeBlock()),
    link: hasTextSelection && canRun((can) => can.setLink({ href: 'https://example.com' })),
    divider: !!table || canRun((can) => can.setHorizontalRule()),
    table: !!table || canRun((can) => can.insertTable()),
    clearFormat: selectionHasFormatting(editor),
    indent: !!item && canRun((can) => can.sinkListItem(item)),
    outdent: !!item && canRun((can) => can.liftListItem(item)),
  })
  for (const action of tableActions) {
    state.tableEnabled[action] =
      !!table &&
      (action.startsWith('align') ||
        canRun((can) => can[action as Exclude<MarkdownTableAction, 'alignLeft' | 'alignCenter' | 'alignRight'>]()))
  }
  return state
}

/** Preserve GFM's first header row after table edits, within the same transaction. */
function normalizeMarkdownTables(transaction: Transaction) {
  transaction.doc.descendants((node, position) => {
    if (node.type.name !== 'table') return
    let rowPosition = position + 1
    node.forEach((row, _offset, index) => {
      let cellPosition = rowPosition + 1
      row.forEach((cell) => {
        const type = transaction.doc.type.schema.nodes[index === 0 ? 'tableHeader' : 'tableCell']!
        if (cell.type !== type) transaction.setNodeMarkup(cellPosition, type, cell.attrs)
        cellPosition += cell.nodeSize
      })
      rowPosition += row.nodeSize
    })
    return false
  })
}

export function runMarkdownTableAction(editor: Editor, action: MarkdownTableAction): boolean {
  const table = currentTable(editor)
  if (!table || !richMarkdownToolbarState(editor).tableEnabled[action]) return false
  if (
    (action === 'deleteRow' && table.top === 0 && table.bottom === table.map.height) ||
    (action === 'deleteColumn' && table.left === 0 && table.right === table.map.width)
  ) {
    return editor.commands.deleteTable()
  }
  if (action.startsWith('align')) {
    const alignment = action === 'alignLeft' ? 'left' : action === 'alignCenter' ? 'center' : 'right'
    return editor
      .chain()
      .command(({ tr }) => {
        for (let row = 0; row < table.map.height; row++) {
          for (let column = table.left; column < table.right; column++) {
            const position = table.tableStart + table.map.map[row * table.map.width + column]!
            const cell = tr.doc.nodeAt(position)!
            if (cell.attrs.align !== alignment)
              tr.setNodeMarkup(position, undefined, { ...cell.attrs, align: alignment })
          }
        }
        return true
      })
      .run()
  }
  const chain = editor.chain()
  chain[action as Exclude<MarkdownTableAction, 'alignLeft' | 'alignCenter' | 'alignRight'>]()
  return chain
    .command(({ tr }) => {
      normalizeMarkdownTables(tr)
      return true
    })
    .run()
}

export function insertRichMarkdownTable(editor: Editor, rows = 2, columns = 2): boolean {
  if (!editor.isEditable || !Number.isFinite(rows) || !Number.isFinite(columns) || currentTable(editor)) return false
  const boundedRows = Math.max(1, Math.min(20, Math.trunc(rows)))
  const boundedColumns = Math.max(1, Math.min(10, Math.trunc(columns)))
  return editor.chain().insertTable({ rows: boundedRows, cols: boundedColumns, withHeaderRow: true }).run()
}

export function indentRichMarkdown(editor: Editor, outdent = false): boolean {
  if (!editor.isEditable) return false
  const item = currentListItem(editor)
  if (!item) return false
  return outdent ? editor.commands.liftListItem(item) : editor.commands.sinkListItem(item)
}

export function clearRichMarkdownFormat(editor: Editor): boolean {
  if (!editor.isEditable) return false
  if (selectionInMarkdownCell(editor)) return editor.commands.unsetAllMarks()
  // Follow clearNodes' block normalization while keeping entire tables opaque;
  // lifting their internal paragraphs would destroy rows and cell boundaries.
  return editor
    .chain()
    .unsetAllMarks()
    .command(({ tr, state }) => {
      for (const { $from, $to } of tr.selection.ranges) {
        state.doc.nodesBetween($from.pos, $to.pos, (node, position) => {
          if (node.type.name === 'table') return false
          if (node.isText) return
          const from = tr.doc.resolve(tr.mapping.map(position))
          const to = tr.doc.resolve(tr.mapping.map(position + node.nodeSize))
          const range = from.blockRange(to)
          if (!range) return
          const target = liftTarget(range)
          if (node.isTextblock) {
            const type = from.parent.contentMatchAt(from.index()).defaultType
            if (type) tr.setNodeMarkup(range.start, type)
          }
          if (target !== null) tr.lift(range, target)
        })
      }
      return true
    })
    .run()
}

export function selectionInMarkdownCell(editor: Editor): boolean {
  const { $from } = editor.state.selection
  for (let depth = $from.depth; depth > 0; depth--) {
    if (['tableCell', 'tableHeader'].includes($from.node(depth).type.name)) return true
  }
  return false
}

/** A GFM cell can keep inline formatting and breaks, but cannot contain blocks. */
export function markdownCellContent(editor: Editor, source: string): JSONContent[] {
  const parsed = editor.markdown!.parse(prepareRichMarkdown(source))
  const join = (parts: JSONContent[][], separator: JSONContent = { type: 'hardBreak' }) =>
    parts.flatMap((part, index) => (index ? [separator, ...part] : part))
  const plain = (text: string, marks?: JSONContent['marks']): JSONContent[] =>
    join(text.split(/\r?\n/).map((line) => (line ? [{ type: 'text', text: line, ...(marks ? { marks } : {}) }] : [])))
  const flatten = (node: JSONContent): JSONContent[] => {
    if (node.type === 'text') return plain(node.text ?? '', node.marks)
    if (node.type === 'preservedImage' || node.type === 'hardBreak') return [node]
    if (node.type === 'preservedMarkdown') return plain(String(node.attrs?.source ?? ''))
    if (node.type === 'horizontalRule') return [{ type: 'text', text: '---' }]
    const children = (node.content ?? []).map(flatten)
    if (['paragraph', 'heading', 'codeBlock', 'tableCell', 'tableHeader'].includes(node.type ?? ''))
      return children.flat()
    if (node.type === 'tableRow') return join(children, { type: 'text', text: ' | ' })
    if (node.type === 'taskItem')
      return [{ type: 'text', text: node.attrs?.checked ? '[x] ' : '[ ] ' }, ...join(children)]
    return join(children)
  }
  return flatten(parsed)
}

export function insertMarkdownDivider(editor: Editor): boolean {
  const { $from } = editor.state.selection
  for (let depth = $from.depth; depth > 0; depth--) {
    if ($from.node(depth).type.name === 'table') {
      return editor.commands.insertContentAt($from.after(depth), [{ type: 'horizontalRule' }, { type: 'paragraph' }])
    }
  }
  return editor.commands.setHorizontalRule()
}

/** Set external content without emitting an edit or resetting an echoed value. */
export function setRichMarkdown(editor: Editor, source: string, resetSelection = false) {
  const { from, to } = editor.state.selection
  editor.commands.setContent(prepareRichMarkdown(source), { contentType: 'markdown', emitUpdate: false })
  const doc = editor.state.doc
  const selection = resetSelection
    ? Selection.atStart(doc)
    : TextSelection.between(doc.resolve(Math.min(from, doc.content.size)), doc.resolve(Math.min(to, doc.content.size)))
  editor.view.dispatch(editor.state.tr.setSelection(selection))
}

/** Convert a typed GFM header and separator on Enter, leaving IME composition alone. */
export function convertTypedMarkdownTable(editor: Editor): boolean {
  const { selection, doc } = editor.state
  const { $from, empty } = selection
  if (
    editor.view.composing ||
    !empty ||
    $from.depth !== 1 ||
    $from.parent.type.name !== 'paragraph' ||
    $from.parentOffset !== $from.parent.content.size
  ) {
    return false
  }
  const separator = $from.parent.textContent
  if (!/^\s*\|?\s*:?-{3,}:?\s*(?:\|\s*:?-{3,}:?\s*)+\|?\s*$/.test(separator)) return false
  const index = $from.index(0)
  if (!index) return false
  const header = doc.child(index - 1)
  if (header.type.name !== 'paragraph' || !header.textContent.includes('|')) return false
  const parsed = editor.markdown?.parse(`${header.textContent}\n${separator}\n`)
  const tableJson = parsed?.content?.[0]
  if (parsed?.content?.length !== 1 || tableJson?.type !== 'table' || !tableJson.content?.[0]?.content?.length)
    return false
  const columnCount = tableJson.content[0].content.length
  tableJson.content.push({
    type: 'tableRow',
    content: Array.from({ length: columnCount }, () => ({ type: 'tableCell', content: [{ type: 'paragraph' }] })),
  })
  const table = editor.schema.nodeFromJSON(tableJson)
  const from = $from.before(1) - header.nodeSize
  const to = $from.after(1)
  const transaction = editor.state.tr.replaceWith(from, to, table)
  const firstBodyCell = from + table.child(0).nodeSize + 4
  transaction.setSelection(TextSelection.near(transaction.doc.resolve(firstBodyCell)))
  editor.view.dispatch(transaction.scrollIntoView())
  return true
}

export function markdownTableTemplate(): JSONContent {
  const cell = (type: string, text: string): JSONContent => ({
    type,
    content: [{ type: 'paragraph', content: [{ type: 'text', text }] }],
  })
  return {
    type: 'table',
    content: [
      { type: 'tableRow', content: [cell('tableHeader', '项目'), cell('tableHeader', '内容')] },
      { type: 'tableRow', content: [cell('tableCell', '项目名称'), cell('tableCell', '填写内容')] },
    ],
  }
}
