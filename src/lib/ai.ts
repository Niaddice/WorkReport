import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { FileFormat } from './workspace'

export type ProviderKind = 'codex' | 'claude' | 'opencode' | 'http'
export interface AiProvider { id: string; name: string; kind: ProviderKind; executable: string; model: string; baseUrl: string; apiKey: string }
export interface AiSettings { providers: AiProvider[]; defaultProviderId: string | null; generationPreference: string; summaryPrompt: string }
// Array operations can retain nested Vue proxies; copy the plain settings schema explicitly.
export function cloneAiSettings(value: AiSettings): AiSettings {
  return { providers: value.providers.map(provider => ({ ...provider })), defaultProviderId: value.defaultProviderId, generationPreference: value.generationPreference, summaryPrompt: value.summaryPrompt }
}
export interface DetectedCli { kind: ProviderKind; name: string; executable: string; version: string }
export interface AiModel { id: string; name: string }
export type SummaryMode = 'ai' | 'direct' | ''
export const aiApi = {
  load: () => invoke<AiSettings>('load_ai_settings'),
  save: (settings: AiSettings) => invoke<void>('save_ai_settings', { settings }),
  detect: () => invoke<DetectedCli[]>('detect_ai_clis'),
  models: (baseUrl: string, apiKey: string) => invoke<AiModel[]>('list_ai_models', { baseUrl, apiKey }),
  cliModels: (kind: ProviderKind, executable: string) => invoke<AiModel[]>('list_cli_models', { kind, executable }),
  polish: (content: string, format: FileFormat, requestId: string) => invoke<string>('polish_entry', { content, format, requestId }),
  summarize: (content: string, requestId: string) => invoke<string>('summarize_entries', { content, requestId }),
  cancel: (requestId: string) => invoke<void>('cancel_ai', { requestId }),
}
export function useAi() {
  const settings = ref<AiSettings>({ providers: [], defaultProviderId: null, generationPreference: '', summaryPrompt: '' })
  const error = ref('')
  const saving = ref(false)
  const settingsSaved = ref(false)
  const defaultProvider = computed(() => settings.value.providers.find((provider) => provider.id === settings.value.defaultProviderId))
  const visible = ref(false)
  const generating = ref(false)
  const elapsedSeconds = ref(0)
  const original = ref('')
  const result = ref('')
  const resultError = ref('')
  const summaryVisible = ref(false)
  const summaryGenerating = ref(false)
  const summaryElapsedSeconds = ref(0)
  const summaryDates = ref<string[]>([])
  const summaryResult = ref('')
  const summaryError = ref('')
  const summaryMode = ref<SummaryMode>('')
  let requestId = ''
  let summaryRequestId = ''
  let summaryContent = ''
  let summaryDirectContent = ''
  let activeFormat: FileFormat = 'md'
  let settingsRevision = 0
  let settingsScope = 0
  let pendingLoad: Promise<boolean> | undefined
  let pendingSave: Promise<boolean> | undefined
  let progressTimer: ReturnType<typeof setInterval> | undefined
  let progressStartedAt = 0
  let summaryProgressTimer: ReturnType<typeof setInterval> | undefined
  let summaryProgressStartedAt = 0

  function stopProgress() {
    if (progressTimer !== undefined) {
      clearInterval(progressTimer)
      progressTimer = undefined
    }
  }
  function startProgress() {
    stopProgress()
    progressStartedAt = Date.now()
    elapsedSeconds.value = 0
    progressTimer = setInterval(() => {
      elapsedSeconds.value = Math.floor((Date.now() - progressStartedAt) / 1000)
    }, 1000)
  }
  function stopSummaryProgress() {
    if (summaryProgressTimer !== undefined) {
      clearInterval(summaryProgressTimer)
      summaryProgressTimer = undefined
    }
  }
  function startSummaryProgress() {
    stopSummaryProgress()
    summaryProgressStartedAt = Date.now()
    summaryElapsedSeconds.value = 0
    summaryProgressTimer = setInterval(() => {
      summaryElapsedSeconds.value = Math.floor((Date.now() - summaryProgressStartedAt) / 1000)
    }, 1000)
  }

  async function load(): Promise<boolean> {
    const scope = settingsScope
    if (pendingSave && !(await pendingSave)) return false
    if (scope !== settingsScope) return false
    if (pendingLoad) return pendingLoad
    const revision = settingsRevision
    error.value = ''
    pendingLoad = (async () => {
      try {
        const value = await aiApi.load()
        if (scope === settingsScope && revision === settingsRevision) settings.value = value
        return scope === settingsScope
      } catch (cause) {
        if (scope === settingsScope && revision === settingsRevision) error.value = String(cause)
        return false
      } finally { if (scope === settingsScope) pendingLoad = undefined }
    })()
    return pendingLoad
  }
  function save(value: AiSettings): Promise<boolean> {
    if (pendingSave) return pendingSave
    const scope = settingsScope
    settingsRevision++
    saving.value = true; error.value = ''; settingsSaved.value = false
    const snapshot = cloneAiSettings(value)
    pendingSave = (async () => {
      try {
        await aiApi.save(snapshot)
        if (scope !== settingsScope) return false
        settings.value = snapshot; settingsSaved.value = true; return true
      }
      catch (cause) { if (scope === settingsScope) error.value = String(cause); return false }
      finally { if (scope === settingsScope) { saving.value = false; pendingSave = undefined } }
    })()
    return pendingSave
  }
  /** Discard previous workspace callbacks before reading the new workspace. */
  function reload(): Promise<boolean> {
    settingsScope++
    settingsRevision = 0
    pendingLoad = undefined; pendingSave = undefined
    settings.value = { providers: [], defaultProviderId: null, generationPreference: '', summaryPrompt: '' }
    saving.value = false; settingsSaved.value = false; error.value = ''
    close(); closeSummary()
    return load()
  }
  function acceptSaved(value: AiSettings) {
    settingsScope++; settingsRevision = 0
    pendingLoad = undefined; pendingSave = undefined
    settings.value = cloneAiSettings(value)
    saving.value = false; settingsSaved.value = true; error.value = ''
  }
  async function generate() {
    if (generating.value || !visible.value) return
    const id = crypto.randomUUID(); requestId = id
    generating.value = true; resultError.value = ''; startProgress()
    try {
      const text = await aiApi.polish(original.value, activeFormat, id)
      if (requestId === id && visible.value) result.value = text
    } catch (cause) { if (requestId === id && visible.value) resultError.value = String(cause) }
    finally {
      if (requestId === id) {
        generating.value = false
        stopProgress()
      }
    }
  }
  async function generateSummary() {
    if (summaryGenerating.value || !summaryVisible.value) return
    if (!summaryContent.trim()) { summaryError.value = '所选日期没有可总结的日报内容'; return }
    const id = crypto.randomUUID(); summaryRequestId = id
    summaryMode.value = 'ai'
    summaryGenerating.value = true; summaryError.value = ''; summaryResult.value = ''; startSummaryProgress()
    try {
      const text = await aiApi.summarize(summaryContent, id)
      if (summaryRequestId === id && summaryVisible.value) summaryResult.value = text
    } catch (cause) {
      if (summaryRequestId === id && summaryVisible.value) summaryError.value = String(cause)
    } finally {
      if (summaryRequestId === id) {
        summaryGenerating.value = false
        stopSummaryProgress()
      }
    }
  }
  function directSummary() {
    if (!summaryVisible.value || summaryGenerating.value) return
    if (!summaryDirectContent.trim()) { summaryMode.value = 'direct'; summaryError.value = '所选日期没有可合并的日报内容'; return }
    summaryMode.value = 'direct'
    summaryError.value = ''
    summaryResult.value = summaryDirectContent
  }
  function openSummary(contents: { ai: string; direct: string }, dates: string[]) {
    summaryContent = contents.ai
    summaryDirectContent = contents.direct
    summaryDates.value = [...dates].sort()
    summaryResult.value = ''; summaryError.value = ''; summaryMode.value = ''; summaryVisible.value = true
  }
  function closeSummary() {
    const id = summaryRequestId; summaryRequestId = ''; summaryVisible.value = false; summaryGenerating.value = false; summaryMode.value = ''; stopSummaryProgress()
    if (id) void aiApi.cancel(id).catch(() => {})
  }
  function open(content: string, format: FileFormat) {
    original.value = content; result.value = ''; activeFormat = format; resultError.value = ''; visible.value = true
    void generate()
  }
  function close() {
    const id = requestId; requestId = ''; visible.value = false; generating.value = false; stopProgress()
    if (id) void aiApi.cancel(id).catch(() => {})
  }
  return { settings, error, saving, settingsSaved, defaultProvider, visible, generating, elapsedSeconds, original, result, resultError, summaryVisible, summaryGenerating, summaryElapsedSeconds, summaryDates, summaryResult, summaryError, summaryMode, load, reload, save, acceptSaved, open, generate, close, generateSummary, directSummary, openSummary, closeSummary }
}
