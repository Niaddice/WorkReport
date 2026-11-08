export type DiffLineKind = 'context' | 'add' | 'remove'

export interface DiffLine {
  kind: DiffLineKind
  oldLine: number | null
  newLine: number | null
  text: string
}

export interface SplitDiffSide {
  line: number | null
  text: string
  kind: 'context' | 'add' | 'remove' | 'empty'
}

export interface SplitDiffRow {
  left: SplitDiffSide
  right: SplitDiffSide
}

export interface DiffResult {
  lines: DiffLine[]
  splitRows: SplitDiffRow[]
  additions: number
  deletions: number
  identical: boolean
}

/**
 * 将两个字符串按行拆分，并保留统一换行符标准。
 */
function splitLines(text: string): string[] {
  if (!text) return []
  const normalized = text.replace(/\r\n/g, '\n').replace(/\r/g, '\n')
  const lines = normalized.split('\n')
  // 保持与 git diff 惯例一致：如果末尾没有换行符，最后一行保留为普通行；若末尾是换行符，最后为空行
  return lines
}

/**
 * 使用 Myers 差异算法计算两个文本行数组之间的差异。
 */
function myersDiff(oldLines: string[], newLines: string[]): Array<{ op: 'equal' | 'add' | 'remove'; text: string }> {
  const n = oldLines.length
  const m = newLines.length
  const max = n + m

  if (n === 0 && m === 0) return []
  if (n === 0) return newLines.map((text) => ({ op: 'add', text }))
  if (m === 0) return oldLines.map((text) => ({ op: 'remove', text }))

  const v: Record<number, number> = { 1: 0 }
  const trace: Array<Record<number, number>> = []

  let foundD = -1

  outer: for (let d = 0; d <= max; d++) {
    const vCopy: Record<number, number> = { ...v }
    trace.push(vCopy)

    for (let k = -d; k <= d; k += 2) {
      let x: number
      if (k === -d || (k !== d && (v[k - 1] ?? 0) < (v[k + 1] ?? 0))) {
        x = v[k + 1] ?? 0
      } else {
        x = (v[k - 1] ?? 0) + 1
      }
      let y = x - k

      while (x < n && y < m && oldLines[x] === newLines[y]) {
        x++
        y++
      }

      v[k] = x

      if (x >= n && y >= m) {
        foundD = d
        break outer
      }
    }
  }

  // 回溯查找最优编辑脚本 (SES)
  const script: Array<{ op: 'equal' | 'add' | 'remove'; text: string }> = []
  let x = n
  let y = m

  for (let d = foundD; d > 0; d--) {
    const vPrev = trace[d]
    const k = x - y

    let prevK: number
    if (k === -d || (k !== d && (vPrev[k - 1] ?? 0) < (vPrev[k + 1] ?? 0))) {
      prevK = k + 1
    } else {
      prevK = k - 1
    }

    const prevX = vPrev[prevK] ?? 0
    const prevY = prevX - prevK

    while (x > prevX && y > prevY) {
      script.push({ op: 'equal', text: oldLines[x - 1] })
      x--
      y--
    }

    if (x === prevX) {
      script.push({ op: 'add', text: newLines[y - 1] })
      y--
    } else {
      script.push({ op: 'remove', text: oldLines[x - 1] })
      x--
    }
  }

  while (x > 0 && y > 0) {
    script.push({ op: 'equal', text: oldLines[x - 1] })
    x--
    y--
  }

  return script.reverse()
}

/**
 * 将平铺差异生成并排双栏行数据 (Split Diff Rows)。
 */
