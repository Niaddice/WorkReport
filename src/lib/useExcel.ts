import { invoke, isTauri } from '@tauri-apps/api/core'
import { ref } from 'vue'
import type { ExcelSheetPreviewData } from './excelPreview'

export interface ExcelSettings { enabled: boolean; templatePath: string; prompt: string; outputDirectory: string }
export interface ExcelExportResult { path: string; filename: string; cells: number }
export interface ExcelPreview {
  id: string
  templateName: string
  outputDirectory: string
  start: string
  end: string
  cells: { sheet: string; cell: string; value: string | number | boolean | null; previousValue: string; replacesFormula: boolean }[]
  styles: { sheet: string; range: string; border: 'black' | null; wrapText: boolean | null; autoRowHeight: boolean | null }[]
  sheets: ExcelSheetPreviewData[]
}
export const EXCEL_STORAGE_KEY = 'work-report.excel-preview'
export const DEFAULT_EXCEL_SETTINGS: ExcelSettings = { enabled: false, templatePath: '', prompt: '', outputDirectory: '' }

export function cloneExcelSettings(value: ExcelSettings): ExcelSettings {
  return { enabled: value.enabled, templatePath: value.templatePath, prompt: value.prompt, outputDirectory: value.outputDirectory }
}

/** Dates use the journal's YYYYMMDD format. Weeks always begin on Monday. */
export function sameWeekRange(dates: string[]): boolean {
  if (!dates.length) return false
  let week: number | undefined
  for (const key of dates) {
    if (!/^\d{8}$/.test(key)) return false
    const year = Number(key.slice(0, 4)); const month = Number(key.slice(4, 6)); const day = Number(key.slice(6, 8))
    if (year < 1 || month < 1 || month > 12 || day < 1 || day > 31) return false
    const date = new Date(0)
    date.setUTCFullYear(year, month - 1, day); date.setUTCHours(0, 0, 0, 0)
    if (date.getUTCFullYear() !== year || date.getUTCMonth() !== month - 1 || date.getUTCDate() !== day) return false
    const start = date.getTime() - ((date.getUTCDay() + 6) % 7) * 86_400_000
    if (week !== undefined && week !== start) return false
    week = start
  }
  return true
}

function readPreviewSettings(): ExcelSettings {
  const saved = globalThis.localStorage.getItem(EXCEL_STORAGE_KEY)
  if (!saved) return cloneExcelSettings(DEFAULT_EXCEL_SETTINGS)
  try {
    const value: unknown = JSON.parse(saved)
    if (value && typeof value === 'object' && 'enabled' in value && typeof value.enabled === 'boolean' && 'templatePath' in value && typeof value.templatePath === 'string' && 'prompt' in value && typeof value.prompt === 'string' && 'outputDirectory' in value && typeof value.outputDirectory === 'string') return cloneExcelSettings(value as ExcelSettings)
  } catch { /* A preview with invalid local preferences starts disabled. */ }
  return cloneExcelSettings(DEFAULT_EXCEL_SETTINGS)
}

