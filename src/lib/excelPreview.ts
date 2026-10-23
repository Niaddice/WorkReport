import type { CSSProperties } from 'vue'

export interface ExcelPreviewBorder { style: 'solid' | 'dashed' | 'dotted' | 'double'; width: number; color: string }
export interface ExcelPreviewCellStyle {
  fontFamily: string
  fontSize: number
  bold: boolean
  italic: boolean
  underline: boolean
  strike: boolean
  color: string
  background: string
  horizontalAlign: 'left' | 'center' | 'right' | 'justify'
  verticalAlign: 'top' | 'middle' | 'bottom'
  wrapText: boolean
  borderTop: ExcelPreviewBorder | null
  borderRight: ExcelPreviewBorder | null
  borderBottom: ExcelPreviewBorder | null
  borderLeft: ExcelPreviewBorder | null
}
export interface ExcelPreviewCell {
  row: number
  column: number
  address: string
  text: string
  formula: string | null
  style: ExcelPreviewCellStyle
  rowSpan: number
  columnSpan: number
  changed: boolean
}
export interface ExcelPreviewColumn { index: number; label: string; width: number; hidden: boolean }
export interface ExcelPreviewRow { index: number; height: number; hidden: boolean }
export interface ExcelPreviewRegion {
  startRow: number
  startColumn: number
  rowCount: number
  columnCount: number
  rows: ExcelPreviewRow[]
  columns: ExcelPreviewColumn[]
  cells: ExcelPreviewCell[]
}
export interface ExcelSheetPreviewData {
  name: string
  state: 'visible' | 'hidden' | 'veryHidden'
  regions: ExcelPreviewRegion[]
  truncated: boolean
}

export interface ExcelGridCell {
  row: number
  column: number
  rowSpan: number
  columnSpan: number
  height: number
  source: ExcelPreviewCell | undefined
}
export interface ExcelGridRow { index: number; height: number; cells: ExcelGridCell[] }
export interface ExcelRegionGrid {
  columns: ExcelPreviewColumn[]
  rows: ExcelGridRow[]
  width: number
  address: string
  truncated: boolean
}

const MAX_ROWS = 240
const MAX_COLUMNS = 80
const ROW_HEADER_WIDTH = 44

function finite(value: number, fallback: number, minimum: number, maximum: number): number {
  return Number.isFinite(value) ? Math.min(maximum, Math.max(minimum, value)) : fallback
}

export function excelColumnName(index: number): string {
  if (!Number.isSafeInteger(index) || index < 1 || index > 16_384) return ''
  let name = ''
  while (index > 0) { index--; name = String.fromCharCode(65 + index % 26) + name; index = Math.floor(index / 26) }
  return name
}

function cellAddress(row: number, column: number): string { return `${excelColumnName(column)}${row}` }

/** Build only a bounded window. Absolute indices preserve far-away sparse cells. */
export function buildExcelRegionGrid(region: ExcelPreviewRegion): ExcelRegionGrid {
  const startRow = Math.floor(finite(region.startRow, 1, 1, 1_048_576))
  const startColumn = Math.floor(finite(region.startColumn, 1, 1, 16_384))
  const rowCount = Math.floor(finite(region.rowCount, 0, 0, Math.min(MAX_ROWS, 1_048_577 - startRow)))
  const columnCount = Math.floor(finite(region.columnCount, 0, 0, Math.min(MAX_COLUMNS, 16_385 - startColumn)))
  const rowSettings = new Map(region.rows.map(row => [row.index, row]))
  const columnSettings = new Map(region.columns.map(column => [column.index, column]))
  const columns: ExcelPreviewColumn[] = []
  const rows: ExcelGridRow[] = []
  for (let offset = 0; offset < columnCount; offset++) {
    const index = startColumn + offset; const column = columnSettings.get(index)
    if (!column?.hidden) columns.push({ index, label: excelColumnName(index), width: finite(column?.width ?? 80, 80, 12, 960), hidden: false })
  }
  for (let offset = 0; offset < rowCount; offset++) {
    const index = startRow + offset; const row = rowSettings.get(index)
    if (!row?.hidden) rows.push({ index, height: finite(row?.height ?? 24, 24, 8, 960), cells: [] })
  }

  const sources = new Map<string, ExcelPreviewCell>()
  const covered = new Set<string>()
  const merged = new Map<string, ExcelGridCell>()
  for (const source of region.cells) {
    if (!Number.isInteger(source.row) || !Number.isInteger(source.column)) continue
    sources.set(`${source.row}:${source.column}`, source)
    const rowSpan = Math.floor(finite(source.rowSpan, 1, 1, 1_048_576))
    const columnSpan = Math.floor(finite(source.columnSpan, 1, 1, 16_384))
    if (rowSpan === 1 && columnSpan === 1) continue
    const visibleRows = rows.filter(row => row.index >= source.row && row.index < source.row + rowSpan)
    const visibleColumns = columns.filter(column => column.index >= source.column && column.index < source.column + columnSpan)
    const firstRow = visibleRows[0]; const firstColumn = visibleColumns[0]
    if (!firstRow || !firstColumn) continue
    const key = `${firstRow.index}:${firstColumn.index}`
    merged.set(key, { row: firstRow.index, column: firstColumn.index, rowSpan: visibleRows.length, columnSpan: visibleColumns.length, height: visibleRows.reduce((height, row) => height + row.height, 0), source })
    for (const row of visibleRows) for (const column of visibleColumns) {
      const coveredKey = `${row.index}:${column.index}`
      if (coveredKey !== key) covered.add(coveredKey)
    }
  }
  for (const row of rows) for (const column of columns) {
    const key = `${row.index}:${column.index}`
    if (covered.has(key)) continue
    row.cells.push(merged.get(key) ?? { row: row.index, column: column.index, rowSpan: 1, columnSpan: 1, height: row.height, source: sources.get(key) })
  }
  return {
    columns, rows,
    width: ROW_HEADER_WIDTH + columns.reduce((width, column) => width + column.width, 0),
    address: `${cellAddress(startRow, startColumn)}:${cellAddress(startRow + Math.max(0, rowCount - 1), startColumn + Math.max(0, columnCount - 1))}`,
    truncated: region.rowCount > rowCount || region.columnCount > columnCount,
  }
}

