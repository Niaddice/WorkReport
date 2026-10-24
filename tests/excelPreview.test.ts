import { describe, expect, it } from 'vitest'
import {
  buildExcelRegionGrid,
  excelCellContentHeight,
  excelCellCss,
  excelColumnName,
  excelGridCellContainsColumn,
  excelPreviewZoom,
  initialExcelSheet,
  nextExcelSheetIndex,
  type ExcelPreviewCell,
  type ExcelPreviewCellStyle,
  type ExcelPreviewRegion,
  type ExcelSheetPreviewData,
} from '../src/lib/excelPreview'

const style: ExcelPreviewCellStyle = {
  fontFamily: '等线',
  fontSize: 16,
  bold: false,
  italic: false,
  underline: false,
  strike: false,
  color: '#222222',
  background: '#ffffff',
  horizontalAlign: 'left',
  verticalAlign: 'bottom',
  wrapText: false,
  borderTop: null,
  borderRight: null,
  borderBottom: null,
  borderLeft: null,
}
function cell(row: number, column: number, change: Partial<ExcelPreviewCell> = {}): ExcelPreviewCell {
  return {
    row,
    column,
    address: `${excelColumnName(column)}${row}`,
    text: '',
    formula: null,
    style,
    rowSpan: 1,
    columnSpan: 1,
    changed: false,
    ...change,
  }
}
function region(change: Partial<ExcelPreviewRegion> = {}): ExcelPreviewRegion {
  return { startRow: 1, startColumn: 1, rowCount: 4, columnCount: 4, rows: [], columns: [], cells: [], ...change }
}

describe('Excel worksheet grid', () => {
  it('uses spreadsheet column names through the XLSX maximum', () => {
    expect([1, 26, 27, 52, 53, 702, 703, 16_384].map(excelColumnName)).toEqual([
      'A',
      'Z',
      'AA',
      'AZ',
      'BA',
      'ZZ',
      'AAA',
      'XFD',
    ])
    expect([0, -1, 1.5, 16_385, Infinity].map(excelColumnName)).toEqual(['', '', '', '', ''])
  })

  it('keeps template labels, empty positions and formatted values at their original addresses', () => {
    const label = cell(1, 1, { text: '每周工作报告', rowSpan: 1, columnSpan: 4 })
    const amount = cell(3, 4, { text: '¥123,456.78', changed: true })
    const input = region({
      cells: [label, amount],
      columns: [{ index: 1, label: 'A', width: 110, hidden: false }],
      rows: [{ index: 3, height: 53, hidden: false }],
    })
    const snapshot = JSON.stringify(input)
    const grid = buildExcelRegionGrid(input)
    expect(grid.address).toBe('A1:D4')
    expect(grid.width).toBe(44 + 110 + 80 * 3)
    expect(grid.rows[0]?.cells).toHaveLength(1)
    expect(grid.rows[0]?.cells[0]?.columnSpan).toBe(4)
    expect(grid.rows[2]?.height).toBe(53)
    expect(grid.rows[2]?.cells[3]?.source).toBe(amount)
    expect(grid.rows[3]?.cells[1]?.source).toBeUndefined()
    expect(JSON.stringify(input)).toBe(snapshot)
  })

  it('omits hidden axes and promotes a hidden merge anchor without losing its content', () => {
    const title = cell(1, 1, { text: '合并标题', rowSpan: 3, columnSpan: 3 })
    const grid = buildExcelRegionGrid(
      region({
        cells: [title],
        rows: [{ index: 1, height: 24, hidden: true }],
        columns: [
          { index: 1, label: 'A', width: 80, hidden: true },
          { index: 3, label: 'C', width: 80, hidden: true },
        ],
      }),
    )
    expect(grid.columns.map((column) => column.label)).toEqual(['B', 'D'])
    expect(grid.rows.map((row) => row.index)).toEqual([2, 3, 4])
    expect(grid.rows[0]?.cells[0]).toMatchObject({ row: 2, column: 2, rowSpan: 2, columnSpan: 1, source: title })
    expect(grid.rows[1]?.cells.map((candidate) => candidate.column)).toEqual([4])
    expect(grid.rows[2]?.cells.map((candidate) => candidate.column)).toEqual([2, 4])
  })

  it('clips merges to a small distant region without allocating the skipped worksheet', () => {
    const title = cell(999_999, 10_000, { text: '远端批注', rowSpan: 10, columnSpan: 10 })
    const grid = buildExcelRegionGrid(
      region({ startRow: 1_000_000, startColumn: 10_001, rowCount: 2, columnCount: 2, cells: [title] }),
    )
    expect(grid.rows).toHaveLength(2)
    expect(grid.columns).toHaveLength(2)
    expect(grid.rows[0]?.cells[0]).toMatchObject({
      row: 1_000_000,
      column: 10_001,
      rowSpan: 2,
      columnSpan: 2,
      source: title,
    })
    expect(grid.rows[1]?.cells).toEqual([])
  })

  it('bounds malformed or enormous windows while retaining absolute row and column identities', () => {
    const grid = buildExcelRegionGrid(
      region({ startRow: 999_000, startColumn: 12_000, rowCount: 1_048_576, columnCount: 16_384 }),
    )
    expect(grid.rows).toHaveLength(240)
    expect(grid.columns).toHaveLength(80)
    expect(grid.rows[0]?.index).toBe(999_000)
    expect(grid.columns[0]?.index).toBe(12_000)
    expect(grid.truncated).toBe(true)
    const invalid = buildExcelRegionGrid(
      region({ startRow: NaN, startColumn: Infinity, rowCount: Infinity, columnCount: NaN }),
    )
    expect(invalid.rows).toEqual([])
    expect(invalid.columns).toEqual([])
  })

  it('lets vertical navigation find a merged cell across hidden columns', () => {
    const grid = buildExcelRegionGrid(
      region({
        startRow: 3,
        startColumn: 7,
        rowCount: 2,
        columnCount: 5,
        columns: [{ index: 9, label: 'I', width: 80, hidden: true }],
        cells: [cell(3, 7, { text: 'G:K 合并，I 隐藏', columnSpan: 5 })],
      }),
    )
    const merged = grid.rows[0]!.cells[0]!
    expect(merged.columnSpan).toBe(4)
    expect(excelGridCellContainsColumn(grid, merged, 11)).toBe(true)
    expect(excelGridCellContainsColumn(grid, merged, 9)).toBe(false)
    expect(excelGridCellContainsColumn(grid, merged, 12)).toBe(false)
  })

  it('keeps long wrapped content within fixed row heights and sums only visible merged rows', () => {
    const longText = '很长的换行内容\n'.repeat(80)
    const grid = buildExcelRegionGrid(
      region({
        rowCount: 3,
        columnCount: 2,
        rows: [
          { index: 1, height: 20, hidden: false },
          { index: 2, height: 100, hidden: true },
          { index: 3, height: 36, hidden: false },
        ],
        cells: [
          cell(1, 1, { text: longText, style: { ...style, wrapText: true } }),
          cell(1, 2, { text: longText, rowSpan: 3 }),
        ],
      }),
    )
    const ordinary = grid.rows[0]!.cells[0]!
    const merged = grid.rows[0]!.cells[1]!
    expect(grid.rows.map((row) => row.height)).toEqual([20, 36])
    expect(ordinary.height).toBe(20)
    expect(excelCellContentHeight(ordinary)).toBe(16)
    expect(merged.height).toBe(56)
    expect(excelCellContentHeight(merged)).toBe(52)
    expect(ordinary.source?.text).toBe(longText)
  })
})

