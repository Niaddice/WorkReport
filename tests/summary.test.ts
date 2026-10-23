import { describe, expect, it } from 'vitest'
import { aiSummarySource, markdownSummaryText, plainTextSummary, summaryExportContent } from '../src/lib/summary'

const documents = [
  { date: '20261018', content: '完成事项 A\n\n- 细节 A' },
  { date: '20261019', content: '\n继续推进 B\n' },
]

describe('summary content', () => {
  it('keeps the weekday and markdown separators for the AI source', () => {
    expect(aiSummarySource(documents)).toBe('### 2026年10月18日 星期日\n\n完成事项 A\n\n- 细节 A\n\n---\n\n### 2026年10月19日 星期一\n\n继续推进 B')
  })
  it('outputs plain text without weekday or separators, numbering items per day', () => {
    const text = plainTextSummary(documents)
    expect(text).toBe('2026年10月18日\n\n1. 完成事项 A\n2. 细节 A\n\n2026年10月19日\n\n1. 继续推进 B')
    expect(text).not.toContain('#')
    expect(text).not.toContain('---')
    expect(text).not.toContain('星期')
    expect(text.split('\n\n2026年10月19日')).toHaveLength(2)
  })
  it('numbers each item per day and skips the date heading', () => {
    const text = plainTextSummary([{ date: '20261019', content: '2026年10月19日 日报\n\n1. 驾驶舱\n重构取数来源。\n2. 补充文档\n' }])
    expect(text).toBe('2026年10月19日\n\n1. 驾驶舱\n   重构取数来源。\n2. 补充文档')
  })
  it('keeps multi-line paragraphs in one numbered item and trims blank lines', () => {
    expect(plainTextSummary([{ date: '20261020', content: '\n\n第一行\n第二行\n\n' }])).toBe('2026年10月20日\n\n1. 第一行\n   第二行')
  })
  it('exports AI Markdown as readable plain text without losing code, links, or task status', () => {
    const markdown = '# 本周总结\n\n完成 **驾驶舱** 与 _文档_。\n\n- [x] 提交 `view.ts`\n- [ ] 阅读 [设计说明](https://example.com/design)\n\n> 下一步：优化\n\n```ts\nconst text = "# 保留代码"\n```'
    expect(markdownSummaryText(markdown)).toBe('本周总结\n\n完成 驾驶舱 与 文档。\n\n- [x] 提交 view.ts\n- [ ] 阅读 设计说明 (https://example.com/design)\n\n下一步：优化\n\nconst text = "# 保留代码"')
  })
  it('preserves ordered list starts and table values while removing presentation markup', () => {
    const markdown = '3. 已完成\n4. 下一步\n\n| 事项 | 状态 |\n| --- | --- |\n| **文档** | 已完成 |\n\n---\n\nA &amp; B<br>下一行'
    expect(markdownSummaryText(markdown)).toBe('3. 已完成\n4. 下一步\n\n事项\t状态\n文档\t已完成\n\nA & B\n下一行')
  })
  it('keeps direct summary text and all Markdown exports byte-for-byte intact', () => {
    const direct = '2026年10月22日\n\n1. 使用 # 标签\n   保留 **原文** 与缩进\n'
    expect(summaryExportContent(direct, 'txt', 'direct')).toBe(direct)
    expect(summaryExportContent(direct, 'md', 'direct')).toBe(direct)
    const markdown = '# 工作总结\n\n**完成**\n'
    expect(summaryExportContent(markdown, 'md', 'ai')).toBe(markdown)
    expect(summaryExportContent(markdown, 'txt', 'ai')).toBe('工作总结\n\n完成')
  })
})
