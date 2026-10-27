import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export type ProviderKind = 'codex' | 'claude' | 'opencode' | 'http' | 'anthropic'
export interface AiProvider {
  id: string
  name: string
  kind: ProviderKind
  executable: string
  model: string
  baseUrl: string
  apiKey: string
}
export interface AiSettings {
  providers: AiProvider[]
  defaultProviderId: string | null
  generationPreference: string
  summaryPrompt: string
}
// Array operations can retain nested Vue proxies; copy the plain settings schema explicitly.
export function cloneAiSettings(value: AiSettings): AiSettings {
  return {
    providers: value.providers.map((provider) => ({ ...provider })),
    defaultProviderId: value.defaultProviderId,
    generationPreference: value.generationPreference,
    summaryPrompt: value.summaryPrompt,
  }
}
export interface DetectedCli {
  kind: ProviderKind
  name: string
  executable: string
  version: string
}
export interface AiModel {
  id: string
  name: string
}
export type SummaryMode = 'ai' | 'direct' | ''
export const aiApi = {
  load: () => invoke<AiSettings>('load_ai_settings'),
  detect: () => invoke<DetectedCli[]>('detect_ai_clis'),
  models: (baseUrl: string, apiKey: string, kind: ProviderKind) =>
    invoke<AiModel[]>('list_ai_models', { baseUrl, apiKey, kind }),
  cliModels: (kind: ProviderKind, executable: string) => invoke<AiModel[]>('list_cli_models', { kind, executable }),
  summarize: (content: string, requestId: string) => invoke<string>('summarize_entries', { content, requestId }),
  cancel: (requestId: string) => invoke<void>('cancel_ai', { requestId }),
}
export function useAi() {
  const settings = ref<AiSettings>({
    providers: [],
    defaultProviderId: null,
    generationPreference: '',
    summaryPrompt: '',
  })
  const error = ref('')
  const defaultProvider = computed(() =>
    settings.value.providers.find((provider) => provider.id === settings.value.defaultProviderId),
  )
  const summaryVisible = ref(false)
  const summaryGenerating = ref(false)
  const summaryElapsedSeconds = ref(0)
  const summaryDates = ref<string[]>([])
  /** 完整框选范围，包含没有日报的日期；summaryDates 仅包含实际参与总结的日报。 */
  const summaryScopeDates = ref<string[]>([])
  const summaryResult = ref('')
  const summaryError = ref('')
  const summaryMode = ref<SummaryMode>('')
  let summaryRequestId = ''
  let summaryContent = ''
  let summaryDirectContent = ''
  let settingsScope = 0
  let pendingLoad: Promise<boolean> | undefined
  let summaryProgressTimer: ReturnType<typeof setInterval> | undefined
  let summaryProgressStartedAt = 0

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
    if (pendingLoad) return pendingLoad
    error.value = ''
    pendingLoad = (async () => {
      try {
        const value = await aiApi.load()
        if (scope === settingsScope) settings.value = value
        return scope === settingsScope
      } catch (cause) {
        if (scope === settingsScope) error.value = String(cause)
        return false
      } finally {
        if (scope === settingsScope) pendingLoad = undefined
      }
    })()
    return pendingLoad
  }
  /** Discard previous workspace callbacks before reading the new workspace. */
  function reload(): Promise<boolean> {
    settingsScope++
    pendingLoad = undefined
    settings.value = { providers: [], defaultProviderId: null, generationPreference: '', summaryPrompt: '' }
    error.value = ''
    closeSummary()
    return load()
  }
  function acceptSaved(value: AiSettings) {
    settingsScope++
    pendingLoad = undefined
    settings.value = cloneAiSettings(value)
    error.value = ''
  }
  async function generateSummary() {
    if (summaryGenerating.value || !summaryVisible.value) return
    if (!summaryContent.trim()) {
      summaryError.value = '所选日期没有可总结的日报内容'
      return
    }
    const id = crypto.randomUUID()
    summaryRequestId = id
    summaryMode.value = 'ai'
    summaryGenerating.value = true
    summaryError.value = ''
    summaryResult.value = ''
    startSummaryProgress()
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
    if (!summaryDirectContent.trim()) {
      summaryMode.value = 'direct'
      summaryError.value = '所选日期没有可合并的日报内容'
      return
    }
    summaryMode.value = 'direct'
    summaryError.value = ''
    summaryResult.value = summaryDirectContent
  }
  function openSummary(contents: { ai: string; direct: string }, dates: string[], scopeDates: string[] = dates) {
    summaryContent = contents.ai
    summaryDirectContent = contents.direct
    summaryDates.value = [...dates].sort()
    summaryScopeDates.value = [...new Set(scopeDates)].sort()
    summaryResult.value = ''
    summaryError.value = ''
    summaryMode.value = ''
    summaryVisible.value = true
  }
  function closeSummary() {
    const id = summaryRequestId
    summaryRequestId = ''
    summaryVisible.value = false
    summaryGenerating.value = false
    summaryMode.value = ''
    stopSummaryProgress()
    if (id) void aiApi.cancel(id).catch(() => {})
  }
  return {
    settings,
    error,
    defaultProvider,
    summaryVisible,
    summaryGenerating,
    summaryElapsedSeconds,
    summaryDates,
    summaryScopeDates,
    summaryResult,
    summaryError,
    summaryMode,
    load,
    reload,
    acceptSaved,
    generateSummary,
    directSummary,
    openSummary,
    closeSummary,
  }
}
