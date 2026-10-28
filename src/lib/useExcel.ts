import { invoke, isTauri } from '@tauri-apps/api/core'
import { ref } from 'vue'
import type { ExcelSheetPreviewData } from './excelPreview'

export interface ExcelSettings {
  enabled: boolean
  templatePath: string
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
}
export interface ExcelExportPreview {
  filename: string
  path: string
  sheets: ExcelSheetPreviewData[]
}
export const EXCEL_STORAGE_KEY = 'work-report.excel-preview'
export const RECOMMENDED_EXCEL_PROMPT = `1、F2填写框选区域的最后日期，格式按照YYYYMMDD
2、根据项目分类提炼成果
3、在第14行开始填写根据分类提炼的成果内容，A列序号，从1开始递增；B列项目分类名称；C列固定文本：开发；D列固定文本：功能正常；E列引入F2内容；F列固定文本：张三；G列根据项目分类预估工时，保证所有任务总和工时为工作日数*8；H列固定文本：进行中；I列固定文本：李四；J列每个项目分类提炼的详细内容；K列留空
4、D11为G列工时的总和
5、B11为任务数量
6、第14行开始的单元格格式使用黑色边框，多个子任务内容换行显示`
export const DEFAULT_EXCEL_SETTINGS: ExcelSettings = {
  enabled: false,
  templatePath: '',
  prompt: '',
}

export function cloneExcelSettings(value: ExcelSettings): ExcelSettings {
  return {
    enabled: value.enabled,
    templatePath: value.templatePath,
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
 * 解析报告文件名中的起止日期，兼容原有的「周报」前缀；
 * 无法识别（非标准命名）时返回 null，调用方按“始终显示”处理。
 */
export function reportDateRange(filename: string): { start: string; end: string } | null {
  const match = /^(?:总结|周报|报告|Excel报告)_(\d{8})-(\d{8})_/.exec(filename)
  if (!match || !validExcelDates([match[1]!, match[2]!]) || match[1]! > match[2]!) return null
  return { start: match[1]!, end: match[2]! }
}

/**
 * 判断报告是否与给定的日期范围（闭区间，YYYYMMDD）有交集；
 * 无法识别日期范围的文件按“始终显示”处理。
 */
export function reportOverlapsRange(filename: string, start: string, end: string): boolean {
  const range = reportDateRange(filename)
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
      typeof value.prompt === 'string'
    )
      return cloneExcelSettings(value as ExcelSettings)
  } catch {
    /* A preview with invalid local preferences starts disabled. */
  }
  return cloneExcelSettings(DEFAULT_EXCEL_SETTINGS)
}

export function useExcel() {
  const settings = ref<ExcelSettings>(cloneExcelSettings(DEFAULT_EXCEL_SETTINGS))
  const loading = ref(false)
  const error = ref('')
  const preparing = ref(false)
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
  let writingPreviewId = ''

  function discardToken(id: string): Promise<void> {
    return isTauri() ? invoke<void>('discard_excel_preview', { previewId: id }).catch(() => {}) : Promise.resolve()
  }

  /** Preparation can be canceled; a confirmed file write keeps its busy state until it finishes. */
  async function cancel(): Promise<void> {
    const id = requestId
    const pending = preview.value
    exportRevision++
    requestId = ''
    preparing.value = false
    preview.value = null
    previewPrompt.value = ''
    previewWorkspace = ''
    await Promise.all([
      id && isTauri() ? invoke<void>('cancel_ai', { requestId: id }).catch(() => {}) : Promise.resolve(),
      pending && pending.id !== writingPreviewId ? discardToken(pending.id) : Promise.resolve(),
    ])
  }

  function discardPreview(): Promise<void> {
    return cancel()
  }

  function closeExportPreview() {
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
    previewOpening.value = true
    previewOpenError.value = ''
    exportPreview.value = null
    try {
      const value = await invoke<ExcelExportPreview>('read_excel_export', { workspacePath, path })
      exportPreview.value = value && Array.isArray(value.sheets) ? value : null
      return !!exportPreview.value
    } catch (cause) {
      previewOpenError.value = `读取 Excel 内容失败：${String(cause)}`
      return false
    } finally {
      previewOpening.value = false
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
    error.value = ''
    return load()
  }

  /** Generate an AI-filled preview without writing an output file. */
  async function prepareSelection(workspacePath: string, dates: string[], prompt?: string): Promise<boolean> {
    if (preparing.value || exporting.value) return false
    const previous = preview.value
    preview.value = null
    previewPrompt.value = prompt ?? settings.value.prompt
    previewWorkspace = ''
    if (previous) void discardToken(previous.id)
    result.value = null
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
      exportError.value = '请填写本次提示词，长度不能超过 6000 字。'
      return false
    }
    if (!isTauri()) {
      exportError.value = '请在桌面应用中使用 Excel 输出。'
      return false
    }
    const selection = [...new Set(dates)].sort()
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
        exportError.value = '请先在设置中启用 Excel 输出。'
        return false
      }
      previewPrompt.value = prompt ?? settings.value.prompt
      const value = await invoke<ExcelPreview>('prepare_excel', {
        workspacePath,
        dates: selection,
        requestId: id,
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
    if (preparing.value || exporting.value) return false
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

  return {
    settings,
    loading,
    error,
    load,
    reload,
    prepareSelection,
    confirmPreview,
    discardPreview,
    cancel,
    clearExport,
    clearSelection,
    preparing,
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
  }
}
