import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { invoke, isTauri } from '@tauri-apps/api/core'
import {
  DEFAULT_EXCEL_SETTINGS,
  EXCEL_STORAGE_KEY,
  RECOMMENDED_EXCEL_CONTENT_PROMPT,
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
  contentPrompt: '按项目整理日报成果，保留事实与工时。',
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
  it('uses recorded dates for custom names and gives valid metadata priority over a legacy name', () => {
    expect(reportDateRange({ filename: '项目成果.xlsx', startDate: '20261019', endDate: '20261025' })).toEqual({
      start: '20261019',
      end: '20261025',
    })
    const renamed = { filename: '总结_20260901-20260902_old.xlsx', startDate: '20261019', endDate: '20261025' }
    expect(reportOverlapsRange(renamed, '20261021', '20261021')).toBe(true)
    expect(reportOverlapsRange(renamed, '20261101', '20261130')).toBe(false)
    expect(reportDateRange({ ...renamed, startDate: '20260229' })).toEqual({ start: '20260901', end: '20260902' })
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
    const legacy = { enabled: configured.enabled, templatePath: configured.templatePath, prompt: configured.prompt }
    vi.mocked(invoke).mockResolvedValueOnce({ ...legacy, outputDirectory: '/old-custom-reports' })
    const excel = useExcel()
    expect(await excel.load()).toBe(true)
    expect(excel.settings.value).toEqual({ ...configured, contentPrompt: RECOMMENDED_EXCEL_CONTENT_PROMPT })
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

describe('Excel content, fill and confirmation lifecycle', () => {
  const generatedContent = '## 客户管理平台\n完成客户检索与导出功能，解决 2 个问题。'
  const dates = ['20261021']
  const settle = async () => {
    for (let index = 0; index < 12; index++) await Promise.resolve()
  }
  function mockConfiguredCommands() {
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === 'load_excel_settings') return Promise.resolve(configured)
      if (command === 'generate_excel_content') return Promise.resolve(generatedContent)
      if (command === 'prepare_excel') return Promise.resolve(prepared)
      if (command === 'confirm_excel') return Promise.resolve(exported)
      return Promise.resolve()
    })
  }
  const commands = () => vi.mocked(invoke).mock.calls.map(([command]) => command)
  async function generate(excel: ReturnType<typeof useExcel>, path = '/workspace', selected = dates) {
    expect(await excel.generateContent(path, selected)).toBe(true)
  }
  async function fill(excel: ReturnType<typeof useExcel>, path = '/workspace', selected = dates) {
    await generate(excel, path, selected)
    expect(await excel.prepareSelection(path, selected, undefined, generatedContent)).toBe(true)
  }

  it('generates readable content for the sorted unique selection and waits for user confirmation before filling', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    expect(await excel.generateContent('/workspace', ['20261025', '20261019', '20261025'])).toBe(true)
    expect(invoke).toHaveBeenCalledWith('generate_excel_content', {
      workspacePath: '/workspace',
      dates: ['20261019', '20261025'],
      requestId: expect.any(String),
    })
    expect(excel.contentDraft.value).toBe(generatedContent)
    expect(excel.preview.value).toBeNull()
    expect(excel.result.value).toBeNull()
    expect(excel.generatingContent.value).toBe(false)
    expect(excel.preparing.value).toBe(false)
    expect(commands()).not.toContain('prepare_excel')
    expect(commands()).not.toContain('confirm_excel')
  })

  it('keeps generation and filling prompts independent and sends the user-edited content to the second AI request', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    const contentPrompt = '只归纳已完成成果，不估算工时'
    const fillingPrompt = '按已确认内容填入 Sheet1!B5，保留模板样式'
    expect(await excel.generateContent('/workspace', dates, contentPrompt)).toBe(true)
    const approved = `${generatedContent}\n用户补充：导出功能已验收。`
    expect(await excel.prepareSelection('/workspace', dates, fillingPrompt, approved)).toBe(true)
    expect(invoke).toHaveBeenCalledWith('generate_excel_content', {
      workspacePath: '/workspace',
      dates,
      requestId: expect.any(String),
      prompt: contentPrompt,
    })
    expect(invoke).toHaveBeenLastCalledWith('prepare_excel', {
      workspacePath: '/workspace',
      dates,
      requestId: expect.any(String),
      prompt: fillingPrompt,
      content: approved,
    })
    expect(excel.contentDraft.value).toBe(generatedContent)
    expect(excel.contentPrompt.value).toBe(contentPrompt)
    expect(excel.previewPrompt.value).toBe(fillingPrompt)
    expect(excel.settings.value).toEqual(configured)
    expect(commands()).not.toContain('save_workspace_settings')
  })

  it('rejects filling before content generation or without confirmed content', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    expect(await excel.prepareSelection('/workspace', dates, configured.prompt, '手工传入正文')).toBe(false)
    expect(commands()).not.toContain('prepare_excel')
    await generate(excel)
    expect(await excel.prepareSelection('/workspace', dates)).toBe(false)
    expect(await excel.prepareSelection('/workspace', dates, configured.prompt, '  ')).toBe(false)
    expect(commands()).not.toContain('prepare_excel')
    expect(excel.contentDraft.value).toBe(generatedContent)
  })

  it.each(['字'.repeat(6001)])('rejects an overlong content prompt before requesting AI', async (prompt) => {
    const excel = useExcel()
    expect(await excel.generateContent('/workspace', dates, prompt)).toBe(false)
    expect(excel.exportError.value).toContain('提示词')
    expect(invoke).not.toHaveBeenCalled()
  })

  it('allows an empty content prompt to use the backend default generation rules', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    expect(await excel.generateContent('/workspace', dates, '')).toBe(true)
    expect(invoke).toHaveBeenLastCalledWith('generate_excel_content', {
      workspacePath: '/workspace',
      dates,
      requestId: expect.any(String),
      prompt: '',
    })
    expect(excel.settings.value).toEqual(configured)
  })

  it.each(['  ', '字'.repeat(6001)])(
    'rejects an invalid filling prompt without discarding generated content',
    async (prompt) => {
      mockConfiguredCommands()
      const excel = useExcel()
      await generate(excel)
      expect(await excel.prepareSelection('/workspace', dates, prompt, generatedContent)).toBe(false)
      expect(excel.exportError.value).toContain('提示词')
      expect(commands()).not.toContain('prepare_excel')
      expect(excel.contentDraft.value).toBe(generatedContent)
    },
  )

  it('accepts an arbitrary range across weeks and years in both stages', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    const selected = [
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
    await fill(excel, '/workspace', selected)
    expect(invoke).toHaveBeenLastCalledWith('prepare_excel', {
      workspacePath: '/workspace',
      dates: selected,
      requestId: expect.any(String),
      content: generatedContent,
    })
  })

  it('binds generated content to its workspace and dates before allowing filling', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    await generate(excel)
    expect(await excel.prepareSelection('/another-workspace', dates, undefined, generatedContent)).toBe(false)
    expect(await excel.prepareSelection('/workspace', ['20261022'], undefined, generatedContent)).toBe(false)
    expect(commands()).not.toContain('prepare_excel')
    expect(await excel.prepareSelection('/workspace', dates, undefined, generatedContent)).toBe(true)
  })

  it('copies the original selection while generation waits for settings', async () => {
    const read = deferred<ExcelSettings>()
    vi.mocked(invoke).mockImplementation((command) =>
      command === 'load_excel_settings' ? read.promise : Promise.resolve(generatedContent),
    )
    const excel = useExcel()
    const selected = ['20261021', '20261022']
    const pending = excel.generateContent('/workspace', selected)
    selected.splice(0, selected.length, '20261026')
    read.resolve(configured)
    expect(await pending).toBe(true)
    expect(invoke).toHaveBeenLastCalledWith('generate_excel_content', {
      workspacePath: '/workspace',
      dates: ['20261021', '20261022'],
      requestId: expect.any(String),
    })
  })

  it('validates workspace and every selected date before reading settings', async () => {
    const excel = useExcel()
    expect(await excel.generateContent('', dates)).toBe(false)
    expect(excel.exportError.value).toContain('工作空间')
    expect(await excel.generateContent('/workspace', ['20261025', '20260229'])).toBe(false)
    expect(excel.exportError.value).toContain('有效的日期')
    expect(invoke).not.toHaveBeenCalled()
  })

  it('reads current enabled settings for generation and never reuses stale enabled configuration', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(configured).mockResolvedValueOnce(DEFAULT_EXCEL_SETTINGS)
    const excel = useExcel()
    await excel.load()
    expect(await excel.generateContent('/workspace', dates)).toBe(false)
    expect(excel.exportError.value).toContain('启用')
    expect(excel.settings.value.enabled).toBe(false)
    expect(commands()).not.toContain('generate_excel_content')
  })

  it('rechecks settings before filling and preserves content if the read fails', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    await generate(excel)
    vi.mocked(invoke).mockRejectedValueOnce('cannot read config')
    expect(await excel.prepareSelection('/workspace', dates, undefined, generatedContent)).toBe(false)
    expect(excel.exportError.value).toContain('cannot read config')
    expect(excel.contentDraft.value).toBe(generatedContent)
    expect(commands()).not.toContain('prepare_excel')
  })

  it('keeps content generation failures visible until the session is cleared', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(configured).mockRejectedValueOnce('AI unavailable')
    const excel = useExcel()
    expect(await excel.generateContent('/workspace', dates)).toBe(false)
    expect(excel.exportError.value).toContain('AI unavailable')
    expect(excel.contentDraft.value).toBe('')
    excel.clearSelection()
    expect(excel.exportError.value).toBe('')
  })

  it.each([
    ['empty', ''],
    ['whitespace', '  '],
    ['larger than 500 KB in UTF-8', '字'.repeat(170_000)],
  ])('rejects %s generated content without enabling filling', async (_label, content) => {
    vi.mocked(invoke).mockImplementation((command) =>
      command === 'load_excel_settings' ? Promise.resolve(configured) : Promise.resolve(content),
    )
    const excel = useExcel()
    expect(await excel.generateContent('/workspace', dates)).toBe(false)
    expect(excel.contentDraft.value).toBe('')
    expect(excel.exportError.value).toContain('500 KB')
    expect(commands()).not.toContain('prepare_excel')
  })

  it('rejects confirmed content above the UTF-8 size limit before asking AI to fill the workbook', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    await generate(excel)
    expect(await excel.prepareSelection('/workspace', dates, configured.prompt, '字'.repeat(170_000))).toBe(false)
    expect(excel.exportError.value).toContain('500 KB')
    expect(excel.contentDraft.value).toBe(generatedContent)
    expect(commands()).not.toContain('prepare_excel')
  })

  it('retains generated content after filling fails and retries only filling', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    await generate(excel)
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === 'load_excel_settings') return Promise.resolve(configured)
      if (command === 'prepare_excel') return Promise.reject('template changed')
      return Promise.resolve()
    })
    expect(await excel.prepareSelection('/workspace', dates, configured.prompt, generatedContent)).toBe(false)
    expect(excel.exportError.value).toContain('template changed')
    expect(excel.contentDraft.value).toBe(generatedContent)
    expect(excel.preview.value).toBeNull()
    mockConfiguredCommands()
    expect(await excel.prepareSelection('/workspace', dates, configured.prompt, generatedContent)).toBe(true)
    expect(commands().filter((command) => command === 'generate_excel_content')).toHaveLength(1)
    expect(commands().filter((command) => command === 'prepare_excel')).toHaveLength(2)
  })

  it('cancels content generation and ignores a late result without starting filling', async () => {
    const native = deferred<string>()
    const canceled = deferred<void>()
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === 'load_excel_settings') return Promise.resolve(configured)
      if (command === 'generate_excel_content') return native.promise
      if (command === 'cancel_ai') return canceled.promise
      return Promise.resolve()
    })
    const excel = useExcel()
    const pending = excel.generateContent('/workspace', dates)
    await settle()
    expect(excel.generatingContent.value).toBe(true)
    expect(await excel.generateContent('/workspace', ['20261022'])).toBe(false)
    expect(await excel.prepareSelection('/workspace', dates, configured.prompt, '未确认结果')).toBe(false)
    const args = vi.mocked(invoke).mock.calls.find(([command]) => command === 'generate_excel_content')![1] as {
      requestId: string
    }
    const cancellation = excel.cancel()
    expect(excel.generatingContent.value).toBe(false)
    expect(invoke).toHaveBeenLastCalledWith('cancel_ai', { requestId: args.requestId })
    let done = false
    void cancellation.then(() => {
      done = true
    })
    await Promise.resolve()
    expect(done).toBe(false)
    canceled.resolve()
    await cancellation
    native.resolve(generatedContent)
    expect(await pending).toBe(false)
    expect(excel.contentDraft.value).toBe('')
    expect(excel.preview.value).toBeNull()
    expect(excel.exportError.value).toBe('')
    expect(commands()).not.toContain('prepare_excel')
  })

  it('does not let an old canceled content request clear a newer generation state', async () => {
    const oldResult = deferred<string>()
    const newResult = deferred<string>()
    let requests = 0
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === 'load_excel_settings') return Promise.resolve(configured)
      if (command === 'generate_excel_content') return ++requests === 1 ? oldResult.promise : newResult.promise
      return Promise.resolve()
    })
    const excel = useExcel()
    const previous = excel.generateContent('/workspace', dates)
    await settle()
    excel.clearSelection()
    const current = excel.generateContent('/workspace', ['20261022'])
    await settle()
    oldResult.resolve('旧内容')
    expect(await previous).toBe(false)
    expect(excel.generatingContent.value).toBe(true)
    expect(excel.contentDraft.value).toBe('')
    newResult.resolve('新内容')
    expect(await current).toBe(true)
    expect(excel.contentDraft.value).toBe('新内容')
    expect(excel.generatingContent.value).toBe(false)
  })

  it('stops generation if the dialog closes during the settings read', async () => {
    const read = deferred<ExcelSettings>()
    vi.mocked(invoke).mockImplementation((command) =>
      command === 'load_excel_settings' ? read.promise : Promise.resolve(),
    )
    const excel = useExcel()
    const pending = excel.generateContent('/workspace', dates)
    expect(excel.generatingContent.value).toBe(true)
    excel.clearExport()
    read.resolve(configured)
    expect(await pending).toBe(false)
    expect(commands()).not.toContain('generate_excel_content')
    expect(commands()).toContain('cancel_ai')
    expect(excel.exportError.value).toBe('')
  })

  it('does not cancel generation when settings are read again, but invalidates it on a workspace reload', async () => {
    const native = deferred<string>()
    vi.mocked(invoke).mockImplementation((command) =>
      command === 'generate_excel_content' ? native.promise : Promise.resolve(configured),
    )
    const excel = useExcel()
    const pending = excel.generateContent('/old-workspace', dates)
    await settle()
    expect(await excel.load()).toBe(true)
    expect(excel.generatingContent.value).toBe(true)
    expect(commands()).not.toContain('cancel_ai')
    expect(await excel.reload()).toBe(true)
    native.resolve(generatedContent)
    expect(await pending).toBe(false)
    expect(excel.contentDraft.value).toBe('')
    expect(excel.exportError.value).toBe('')
  })

  it('cancels filling and discards the late native workbook instead of allowing it to be confirmed', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    await generate(excel)
    const native = deferred<ExcelPreview>()
    vi.mocked(invoke).mockImplementation((command) =>
      command === 'prepare_excel' ? native.promise : Promise.resolve(configured),
    )
    const pending = excel.prepareSelection('/workspace', dates, undefined, generatedContent)
    await settle()
    expect(excel.preparing.value).toBe(true)
    await excel.cancel()
    native.resolve(prepared)
    expect(await pending).toBe(false)
    expect(excel.contentDraft.value).toBe('')
    expect(excel.preview.value).toBeNull()
    expect(invoke).toHaveBeenLastCalledWith('discard_excel_preview', { previewId: prepared.id })
  })

  it('discards only the filled workbook when returning to edit content', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    await fill(excel)
    await excel.discardWorkbookPreview()
    expect(excel.preview.value).toBeNull()
    expect(excel.contentDraft.value).toBe(generatedContent)
    expect(invoke).toHaveBeenLastCalledWith('discard_excel_preview', { previewId: prepared.id })
    expect(await excel.prepareSelection('/workspace', dates, undefined, '用户重新确认的正文')).toBe(true)
    expect(commands().filter((command) => command === 'generate_excel_content')).toHaveLength(1)
    expect(commands().filter((command) => command === 'prepare_excel')).toHaveLength(2)
  })

  it('removes old content and workbook approval before regenerating content', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    await fill(excel)
    const native = deferred<string>()
    vi.mocked(invoke).mockImplementation((command) =>
      command === 'generate_excel_content' ? native.promise : Promise.resolve(configured),
    )
    const generation = excel.generateContent('/workspace', ['20261022'])
    expect(excel.contentDraft.value).toBe('')
    expect(excel.preview.value).toBeNull()
    expect(invoke).toHaveBeenCalledWith('discard_excel_preview', { previewId: prepared.id })
    native.resolve('下一天的新成果')
    expect(await generation).toBe(true)
  })

  it('writes exactly the approved workbook after both AI stages and never requests AI during confirmation', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    await fill(excel)
    const write = deferred<ExcelExportResult>()
    vi.mocked(invoke).mockReturnValueOnce(write.promise)
    const confirming = excel.confirmPreview('/workspace')
    expect(excel.exporting.value).toBe(true)
    expect(invoke).toHaveBeenLastCalledWith('confirm_excel', { workspacePath: '/workspace', previewId: prepared.id })
    expect(await excel.confirmPreview('/workspace')).toBe(false)
    expect(await excel.generateContent('/workspace', dates)).toBe(false)
    expect(await excel.prepareSelection('/workspace', dates, undefined, generatedContent)).toBe(false)
    write.resolve(exported)
    expect(await confirming).toBe(true)
    expect(excel.result.value).toEqual(exported)
    expect(excel.preview.value).toBeNull()
    expect(excel.exporting.value).toBe(false)
    expect(
      commands().filter((command) => ['generate_excel_content', 'prepare_excel', 'confirm_excel'].includes(command)),
    ).toEqual(['generate_excel_content', 'prepare_excel', 'confirm_excel'])
  })

  it('retains a preview after a publication failure and retries saving the same workbook', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    await fill(excel)
    vi.mocked(invoke).mockRejectedValueOnce('directory is read only').mockResolvedValueOnce(exported)
    expect(await excel.confirmPreview('/workspace')).toBe(false)
    expect(excel.exportError.value).toContain('directory is read only')
    expect(excel.preview.value).toEqual(prepared)
    expect(excel.contentDraft.value).toBe(generatedContent)
    expect(await excel.confirmPreview('/workspace')).toBe(true)
    expect(commands().filter((command) => command === 'prepare_excel')).toHaveLength(1)
    expect(commands().filter((command) => command === 'confirm_excel')).toHaveLength(2)
  })

  it('requires a preview and invalidates its approval when the workspace changes', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    expect(await excel.confirmPreview('/workspace')).toBe(false)
    expect(invoke).not.toHaveBeenCalled()
    await fill(excel)
    expect(await excel.confirmPreview('/another-workspace')).toBe(false)
    expect(excel.exportError.value).toContain('工作空间已更改')
    expect(excel.preview.value).toBeNull()
    expect(invoke).toHaveBeenLastCalledWith('discard_excel_preview', { previewId: prepared.id })
    expect(commands()).not.toContain('confirm_excel')
  })

  it('does not pretend to cancel an in-progress file write or show its stale error after a workspace change', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    await fill(excel, '/old-workspace')
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
    expect(commands()).not.toContain('cancel_ai')
    write.reject('old write failed')
    expect(await confirming).toBe(false)
    expect(excel.exporting.value).toBe(false)
    expect(excel.result.value).toBeNull()
    expect(excel.exportError.value).toBe('')
    expect(invoke).toHaveBeenLastCalledWith('discard_excel_preview', { previewId: prepared.id })
  })

  it('invalidates both steps on saved settings or changed output rules and preserves exported files', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    await fill(excel)
    excel.acceptFileSettings({ ...excel.fileRule.value })
    expect(excel.preview.value).toEqual(prepared)
    excel.acceptFileSettings({ directory: 'excel', filenameTemplate: '项目成果_{date}' })
    expect(excel.contentDraft.value).toBe('')
    expect(excel.preview.value).toBeNull()
    expect(invoke).toHaveBeenLastCalledWith('discard_excel_preview', { previewId: prepared.id })
    await generate(excel)
    excel.acceptSaved({ ...configured, contentPrompt: '新生成规则', prompt: '新填充规则' })
    expect(excel.contentDraft.value).toBe('')
    expect(excel.settings.value).toMatchObject({ contentPrompt: '新生成规则', prompt: '新填充规则' })
    expect(commands()).not.toContain('confirm_excel')
  })

  it('ignores filling and exported-file reads finishing after the output directory changes', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    await generate(excel)
    const generated = deferred<ExcelPreview>()
    const read = deferred<{ filename: string; path: string; sheets: [] }>()
    vi.mocked(invoke).mockImplementation((command) => {
      if (command === 'load_excel_settings') return Promise.resolve(configured)
      if (command === 'prepare_excel') return generated.promise
      if (command === 'read_excel_export') return read.promise
      return Promise.resolve()
    })
    const filling = excel.prepareSelection('/workspace', dates, configured.prompt, generatedContent)
    const opening = excel.openExport('/workspace', '/workspace/excel/old.xlsx')
    await settle()
    excel.acceptFileSettings({ directory: '项目报告', filenameTemplate: excel.fileRule.value.filenameTemplate })
    generated.resolve(prepared)
    read.resolve({ filename: 'old.xlsx', path: '/workspace/excel/old.xlsx', sheets: [] })
    expect(await filling).toBe(false)
    expect(await opening).toBe(false)
    expect(excel.preview.value).toBeNull()
    expect(excel.contentDraft.value).toBe('')
    expect(excel.exportPreview.value).toBeNull()
    expect(excel.previewOpening.value).toBe(false)
    expect(invoke).toHaveBeenCalledWith('cancel_ai', { requestId: expect.any(String) })
    expect(invoke).toHaveBeenLastCalledWith('discard_excel_preview', { previewId: prepared.id })
  })

  it('discards an unconfirmed preview even if native cleanup fails, without writing a file', async () => {
    mockConfiguredCommands()
    const excel = useExcel()
    await fill(excel)
    vi.mocked(invoke).mockRejectedValueOnce('already expired')
    await expect(excel.discardPreview()).resolves.toBeUndefined()
    expect(excel.contentDraft.value).toBe('')
    expect(excel.preview.value).toBeNull()
    expect(commands()).not.toContain('confirm_excel')
  })

  it('keeps browser preferences local and never starts either AI stage in browser preview', async () => {
    const storage = new Map<string, string>([[EXCEL_STORAGE_KEY, JSON.stringify(configured)]])
    vi.stubGlobal('localStorage', {
      getItem: vi.fn((key: string) => storage.get(key) ?? null),
      setItem: vi.fn((key: string, value: string) => storage.set(key, value)),
    })
    vi.mocked(isTauri).mockReturnValue(false)
    const excel = useExcel()
    await excel.load()
    expect(excel.settings.value).toEqual(configured)
    expect(await excel.generateContent('/workspace', dates)).toBe(false)
    expect(excel.exportError.value).toContain('桌面应用')
    expect(invoke).not.toHaveBeenCalled()
    expect(JSON.parse(storage.get(EXCEL_STORAGE_KEY)!).prompt).toBe(configured.prompt)
    expect(JSON.parse(storage.get(EXCEL_STORAGE_KEY)!).contentPrompt).toBe(configured.contentPrompt)
  })
})