export function useExcel() {
  const settings = ref<ExcelSettings>(cloneExcelSettings(DEFAULT_EXCEL_SETTINGS))
  const loading = ref(false); const saving = ref(false); const error = ref('')
  const preparing = ref(false); const exporting = ref(false)
  const preview = ref<ExcelPreview | null>(null); const result = ref<ExcelExportResult | null>(null); const exportError = ref('')
  let loadTask: Promise<boolean> | undefined
  let saveTask: Promise<boolean> | undefined
  let desired: ExcelSettings | undefined
  let persisted = JSON.stringify(settings.value)
  let revision = 0; let settledRevision = 0
  let scope = 0
  let requestId = ''
  let exportRevision = 0
  let previewWorkspace = ''
  let writingPreviewId = ''
  const needsSave = () => desired !== undefined && JSON.stringify(desired) !== persisted

  function discardToken(id: string): Promise<void> {
    return isTauri() ? invoke<void>('discard_excel_preview', { previewId: id }).catch(() => {}) : Promise.resolve()
  }

  /** Preparation can be canceled; a confirmed file write keeps its busy state until it finishes. */
  async function cancel(): Promise<void> {
    const id = requestId
    const pending = preview.value
    exportRevision++
    requestId = ''; preparing.value = false
    preview.value = null; previewWorkspace = ''
    await Promise.all([
      id && isTauri() ? invoke<void>('cancel_ai', { requestId: id }).catch(() => {}) : Promise.resolve(),
      pending && pending.id !== writingPreviewId ? discardToken(pending.id) : Promise.resolve(),
    ])
  }

  function discardPreview(): Promise<void> { return cancel() }

  function clearExport() {
    void cancel(); result.value = null; exportError.value = ''
  }

  function load(): Promise<boolean> {
    if (loadTask) return loadTask
    const currentScope = scope
    const currentRevision = revision
    loading.value = true; error.value = ''
    loadTask = (async () => {
      try {
        const value = await (isTauri() ? invoke<ExcelSettings>('load_excel_settings') : Promise.resolve(readPreviewSettings()))
        if (scope === currentScope && revision === currentRevision && !saveTask && !needsSave()) {
          settings.value = cloneExcelSettings(value)
          persisted = JSON.stringify(value)
          desired = undefined
        }
        return scope === currentScope
      } catch (cause) {
        if (scope === currentScope && revision === currentRevision) error.value = `读取 Excel 输出设置失败：${String(cause)}`
        return false
      } finally { if (scope === currentScope) { loading.value = false; loadTask = undefined } }
    })()
    return loadTask
  }

  async function persistPending(): Promise<boolean> {
    const currentScope = scope
    while (desired && needsSave()) {
      const snapshot = cloneExcelSettings(desired); const writeRevision = revision
      try {
        if (isTauri()) await invoke<void>('save_excel_settings', { settings: snapshot })
        else globalThis.localStorage.setItem(EXCEL_STORAGE_KEY, JSON.stringify(snapshot))
        if (scope !== currentScope) return false
        persisted = JSON.stringify(snapshot)
        if (revision === writeRevision) settings.value = snapshot
      } catch (cause) {
        if (scope !== currentScope) return false
        if (revision !== writeRevision) continue
        settledRevision = revision
        error.value = `保存 Excel 输出设置失败：${String(cause)}`
        return false
      }
    }
    if (desired) settings.value = cloneExcelSettings(desired)
    settledRevision = revision; error.value = ''
    return true
  }

  function flush(): Promise<boolean> {
    const currentScope = scope
    const finish = (success: boolean) => scope !== currentScope ? false : revision > settledRevision ? flush() : success
    if (saveTask) return saveTask.then(finish)
    if (!needsSave()) return Promise.resolve(true)
    saving.value = true; error.value = ''
    saveTask = persistPending().finally(() => { if (scope === currentScope) { saving.value = false; saveTask = undefined } })
    return saveTask.then(finish)
  }

  function save(value: ExcelSettings): Promise<boolean> {
    desired = cloneExcelSettings(value); revision++
    return flush()
  }

  /** A workspace change starts an isolated settings scope immediately. */
  function reload(): Promise<boolean> {
    scope++
    clearExport()
    loadTask = undefined; saveTask = undefined; desired = undefined
    revision = 0; settledRevision = 0
    settings.value = cloneExcelSettings(DEFAULT_EXCEL_SETTINGS)
    persisted = JSON.stringify(settings.value)
    saving.value = false; error.value = ''
    return load()
  }

  /** Generate an AI-filled preview without writing an output file. */
  async function prepareSelection(workspacePath: string, dates: string[]): Promise<boolean> {
    if (preparing.value || exporting.value) return false
    const previous = preview.value
    preview.value = null; previewWorkspace = ''
    if (previous) void discardToken(previous.id)
    result.value = null; exportError.value = ''
    if (!workspacePath) { exportError.value = '请先打开工作空间。'; return false }
    if (!sameWeekRange(dates)) { exportError.value = 'Excel 输出仅支持同一周（周一至周日）内的日期，请重新框选。'; return false }
    if (!isTauri()) { exportError.value = '请在桌面应用中使用 Excel 输出。'; return false }
    const selection = [...new Set(dates)].sort()
    const currentScope = scope
    const id = crypto.randomUUID(); requestId = id; preparing.value = true
    const current = () => scope === currentScope && requestId === id
    try {
      if (!(await load())) { if (current()) exportError.value = error.value || '读取 Excel 输出设置失败，请重试。'; return false }
      if (!current()) return false
      if (!(await flush())) { if (current()) exportError.value = error.value || '保存 Excel 输出设置失败，请重试。'; return false }
      if (!current()) return false
      if (!settings.value.enabled) { exportError.value = '请先在设置中启用 Excel 输出。'; return false }
      const value = await invoke<ExcelPreview>('prepare_excel', { workspacePath, dates: selection, requestId: id })
      if (!current()) { await discardToken(value.id); return false }
      preview.value = value; previewWorkspace = workspacePath
      return true
    } catch (cause) {
      if (current()) exportError.value = String(cause)
      return false
    } finally {
      if (requestId === id) { requestId = ''; preparing.value = false }
    }
  }

  /** Confirm exactly the prepared workbook; this command never requests AI again. */
  async function confirmPreview(workspacePath: string): Promise<boolean> {
    if (preparing.value || exporting.value) return false
    const approved = preview.value
    if (!approved) { exportError.value = '请先生成 Excel 预览。'; return false }
    if (!workspacePath || workspacePath !== previewWorkspace) {
      void discardPreview()
      exportError.value = '工作空间已更改，请重新生成 Excel 预览。'
      return false
    }
    if (!isTauri()) { exportError.value = '请在桌面应用中使用 Excel 输出。'; return false }
    const currentScope = scope; const currentRevision = exportRevision
    const current = () => scope === currentScope && exportRevision === currentRevision && preview.value?.id === approved.id
    writingPreviewId = approved.id; exporting.value = true
    result.value = null; exportError.value = ''
    try {
      const value = await invoke<ExcelExportResult>('confirm_excel', { workspacePath, previewId: approved.id })
      if (!current()) return false
      result.value = value
      preview.value = null; previewWorkspace = ''
      return true
    } catch (cause) {
      if (current()) exportError.value = String(cause)
      return false
    } finally {
      // A reset during a write must not leave a failed, no-longer-visible preview cached.
      if (scope !== currentScope || exportRevision !== currentRevision) await discardToken(approved.id)
      if (writingPreviewId === approved.id) { writingPreviewId = ''; exporting.value = false }
    }
  }

  function acceptSaved(value: ExcelSettings) {
    scope++; clearExport()
    loadTask = undefined; saveTask = undefined; desired = undefined
    revision = 0; settledRevision = 0
    settings.value = cloneExcelSettings(value); persisted = JSON.stringify(settings.value)
    loading.value = false; saving.value = false; error.value = ''
  }

  return { settings, loading, saving, error, load, reload, save, flush, prepareSelection, confirmPreview, discardPreview, cancel, clearExport, preparing, preview, exporting, result, exportError, acceptSaved }
}
