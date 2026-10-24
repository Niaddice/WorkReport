import { computed, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { cloneAiSettings, type AiSettings } from './ai'
import { cloneExcelSettings, type ExcelSettings } from './useExcel'
import type { SummaryOutputSettings } from './useSummaryOutput'
import { cloneDatabaseSettings, type DatabaseSettings } from './database'

export interface WorkspaceSettings {
  appearance: { scale: number }
  ai: AiSettings
  excel: ExcelSettings
  summary: SummaryOutputSettings
  database: DatabaseSettings
}

export function cloneWorkspaceSettings(value: WorkspaceSettings): WorkspaceSettings {
  return {
    appearance: { scale: value.appearance.scale },
    ai: cloneAiSettings(value.ai),
    excel: cloneExcelSettings(value.excel),
    summary: { outputDirectory: value.summary.outputDirectory },
    database: cloneDatabaseSettings(value.database),
  }
}

export const settingsApi = {
  save: (workspacePath: string, settings: WorkspaceSettings) =>
    invoke<WorkspaceSettings>('save_workspace_settings', { workspacePath, settings: cloneWorkspaceSettings(settings) }),
}

/** Editing never persists; only save() submits the complete draft. */
export function useSettingsDraft(
  source: () => WorkspaceSettings,
  persist: (settings: WorkspaceSettings) => Promise<boolean>,
) {
  const baseline = ref(cloneWorkspaceSettings(source()))
  const draft = ref(cloneWorkspaceSettings(source()))
  const unchanged = computed(() => JSON.stringify(draft.value) === JSON.stringify(baseline.value))
  const saving = ref(false)
  const saved = ref(false)
  const error = ref('')
  let pending: Promise<boolean> | undefined
  let scope = 0

  watch(
    source,
    (value) => {
      if (pending || !unchanged.value) return
      baseline.value = cloneWorkspaceSettings(value)
      draft.value = cloneWorkspaceSettings(value)
    },
    { deep: true },
  )
  watch(
    draft,
    () => {
      saved.value = false
      error.value = ''
    },
    { deep: true, flush: 'sync' },
  )

  function reset() {
    scope++
    baseline.value = cloneWorkspaceSettings(source())
    draft.value = cloneWorkspaceSettings(source())
    saved.value = false
    error.value = ''
  }

  function save(): Promise<boolean> {
    if (pending) return pending
    if (unchanged.value) return Promise.resolve(true)
    const currentScope = scope
    const snapshot = cloneWorkspaceSettings(draft.value)
    const sourceBefore = JSON.stringify(source())
    saving.value = true
    saved.value = false
    error.value = ''
    pending = (async () => {
      try {
        if (!(await persist(snapshot))) {
          if (scope === currentScope) error.value = '保存设置失败，请检查设置内容后重试。'
          return false
        }
        if (scope !== currentScope) return false
        const committed = JSON.stringify(source()) === sourceBefore ? snapshot : cloneWorkspaceSettings(source())
        const editedWhileSaving = JSON.stringify(draft.value) !== JSON.stringify(snapshot)
        baseline.value = cloneWorkspaceSettings(committed)
        if (!editedWhileSaving) draft.value = cloneWorkspaceSettings(committed)
        saved.value = unchanged.value
        if (!unchanged.value) error.value = '还有未保存的修改，请再次保存。'
        return unchanged.value
      } catch (cause) {
        if (scope === currentScope) error.value = `保存设置失败：${String(cause)}`
        return false
      } finally {
        saving.value = false
        pending = undefined
      }
    })()
    return pending
  }

  return { draft, unchanged, save, reset, saving, saved, error }
}
