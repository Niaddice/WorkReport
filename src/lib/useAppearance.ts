import { invoke, isTauri } from '@tauri-apps/api/core'
import { ref } from 'vue'

export const APPEARANCE_SCALES = [80, 90, 100, 110, 120] as const
export const DEFAULT_APPEARANCE_SCALE = 100
export const APPEARANCE_STORAGE_KEY = 'work-report.appearance'

export interface AppearanceSettings {
  scale: number
}

function validScale(value: unknown): value is number {
  return typeof value === 'number' && APPEARANCE_SCALES.some((scale) => scale === value)
}

function readScale(settings: unknown): number {
  const scale = settings && typeof settings === 'object' && 'scale' in settings ? settings.scale : undefined
  return validScale(scale) ? scale : DEFAULT_APPEARANCE_SCALE
}

async function readSettings(): Promise<AppearanceSettings> {
  if (isTauri()) return invoke<AppearanceSettings>('load_appearance_settings')
  const saved = globalThis.localStorage.getItem(APPEARANCE_STORAGE_KEY)
  if (!saved) return { scale: DEFAULT_APPEARANCE_SCALE }
  try {
    return { scale: readScale(JSON.parse(saved)) }
  } catch {
    return { scale: DEFAULT_APPEARANCE_SCALE }
  }
}

export function useAppearance() {
  const scale = ref<number>(DEFAULT_APPEARANCE_SCALE)
  const loading = ref(false)
  const error = ref('')
  let loadTask: Promise<boolean> | undefined
  let scope = 0

  function load(): Promise<boolean> {
    if (loadTask) return loadTask
    const currentScope = scope
    loading.value = true
    error.value = ''
    loadTask = (async () => {
      try {
        const settings = await readSettings()
        if (scope === currentScope) scale.value = readScale(settings)
        return scope === currentScope
      } catch (cause) {
        if (scope === currentScope) error.value = `读取界面设置失败：${String(cause)}`
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

  /** Re-read the stored scale, e.g. after switching to another workspace. */
  function reload(): Promise<boolean> {
    scope++
    loadTask = undefined
    scale.value = DEFAULT_APPEARANCE_SCALE
    error.value = ''
    return load()
  }

  /** Adopt the values that the unified save just persisted. */
  function acceptSaved(value: AppearanceSettings) {
    scope++
    loadTask = undefined
    scale.value = readScale(value)
    loading.value = false
    error.value = ''
  }

  return { scale, loading, error, load, reload, acceptSaved }
}
