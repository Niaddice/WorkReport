import { marked } from 'marked'

export interface MarkdownScrollPosition {
  sourceKey: string
  /** 发起滚动的模块实例，用于忽略自身广播。 */
  origin?: string
  /** 原文的逻辑行，包含当前行内的滚动比例；无法映射原文时为 null。 */
  line: number | null
  ratio: number
  atBottom: boolean
}

export interface MarkdownScrollAnchor {
  line: number
  top: number
}

function clamp(value: number, maximum = 1) {
  return Math.min(maximum, Math.max(0, Number.isFinite(value) ? value : 0))
}

/** 返回带原文行号的 Markdown 块；调用方仍需按预览策略清洗 HTML。 */
export function markdownWithSourceLines(content: string): string {
  const source = content.replace(/\r\n?/g, '\n')
  let offset = 0
  let line = 0
  return marked
    .lexer(source)
    .map((token) => {
      const found = source.indexOf(token.raw, offset)
      const start = found < 0 ? offset : found
      line += source.slice(offset, start).split('\n').length - 1
      const startLine = line
      const endLine = startLine + token.raw.replace(/\n+$/, '').split('\n').length
      line += token.raw.split('\n').length - 1
      offset = start + token.raw.length
      const html = marked.parser([token], { async: false })
      return html
        ? `<div class="markdown-source-block" data-source-line="${startLine}" data-source-end="${endLine}">${html}</div>`
        : ''
    })
    .join('')
}

export function elementScrollPosition(element: HTMLElement, sourceKey: string): MarkdownScrollPosition {
  const maximum = Math.max(0, element.scrollHeight - element.clientHeight)
  const top = clamp(element.scrollTop, maximum)
  return { sourceKey, line: null, ratio: maximum ? top / maximum : 0, atBottom: maximum > 0 && top >= maximum - 1 }
}

/** 从可见原文位置插值到预览段落；两边的标题、表格和代码块可有不同高度。 */
export function markdownPreviewScrollTop(
  position: MarkdownScrollPosition,
  anchors: MarkdownScrollAnchor[],
  maximum: number,
): number {
  maximum = Math.max(0, maximum)
  if (position.atBottom) return maximum
  if (position.ratio <= 0) return 0
  if (position.line === null || !anchors.length || anchors.at(-1)!.top <= 0) return clamp(position.ratio) * maximum
  const line = position.line
  const next = anchors.findIndex((anchor) => anchor.line > line)
  if (next === 0) return clamp(anchors[0].top, maximum)
  if (next < 0) return clamp(anchors.at(-1)!.top, maximum)
  const before = anchors[next - 1]
  const after = anchors[next]
  const fraction = clamp((line - before.line) / (after.line - before.line))
  return clamp(before.top + fraction * (after.top - before.top), maximum)
}

/** 收集预览段落与原文行号的对应位置，供两个方向共用。 */
export function markdownScrollAnchors(body: HTMLElement, content: string): MarkdownScrollAnchor[] {
  const bounds = body.getBoundingClientRect()
  const totalLines = content.replace(/\r\n?/g, '\n').split('\n').length
  const points = new Map<number, number>([[0, 0]])
  for (const block of body.querySelectorAll<HTMLElement>('.markdown-source-block')) {
    const line = Number(block.dataset.sourceLine)
    const end = Number(block.dataset.sourceEnd)
    if (!Number.isFinite(line) || !Number.isFinite(end) || line < 0 || end <= line || end > totalLines) continue
    const rect = block.getBoundingClientRect()
    points.set(line, Math.max(0, rect.top - bounds.top))
    points.set(end, Math.max(0, rect.bottom - bounds.top))
  }
  points.set(totalLines, Math.max(body.scrollHeight, bounds.height, ...points.values()))
  return Array.from(points, ([line, top]) => ({ line, top })).sort((left, right) => left.line - right.line)
}

/** 从预览位置反推原文行；无法测量时使用滚动比例。 */
export function markdownPreviewScrollPosition(
  element: HTMLElement,
  sourceKey: string,
  anchors: MarkdownScrollAnchor[],
): MarkdownScrollPosition {
  const position = elementScrollPosition(element, sourceKey)
  if (!anchors.length || anchors.at(-1)!.top <= 0) return position
  const next = anchors.findIndex((anchor) => anchor.top > element.scrollTop)
  if (next === 0) position.line = anchors[0].line
  else if (next < 0) position.line = anchors.at(-1)!.line
  else {
    const before = anchors[next - 1]
    const after = anchors[next]
    const fraction = clamp((element.scrollTop - before.top) / Math.max(1, after.top - before.top))
    position.line = before.line + fraction * (after.line - before.line)
  }
  return position
}

