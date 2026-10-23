import { invoke, isTauri } from '@tauri-apps/api/core'
import { ref } from 'vue'
import { summaryExportContent } from './summary'

export interface SummaryOutputSettings { outputDirectory: string }
export interface SummaryExportResult { filename: string; path: string }
export type SummaryExportFormat = 'md' | 'txt'
export const DEFAULT_SUMMARY_OUTPUT_SETTINGS: SummaryOutputSettings = { outputDirectory: '总结' }
export const SUMMARY_OUTPUT_STORAGE_KEY = 'work-report.summary-output-preview'

function cloneSettings(value: SummaryOutputSettings): SummaryOutputSettings {
  return { outputDirectory: value.outputDirectory }
}

function readPreviewSettings(): SummaryOutputSettings {
  const saved = globalThis.localStorage.getItem(SUMMARY_OUTPUT_STORAGE_KEY)
  if (saved) {
    try {
      const value: unknown = JSON.parse(saved)
      if (value && typeof value === 'object' && 'outputDirectory' in value && typeof value.outputDirectory === 'string' && value.outputDirectory.trim()) return cloneSettings(value as SummaryOutputSettings)
    } catch { /* An invalid browser preference falls back to the default directory. */ }
  }
  return cloneSettings(DEFAULT_SUMMARY_OUTPUT_SETTINGS)
}

