import { computed, ref } from 'vue'
import { api, type Document, type Entry, type Workspace, type FileFormat } from './workspace'
import { dateKey } from './calendar'

function emptyDocument(date: string, format: FileFormat): Document {
  return { date, filename: `${date}.${format}`, format, content: '', revision: '' }
}

export function useJournal() {
  const workspace = ref<Workspace | null>(null)
  const entries = ref<Entry[]>([])
  const previews = ref<Record<string, string>>({})
  const selectedDate = ref(dateKey())
  const document = ref<Document | null>(null)
  const isNewDraft = ref(false)
  const draft = ref('')
  const busy = ref(false)
  const saving = ref(false)
  const error = ref('')
  const savedAt = ref<Date | null>(null)
  const dirty = computed(() => document.value !== null && draft.value !== document.value.content)
  let timer: ReturnType<typeof setTimeout> | undefined
  let pendingSave: Promise<boolean> | null = null
  let previewRevision = 0
  const pendingPreviews = new Map<string, object>()
  const previewVersions = new Map<string, number>()

  function reportError(cause: unknown) {
    error.value = String(cause)
  }

  function clearPreviews() {
    previewRevision++
    previews.value = {}
    pendingPreviews.clear()
    previewVersions.clear()
  }

  async function refreshIndex() {
    clearPreviews()
    entries.value = await api.list()
  }

  function invalidatePreview(date: string) {
    previewVersions.set(date, (previewVersions.get(date) ?? 0) + 1)
    delete previews.value[date]
    pendingPreviews.delete(date)
  }

  function upsertEntry(value: Document) {
    const next = entries.value.filter((entry) => entry.date !== value.date)
    next.push({ date: value.date, filename: value.filename, format: value.format })
    next.sort((left, right) => left.date.localeCompare(right.date))
    entries.value = next
  }

  function applyDocument(value: Document | null) {
    isNewDraft.value = false
    document.value = value
    draft.value = value?.content ?? ''
    savedAt.value = null
  }

  function applyDraft(date: string, format: FileFormat = 'md') {
    isNewDraft.value = true
    document.value = emptyDocument(date, format)
    draft.value = ''
    savedAt.value = null
  }

  async function loadPreviews(dates: string[]) {
    if (!workspace.value) return
    const requested = [...new Set(dates)].filter((date) => previews.value[date] === undefined && !pendingPreviews.has(date))
    if (!requested.length) return
    const revision = previewRevision
    const versions = new Map(requested.map((date) => [date, previewVersions.get(date) ?? 0]))
    const tokens = new Map<string, object>()
    for (const date of requested) {
      const token = {}
      tokens.set(date, token)
      pendingPreviews.set(date, token)
    }
    try {
      const result = await api.previews(requested)
      if (revision !== previewRevision) return
      for (const preview of result) {
        if ((previewVersions.get(preview.date) ?? 0) !== versions.get(preview.date)) continue
        previews.value[preview.date] = preview.preview
      }
    } catch {
      // 预览是可选信息，失败不应阻塞编辑或自动保存。
    } finally {
      for (const date of requested) {
        if (pendingPreviews.get(date) === tokens.get(date)) pendingPreviews.delete(date)
      }
    }
  }

  async function flush(): Promise<boolean> {
    clearTimeout(timer)
    if (pendingSave) return pendingSave
    if (!document.value) return true
    if (isNewDraft.value && !draft.value.trim()) return true
    const task = (async () => {
      saving.value = true
      let lastSaved: Document | null = null
      try {
        if (isNewDraft.value && document.value) {
          const created = await api.create(document.value.date, document.value.format, draft.value)
          document.value = created
          isNewDraft.value = false
          savedAt.value = new Date()
          lastSaved = created
        }
        // Serialize writes and keep saving if typing continued during a write.
        while (document.value && dirty.value) {
          const snapshot = draft.value
          const saved = await api.save(document.value.date, snapshot, document.value.revision, document.value.format)
          document.value = saved
          savedAt.value = new Date()
          lastSaved = saved
        }
        error.value = ''
        if (lastSaved) {
          upsertEntry(lastSaved)
          invalidatePreview(lastSaved.date)
          await loadPreviews([lastSaved.date])
        }
        return true
      } catch (cause) {
        reportError(cause)
        return false
      } finally {
        saving.value = false
      }
    })()
    pendingSave = task
    void task.finally(() => {
      if (pendingSave === task) {
        pendingSave = null
        saving.value = false
      }
    })
    return task
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
      if (entry) applyDocument(await api.read(date, entry.format))
      else applyDraft(date)
      selectedDate.value = date
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
      const entry = entries.value.find((entry) => entry.date === selectedDate.value)
      if (entry) applyDocument(await api.read(selectedDate.value, entry.format))
      else applyDraft(selectedDate.value)
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
      const format = isNewDraft.value && document.value ? document.value.format : 'md'
      await refreshIndex()
      const entry = entries.value.find((entry) => entry.date === selectedDate.value && entry.format === document.value?.format) ?? entries.value.find((entry) => entry.date === selectedDate.value)
      if (entry) applyDocument(await api.read(selectedDate.value, entry.format))
      else applyDraft(selectedDate.value, format)
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
      if (isNewDraft.value && document.value) {
        document.value = { ...document.value, format, filename: `${document.value.date}.${format}` }
        error.value = ''
        return true
      }
      const current = document.value!
      const switched = await api.switchFormat(current.date, current.format, format, current.revision)
      applyDocument(switched)
      upsertEntry(switched)
      error.value = ''
      return true
    } catch (cause) { reportError(cause); return false }
    finally { busy.value = false }
  }

  function dispose() { clearTimeout(timer) }

  return { workspace, entries, previews, selectedDate, document, isNewDraft, draft, dirty, busy, saving, error, savedAt, edit, flush, select, open, switchFormat, reload, refresh, loadPreviews, reportError, dispose }
}
