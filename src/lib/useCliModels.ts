import { onScopeDispose, ref, watch } from 'vue'
import { aiApi, type AiModel, type ProviderKind } from './ai'

export function useCliModels(source: () => { kind: ProviderKind; executable: string }) {
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
  watch(() => [source().kind, source().executable], reset, { flush: 'sync' })
  onScopeDispose(reset)

  async function fetchModels() {
    if (loading.value) return false
    const { kind, executable } = source()
    reset()
    if (!executable.trim()) {
      error.value = '请先填写 CLI 路径'
      return false
    }
    const current = revision
    loading.value = true
    try {
      const result = await aiApi.cliModels(kind, executable.trim())
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
