import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { invoke, isTauri } from '@tauri-apps/api/core'
import {
  DEFAULT_EXCEL_SETTINGS,
  EXCEL_STORAGE_KEY,
  reportDateRange,
  reportOverlapsRange,
  validExcelDates,
  useExcel,
  type ExcelExportFile,
  type ExcelExportResult,
  type ExcelPreview,
  type ExcelSettings,
} from '../src/lib/useExcel'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }))

const configured: ExcelSettings = {
  enabled: true,
  templatePath: '/templates/week.xlsx',
  prompt: '将本周工作填入周报 B5，保留公式。',
}
const exported: ExcelExportResult = { path: '/reports/week.xlsx', filename: 'week.xlsx', cells: 3 }
const prepared: ExcelPreview = {
  id: 'preview-week',
  templateName: 'week.xlsx',
  outputDirectory: '/reports',
  start: '20261019',
  end: '20261025',
  cells: [{ sheet: 'Sheet1', cell: 'B5', value: '本周工作', previousValue: '=SUM(B1:B4)', replacesFormula: true }],
  styles: [{ sheet: 'Sheet1', range: 'A14:F20', border: 'black', wrapText: true, autoRowHeight: true }],
  sheets: [],
}
function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (cause: unknown) => void
  const promise = new Promise<T>((done, fail) => {
    resolve = done
    reject = fail
  })
  return { promise, resolve, reject }
}

beforeEach(() => {
  vi.resetAllMocks()
  vi.mocked(isTauri).mockReturnValue(true)
})
afterEach(() => {
  vi.unstubAllGlobals()
})

describe('Excel selection dates', () => {
  it('accepts any dates from Monday through Sunday in the same week', () => {
    expect(validExcelDates(['20261019', '20261025'])).toBe(true)
    expect(validExcelDates(['20261025', '20261021', '20261021'])).toBe(true)
    expect(validExcelDates(['20261021'])).toBe(true)
  })
  it('accepts weeks that cross a month or year boundary', () => {
    expect(validExcelDates(['20260928', '20261004'])).toBe(true)
    expect(validExcelDates(['20261228', '20270103'])).toBe(true)
  })
  it('accepts ranges that span weeks', () => {
    expect(validExcelDates(['20261018', '20261019'])).toBe(true)
    expect(validExcelDates(['20261025', '20261026'])).toBe(true)
  })
  it('strictly validates actual calendar dates without local timezone conversion', () => {
    for (const dates of [
      [],
      [''],
      ['2026-10-21'],
      ['20260229'],
      ['20261301'],
      ['20260001'],
      ['20261000'],
      ['00001021'],
      ['202610211'],
      ['20260431'],
    ])
      expect(validExcelDates(dates)).toBe(false)
    expect(validExcelDates(['20240229'])).toBe(true)
  })
})

describe('report date range parsing', () => {
  it('parses the standard weekly filename range', () => {
    expect(reportDateRange('周报_20261019-20261025_abc123.xlsx')).toEqual({ start: '20261019', end: '20261025' })
    expect(reportDateRange('周报_20260928-20261004_xyz.xlsx')).toEqual({ start: '20260928', end: '20261004' })
  })
  it('parses general report names and keeps legacy weekly names compatible', () => {
    expect(reportDateRange('总结_20261001-20261115_20261028_160500.xlsx')).toEqual({
      start: '20261001',
      end: '20261115',
    })
    expect(reportDateRange('总结_20261001-20261115_20261028_160500_1.xlsx')).toEqual({
      start: '20261001',
      end: '20261115',
    })
    expect(reportDateRange('报告_20261001-20261115_new.xlsx')).toEqual({ start: '20261001', end: '20261115' })
    expect(reportDateRange('Excel报告_20261201-20270131_new.xlsx')).toEqual({ start: '20261201', end: '20270131' })
  })
  it('does not treat invalid or reversed date ranges as recognised metadata', () => {
    expect(reportDateRange('报告_20260229-20260301_bad.xlsx')).toBeNull()
    expect(reportDateRange('报告_20261101-20261001_bad.xlsx')).toBeNull()
  })
  it('returns null for unrecognised filenames', () => {
    expect(reportDateRange('week.xlsx')).toBeNull()
    expect(reportDateRange('周报_20261019.xlsx')).toBeNull()
    expect(reportDateRange('20261019-20261025.xlsx')).toBeNull()
    expect(reportDateRange('')).toBeNull()
  })
})

