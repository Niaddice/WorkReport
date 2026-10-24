import { invoke } from '@tauri-apps/api/core'
import { DEFAULT_SKIN, SKINS, type SkinId } from './skins'

export type LayoutAxis = 'row' | 'column'
export type LayoutModule = 'calendar' | 'editor'

export interface UiPreferences {
  axis: LayoutAxis
  /** 两个模块的排列顺序，长度固定为 2。 */
  order: LayoutModule[]
  skin: SkinId
}

export const DEFAULT_UI_PREFERENCES: UiPreferences = {
  axis: 'row',
  order: ['calendar', 'editor'],
  skin: DEFAULT_SKIN,
}

export function normalizeUiPreferences(value: unknown): UiPreferences {
  const preferences = (value ?? {}) as Partial<UiPreferences>
  const order = Array.isArray(preferences.order) ? preferences.order : []
  const valid = order.length === 2 && order.includes('calendar') && order.includes('editor')
  const skin = SKINS.find((item) => item.id === preferences.skin)?.id ?? DEFAULT_SKIN
  return {
    axis: preferences.axis === 'column' ? 'column' : 'row',
    order: valid ? [order[0] as LayoutModule, order[1] as LayoutModule] : [...DEFAULT_UI_PREFERENCES.order],
    skin,
  }
}

export const uiPreferencesApi = {
  load: () => invoke<UiPreferences>('load_ui_preferences'),
  save: (preferences: UiPreferences) => invoke<void>('save_ui_preferences', { preferences }),
}