function color(value: string, fallback: string): string {
  return /^(?:#[\da-f]{3,8}|transparent|black|white)$/i.test(value) ? value : fallback
}
function border(value: ExcelPreviewBorder | null): string | undefined {
  if (!value) return undefined
  const lineStyle = ['solid', 'dashed', 'dotted', 'double'].includes(value.style) ? value.style : 'solid'
  return `${finite(value.width, 1, 0.5, 8)}px ${lineStyle} ${color(value.color, '#000000')}`
}

/** Workbook values remain text; only structured formatting becomes CSS. */
export function excelCellCss(style: ExcelPreviewCellStyle | undefined): CSSProperties {
  if (!style) return {}
  return {
    fontFamily: style.fontFamily || 'Calibri, Arial, sans-serif',
    fontSize: `${finite(style.fontSize, 14, 6, 120)}px`,
    fontWeight: style.bold ? 700 : 400,
    fontStyle: style.italic ? 'italic' : 'normal',
    textDecoration: [style.underline ? 'underline' : '', style.strike ? 'line-through' : ''].filter(Boolean).join(' ') || 'none',
    color: color(style.color, '#222222'),
    backgroundColor: color(style.background, '#ffffff'),
    textAlign: ['left', 'center', 'right', 'justify'].includes(style.horizontalAlign) ? style.horizontalAlign : 'left',
    verticalAlign: ['top', 'middle', 'bottom'].includes(style.verticalAlign) ? style.verticalAlign : 'bottom',
    whiteSpace: style.wrapText ? 'pre-wrap' : 'pre',
    overflowWrap: style.wrapText ? 'anywhere' : 'normal',
    borderTop: border(style.borderTop), borderRight: border(style.borderRight),
    borderBottom: border(style.borderBottom), borderLeft: border(style.borderLeft),
  }
}

export function excelPreviewZoom(mode: 'fit' | number, viewportWidth: number, gridWidth: number): number {
  if (mode !== 'fit') return finite(mode, 1, 0.4, 2)
  if (!(viewportWidth > 0) || !(gridWidth > 0)) return 1
  return Math.max(0.4, Math.min(1, (viewportWidth - 32) / gridWidth))
}

export function excelGridCellContainsColumn(grid: ExcelRegionGrid, cell: ExcelGridCell, column: number): boolean {
  const start = grid.columns.findIndex(candidate => candidate.index === cell.column)
  const target = grid.columns.findIndex(candidate => candidate.index === column)
  return start >= 0 && target >= start && target < start + cell.columnSpan
}

export function excelCellContentHeight(cell: ExcelGridCell): number {
  const top = finite(cell.source?.style.borderTop?.width ?? 1, 1, 0.5, 8)
  const bottom = finite(cell.source?.style.borderBottom?.width ?? 1, 1, 0.5, 8)
  // The source row height includes borders and the worksheet cell's 1px padding.
  return Math.max(0, cell.height - top - bottom - 2)
}

export function initialExcelSheet(sheets: ExcelSheetPreviewData[]): string {
  return (sheets.find(sheet => sheet.state === 'visible') ?? sheets[0])?.name ?? ''
}

export function nextExcelSheetIndex(current: number, count: number, key: string): number | null {
  if (!count) return null
  if (key === 'Home') return 0
  if (key === 'End') return count - 1
  if (key === 'ArrowLeft') return (current - 1 + count) % count
  if (key === 'ArrowRight') return (current + 1) % count
  return null
}
