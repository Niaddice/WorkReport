import { onScopeDispose, ref, watch } from 'vue'
import { aiApi, type AiModel, type ProviderKind } from './ai'

export function useAiModels(source: () => { baseUrl: string; apiKey: string; kind: ProviderKind }) {
  const models = ref<AiModel[]>([])
  const loading = ref(false)
  const error = ref('')
  const queried = ref(false)
  let revision = 0

  function reset() {
    revision++
    models.value = []
    loading.value = false
    error.value = ''
    queried.value = false
  }
  watch(() => [source().baseUrl, source().apiKey, source().kind], reset, { flush: 'sync' })
  onScopeDispose(reset)

  async function fetchModels() {
    if (loading.value) return false
    const { baseUrl, apiKey, kind } = source()
    reset()
    if (!baseUrl.trim()) {
      error.value = '请先填写接口地址'
      return false
    }
    const current = revision
    loading.value = true
    try {
      const result = await aiApi.models(baseUrl.trim(), apiKey, kind)
      if (current !== revision) return false
      models.value = result
      queried.value = true
      return true
    } catch (cause) {
      if (current === revision) error.value = String(cause)
      return false
    } finally {
      if (current === revision) loading.value = false
    }
  }
  return { models, loading, error, queried, fetchModels }
}
