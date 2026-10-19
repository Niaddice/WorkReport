import { computed, ref, watch } from 'vue'
import { cloneAiSettings as clone, type AiSettings } from './ai'

export function useAiSettingsDraft(source: () => AiSettings, persist: (settings: AiSettings) => Promise<boolean>) {
  const draft = ref<AiSettings>(clone(source()))
  const unchanged = computed(() => JSON.stringify(draft.value) === JSON.stringify(source()))
  let pending: Promise<boolean> | undefined
  watch(source, (settings) => { draft.value = clone(settings) })

  function flush(): Promise<boolean> {
    if (pending) return pending
    if (unchanged.value) return Promise.resolve(true)
    pending = persist(clone(draft.value)).finally(() => { pending = undefined })
    return pending
  }

  async function selectDefault(id: string | null) {
    if (pending) return pending
    const previous = draft.value.defaultProviderId
    draft.value.defaultProviderId = id
    const saved = await flush()
    if (!saved) draft.value.defaultProviderId = previous
    return saved
  }

  return { draft, unchanged, flush, selectDefault }
}
