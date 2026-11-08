import { describe, expect, it } from 'vitest'
import { parseGitDiff } from '../src/lib/gitDiff'

describe('unified Git diff display', () => {
  it('numbers old and new hunks independently and excludes file metadata from statistics', () => {
    const result = parseGitDiff(
      'diff --git a/old b/new\n--- a/old\n+++ b/new\n@@ -7,3 +9,3 @@\n same\n-old\n+new\n tail\n\\ No newline at end of file\n@@ -20,0 +23,1 @@\n+added\n',
    )
    expect(result.additions).toBe(2)
    expect(result.deletions).toBe(1)
    expect(result.lines.slice(4, 8).map((row) => [row.oldLine, row.newLine])).toEqual([
      [7, 9],
      [8, null],
      [null, 10],
      [9, 11],
    ])
    expect(result.lines.at(-1)).toMatchObject({ oldLine: null, newLine: 23, kind: 'add' })
    expect(result.lines[8]).toMatchObject({ oldLine: null, newLine: null, kind: 'meta' })
  })
  it('keeps header-like source code inside hunks and resets numbering for the next file', () => {
    const result = parseGitDiff(
      '@@ -1 +1 @@\n--- decrement\n+++ increment\ndiff --git a/b b/b\n--- a/b\n+++ b/b\n@@ -100 +200 @@\n-old\n+new',
    )
    expect(result.lines[1]).toMatchObject({ kind: 'remove', oldLine: 1 })
    expect(result.lines[2]).toMatchObject({ kind: 'add', newLine: 1 })
    expect(result.lines.at(-1)).toMatchObject({ kind: 'add', newLine: 200 })
    expect(result.additions).toBe(2)
    expect(result.deletions).toBe(2)
  })
  it('extracts valid hunk patches with file headers attached for staging', () => {
    const raw = [
      'diff --git a/test.txt b/test.txt',
      'index 123456..789abc 100644',
      '--- a/test.txt',
      '+++ b/test.txt',
      '@@ -1,3 +1,3 @@',
      ' line 1',
      '-line 2',
      '+line 2 modified',
      ' line 3',
      '@@ -10,3 +10,3 @@',
      ' line 10',
      '-line 11',
      '+line 11 modified',
      ' line 12',
    ].join('\n')

    const result = parseGitDiff(raw)
    const hunkLines = result.lines.filter((l) => l.kind === 'hunk')
    expect(hunkLines).toHaveLength(2)

    expect(hunkLines[0].patch).toBe(
      [
        'diff --git a/test.txt b/test.txt',
        'index 123456..789abc 100644',
        '--- a/test.txt',
        '+++ b/test.txt',
        '@@ -1,3 +1,3 @@',
        ' line 1',
        '-line 2',
        '+line 2 modified',
        ' line 3',
        '',
      ].join('\n'),
    )

    expect(hunkLines[1].patch).toBe(
      [
        'diff --git a/test.txt b/test.txt',
        'index 123456..789abc 100644',
        '--- a/test.txt',
        '+++ b/test.txt',
        '@@ -10,3 +10,3 @@',
        ' line 10',
        '-line 11',
        '+line 11 modified',
        ' line 12',
        '',
      ].join('\n'),
    )
  })

  it('does not invent line numbers for binary diffs or fragments without a hunk header', () => {
    expect(parseGitDiff('Binary files a/a and b/a differ').lines[0]).toMatchObject({
      kind: 'meta',
      oldLine: null,
      newLine: null,
    })
    expect(parseGitDiff('+fragment').lines[0]).toMatchObject({ kind: 'add', oldLine: null, newLine: null })
    expect(parseGitDiff('').lines).toEqual([])
  })
})
