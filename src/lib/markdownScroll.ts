import { marked } from 'marked'

export interface MarkdownScrollPosition {
  sourceKey: string
  /** 原文的逻辑行，包含当前行内的滚动比例；渲染视图滚动时为 null。 */
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
  return { read, dispose }
}
