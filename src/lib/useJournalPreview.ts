import { computed, ref, watch, type Ref } from 'vue'
import type { useJournal } from './useJournal'
import type { Document } from './workspace'
import type { MarkdownScrollPosition } from './markdownScroll'

export function useJournalPreview(
  journal: Pick<ReturnType<typeof useJournal>, 'workspace' | 'document' | 'selectedDate' | 'draft'>,
  selection: { dates: Ref<string[]>; documents: Ref<Document[]>; loading: Ref<boolean>; error: Ref<string> },
) {
  const visible = ref(false)
  const scrollPosition = ref<MarkdownScrollPosition | null>(null)
  const isRange = computed(() => selection.dates.value.length > 0)
  const documents = computed(() =>
    isRange.value
      ? [...selection.documents.value].sort((left, right) => right.date.localeCompare(left.date))
      : journal.document.value
        ? [{ ...journal.document.value, content: journal.draft.value }]
        : [],
  )
  const sourceKey = computed(
    () =>
      `${journal.workspace.value?.path ?? ''}\0${
        isRange.value ? [...selection.dates.value].sort().join(',') : journal.selectedDate.value
      }`,
  )
  const loading = computed(() => isRange.value && selection.loading.value)
  const error = computed(() => (isRange.value ? selection.error.value : ''))
  const available = computed(
    () =>
      !!journal.workspace.value &&
      !loading.value &&
      !error.value &&
      documents.value.length > 0 &&
      (selection.dates.value.length > 1 || documents.value[0].format === 'md'),
  )
  const label = computed(() =>
    isRange.value
      ? `${selection.dates.value.length} 天 · ${documents.value.length} 篇日报`
      : (documents.value[0]?.filename ?? ''),
  )

  function close() {
    visible.value = false
  }
  function toggle() {
    if (visible.value) close()
    else if (available.value) visible.value = true
  }
  watch(
    sourceKey,
    () => {
      scrollPosition.value = null
    },
    { flush: 'sync' },
  )
  watch(() => journal.workspace.value?.path, close, { flush: 'sync' })
  watch(
    () =>
      !loading.value &&
      documents.value.length === 1 &&
      selection.dates.value.length <= 1 &&
      documents.value[0].format === 'txt',
    (plainText) => {
      if (plainText) close()
    },
    { flush: 'sync' },
  )

  return { visible, available, documents, isRange, sourceKey, label, loading, error, scrollPosition, close, toggle }
}
