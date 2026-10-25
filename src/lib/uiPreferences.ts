import { invoke, isTauri } from '@tauri-apps/api/core'
import { normalizeUiPreferences } from '../modules/layout'
import type { UiPreferences } from '../modules/types'

export type { ModuleId, ModulePlacement, UiPreferences } from '../modules/types'
export { DEFAULT_UI_PREFERENCES, normalizeUiPreferences } from '../modules/layout'

/** 浏览器预览（npm run dev）使用独立的本地缓存，不与桌面工作空间混用。 */
export const UI_PREFERENCES_STORAGE_KEY = 'work-report.ui-preview'

function readPreviewPreferences(): UiPreferences {
  try {
    const saved = globalThis.localStorage.getItem(UI_PREFERENCES_STORAGE_KEY)
    return normalizeUiPreferences(saved ? JSON.parse(saved) : null)
  } catch {
    return normalizeUiPreferences(null)
  }
}

function writePreviewPreferences(preferences: UiPreferences) {
  try {
    globalThis.localStorage.setItem(UI_PREFERENCES_STORAGE_KEY, JSON.stringify(preferences))
  } catch {
    /* 浏览器预览写缓存失败不应阻断拖放，桌面端仍会报错。 */
  }
}

export const uiPreferencesApi = {
  load: async (): Promise<UiPreferences> => {
    if (!isTauri()) return readPreviewPreferences()
    return normalizeUiPreferences(await invoke<unknown>('load_ui_preferences'))
  },
  save: async (preferences: UiPreferences, workspacePath: string): Promise<void> => {
    const normalized = normalizeUiPreferences(preferences)
    if (!isTauri()) {
      writePreviewPreferences(normalized)
      return
    }
    // 后端会再次校验工作空间路径，迟到的保存不会写进切换后的工作空间。
    await invoke<void>('save_ui_preferences', { workspacePath, preferences: normalized })
  },
}
