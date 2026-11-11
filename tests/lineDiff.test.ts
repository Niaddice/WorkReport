import { describe, expect, it } from 'vitest'
import { computeLineDiff } from '../src/lib/lineDiff'

describe('computeLineDiff', () => {
  it('returns identical result when texts match', () => {
    const text = '第一行\n第二行\n第三行'
    const result = computeLineDiff(text, text)
    expect(result.identical).toBe(true)
    expect(result.additions).toBe(0)
    expect(result.deletions).toBe(0)
    expect(result.lines).toHaveLength(3)
    expect(result.lines.every((line) => line.kind === 'context')).toBe(true)
    expect(result.splitRows).toHaveLength(3)
  })

  it('detects simple line additions', () => {
    const oldText = '任务一\n任务二'
    const newText = '任务一\n新插入任务\n任务二'
    const result = computeLineDiff(oldText, newText)
    expect(result.identical).toBe(false)
    expect(result.additions).toBe(1)
    expect(result.deletions).toBe(0)

    const addLine = result.lines.find((line) => line.kind === 'add')
    expect(addLine).toBeDefined()
    expect(addLine?.text).toBe('新插入任务')
    expect(addLine?.newLine).toBe(2)
    expect(addLine?.oldLine).toBeNull()
  })

  it('detects line deletions', () => {
    const oldText = '标题\n待删除项\n结尾'
    const newText = '标题\n结尾'
    const result = computeLineDiff(oldText, newText)
    expect(result.identical).toBe(false)
    expect(result.additions).toBe(0)
    expect(result.deletions).toBe(1)

    const removeLine = result.lines.find((line) => line.kind === 'remove')
    expect(removeLine).toBeDefined()
    expect(removeLine?.text).toBe('待删除项')
    expect(removeLine?.oldLine).toBe(2)
    expect(removeLine?.newLine).toBeNull()
  })

  it('aligns modifications in split rows correctly', () => {
    const oldText = '保持行1\n旧内容\n保持行2'
    const newText = '保持行1\n新内容\n保持行2'
    const result = computeLineDiff(oldText, newText)

    expect(result.additions).toBe(1)
    expect(result.deletions).toBe(1)
    expect(result.splitRows).toHaveLength(3)

    // Row 1: context
    expect(result.splitRows[0].left.kind).toBe('context')
    expect(result.splitRows[0].right.kind).toBe('context')

    // Row 2: left remove, right add
    expect(result.splitRows[1].left.kind).toBe('remove')
    expect(result.splitRows[1].left.text).toBe('旧内容')
    expect(result.splitRows[1].right.kind).toBe('add')
    expect(result.splitRows[1].right.text).toBe('新内容')

    // Row 3: context
    expect(result.splitRows[2].left.kind).toBe('context')
    expect(result.splitRows[2].right.kind).toBe('context')
  })

  it('handles empty oldText or newText', () => {
    const resultFromEmpty = computeLineDiff('', '全新内容')
    expect(resultFromEmpty.additions).toBe(1)
    expect(resultFromEmpty.deletions).toBe(0)

    const resultToEmpty = computeLineDiff('全部删除', '')
    expect(resultToEmpty.additions).toBe(0)
    expect(resultToEmpty.deletions).toBe(1)
  })
})
