import { invoke, isTauri } from '@tauri-apps/api/core'
import { ref } from 'vue'
import type { ExcelSheetPreviewData } from './excelPreview'
import { DEFAULT_FILE_SETTINGS, type FileRule } from './fileSettings'

export interface ExcelSettings {
  enabled: boolean
  templatePath: string
  /** 日报整理规则；旧配置缺失时使用默认内容生成规则。 */
  contentPrompt?: string
  /** 已确认正文如何填入工作簿；保留旧字段以兼容已有设置。 */
  prompt: string
}
export interface ExcelExportResult {
  path: string
  filename: string
  cells: number
}
export interface ExcelPreview {
  id: string
  templateName: string
  outputDirectory: string
  start: string
  end: string
  cells: {
    sheet: string
    cell: string
    value: string | number | boolean | null
    previousValue: string
    replacesFormula: boolean
  }[]
  styles: {
    sheet: string
    range: string
    border: 'black' | null
    wrapText: boolean | null
    autoRowHeight: boolean | null
  }[]
  sheets: ExcelSheetPreviewData[]
}
export interface ExcelExportFile {
  filename: string
  path: string
  modified: number
  size: number
  startDate?: string
  endDate?: string
}
export interface ExcelExportPreview {
  filename: string
  path: string
  sheets: ExcelSheetPreviewData[]
}
export const EXCEL_STORAGE_KEY = 'work-report.excel-preview'
export const RECOMMENDED_EXCEL_CONTENT_PROMPT =
  '根据所选日期的日报，按项目归类整理工作成果、进展、问题和下一步计划，保留事实、数字和完成状态。输出清晰、可核对的 Markdown 正文，不包含 Excel 单元格坐标或填表规则。'
export const RECOMMENDED_EXCEL_PROMPT = `1、F2填写框选区域的最后日期，格式按照YYYYMMDD
2、使用用户已确认的项目分类与成果内容，不重新生成或改写成果事实
3、在第14行开始填写根据分类提炼的成果内容，A列序号，从1开始递增；B列项目分类名称；C列固定文本：开发；D列固定文本：功能正常；E列引入F2内容；F列固定文本：张三；G列根据项目分类预估工时，保证所有任务总和工时为工作日数*8；H列固定文本：进行中；I列固定文本：李四；J列每个项目分类提炼的详细内容；K列留空
4、D11为G列工时的总和
5、B11为任务数量
6、第14行开始的单元格格式使用黑色边框，多个子任务内容换行显示`
export const DEFAULT_EXCEL_SETTINGS: ExcelSettings = {
  enabled: false,
  templatePath: '',
  contentPrompt: RECOMMENDED_EXCEL_CONTENT_PROMPT,
  prompt: '',
}

export function cloneExcelSettings(value: ExcelSettings): ExcelSettings {
  return {
    enabled: value.enabled,
    templatePath: value.templatePath,
    contentPrompt: value.contentPrompt ?? RECOMMENDED_EXCEL_CONTENT_PROMPT,
    prompt: value.prompt,
  }
}

/** Validate journal dates without restricting the selected range to one week. */
export function validExcelDates(dates: string[]): boolean {
  if (!dates.length) return false
  for (const key of dates) {
    if (!/^\d{8}$/.test(key)) return false
    const year = Number(key.slice(0, 4))
    const month = Number(key.slice(4, 6))
    const day = Number(key.slice(6, 8))
    if (year < 1 || month < 1 || month > 12 || day < 1 || day > 31) return false
    const date = new Date(0)
    date.setUTCFullYear(year, month - 1, day)
    date.setUTCHours(0, 0, 0, 0)
    if (date.getUTCFullYear() !== year || date.getUTCMonth() !== month - 1 || date.getUTCDate() !== day) return false
  }
  return true
}

/**
 * 优先使用导出时记录的日期，兼容原有文件名中的「周报」等前缀；
 * 无法识别（非标准命名）时返回 null，调用方按“始终显示”处理。
 */
