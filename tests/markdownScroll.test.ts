// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest'
import {
  createTextareaScrollReader,
  elementScrollPosition,
  markdownPreviewScrollTop,
  markdownWithSourceLines,
  type MarkdownScrollPosition,
} from '../src/lib/markdownScroll'

describe('Markdown source scroll mapping', () => {
  it('keeps original line numbers across empty lines, references, tables and fenced code', () => {
    const source =
      '# 标题\r\n\r\n[ref]: https://example.test\r\n\r\n段落 [链接][ref]\r\n\r\n' +
      '| 列 |\r\n| --- |\r\n| 值 |\r\n\r\n```ts\r\nconst x = 1\r\n```\r\n'
    const document = new DOMParser().parseFromString(markdownWithSourceLines(source), 'text/html')
    const blocks = Array.from(document.querySelectorAll<HTMLElement>('.markdown-source-block'))
    expect(blocks.map((block) => [Number(block.dataset.sourceLine), Number(block.dataset.sourceEnd)])).toEqual([
      [0, 1],
      [4, 5],
      [6, 9],
      [10, 13],
    ])
    expect(document.querySelector('a')?.getAttribute('href')).toBe('https://example.test')
    expect(blocks[2]!.querySelector('table')).not.toBeNull()
    expect(blocks[3]!.querySelector('code')?.textContent).toContain('const x = 1')
  })

  it('follows the matching source block instead of the overall scroll percentage', () => {
    const position: MarkdownScrollPosition = { sourceKey: '20261026', line: 12, ratio: 0.2, atBottom: false }
    const anchors = [
      { line: 0, top: 0 },
      { line: 10, top: 500 },
      { line: 14, top: 700 },
      { line: 30, top: 900 },
    ]
    expect(markdownPreviewScrollTop(position, anchors, 1000)).toBe(600)
    expect(markdownPreviewScrollTop({ ...position, atBottom: true }, anchors, 1000)).toBe(1000)
    expect(markdownPreviewScrollTop({ ...position, ratio: 0 }, anchors, 1000)).toBe(0)
    expect(markdownPreviewScrollTop(position, anchors, 100)).toBe(100)
  })

  it('uses proportional scrolling for rendered editor views and handles short content', () => {
    const position: MarkdownScrollPosition = { sourceKey: '20261026', line: null, ratio: 0.4, atBottom: false }
    expect(markdownPreviewScrollTop(position, [], 500)).toBe(200)
    expect(markdownPreviewScrollTop(position, [], -10)).toBe(0)
    const element = document.createElement('div')
    Object.defineProperties(element, { scrollHeight: { value: 100 }, clientHeight: { value: 200 } })
    expect(elementScrollPosition(element, position.sourceKey)).toEqual({ ...position, ratio: 0 })
  })

  it('measures wrapped source lines once, invalidates after edits and removes the hidden mirror', () => {
    const textarea = document.createElement('textarea')
    textarea.value = '标题\n会折成三行的长段落\n结尾'
    textarea.style.cssText = 'font-size:10px;line-height:20px;padding:10px;'
    Object.defineProperties(textarea, {
      clientWidth: { value: 200 },
      clientHeight: { value: 100 },
      scrollHeight: { value: 500 },
    })
    textarea.scrollTop = 70
    document.body.append(textarea)
    const offsets: Record<string, number> = { 标题: 10, 会折成三行的长段落: 30, 结尾: 90, 新增行: 110 }
    const top = vi.spyOn(HTMLElement.prototype, 'offsetTop', 'get').mockImplementation(function (this: HTMLElement) {
      return this.tagName === 'SPAN' ? (offsets[this.textContent!] ?? 0) : 0
    })
    const height = vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(20)
    const reader = createTextareaScrollReader()
    try {
      expect(reader.read(textarea, '20261026').line).toBeCloseTo(1 + 2 / 3)
      const measurements = top.mock.calls.length
      textarea.scrollTop = 80
      expect(reader.read(textarea, '20261026').line).toBeCloseTo(1 + 5 / 6)
      expect(top.mock.calls).toHaveLength(measurements)
      textarea.value += '\n新增行'
      reader.read(textarea, '20261026')
      expect(top.mock.calls.length).toBeGreaterThan(measurements)
      expect(document.body.querySelector('[aria-hidden="true"]')).not.toBeNull()
    } finally {
      reader.dispose()
      textarea.remove()
      top.mockRestore()
      height.mockRestore()
    }
    expect(document.body.querySelector('[aria-hidden="true"]')).toBeNull()
  })
})