describe('report range overlap', () => {
  const report = '周报_20261026-20261101_abc.xlsx'
  it('matches when the report overlaps the selected range', () => {
    // 单日、范围边界与任意交集均匹配。
    expect(reportOverlapsRange(report, '20261026', '20261101')).toBe(true)
    expect(reportOverlapsRange(report, '20261028', '20261028')).toBe(true)
    expect(reportOverlapsRange(report, '20261028', '20261030')).toBe(true)
    expect(reportOverlapsRange(report, '20261001', '20261026')).toBe(true)
    expect(reportOverlapsRange(report, '20261101', '20261130')).toBe(true)
    expect(reportOverlapsRange(report, '20261001', '20261231')).toBe(true)
  })
  it('keeps unrecognised files visible regardless of the range', () => {
    expect(reportOverlapsRange('week.xlsx', '20261026', '20261101')).toBe(true)
  })
  it('hides reports from a different week', () => {
    expect(reportOverlapsRange('周报_20261019-20261025_old.xlsx', '20261026', '20261101')).toBe(false)
    expect(reportOverlapsRange(report, '20261102', '20261108')).toBe(false)
  })
})

describe('Excel export list refresh', () => {
  const file = (name: string): ExcelExportFile => ({
    filename: name,
    path: `/workspace/excel/${name}`,
    modified: 0,
    size: 1,
  })

  it('keeps the latest list when an earlier directory read finishes later', async () => {
    const earlier = deferred<ExcelExportFile[]>()
    const latest = deferred<ExcelExportFile[]>()
    vi.mocked(invoke).mockReturnValueOnce(earlier.promise).mockReturnValueOnce(latest.promise)
    const excel = useExcel()
    const previousRead = excel.loadExports('/workspace')
    const currentRead = excel.loadExports('/workspace')
    latest.resolve([file('new.xlsx')])
    expect(await currentRead).toBe(true)
    earlier.resolve([])
    expect(await previousRead).toBe(false)
    expect(excel.exports.value).toEqual([file('new.xlsx')])
  })

  it('ignores a stale error while the latest workspace read is pending', async () => {
    const earlier = deferred<ExcelExportFile[]>()
    const latest = deferred<ExcelExportFile[]>()
    vi.mocked(invoke).mockReturnValueOnce(earlier.promise).mockReturnValueOnce(latest.promise)
    const excel = useExcel()
    excel.exports.value = [file('old-workspace.xlsx')]
    const previousRead = excel.loadExports('/old-workspace')
    const currentRead = excel.loadExports('/new-workspace')
    expect(excel.exports.value).toEqual([])
    earlier.reject('旧目录已关闭')
    expect(await previousRead).toBe(false)
    expect(excel.exportsError.value).toBe('')
    expect(excel.exportsLoading.value).toBe(true)
    latest.resolve([file('new-workspace.xlsx')])
    expect(await currentRead).toBe(true)
    expect(excel.exportsLoading.value).toBe(false)
  })

  it('preserves exported files on selection resets and invalidates pending reads on a full reset', async () => {
    const excel = useExcel()
    excel.exports.value = [file('existing.xlsx')]
    excel.clearSelection()
    expect(excel.exports.value).toEqual([file('existing.xlsx')])
    excel.acceptSaved(configured)
    expect(excel.exports.value).toEqual([file('existing.xlsx')])
    const pending = deferred<ExcelExportFile[]>()
    vi.mocked(invoke).mockReturnValueOnce(pending.promise)
    const read = excel.loadExports('/workspace')
    excel.clearExport()
    pending.resolve([file('late.xlsx')])
    expect(await read).toBe(false)
    expect(excel.exports.value).toEqual([])
    expect(excel.exportsLoading.value).toBe(false)
  })
})

