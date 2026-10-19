import { computed, ref } from 'vue'
import { api, type Document, type Entry, type Workspace, type FileFormat } from './workspace'
import { dateKey } from './calendar'

export function useJournal() {
  const workspace = ref<Workspace | null>(null)
  const entries = ref<Entry[]>([])
  const selectedDate = ref(dateKey())
  const document = ref<Document | null>(null)
  const draft = ref('')
  const busy = ref(false)
  const saving = ref(false)
  const error = ref('')
  const savedAt = ref<Date | null>(null)
  const dirty = computed(() => document.value !== null && draft.value !== document.value.content)
  let timer: ReturnType<typeof setTimeout> | undefined
  let pendingSave: Promise<boolean> | null = null

  function reportError(cause: unknown) {
    error.value = String(cause)
  }

  async function refreshIndex() {
    entries.value = await api.list()
  }

  function applyDocument(value: Document | null) {
    document.value = value
    draft.value = value?.content ?? ''
    savedAt.value = null
  }

  async function flush(): Promise<boolean> {
    clearTimeout(timer)
    if (pendingSave) return pendingSave
    if (!dirty.value) return true
    pendingSave = (async () => {
      saving.value = true
      try {
        // Serialize writes and keep saving if typing continued during a write.
        while (document.value && dirty.value) {
          const snapshot = draft.value
          const saved = await api.save(document.value.date, snapshot, document.value.revision, document.value.format)
          document.value = saved
          savedAt.value = new Date()
        }
        error.value = ''
        await refreshIndex()
        return true
      } catch (cause) {
        reportError(cause)
        return false
      } finally {
        saving.value = false
        pendingSave = null
      }
    })()
    return pendingSave
  }

  function edit(content: string) {
    draft.value = content
    clearTimeout(timer)
    // A failed save needs a deliberate retry; keep the draft intact.
    if (!error.value) timer = setTimeout(() => { void flush() }, 650)
  }

  async function select(date: string): Promise<boolean> {
    if (busy.value) return false
    if (date === selectedDate.value) return true
    busy.value = true
    try {
      if (!(await flush())) return false
      const entry = entries.value.find((entry) => entry.date === date)
      const value = entry ? await api.read(date, entry.format) : null
      selectedDate.value = date
      applyDocument(value)
      error.value = ''
      return true
    } catch (cause) {
      reportError(cause)
      return false
    } finally { busy.value = false }
  }

  async function open(path: string) {
    if (busy.value) return false
    busy.value = true
    try {
      if (!(await flush())) return false
      const nextWorkspace = await api.open(path)
      workspace.value = nextWorkspace
      // Clear the old draft immediately after the backend changes workspace.
      applyDocument(null)
      entries.value = []
      await refreshIndex()
      if (entries.value.some((entry) => entry.date === selectedDate.value)) {
        applyDocument(await api.read(selectedDate.value, entries.value.find((entry) => entry.date === selectedDate.value)!.format))
      }
      error.value = ''
      return true
    } catch (cause) {
      reportError(cause)
      return false
    } finally { busy.value = false }
  }

  async function create(date: string, format: FileFormat = 'md') {
    if (busy.value || !(await select(date))) return false
    busy.value = true
    try {
      applyDocument(await api.create(date, format))
      await refreshIndex()
      error.value = ''
      return true
    } catch (cause) {
      reportError(cause)
      return false
    } finally { busy.value = false }
  }

  async function reload() {
    if (busy.value || saving.value || !workspace.value) return
    busy.value = true
    try {
      await refreshIndex()
      const entry = entries.value.find((entry) => entry.date === selectedDate.value && entry.format === document.value?.format) ?? entries.value.find((entry) => entry.date === selectedDate.value)
      applyDocument(entry ? await api.read(selectedDate.value, entry.format) : null)
      error.value = ''
    } catch (cause) { reportError(cause) }
    finally { busy.value = false }
  }

  async function refresh() {
    if (busy.value || !workspace.value) return
    if (await flush()) await reload()
  }

  async function switchFormat(format: FileFormat) {
    if (busy.value || !document.value) return false
    busy.value = true
    try {
      if (!(await flush())) return false
      const current = document.value!
      applyDocument(await api.switchFormat(current.date, current.format, format, current.revision))
      await refreshIndex()
      error.value = ''
      return true
    } catch (cause) { reportError(cause); return false }
    finally { busy.value = false }
  }

  function dispose() { clearTimeout(timer) }

  return { workspace, entries, selectedDate, document, draft, dirty, busy, saving, error, savedAt, edit, flush, select, open, create, switchFormat, reload, refresh, reportError, dispose }
}
