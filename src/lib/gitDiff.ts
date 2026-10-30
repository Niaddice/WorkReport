export interface GitDiffLine {
  text: string
  kind: 'meta' | 'hunk' | 'add' | 'remove' | 'context'
  oldLine: number | null
  newLine: number | null
}

/** Parse unified hunks; metadata and missing-newline markers never advance line numbers. */
export function parseGitDiff(text: string) {
  let oldLine: number | null = null
  let newLine: number | null = null
  let oldRemaining = 0
  let newRemaining = 0
  let additions = 0
  let deletions = 0
  const source = text.split('\n')
  if (source.at(-1) === '') source.pop()
  const lines: GitDiffLine[] = source.map((text) => {
    const row: GitDiffLine = { text, kind: 'meta', oldLine: null, newLine: null }
    const inHunk = oldRemaining > 0 || newRemaining > 0
    const hunk = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/.exec(text)
    if (hunk) {
      oldLine = Number(hunk[1])
      newLine = Number(hunk[3])
      oldRemaining = hunk[2] === undefined ? 1 : Number(hunk[2])
      newRemaining = hunk[4] === undefined ? 1 : Number(hunk[4])
      row.kind = 'hunk'
    } else if (text.startsWith('diff ') || (!inHunk && (text.startsWith('--- ') || text.startsWith('+++ ')))) {
      oldLine = newLine = null
      oldRemaining = newRemaining = 0
    } else if (text.startsWith('+')) {
      row.kind = 'add'
      row.newLine = newRemaining > 0 ? newLine : null
      if (newLine !== null && newRemaining > 0) {
        newLine++
        newRemaining--
      }
      additions++
    } else if (text.startsWith('-')) {
      row.kind = 'remove'
      row.oldLine = oldRemaining > 0 ? oldLine : null
      if (oldLine !== null && oldRemaining > 0) {
        oldLine++
        oldRemaining--
      }
      deletions++
    } else if (text.startsWith(' ') && inHunk && oldLine !== null && newLine !== null) {
      row.kind = 'context'
      row.oldLine = oldLine++
      row.newLine = newLine++
      oldRemaining--
      newRemaining--
    }
    return row
  })
  return { lines, additions, deletions }
}
