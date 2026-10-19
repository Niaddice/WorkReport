import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { FileFormat } from './workspace'

export type ProviderKind = 'codex' | 'claude' | 'opencode' | 'http'
export interface AiProvider { id: string; name: string; kind: ProviderKind; executable: string; model: string; baseUrl: string; apiKey: string }
export interface AiSettings { providers: AiProvider[]; defaultProviderId: string | null; generationPreference: string }
// Array operations can retain nested Vue proxies; copy the plain settings schema explicitly.
export function cloneAiSettings(value: AiSettings): AiSettings {
  return { providers: value.providers.map(provider => ({ ...provider })), defaultProviderId: value.defaultProviderId, generationPreference: value.generationPreference }
}
export interface DetectedCli { kind: ProviderKind; name: string; executable: string; version: string }
export interface AiModel { id: string; name: string }
export const aiApi = {
  load: () => invoke<AiSettings>('load_ai_settings'),
  save: (settings: AiSettings) => invoke<void>('save_ai_settings', { settings }),
  detect: () => invoke<DetectedCli[]>('detect_ai_clis'),
  models: (baseUrl: string, apiKey: string) => invoke<AiModel[]>('list_ai_models', { baseUrl, apiKey }),
  polish: (content: string, format: FileFormat, requestId: string) => invoke<string>('polish_entry', { content, format, requestId }),
  cancel: (requestId: string) => invoke<void>('cancel_ai', { requestId }),
}
export function useAi() {
  const settings = ref<AiSettings>({ providers: [], defaultProviderId: null, generationPreference: '' })
  const error = ref('')
  const saving = ref(false)
  const settingsSaved = ref(false)
  const defaultProvider = computed(() => settings.value.providers.find((provider) => provider.id === settings.value.defaultProviderId))
  const visible = ref(false)
  const generating = ref(false)
  const original = ref('')
  const result = ref('')
  const resultError = ref('')
  let requestId = ''
  let activeFormat: FileFormat = 'md'
  let settingsRevision = 0
  let pendingLoad: Promise<boolean> | undefined
  let pendingSave: Promise<boolean> | undefined

  async function load(): Promise<boolean> {
    if (pendingSave && !(await pendingSave)) return false
    if (pendingLoad) return pendingLoad
    const revision = settingsRevision
    error.value = ''
    pendingLoad = (async () => {
      try {
        const value = await aiApi.load()
        if (revision === settingsRevision) settings.value = value
        return true
      } catch (cause) {
        if (revision === settingsRevision) error.value = String(cause)
        return false
      } finally { pendingLoad = undefined }
    })()
    return pendingLoad
  }
  function save(value: AiSettings): Promise<boolean> {
    if (pendingSave) return pendingSave
    settingsRevision++
    saving.value = true; error.value = ''; settingsSaved.value = false
    const snapshot = cloneAiSettings(value)
    pendingSave = (async () => {
      try { await aiApi.save(snapshot); settings.value = snapshot; settingsSaved.value = true; return true }
      catch (cause) { error.value = String(cause); return false }
      finally { saving.value = false; pendingSave = undefined }
    })()
    return pendingSave
  }
  async function generate() {
    if (generating.value || !visible.value) return
    const id = crypto.randomUUID(); requestId = id
    generating.value = true; resultError.value = ''
    try {
      const text = await aiApi.polish(original.value, activeFormat, id)
      if (requestId === id && visible.value) result.value = text
    } catch (cause) { if (requestId === id && visible.value) resultError.value = String(cause) }
    finally { if (requestId === id) generating.value = false }
  }
  function open(content: string, format: FileFormat) {
    original.value = content; result.value = ''; activeFormat = format; resultError.value = ''; visible.value = true
    void generate()
  }
  function close() {
    const id = requestId; requestId = ''; visible.value = false; generating.value = false
    if (id) void aiApi.cancel(id).catch(() => {})
  }
  return { settings, error, saving, settingsSaved, defaultProvider, visible, generating, original, result, resultError, load, save, open, generate, close }
}