describe('Excel settings state', () => {
  it('ignores the custom output directory from legacy settings', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ ...configured, outputDirectory: '/old-custom-reports' })
    const excel = useExcel()
    expect(await excel.load()).toBe(true)
    expect(excel.settings.value).toEqual(configured)
    expect(excel.settings.value).not.toHaveProperty('outputDirectory')
  })
  it('loads settings and does not write disabled defaults', async () => {
    const excel = useExcel()
    expect(excel.settings.value).toEqual(DEFAULT_EXCEL_SETTINGS)
    expect(invoke).not.toHaveBeenCalled()
    vi.mocked(invoke).mockResolvedValueOnce(configured)
    expect(await excel.load()).toBe(true)
    expect(excel.settings.value).toEqual(configured)
    expect(invoke).toHaveBeenCalledExactlyOnceWith('load_excel_settings')
  })
  it('exposes a read failure without replacing current settings', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(configured).mockRejectedValueOnce('cannot read')
    const excel = useExcel()
    await excel.load()
    expect(await excel.load()).toBe(false)
    expect(excel.error.value).toContain('cannot read')
    expect(excel.settings.value).toEqual(configured)
    expect(excel.loading.value).toBe(false)
  })
  it('starts an independent load for a new workspace and ignores an old slow read', async () => {
    const first = deferred<ExcelSettings>()
    const second = deferred<ExcelSettings>()
    vi.mocked(invoke).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)
    const excel = useExcel()
    const oldLoad = excel.load()
    const newLoad = excel.reload()
    expect(invoke).toHaveBeenCalledTimes(2)
    expect(excel.settings.value.enabled).toBe(false)
    first.resolve(configured)
    expect(await oldLoad).toBe(false)
    expect(excel.loading.value).toBe(true)
    expect(excel.settings.value.enabled).toBe(false)
    second.resolve({ ...configured, prompt: '新工作空间' })
    expect(await newLoad).toBe(true)
    expect(excel.settings.value.prompt).toBe('新工作空间')
    expect(excel.loading.value).toBe(false)
  })
  it('keeps a new workspace disabled when its settings cannot be read', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(configured).mockRejectedValueOnce('unreadable config')
    const excel = useExcel()
    await excel.load()
    expect(await excel.reload()).toBe(false)
    expect(excel.settings.value).toEqual(DEFAULT_EXCEL_SETTINGS)
    expect(excel.error.value).toContain('unreadable config')
  })
})