function buildSplitRows(lines: DiffLine[]): SplitDiffRow[] {
  const rows: SplitDiffRow[] = []
  let i = 0

  while (i < lines.length) {
    const current = lines[i]

    if (current.kind === 'context') {
      rows.push({
        left: { line: current.oldLine, text: current.text, kind: 'context' },
        right: { line: current.newLine, text: current.text, kind: 'context' },
      })
      i++
      continue
    }

    // 收集连续的 remove 和 add 块
    const removeBlock: DiffLine[] = []
    const addBlock: DiffLine[] = []

    while (i < lines.length && (lines[i].kind === 'remove' || lines[i].kind === 'add')) {
      if (lines[i].kind === 'remove') {
        removeBlock.push(lines[i])
      } else {
        addBlock.push(lines[i])
      }
      i++
    }

    const maxCount = Math.max(removeBlock.length, addBlock.length)
    for (let j = 0; j < maxCount; j++) {
      const leftLine = removeBlock[j]
      const rightLine = addBlock[j]

      rows.push({
        left: leftLine
          ? { line: leftLine.oldLine, text: leftLine.text, kind: 'remove' }
          : { line: null, text: '', kind: 'empty' },
        right: rightLine
          ? { line: rightLine.newLine, text: rightLine.text, kind: 'add' }
          : { line: null, text: '', kind: 'empty' },
      })
    }
  }

  return rows
}

/**
 * 计算旧文本与新文本的逐行差异。
 * 支持单栏逐行统一视图 (Unified Lines) 与并排双栏视图 (Split Rows)。
 */
export function computeLineDiff(oldText: string, newText: string): DiffResult {
  if (oldText === newText) {
    const lines = splitLines(oldText)
    const diffLines: DiffLine[] = lines.map((text, idx) => ({
      kind: 'context',
      oldLine: idx + 1,
      newLine: idx + 1,
      text,
    }))
    const splitRows: SplitDiffRow[] = diffLines.map((item) => ({
      left: { line: item.oldLine, text: item.text, kind: 'context' },
      right: { line: item.newLine, text: item.text, kind: 'context' },
    }))
    return {
      lines: diffLines,
      splitRows,
      additions: 0,
      deletions: 0,
      identical: true,
    }
  }

  const oldLines = splitLines(oldText)
  const newLines = splitLines(newText)

  // 1. 消除公共前后缀以提升长文本比较性能
  let start = 0
  while (start < oldLines.length && start < newLines.length && oldLines[start] === newLines[start]) {
    start++
  }

  let oldEnd = oldLines.length - 1
  let newEnd = newLines.length - 1
  while (oldEnd >= start && newEnd >= start && oldLines[oldEnd] === newLines[newEnd]) {
    oldEnd--
    newEnd--
  }

  const prefix = oldLines.slice(0, start)
  const suffix = oldLines.slice(oldEnd + 1)
  const middleOld = oldLines.slice(start, oldEnd + 1)
  const middleNew = newLines.slice(start, newEnd + 1)

  const middleScript = myersDiff(middleOld, middleNew)

  let oldLineNum = 1
  let newLineNum = 1
  let additions = 0
  let deletions = 0

  const diffLines: DiffLine[] = []

  // 前缀
  for (const text of prefix) {
    diffLines.push({
      kind: 'context',
      oldLine: oldLineNum++,
      newLine: newLineNum++,
      text,
    })
  }

  // 中间差异
  for (const item of middleScript) {
    if (item.op === 'equal') {
      diffLines.push({
        kind: 'context',
        oldLine: oldLineNum++,
        newLine: newLineNum++,
        text: item.text,
      })
    } else if (item.op === 'remove') {
      diffLines.push({
        kind: 'remove',
        oldLine: oldLineNum++,
        newLine: null,
        text: item.text,
      })
      deletions++
    } else if (item.op === 'add') {
      diffLines.push({
        kind: 'add',
        oldLine: null,
        newLine: newLineNum++,
        text: item.text,
      })
      additions++
    }
  }

  // 后缀
  for (const text of suffix) {
    diffLines.push({
      kind: 'context',
      oldLine: oldLineNum++,
      newLine: newLineNum++,
      text,
    })
  }

  const splitRows = buildSplitRows(diffLines)

  return {
    lines: diffLines,
    splitRows,
    additions,
    deletions,
    identical: additions === 0 && deletions === 0,
  }
}