/** 程序同步产生的 scroll 事件不再广播；用户滚到新位置后立即重新接管。 */
export function createScrollSyncGuard() {
  const applied = new WeakMap<HTMLElement, number>()
  return {
    apply(element: HTMLElement, top: number) {
      element.scrollTop = clamp(top, Math.max(0, element.scrollHeight - element.clientHeight))
      applied.set(element, element.scrollTop)
    },
    accept(element: HTMLElement): boolean {
      const top = applied.get(element)
      if (top !== undefined && Math.abs(element.scrollTop - top) < 1) return false
      applied.delete(element)
      return true
    },
  }
}

/** 缓存 textarea 的换行测量；滚动时只查找行号，不重建测量 DOM。 */
export function createTextareaScrollReader() {
  let mirror: HTMLDivElement | undefined
  let signature = ''
  let content = ''
  let starts: number[] = []
  let end = 0
  const properties = [
    'font-family',
    'font-size',
    'font-weight',
    'font-style',
    'line-height',
    'letter-spacing',
    'word-spacing',
    'text-indent',
    'text-transform',
    'tab-size',
    'padding-top',
    'padding-right',
    'padding-bottom',
    'padding-left',
  ]

  function read(element: HTMLTextAreaElement, sourceKey: string): MarkdownScrollPosition {
    const position = elementScrollPosition(element, sourceKey)
    const style = element.ownerDocument.defaultView!.getComputedStyle(element)
    const values = properties.map((property) => style.getPropertyValue(property))
    const nextSignature = JSON.stringify([element.clientWidth, values])
    if (signature !== nextSignature || content !== element.value || !mirror) {
      signature = nextSignature
      content = element.value
      mirror ??= element.ownerDocument.createElement('div')
      mirror.setAttribute('aria-hidden', 'true')
      mirror.style.cssText =
        'position:fixed;left:-100000px;top:0;visibility:hidden;pointer-events:none;box-sizing:border-box;white-space:pre-wrap;overflow-wrap:break-word;border:0;margin:0;'
      mirror.style.width = `${element.clientWidth}px`
      properties.forEach((property, index) => mirror!.style.setProperty(property, values[index]))
      const lines = element.value.split('\n').map((line) => {
        const span = element.ownerDocument.createElement('span')
        span.style.display = 'block'
        span.textContent = line || '\u200b'
        return span
      })
      mirror.replaceChildren(...lines)
      if (!mirror.isConnected) element.ownerDocument.body.append(mirror)
      const origin = lines[0].offsetTop
      starts = lines.map((line) => line.offsetTop - origin)
      const lineHeight = Number.parseFloat(style.lineHeight) || (Number.parseFloat(style.fontSize) || 12) * 1.85
      if (lines.length > 1 && starts.at(-1) === 0) starts = lines.map((_, index) => index * lineHeight)
      end = Math.max(starts.at(-1)! + lineHeight, lines.at(-1)!.offsetTop + lines.at(-1)!.offsetHeight - origin)
    }
    const top = Math.max(0, element.scrollTop - (Number.parseFloat(style.paddingTop) || 0))
    let low = 0
    let high = starts.length - 1
    while (low < high) {
      const middle = Math.ceil((low + high) / 2)
      if (starts[middle] <= top) low = middle
      else high = middle - 1
    }
    const bottom = starts[low + 1] ?? end
    position.line = low + clamp((top - starts[low]) / Math.max(1, bottom - starts[low]))
    return position
  }

  function dispose() {
    mirror?.remove()
    mirror = undefined
    signature = ''
    content = ''
  }
  function scrollTopFor(element: HTMLTextAreaElement, position: MarkdownScrollPosition): number {
    const maximum = Math.max(0, element.scrollHeight - element.clientHeight)
    if (position.atBottom) return maximum
    if (position.ratio <= 0) return 0
    if (position.line === null) return clamp(position.ratio) * maximum
    read(element, position.sourceKey)
    const line = clamp(position.line, starts.length)
    const index = Math.min(starts.length - 1, Math.floor(line))
    const top = starts[index]
    const bottom = starts[index + 1] ?? end
    const padding = Number.parseFloat(element.ownerDocument.defaultView!.getComputedStyle(element).paddingTop) || 0
    return clamp(padding + top + (bottom - top) * (line - index), maximum)
  }
  return { read, scrollTopFor, dispose }
}