describe('Excel preview and confirmation lifecycle', () => {
  const settle = async () => {
    for (let index = 0; index < 8; index++) await Promise.resolve()
  }

  function mockConfiguredCommands() {
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === 'load_excel_settings') return Promise.resolve(configured)
      if (command === 'prepare_excel') return Promise.resolve(prepared)
      if (command === 'confirm_excel') return Promise.resolve(exported)
      return Promise.resolve()
    })
  }

  it('prepares the sorted unique selection without writing an Excel file or requiring a summary', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    expect(await excel.prepareSelection('/workspace', ['20261025', '20261019', '20261025'])).toBe(true)
    expect(invoke).toHaveBeenNthCalledWith(1, 'load_excel_settings')
    expect(invoke).toHaveBeenLastCalledWith('prepare_excel', {
      workspacePath: '/workspace',
      dates: ['20261019', '20261025'],
      requestId: expect.any(String),
    })
    expect(excel.preview.value).toEqual(prepared)
    expect(excel.result.value).toBeNull()
    expect(excel.preparing.value).toBe(false)
    expect(excel.exporting.value).toBe(false)
    expect(vi.mocked(invoke).mock.calls.some((call) => call[0] === 'confirm_excel' || call[0] === 'export_excel')).toBe(
      false,
    )
  })

  it('uses a temporary prompt only for the current request and preserves saved settings', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    expect(await excel.prepareSelection('/workspace', ['20261021'], '仅本次按项目整理')).toBe(true)
    expect(invoke).toHaveBeenLastCalledWith('prepare_excel', {
      workspacePath: '/workspace',
      dates: ['20261021'],
      requestId: expect.any(String),
      prompt: '仅本次按项目整理',
    })
    expect(excel.settings.value).toEqual(configured)
    expect(excel.previewPrompt.value).toBe('仅本次按项目整理')
    await excel.discardPreview()
    expect(excel.previewPrompt.value).toBe('')
    expect(await excel.prepareSelection('/workspace', ['20261021'])).toBe(true)
    expect(invoke).toHaveBeenLastCalledWith('prepare_excel', {
      workspacePath: '/workspace',
      dates: ['20261021'],
      requestId: expect.any(String),
    })
    expect(vi.mocked(invoke).mock.calls.some(([command]) => command === 'save_workspace_settings')).toBe(false)
  })

  it.each(['  ', '字'.repeat(6001)])('rejects an invalid temporary prompt before requesting AI', async (prompt) => {
    const excel = useExcel()
    expect(await excel.prepareSelection('/workspace', ['20261021'], prompt)).toBe(false)
    expect(excel.exportError.value).toContain('本次提示词')
    expect(invoke).not.toHaveBeenCalled()
  })

  it('prepares a range of more than seven days across weeks and years', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    const dates = [
      '20261225',
      '20261226',
      '20261227',
      '20261228',
      '20261229',
      '20261230',
      '20261231',
      '20270101',
      '20270102',
    ]
    expect(await excel.prepareSelection('/workspace', dates)).toBe(true)
    expect(invoke).toHaveBeenLastCalledWith('prepare_excel', {
      workspacePath: '/workspace',
      dates,
      requestId: expect.any(String),
    })
  })

  it('writes only after confirmation using the same preview without requesting AI again', async () => {
    mockConfiguredCommands()
    const write = deferred<ExcelExportResult>()
    const excel = useExcel()
    await excel.prepareSelection('/workspace', ['20261021'])
    vi.mocked(invoke).mockReturnValueOnce(write.promise)
    const confirming = excel.confirmPreview('/workspace')
    expect(excel.preparing.value).toBe(false)
    expect(excel.exporting.value).toBe(true)
    expect(excel.result.value).toBeNull()
    expect(invoke).toHaveBeenLastCalledWith('confirm_excel', { workspacePath: '/workspace', previewId: prepared.id })
    write.resolve(exported)
    expect(await confirming).toBe(true)
    expect(excel.result.value).toEqual(exported)
    expect(excel.preview.value).toBeNull()
    expect(excel.exporting.value).toBe(false)
    expect(vi.mocked(invoke).mock.calls.map((call) => call[0])).toEqual([
      'load_excel_settings',
      'prepare_excel',
      'confirm_excel',
    ])
  })

  it('keeps the original selection while loading settings', async () => {
    const read = deferred<ExcelSettings>()
    vi.mocked(invoke).mockReturnValueOnce(read.promise).mockResolvedValueOnce(prepared)
    const excel = useExcel()
    const dates = ['20261021', '20261022']
    const pending = excel.prepareSelection('/workspace', dates)
    dates.splice(0, dates.length, '20261026')
    read.resolve(configured)
    expect(await pending).toBe(true)
    expect(invoke).toHaveBeenLastCalledWith('prepare_excel', {
      workspacePath: '/workspace',
      dates: ['20261021', '20261022'],
      requestId: expect.any(String),
    })
  })

  it('rejects a missing workspace before invoking native commands', async () => {
    const excel = useExcel()
    expect(await excel.prepareSelection('', ['20261021'])).toBe(false)
    expect(excel.exportError.value).toContain('工作空间')
    expect(invoke).not.toHaveBeenCalled()
  })

  it('checks all selected dates, including dates without a journal, before reading config', async () => {
    const excel = useExcel()
    expect(await excel.prepareSelection('/workspace', ['20261025', '20260229'])).toBe(false)
    expect(excel.exportError.value).toContain('有效的日期')
    expect(invoke).not.toHaveBeenCalled()
  })

  it('checks the latest enabled setting rather than an earlier in-memory value', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(configured).mockResolvedValueOnce(DEFAULT_EXCEL_SETTINGS)
    const excel = useExcel()
    await excel.load()
    expect(await excel.prepareSelection('/workspace', ['20261021'])).toBe(false)
    expect(excel.exportError.value).toContain('启用')
    expect(excel.settings.value.enabled).toBe(false)
    expect(vi.mocked(invoke).mock.calls.map((call) => call[0])).toEqual(['load_excel_settings', 'load_excel_settings'])
  })

  it('does not prepare old enabled config when the latest settings read fails', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(configured).mockRejectedValueOnce('cannot read config')
    const excel = useExcel()
    await excel.load()
    expect(await excel.prepareSelection('/workspace', ['20261021'])).toBe(false)
    expect(excel.exportError.value).toContain('cannot read config')
    expect(excel.preparing.value).toBe(false)
    expect(vi.mocked(invoke).mock.calls.some((call) => call[0] === 'prepare_excel')).toBe(false)
  })

  it('blocks concurrent preparation and awaits cancellation, discarding a late native preview', async () => {
    const pendingPreview = deferred<ExcelPreview>()
    const canceled = deferred<void>()
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === 'load_excel_settings') return Promise.resolve(configured)
      if (command === 'prepare_excel') return pendingPreview.promise
      if (command === 'cancel_ai') return canceled.promise
      return Promise.resolve()
    })
    const excel = useExcel()
    const pending = excel.prepareSelection('/workspace', ['20261021'])
    expect(await excel.prepareSelection('/workspace', ['20261022'])).toBe(false)
    await settle()
    const args = vi.mocked(invoke).mock.calls.find((call) => call[0] === 'prepare_excel')?.[1] as { requestId: string }
    const cancel = excel.cancel()
    expect(excel.preparing.value).toBe(false)
    expect(invoke).toHaveBeenLastCalledWith('cancel_ai', { requestId: args.requestId })
    let cancellationDone = false
    void cancel.then(() => {
      cancellationDone = true
    })
    await Promise.resolve()
    expect(cancellationDone).toBe(false)
    canceled.resolve()
    await cancel
    pendingPreview.resolve(prepared)
    expect(await pending).toBe(false)
    expect(invoke).toHaveBeenLastCalledWith('discard_excel_preview', { previewId: prepared.id })
    expect(excel.preview.value).toBeNull()
    expect(excel.result.value).toBeNull()
    expect(excel.exportError.value).toBe('')
  })

  it('stops preparation if the dialog closes during settings loading', async () => {
    const read = deferred<ExcelSettings>()
    vi.mocked(invoke).mockImplementation((command) =>
      command === 'load_excel_settings' ? read.promise : Promise.resolve(),
    )
    const excel = useExcel()
    const pending = excel.prepareSelection('/workspace', ['20261021'])
    expect(excel.preparing.value).toBe(true)
    expect(invoke).toHaveBeenCalledExactlyOnceWith('load_excel_settings')
    excel.clearExport()
    read.resolve(configured)
    expect(await pending).toBe(false)
    expect(vi.mocked(invoke).mock.calls.map((call) => call[0])).toEqual(['load_excel_settings', 'cancel_ai'])
    expect(excel.exportError.value).toBe('')
  })

  it('does not cancel running preparation when settings are read again', async () => {
    const native = deferred<ExcelPreview>()
    vi.mocked(invoke).mockImplementation((command) =>
      command === 'prepare_excel' ? native.promise : Promise.resolve(configured),
    )
    const excel = useExcel()
    const pending = excel.prepareSelection('/workspace', ['20261021'])
    await settle()
    expect(await excel.load()).toBe(true)
    expect(excel.preparing.value).toBe(true)
    expect(vi.mocked(invoke).mock.calls.some((call) => call[0] === 'cancel_ai')).toBe(false)
    native.resolve(prepared)
    expect(await pending).toBe(true)
  })

  it('invalidates preparation when another workspace starts loading', async () => {
    const oldRead = deferred<ExcelSettings>()
    let reads = 0
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === 'load_excel_settings')
        return ++reads === 1 ? oldRead.promise : Promise.resolve({ ...configured, prompt: '新工作空间' })
      return Promise.resolve()
    })
    const excel = useExcel()
    const pending = excel.prepareSelection('/old-workspace', ['20261021'])
    expect(await excel.reload()).toBe(true)
    oldRead.reject('old config failure')
    expect(await pending).toBe(false)
    expect(excel.settings.value.prompt).toBe('新工作空间')
    expect(excel.exportError.value).toBe('')
    expect(vi.mocked(invoke).mock.calls.some((call) => call[0] === 'prepare_excel')).toBe(false)
  })

  it('discards a native preview returned after changing workspace', async () => {
    const pendingPreview = deferred<ExcelPreview>()
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === 'load_excel_settings') return Promise.resolve(configured)
      if (command === 'prepare_excel') return pendingPreview.promise
      return Promise.resolve()
    })
    const excel = useExcel()
    const pending = excel.prepareSelection('/old-workspace', ['20261021'])
    await settle()
    expect(await excel.reload()).toBe(true)
    pendingPreview.resolve(prepared)
    expect(await pending).toBe(false)
    expect(invoke).toHaveBeenLastCalledWith('discard_excel_preview', { previewId: prepared.id })
    expect(excel.preview.value).toBeNull()
    expect(excel.exportError.value).toBe('')
  })

  it('does not let an old canceled request clear a newer modal’s preparation state', async () => {
    const oldPreview = deferred<ExcelPreview>()
    const newPreview = deferred<ExcelPreview>()
    let requests = 0
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === 'load_excel_settings') return Promise.resolve(configured)
      if (command === 'prepare_excel') return ++requests === 1 ? oldPreview.promise : newPreview.promise
      return Promise.resolve()
    })
    const excel = useExcel()
    const previous = excel.prepareSelection('/workspace', ['20261021'])
    await settle()
    excel.clearExport()
    const current = excel.prepareSelection('/workspace', ['20261022'])
    await settle()
    oldPreview.resolve(prepared)
    expect(await previous).toBe(false)
    expect(excel.preparing.value).toBe(true)
    expect(excel.preview.value).toBeNull()
    const newer = { ...prepared, id: 'new-preview' }
    newPreview.resolve(newer)
    expect(await current).toBe(true)
    expect(excel.preview.value).toEqual(newer)
    expect(excel.preparing.value).toBe(false)
  })

  it('discards an unconfirmed preview without invoking a file write', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    await excel.prepareSelection('/workspace', ['20261021'])
    await excel.discardPreview()
    expect(excel.preview.value).toBeNull()
    expect(invoke).toHaveBeenLastCalledWith('discard_excel_preview', { previewId: prepared.id })
    expect(vi.mocked(invoke).mock.calls.some((call) => call[0] === 'confirm_excel' || call[0] === 'cancel_ai')).toBe(
      false,
    )
  })

  it('clears a discarded preview even when native cleanup fails', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    await excel.prepareSelection('/workspace', ['20261021'])
    vi.mocked(invoke).mockRejectedValueOnce('already expired')
    await expect(excel.discardPreview()).resolves.toBeUndefined()
    expect(excel.preview.value).toBeNull()
    expect(excel.exportError.value).toBe('')
  })

  it('keeps generation failures visible until the export state is cleared', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(configured).mockRejectedValueOnce('template changed')
    const excel = useExcel()
    expect(await excel.prepareSelection('/workspace', ['20261021'])).toBe(false)
    expect(excel.exportError.value).toContain('template changed')
    excel.clearExport()
    expect(excel.exportError.value).toBe('')
    expect(excel.preview.value).toBeNull()
  })

  it('requires a preview before confirming and invalidates it for a different workspace', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    expect(await excel.confirmPreview('/workspace')).toBe(false)
    expect(invoke).not.toHaveBeenCalled()
    expect(excel.exportError.value).toContain('预览')
    await excel.prepareSelection('/workspace', ['20261021'])
    expect(await excel.confirmPreview('/another-workspace')).toBe(false)
    expect(excel.exportError.value).toContain('工作空间已更改')
    expect(excel.preview.value).toBeNull()
    expect(invoke).toHaveBeenLastCalledWith('discard_excel_preview', { previewId: prepared.id })
    expect(vi.mocked(invoke).mock.calls.some((call) => call[0] === 'confirm_excel')).toBe(false)
  })

  it('prevents duplicate confirmations and new preparation during the actual write', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    await excel.prepareSelection('/workspace', ['20261021'])
    const write = deferred<ExcelExportResult>()
    vi.mocked(invoke).mockReturnValueOnce(write.promise)
    const confirming = excel.confirmPreview('/workspace')
    expect(await excel.confirmPreview('/workspace')).toBe(false)
    expect(await excel.prepareSelection('/workspace', ['20261022'])).toBe(false)
    expect(excel.exporting.value).toBe(true)
    expect(vi.mocked(invoke).mock.calls.filter((call) => call[0] === 'confirm_excel')).toHaveLength(1)
    write.resolve(exported)
    expect(await confirming).toBe(true)
  })

  it('retains the preview after confirmation failure so the same approved workbook can be retried', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    await excel.prepareSelection('/workspace', ['20261021'])
    vi.mocked(invoke).mockRejectedValueOnce('directory is read only').mockResolvedValueOnce(exported)
    expect(await excel.confirmPreview('/workspace')).toBe(false)
    expect(excel.exportError.value).toContain('directory is read only')
    expect(excel.preview.value).toEqual(prepared)
    expect(excel.exporting.value).toBe(false)
    expect(await excel.confirmPreview('/workspace')).toBe(true)
    expect(excel.exportError.value).toBe('')
    expect(vi.mocked(invoke).mock.calls.filter((call) => call[0] === 'prepare_excel')).toHaveLength(1)
    expect(vi.mocked(invoke).mock.calls.filter((call) => call[0] === 'confirm_excel')).toHaveLength(2)
  })

  it('does not pretend to cancel a file write or display its stale failure after changing workspace', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    await excel.prepareSelection('/old-workspace', ['20261021'])
    const write = deferred<ExcelExportResult>()
    vi.mocked(invoke).mockImplementation((command) =>
      command === 'confirm_excel'
        ? write.promise
        : command === 'load_excel_settings'
          ? Promise.resolve(configured)
          : Promise.resolve(),
    )
    const confirming = excel.confirmPreview('/old-workspace')
    expect(await excel.reload()).toBe(true)
    expect(excel.exporting.value).toBe(true)
    expect(excel.preview.value).toBeNull()
    expect(await excel.prepareSelection('/new-workspace', ['20261021'])).toBe(false)
    expect(vi.mocked(invoke).mock.calls.some((call) => call[0] === 'cancel_ai')).toBe(false)
    write.reject('old write failed')
    expect(await confirming).toBe(false)
    expect(excel.exporting.value).toBe(false)
    expect(excel.result.value).toBeNull()
    expect(excel.exportError.value).toBe('')
    expect(invoke).toHaveBeenLastCalledWith('discard_excel_preview', { previewId: prepared.id })
  })

  it('discards an old preview before regenerating instead of leaving stale content to confirm', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    await excel.prepareSelection('/workspace', ['20261021'])
    vi.mocked(invoke).mockImplementation((command) =>
      command === 'prepare_excel'
        ? Promise.reject('AI unavailable')
        : command === 'load_excel_settings'
          ? Promise.resolve(configured)
          : Promise.resolve(),
    )
    expect(await excel.prepareSelection('/workspace', ['20261022'])).toBe(false)
    expect(excel.preview.value).toBeNull()
    expect(excel.exportError.value).toContain('AI unavailable')
    expect(invoke).toHaveBeenCalledWith('discard_excel_preview', { previewId: prepared.id })
    expect(vi.mocked(invoke).mock.calls.some((call) => call[0] === 'confirm_excel')).toBe(false)
  })

  it('clears a pending preview when accepting saved settings without altering the accepted values', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    await excel.prepareSelection('/workspace', ['20261021'])
    excel.acceptSaved({ ...configured, prompt: '新规则' })
    expect(excel.settings.value.prompt).toBe('新规则')
    expect(excel.preview.value).toBeNull()
    expect(invoke).toHaveBeenLastCalledWith('discard_excel_preview', { previewId: prepared.id })
  })

  it('reads browser preferences locally and never sends browser generation to native or AI', async () => {
    const storage = new Map<string, string>([[EXCEL_STORAGE_KEY, JSON.stringify(configured)]])
    vi.stubGlobal('localStorage', {
      getItem: vi.fn((key: string) => storage.get(key) ?? null),
      setItem: vi.fn((key: string, value: string) => storage.set(key, value)),
    })
    vi.mocked(isTauri).mockReturnValue(false)
    const excel = useExcel()
    await excel.load()
    expect(excel.settings.value).toEqual(configured)
    expect(await excel.prepareSelection('/workspace', ['20261021'])).toBe(false)
    expect(excel.exportError.value).toContain('桌面应用')
    expect(invoke).not.toHaveBeenCalled()
    expect(JSON.parse(storage.get(EXCEL_STORAGE_KEY)!).prompt).toBe(configured.prompt)
  })
})