export function reportDateRange(
  file: string | Pick<ExcelExportFile, 'filename' | 'startDate' | 'endDate'>,
): { start: string; end: string } | null {
  if (
    typeof file !== 'string' &&
    file.startDate &&
    file.endDate &&
    validExcelDates([file.startDate, file.endDate]) &&
    file.startDate <= file.endDate
  )
    return { start: file.startDate, end: file.endDate }
  const filename = typeof file === 'string' ? file : file.filename
  const match = /^(?:总结|周报|报告|Excel报告)_(\d{8})-(\d{8})_/.exec(filename)
  if (!match || !validExcelDates([match[1]!, match[2]!]) || match[1]! > match[2]!) return null
  return { start: match[1]!, end: match[2]! }
}

/**
 * 判断报告是否与给定的日期范围（闭区间，YYYYMMDD）有交集；
 * 无法识别日期范围的文件按“始终显示”处理。
 */
export function reportOverlapsRange(
  file: string | Pick<ExcelExportFile, 'filename' | 'startDate' | 'endDate'>,
  start: string,
  end: string,
): boolean {
  const range = reportDateRange(file)
  return !range || (range.start <= end && range.end >= start)
}

function readPreviewSettings(): ExcelSettings {
  const saved = globalThis.localStorage.getItem(EXCEL_STORAGE_KEY)
  if (!saved) return cloneExcelSettings(DEFAULT_EXCEL_SETTINGS)
  try {
    const value: unknown = JSON.parse(saved)
    if (
      value &&
      typeof value === 'object' &&
      'enabled' in value &&
      typeof value.enabled === 'boolean' &&
      'templatePath' in value &&
      typeof value.templatePath === 'string' &&
      'prompt' in value &&
      typeof value.prompt === 'string' &&
      (!('contentPrompt' in value) || typeof value.contentPrompt === 'string')
    )
      return cloneExcelSettings(value as ExcelSettings)
  } catch {
    /* A preview with invalid local preferences starts disabled. */
  }
  return cloneExcelSettings(DEFAULT_EXCEL_SETTINGS)
}

