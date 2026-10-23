import { invoke, isTauri } from '@tauri-apps/api/core'
import { ref } from 'vue'

export const APPEARANCE_SCALES = [80, 90, 100, 110, 120] as const
export const DEFAULT_APPEARANCE_SCALE = 100
export const APPEARANCE_STORAGE_KEY = 'work-report.appearance'

export interface AppearanceSettings { scale: number }

function validScale(value: unknown): value is number {
  return typeof value === 'number' && APPEARANCE_SCALES.some(scale => scale === value)
}

function readScale(settings: unknown): number {
  const scale = settings && typeof settings === 'object' && 'scale' in settings ? settings.scale : undefined
  return validScale(scale) ? scale : DEFAULT_APPEARANCE_SCALE
}

async function readSettings(): Promise<AppearanceSettings> {
  if (isTauri()) return invoke<AppearanceSettings>('load_appearance_settings')
  const saved = globalThis.localStorage.getItem(APPEARANCE_STORAGE_KEY)
  if (!saved) return { scale: DEFAULT_APPEARANCE_SCALE }
  try { return { scale: readScale(JSON.parse(saved)) } }
  catch { return { scale: DEFAULT_APPEARANCE_SCALE } }
}

async function writeSettings(settings: AppearanceSettings): Promise<void> {
  if (isTauri()) await invoke<void>('save_appearance_settings', { settings })
  else globalThis.localStorage.setItem(APPEARANCE_STORAGE_KEY, JSON.stringify(settings))
}

export function useAppearance() {
  const scale = ref<number>(DEFAULT_APPEARANCE_SCALE)
  const loading = ref(false)
  const saving = ref(false)
  const error = ref('')
  let selectionRevision = 0
  let settledRevision = 0
  let persistedScale: number | undefined
  let loadTask: Promise<boolean> | undefined
  let saveTask: Promise<boolean> | undefined
  let scope = 0

  const needsSave = () => selectionRevision > 0 && scale.value !== persistedScale

  function load(): Promise<boolean> {
    if (loadTask) return loadTask
    const currentScope = scope
    const revision = selectionRevision
    const pendingSelection = !!saveTask || needsSave()
    loading.value = true
    error.value = ''
    loadTask = (async () => {
      try {
        const settings = await readSettings()
        // A startup read may finish after a user has already chosen a scale.
        if (scope === currentScope && revision === selectionRevision && !pendingSelection && !saveTask) {
          scale.value = readScale(settings)
          persistedScale = scale.value
        }
        return scope === currentScope
      } catch (cause) {
        if (scope === currentScope && revision === selectionRevision && !pendingSelection) error.value = `读取界面设置失败：${String(cause)}`
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

  async function persistPending(): Promise<boolean> {
    const currentScope = scope
    // Write one value at a time; intermediate clicks can be folded into the next write.
    while (needsSave()) {
      const next = scale.value
      const revision = selectionRevision
      try {
        await writeSettings({ scale: next })
        if (scope !== currentScope) return false
        persistedScale = next
      } catch (cause) {
        if (scope !== currentScope) return false
        if (revision !== selectionRevision && scale.value !== next) continue
        settledRevision = selectionRevision
        error.value = `保存界面设置失败：${String(cause)}`
        return false
      }
    }
    settledRevision = selectionRevision
    error.value = ''
    return true
  }

  function flush(): Promise<boolean> {
    const currentScope = scope
    if (saveTask) {
      return saveTask.then(result => scope !== currentScope ? false : selectionRevision > settledRevision ? flush() : result)
    }
    if (!needsSave()) return Promise.resolve(true)
    saving.value = true
    error.value = ''
    saveTask = persistPending().finally(() => {
      if (scope === currentScope) {
        saving.value = false
        saveTask = undefined
      }
    })
    return saveTask
  }

  function setScale(next: number): Promise<boolean> {
    if (!validScale(next)) {
      error.value = '界面缩放范围为 80%–120%，每次调整 10%'
      return Promise.resolve(false)
    }
    selectionRevision++
    scale.value = next
    error.value = ''
    return flush()
  }

  /** Re-read the stored scale, e.g. after switching to another workspace. */
  function reload(): Promise<boolean> {
    scope++
    loadTask = undefined
    saveTask = undefined
    selectionRevision = 0
    settledRevision = 0
    persistedScale = undefined
    scale.value = DEFAULT_APPEARANCE_SCALE
    saving.value = false
    error.value = ''
    return load()
  }

  function acceptSaved(value: AppearanceSettings) {
    scope++; loadTask = undefined; saveTask = undefined
    selectionRevision = 0; settledRevision = 0
    scale.value = readScale(value); persistedScale = scale.value
    loading.value = false; saving.value = false; error.value = ''
  }

  return { scale, loading, saving, error, load, reload, setScale, flush, acceptSaved }
}