export function useSummaryOutput() {
  const settings = ref<SummaryOutputSettings>(cloneSettings(DEFAULT_SUMMARY_OUTPUT_SETTINGS))
  const loading = ref(false); const saving = ref(false); const error = ref('')
  const exporting = ref(false); const result = ref<SummaryExportResult | null>(null); const exportError = ref('')
  const opening = ref(false); const openError = ref('')
  let loadTask: Promise<boolean> | undefined
  let saveTask: Promise<boolean> | undefined
  let desired: SummaryOutputSettings | undefined
  let persisted = JSON.stringify(settings.value)
  let revision = 0; let settledRevision = 0; let scope = 0
  let presentationRevision = 0; let operationId = 0
  let activeExport: number | undefined; let activeOpen: number | undefined
  const needsSave = () => desired !== undefined && JSON.stringify(desired) !== persisted

  /** Clear a previous dialog's feedback without treating an actual file write as canceled. */
  function clearExport() {
    presentationRevision++
    result.value = null; exportError.value = ''; openError.value = ''
  }

  function load(): Promise<boolean> {
    if (loadTask) return loadTask
    const currentScope = scope; const currentRevision = revision
    loading.value = true; error.value = ''
    loadTask = (async () => {
      try {
        const value = await (isTauri() ? invoke<SummaryOutputSettings>('load_summary_settings') : Promise.resolve(readPreviewSettings()))
        if (scope === currentScope && revision === currentRevision && !saveTask && !needsSave()) {
          settings.value = cloneSettings(value)
          persisted = JSON.stringify(value)
          desired = undefined
        }
        return scope === currentScope
      } catch (cause) {
        if (scope === currentScope && revision === currentRevision) error.value = `读取总结输出设置失败：${String(cause)}`
        return false
      } finally {
        if (scope === currentScope) { loading.value = false; loadTask = undefined }
      }
    })()
    return loadTask
  }

  async function persistPending(): Promise<boolean> {
    const currentScope = scope
    while (desired && needsSave()) {
      const snapshot = cloneSettings(desired); const writeRevision = revision
      try {
        if (!snapshot.outputDirectory.trim()) throw new Error('总结输出目录不能为空')
        if (isTauri()) await invoke<void>('save_summary_settings', { settings: snapshot })
        else globalThis.localStorage.setItem(SUMMARY_OUTPUT_STORAGE_KEY, JSON.stringify(snapshot))
        if (scope !== currentScope) return false
        persisted = JSON.stringify(snapshot)
        if (revision === writeRevision) settings.value = snapshot
      } catch (cause) {
        if (scope !== currentScope) return false
        if (revision !== writeRevision) continue
        settledRevision = revision
        error.value = `保存总结输出设置失败：${String(cause)}`
        return false
      }
    }
    if (desired) settings.value = cloneSettings(desired)
    settledRevision = revision; error.value = ''
    return true
  }

  function flush(): Promise<boolean> {
    const currentScope = scope
    const finish = (success: boolean) => scope !== currentScope ? false : revision > settledRevision ? flush() : success
    if (saveTask) return saveTask.then(finish)
    if (!needsSave()) return Promise.resolve(true)
    saving.value = true; error.value = ''
    saveTask = persistPending().finally(() => {
      if (scope === currentScope) { saving.value = false; saveTask = undefined }
    })
    return saveTask.then(finish)
  }

  function save(value: SummaryOutputSettings): Promise<boolean> {
    desired = cloneSettings(value); revision++
    return flush()
  }

  /** Reset state immediately so late callbacks cannot reuse another workspace's directory. */
  function reload(): Promise<boolean> {
    scope++; clearExport()
    loadTask = undefined; saveTask = undefined; desired = undefined
    revision = 0; settledRevision = 0
    activeExport = undefined; activeOpen = undefined
    settings.value = cloneSettings(DEFAULT_SUMMARY_OUTPUT_SETTINGS)
    persisted = JSON.stringify(settings.value)
    saving.value = false; exporting.value = false; opening.value = false; error.value = ''
    return load()
  }

  async function exportSummary(workspacePath: string, dates: string[], content: string, format: SummaryExportFormat, mode: 'ai' | 'direct' | '' = 'ai'): Promise<boolean> {
    if (activeExport !== undefined) return false
    result.value = null; exportError.value = ''
    if (!isTauri()) { exportError.value = '请在桌面应用中导出总结。'; return false }
    if (!workspacePath || !content.trim()) { exportError.value = '请先打开工作空间并生成总结。'; return false }
    const currentScope = scope; const currentPresentation = presentationRevision; const id = ++operationId
    activeExport = id; exporting.value = true
    const current = () => scope === currentScope && presentationRevision === currentPresentation
    try {
      if (!(await load()) || scope !== currentScope) { if (current()) exportError.value = error.value; return false }
      if (!(await flush()) || scope !== currentScope) { if (current()) exportError.value = error.value; return false }
      // Closing the dialog before the write starts must not start a hidden export.
      if (!current()) return false
      const value = await invoke<SummaryExportResult>('export_summary', {
        workspacePath, dates: [...new Set(dates)].sort(), content: summaryExportContent(content, format, mode), format,
      })
      if (!current()) return false
      result.value = value
      return true
    } catch (cause) {
      if (current()) exportError.value = `导出总结失败：${String(cause)}`
      return false
    } finally {
      if (activeExport === id) { activeExport = undefined; exporting.value = false }
    }
  }

  async function openDirectory(workspacePath: string): Promise<boolean> {
    if (activeOpen !== undefined) return false
    openError.value = ''
    if (!isTauri()) { openError.value = '请在桌面应用中打开总结目录。'; return false }
    if (!workspacePath) { openError.value = '请先打开工作空间。'; return false }
    const currentScope = scope; const currentPresentation = presentationRevision; const id = ++operationId
    activeOpen = id; opening.value = true
    const current = () => scope === currentScope && presentationRevision === currentPresentation
    try {
      if (!(await load()) || scope !== currentScope) { if (current()) openError.value = error.value; return false }
      if (!(await flush()) || scope !== currentScope) { if (current()) openError.value = error.value; return false }
      if (!current()) return false
      await invoke<void>('open_summary_output_directory', { workspacePath })
      return current()
    } catch (cause) {
      if (current()) openError.value = `打开总结目录失败：${String(cause)}`
      return false
    } finally {
      if (activeOpen === id) { activeOpen = undefined; opening.value = false }
    }
  }

  function acceptSaved(value: SummaryOutputSettings) {
    scope++; clearExport()
    loadTask = undefined; saveTask = undefined; desired = undefined
    revision = 0; settledRevision = 0
    settings.value = cloneSettings(value); persisted = JSON.stringify(settings.value)
    loading.value = false; saving.value = false; error.value = ''
  }

  return { settings, loading, saving, error, load, reload, save, flush, exportSummary, exporting, result, exportError, clearExport, openDirectory, opening, openError, acceptSaved }
}