export function useExcel() {
  const settings = ref<ExcelSettings>(cloneExcelSettings(DEFAULT_EXCEL_SETTINGS))
  const fileRule = ref<FileRule>({ ...DEFAULT_FILE_SETTINGS.excel })
  const loading = ref(false)
  const error = ref('')
  const preparing = ref(false)
  const generatingContent = ref(false)
  const contentDraft = ref('')
  const contentPrompt = ref('')
  const exporting = ref(false)
  const preview = ref<ExcelPreview | null>(null)
  const previewPrompt = ref('')
  const result = ref<ExcelExportResult | null>(null)
  const exportError = ref('')
  const exports = ref<ExcelExportFile[]>([])
  const exportsLoading = ref(false)
  const exportsError = ref('')
  const exportPreview = ref<ExcelExportPreview | null>(null)
  const previewOpening = ref(false)
  const previewOpenError = ref('')
  let loadTask: Promise<boolean> | undefined
  let scope = 0
  let requestId = ''
  let exportRevision = 0
  let exportsRevision = 0
  let exportsWorkspace = ''
  let previewWorkspace = ''
  let contentWorkspace = ''
  let contentDates = ''
  let writingPreviewId = ''
  let exportPreviewRevision = 0

  function discardToken(id: string): Promise<void> {
    return isTauri() ? invoke<void>('discard_excel_preview', { previewId: id }).catch(() => {}) : Promise.resolve()
  }

  function clearWorkbook(): Promise<void> {
    const pending = preview.value
    preview.value = null
    previewPrompt.value = ''
    previewWorkspace = ''
    return pending && pending.id !== writingPreviewId ? discardToken(pending.id) : Promise.resolve()
  }

  /** 返回内容确认步骤时，只撤销填表预览，保留已生成的正文和生成提示词。 */
  async function discardWorkbookPreview(): Promise<void> {
    if (exporting.value) return
    const id = preparing.value ? requestId : ''
    exportRevision++
    if (id) requestId = ''
    preparing.value = false
    await Promise.all([
      clearWorkbook(),
      id && isTauri() ? invoke<void>('cancel_ai', { requestId: id }).catch(() => {}) : Promise.resolve(),
    ])
  }

  /** Preparation can be canceled; a confirmed file write keeps its busy state until it finishes. */
  async function cancel(): Promise<void> {
    const id = requestId
    exportRevision++
    requestId = ''
    preparing.value = false
    generatingContent.value = false
    contentDraft.value = ''
    contentPrompt.value = ''
    contentWorkspace = ''
    contentDates = ''
    await Promise.all([
      id && isTauri() ? invoke<void>('cancel_ai', { requestId: id }).catch(() => {}) : Promise.resolve(),
      clearWorkbook(),
    ])
  }

  function discardPreview(): Promise<void> {
    return cancel()
  }

  function closeExportPreview() {
    exportPreviewRevision++
    exportPreview.value = null
    previewOpening.value = false
    previewOpenError.value = ''
  }

  /** 列出输出目录中已导出的报告。 */
  async function loadExports(workspacePath: string): Promise<boolean> {
    const revision = ++exportsRevision
    const current = () => revision === exportsRevision
    if (exportsWorkspace !== workspacePath) exports.value = []
    exportsWorkspace = workspacePath
    exportsError.value = ''
    if (!isTauri() || !workspacePath) {
      exports.value = []
      exportsLoading.value = false
      return false
    }
    exportsLoading.value = true
    try {
      const value = await invoke<ExcelExportFile[]>('list_excel_exports', { workspacePath })
      if (!current()) return false
      exports.value = Array.isArray(value) ? value : []
      return true
    } catch (cause) {
      if (current()) exportsError.value = `读取已导出报告失败：${String(cause)}`
      return false
    } finally {
      if (current()) exportsLoading.value = false
    }
  }

  /** 读取某个已导出文件，供只读预览。 */
  async function openExport(workspacePath: string, path: string): Promise<boolean> {
    if (!isTauri() || !workspacePath) return false
    const revision = ++exportPreviewRevision
    const current = () => revision === exportPreviewRevision
    previewOpening.value = true
    previewOpenError.value = ''
    exportPreview.value = null
    try {
      const value = await invoke<ExcelExportPreview>('read_excel_export', { workspacePath, path })
      if (!current()) return false
      exportPreview.value = value && Array.isArray(value.sheets) ? value : null
      return !!exportPreview.value
    } catch (cause) {
      if (current()) previewOpenError.value = `读取 Excel 内容失败：${String(cause)}`
      return false
    } finally {
      if (current()) previewOpening.value = false
    }
  }

  /** Calendar selection changes invalidate the generated preview, while exported files remain available. */
  function clearSelection() {
    void cancel()
    result.value = null
    exportError.value = ''
    closeExportPreview()
  }

  function clearExport() {
    clearSelection()
    exportsRevision++
    exportsWorkspace = ''
    exports.value = []
    exportsLoading.value = false
    exportsError.value = ''
  }

  function load(): Promise<boolean> {
    if (loadTask) return loadTask
    const currentScope = scope
    loading.value = true
    error.value = ''
    loadTask = (async () => {
      try {
        const value = await (isTauri()
          ? invoke<ExcelSettings>('load_excel_settings')
          : Promise.resolve(readPreviewSettings()))
        if (scope === currentScope) settings.value = cloneExcelSettings(value)
        return scope === currentScope
      } catch (cause) {
        if (scope === currentScope) error.value = `读取 Excel 输出设置失败：${String(cause)}`
        return false
      } finally {
        if (scope === currentScope) {
          loading.value = false
          loadTask = undefined
        }
      }
    })()
    return loadTask
  }

  /** A workspace change starts an isolated settings scope immediately. */
  function reload(): Promise<boolean> {
    scope++
    clearExport()
    loadTask = undefined
    settings.value = cloneExcelSettings(DEFAULT_EXCEL_SETTINGS)
    fileRule.value = { ...DEFAULT_FILE_SETTINGS.excel }
    error.value = ''
    return load()
  }

  /** 第一步只整理日报正文；用户确认前不请求填表，也不创建 Excel 文件。 */
  async function generateContent(workspacePath: string, dates: string[], prompt?: string): Promise<boolean> {
    if (generatingContent.value || preparing.value || exporting.value) return false
    void discardWorkbookPreview()
    exportError.value = ''
    result.value = null
    if (!workspacePath) {
      exportError.value = '请先打开工作空间。'
      return false
    }
    if (!validExcelDates(dates)) {
      exportError.value = '请选择有效的日期或日期范围。'
      return false
    }
    if (prompt !== undefined && prompt.length > 6000) {
      exportError.value = '内容生成提示词不能超过 6000 字。'
      return false
    }
    if (!isTauri()) {
      exportError.value = '请在桌面应用中使用 Excel 输出。'
      return false
    }
    const selection = [...new Set(dates)].sort()
    const dateScope = selection.join(',')
    if (contentWorkspace !== workspacePath || contentDates !== dateScope) {
      contentDraft.value = ''
      contentWorkspace = ''
      contentDates = ''
    }
    const currentScope = scope
    const id = crypto.randomUUID()
    requestId = id
    generatingContent.value = true
    contentPrompt.value = prompt ?? settings.value.contentPrompt ?? RECOMMENDED_EXCEL_CONTENT_PROMPT
    const current = () => scope === currentScope && requestId === id
    try {
      if (!(await load())) {
        if (current()) exportError.value = error.value || '读取 Excel 输出设置失败，请重试。'
        return false
      }
      if (!current()) return false
      if (!settings.value.enabled) {
        exportError.value = '请先在 Excel 设置中启用输出。'
        return false
      }
      contentPrompt.value = prompt ?? settings.value.contentPrompt ?? RECOMMENDED_EXCEL_CONTENT_PROMPT
      const value = await invoke<string>('generate_excel_content', {
        workspacePath,
        dates: selection,
        requestId: id,
        ...(prompt === undefined ? {} : { prompt }),
      })
      if (!current()) return false
      if (typeof value !== 'string' || !value.trim() || new TextEncoder().encode(value).length > 500_000) {
        exportError.value = '生成内容为空或超过 500 KB，请调整提示词或缩小日期范围后重试。'
        return false
      }
      contentDraft.value = value
      contentWorkspace = workspacePath
      contentDates = dateScope
      return true
    } catch (cause) {
      if (current()) exportError.value = String(cause)
      return false
    } finally {
      if (requestId === id) {
        requestId = ''
        generatingContent.value = false
      }
    }
  }

  /** 第二步仅把用户确认的正文填入模板，不会重新整理原始日报。 */
  async function prepareSelection(
    workspacePath: string,
    dates: string[],
    prompt?: string,
    content?: string,
  ): Promise<boolean> {
    if (generatingContent.value || preparing.value || exporting.value) return false
    exportError.value = ''
    if (!workspacePath) {
      exportError.value = '请先打开工作空间。'
      return false
    }
    if (!validExcelDates(dates)) {
      exportError.value = '请选择有效的日期或日期范围。'
      return false
    }
    if (prompt !== undefined && (!prompt.trim() || prompt.length > 6000)) {
      exportError.value = '请填写 Excel 填充提示词，长度不能超过 6000 字。'
      return false
    }
    if (!isTauri()) {
      exportError.value = '请在桌面应用中使用 Excel 输出。'
      return false
    }
    const selection = [...new Set(dates)].sort()
    if (!contentDraft.value || contentWorkspace !== workspacePath || contentDates !== selection.join(',')) {
      exportError.value = '请先为当前工作空间和日期范围生成内容，再确认填充 Excel。'
      return false
    }
    if (!content?.trim() || new TextEncoder().encode(content).length > 500_000) {
      exportError.value = '请确认需要填充的内容，正文不能为空或超过 500 KB。'
      return false
    }
    void discardWorkbookPreview()
    previewPrompt.value = prompt ?? settings.value.prompt
    result.value = null
    const currentScope = scope
    const id = crypto.randomUUID()
    requestId = id
    preparing.value = true
    const current = () => scope === currentScope && requestId === id
    try {
      if (!(await load())) {
        if (current()) exportError.value = error.value || '读取 Excel 输出设置失败，请重试。'
        return false
      }
      if (!current()) return false
      if (!settings.value.enabled) {
        exportError.value = '请先在 Excel 设置中启用输出。'
        return false
      }
      previewPrompt.value = prompt ?? settings.value.prompt
      const value = await invoke<ExcelPreview>('prepare_excel', {
        workspacePath,
        dates: selection,
        requestId: id,
        content,
        ...(prompt === undefined ? {} : { prompt }),
      })
      if (!current()) {
        await discardToken(value.id)
        return false
      }
      preview.value = value
      previewWorkspace = workspacePath
      return true
    } catch (cause) {
      if (current()) exportError.value = String(cause)
      return false
    } finally {
      if (requestId === id) {
        requestId = ''
        preparing.value = false
      }
    }
  }

  /** Confirm exactly the prepared workbook; this command never requests AI again. */
  async function confirmPreview(workspacePath: string): Promise<boolean> {
    if (generatingContent.value || preparing.value || exporting.value) return false
    const approved = preview.value
    if (!approved) {
      exportError.value = '请先生成 Excel 预览。'
      return false
    }
    if (!workspacePath || workspacePath !== previewWorkspace) {
      void discardPreview()
      exportError.value = '工作空间已更改，请重新生成 Excel 预览。'
      return false
    }
    if (!isTauri()) {
      exportError.value = '请在桌面应用中使用 Excel 输出。'
      return false
    }
    const currentScope = scope
    const currentRevision = exportRevision
    const current = () =>
      scope === currentScope && exportRevision === currentRevision && preview.value?.id === approved.id
    writingPreviewId = approved.id
    exporting.value = true
    result.value = null
    exportError.value = ''
    try {
      const value = await invoke<ExcelExportResult>('confirm_excel', { workspacePath, previewId: approved.id })
      if (!current()) return false
      result.value = value
      preview.value = null
      previewPrompt.value = ''
      previewWorkspace = ''
      contentDraft.value = ''
      contentPrompt.value = ''
      contentWorkspace = ''
      contentDates = ''
      return true
    } catch (cause) {
      if (current()) exportError.value = String(cause)
      return false
    } finally {
      // A reset during a write must not leave a failed, no-longer-visible preview cached.
      if (scope !== currentScope || exportRevision !== currentRevision) await discardToken(approved.id)
      if (writingPreviewId === approved.id) {
        writingPreviewId = ''
        exporting.value = false
      }
    }
  }

  function acceptSaved(value: ExcelSettings) {
    scope++
    clearSelection()
    loadTask = undefined
    settings.value = cloneExcelSettings(value)
    loading.value = false
    error.value = ''
  }

  /** A changed filename rule invalidates approval just as a directory change does. */
  function acceptFileSettings(value: FileRule) {
    if (fileRule.value.directory === value.directory && fileRule.value.filenameTemplate === value.filenameTemplate)
      return
    scope++
    clearExport()
    loadTask = undefined
    fileRule.value = { ...value }
    loading.value = false
    error.value = ''
  }

  return {
    settings,
    fileRule,
    loading,
    error,
    load,
    reload,
    generateContent,
    prepareSelection,
    confirmPreview,
    discardPreview,
    discardWorkbookPreview,
    cancel,
    clearExport,
    clearSelection,
    preparing,
    generatingContent,
    contentDraft,
    contentPrompt,
    preview,
    previewPrompt,
    exporting,
    result,
    exportError,
    exports,
    exportsLoading,
    exportsError,
    exportPreview,
    previewOpening,
    previewOpenError,
    loadExports,
    openExport,
    closeExportPreview,
    acceptSaved,
    acceptFileSettings,
  }
}