describe('Excel worksheet presentation', () => {
  it('preserves source typography, fills, alignment, wrapping and individual borders', () => {
    const border = { style: 'double' as const, width: 3, color: '#123456' }
    expect(
      excelCellCss({
        ...style,
        bold: true,
        italic: true,
        underline: true,
        strike: true,
        wrapText: true,
        background: '#ddeeff',
        horizontalAlign: 'center',
        verticalAlign: 'middle',
        borderBottom: border,
      }),
    ).toMatchObject({
      fontFamily: '等线',
      fontSize: '16px',
      fontWeight: 700,
      fontStyle: 'italic',
      textDecoration: 'underline line-through',
      backgroundColor: '#ddeeff',
      textAlign: 'center',
      verticalAlign: 'middle',
      whiteSpace: 'pre-wrap',
      overflowWrap: 'anywhere',
      borderBottom: '3px double #123456',
    })
    expect(excelCellCss(style).whiteSpace).toBe('pre')
    expect(excelCellCss(undefined)).toEqual({})
  })

  it('does not turn unsupported color data or unreasonable sizes into CSS', () => {
    const result = excelCellCss({
      ...style,
      fontSize: Infinity,
      color: 'url(https://example.com/pixel)',
      background: 'var(--custom)',
      borderTop: { style: 'solid', width: NaN, color: 'invalid' },
    })
    expect(result.fontSize).toBe('14px')
    expect(result.color).toBe('#222222')
    expect(result.backgroundColor).toBe('#ffffff')
    expect(result.borderTop).toBe('1px solid #000000')
  })

  it('fits normal worksheets, preserves manual zoom and safely handles a hidden modal', () => {
    expect(excelPreviewZoom('fit', 832, 1000)).toBe(0.8)
    expect(excelPreviewZoom('fit', 1400, 1000)).toBe(1)
    expect(excelPreviewZoom('fit', 200, 5000)).toBe(0.4)
    expect(excelPreviewZoom('fit', 0, 1000)).toBe(1)
    expect(excelPreviewZoom(1.2, 500, 1000)).toBe(1.2)
    expect(excelPreviewZoom(20, 500, 1000)).toBe(2)
  })

  it('selects a visible worksheet by default and keeps hidden sheets available to review', () => {
    const sheets: ExcelSheetPreviewData[] = [
      { name: 'Hidden', state: 'hidden', regions: [], truncated: false },
      { name: '周报', state: 'visible', regions: [], truncated: false },
    ]
    expect(initialExcelSheet(sheets)).toBe('周报')
    expect(initialExcelSheet(sheets.slice(0, 1))).toBe('Hidden')
    expect(initialExcelSheet([])).toBe('')
    expect(nextExcelSheetIndex(1, 2, 'ArrowRight')).toBe(0)
    expect(nextExcelSheetIndex(0, 2, 'ArrowLeft')).toBe(1)
    expect(nextExcelSheetIndex(1, 2, 'Home')).toBe(0)
    expect(nextExcelSheetIndex(0, 2, 'End')).toBe(1)
    expect(nextExcelSheetIndex(0, 0, 'End')).toBeNull()
    expect(nextExcelSheetIndex(0, 2, 'Tab')).toBeNull()
  })
})
