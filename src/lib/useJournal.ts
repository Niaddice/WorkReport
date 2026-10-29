import { computed, ref } from 'vue'
import {
  api,
  type Document,
  type Entry,
  type Workspace,
  type FileFormat,
  type GitAppendResult,
  type GitReportEntry,
} from './workspace'
import { dateKey } from './calendar'
import { DEFAULT_DATABASE_SETTINGS } from './database'

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
  const appendingGit = ref(false)
  const error = ref('')
  const backupWarning = ref('')
  const backupRetrying = ref(false)
  const savedAt = ref<Date | null>(null)
  const canUndo = ref(false)
  const canRedo = ref(false)
  const dirty = computed(() => document.value !== null && draft.value !== document.value.content)
  const undoStack: string[] = []
  const redoStack: string[] = []
  const HISTORY_LIMIT = 100
  let lastHistoryAt = 0
  let timer: ReturnType<typeof setTimeout> | undefined
  let pendingSave: Promise<boolean> | null = null
  let previewRevision = 0
  const pendingPreviews = new Map<string, object>()
  const previewVersions = new Map<string, number>()
  /** 新建日报使用的格式，由「数据存储」中的默认格式设置驱动。 */
  let defaultFormat: FileFormat = DEFAULT_DATABASE_SETTINGS.defaultFormat

  function setDefaultFormat(format: FileFormat) {
    defaultFormat = format === 'md' ? 'md' : 'txt'
  }

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
    try {
      backupWarning.value = (await api.backupWarning()) ?? ''
    } catch {
      /* Status cannot block reading the primary store. */
    }
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

  function updateHistoryFlags() {
    canUndo.value = undoStack.length > 0
    canRedo.value = redoStack.length > 0
  }

  function resetHistory() {
    undoStack.length = 0
    redoStack.length = 0
    lastHistoryAt = 0
    updateHistoryFlags()
  }

  function pushUndo(value: string) {
    if (undoStack[undoStack.length - 1] === value) return
    undoStack.push(value)
    if (undoStack.length > HISTORY_LIMIT) undoStack.shift()
    redoStack.length = 0
    updateHistoryFlags()
  }

  function applyDocument(value: Document | null, preserveHistory = false) {
    isNewDraft.value = false
    document.value = value
    if (value) backupWarning.value = value.backupWarning ?? ''
    draft.value = value?.content ?? ''
    savedAt.value = null
    if (!preserveHistory) resetHistory()
  }

  function applyDraft(date: string, format: FileFormat = defaultFormat) {
    isNewDraft.value = true
    document.value = emptyDocument(date, format)
    draft.value = ''
    savedAt.value = null
    resetHistory()
  }

  async function loadPreviews(dates: string[]) {
    if (!workspace.value) return
    const requested = [...new Set(dates)].filter(
      (date) => previews.value[date] === undefined && !pendingPreviews.has(date),
    )
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
    if (pendingSave) return pendingSave.then((success) => (success && dirty.value ? flush() : success))
    if (!document.value) return true
    if (isNewDraft.value && !draft.value.trim()) return true
    const task = (async () => {
      saving.value = true
      let lastSaved: Document | null = null
      try {
        if (isNewDraft.value && document.value) {
          const created = await api.create(document.value.date, document.value.format, draft.value)
          document.value = created
          backupWarning.value = created.backupWarning ?? ''
          isNewDraft.value = false
          savedAt.value = new Date()
          lastSaved = created
        }
        // Serialize writes and keep saving if typing continued during a write.
        while (document.value && dirty.value) {
          const snapshot = draft.value
          const saved = await api.save(document.value.date, snapshot, document.value.revision, document.value.format)
          document.value = saved
          backupWarning.value = saved.backupWarning ?? ''
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
    // Preview reads can outlive the write; include any edits made during them.
    return task.then((success) => (success && dirty.value ? flush() : success))
  }

  function scheduleSave() {
    clearTimeout(timer)
    // A failed save needs a deliberate retry; keep the draft intact.
    if (!error.value)
      timer = setTimeout(() => {
        void flush()
      }, 650)
  }

  function edit(content: string, forceHistory = false) {
    if (appendingGit.value || !document.value || content === draft.value) return
    const now = Date.now()
    if (forceHistory || now - lastHistoryAt > 800) {
      pushUndo(draft.value)
      lastHistoryAt = now
    }
    draft.value = content
    scheduleSave()
  }

  function undo(): boolean {
    if (appendingGit.value || !document.value || !undoStack.length) return false
    clearTimeout(timer)
    redoStack.push(draft.value)
    draft.value = undoStack.pop()!
    lastHistoryAt = 0
    updateHistoryFlags()
    scheduleSave()
    return true
  }

  function redo(): boolean {
    if (appendingGit.value || !document.value || !redoStack.length) return false
    clearTimeout(timer)
    undoStack.push(draft.value)
    if (undoStack.length > HISTORY_LIMIT) undoStack.shift()
    draft.value = redoStack.pop()!
    lastHistoryAt = 0
    updateHistoryFlags()
    scheduleSave()
    return true
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
    } finally {
      busy.value = false
    }
  }

  async function open(path: string, syncSettingsFrom?: string) {
    if (busy.value) return false
    busy.value = true
    try {
      if (!(await flush())) return false
      // 每次重试都重新报告失败，避免相同错误无法再次触发通知。
      error.value = ''
      const nextWorkspace = await api.open(path, syncSettingsFrom)
      workspace.value = nextWorkspace
      backupWarning.value = ''
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
    } finally {
      busy.value = false
    }
  }

  async function reload(preserveUnchangedHistory = false) {
    if (busy.value || saving.value || !workspace.value) return
    busy.value = true
    try {
      const format = isNewDraft.value && document.value ? document.value.format : defaultFormat
      await refreshIndex()
      const entry =
        entries.value.find((entry) => entry.date === selectedDate.value && entry.format === document.value?.format) ??
        entries.value.find((entry) => entry.date === selectedDate.value)
      if (entry) {
        const value = await api.read(selectedDate.value, entry.format)
        const unchanged =
          preserveUnchangedHistory &&
          !isNewDraft.value &&
          document.value?.date === value.date &&
          document.value.format === value.format &&
          document.value.content === value.content &&
          draft.value === value.content
        applyDocument(value, unchanged)
      } else {
        // Returning to the app after undoing a new draft to empty must keep redo.
        const unchangedDraft =
          preserveUnchangedHistory &&
          isNewDraft.value &&
          document.value?.date === selectedDate.value &&
          document.value.format === format &&
          draft.value === ''
        if (!unchangedDraft) applyDraft(selectedDate.value, format)
      }
      error.value = ''
    } catch (cause) {
      reportError(cause)
    } finally {
      busy.value = false
    }
  }

  async function refresh() {
    if (busy.value || !workspace.value) return
    // A focus refresh updates revisions and the index without erasing local
    // editing history when the same report's saved contents have not changed.
    if (await flush()) await reload(true)
  }

  /** Save the visible draft before appending other dates, keeping navigation locked until views are current. */
  async function appendGitReports(
    workspacePath: string,
    reports: GitReportEntry[],
    options: { isCurrent?: () => boolean; afterSave?: () => Promise<void> } = {},
  ): Promise<GitAppendResult | null> {
    if (busy.value || saving.value || workspace.value?.path !== workspacePath || !reports.length) return null
    const date = selectedDate.value
    const current = () =>
      workspace.value?.path === workspacePath && selectedDate.value === date && (options.isCurrent?.() ?? true)
    const requested = reports.map((report) => ({ ...report }))
    busy.value = true
    appendingGit.value = true
    try {
      if (!(await flush()) || !current()) return null
      const result = await api.appendGitReports(workspacePath, requested)
      if (!current()) return null

      const acceptDocument = (value: Document) => {
        if (value.date !== date) return
        if (draft.value !== value.content) pushUndo(draft.value)
        applyDocument(value, true)
        savedAt.value = new Date()
      }
      const changedDates = result.documents.map((value) => value.date)
      for (const value of result.documents) {
        upsertEntry(value)
        invalidatePreview(value.date)
        acceptDocument(value)
      }
      const backupWarnings = result.documents.flatMap((value) => (value.backupWarning ? [value.backupWarning] : []))
      let refreshError = ''
      try {
        const nextEntries = await api.list()
        if (!current()) return null
        clearPreviews()
        entries.value = nextEntries
        // A skipped date may have changed outside the app before the batch.
        if (result.skippedDates.includes(date)) {
          const entry = nextEntries.find((entry) => entry.date === date)
          if (entry) {
            const value = await api.read(date, entry.format)
            if (!current()) return null
            acceptDocument(value)
          }
        }
        const previewDates = [...new Set([...changedDates, ...result.skippedDates])]
        if (previewDates.length) {
          try {
            const nextPreviews = await api.previews(previewDates)
            if (!current()) return null
            for (const preview of nextPreviews) previews.value[preview.date] = preview.preview
          } catch {
            // Calendar snippets are optional; still refresh the selected journals.
          }
        }
        try {
          const warning = await api.backupWarning()
          if (!current()) return null
          backupWarning.value = warning ?? ''
        } catch {
          // Reading backup status cannot change the result of persisted writes.
        }
      } catch (cause) {
        refreshError = `Git 工作记录已处理，但刷新日报失败：${String(cause)}`
      }
      if (!current()) return null
      try {
        if (options.afterSave) await options.afterSave()
      } catch (cause) {
        refreshError = [refreshError, `Git 工作记录已处理，但刷新所选日报失败：${String(cause)}`]
          .filter(Boolean)
          .join('\n')
      }
      if (!current()) return null
      if (backupWarnings.length) backupWarning.value = [...new Set(backupWarnings)].join('\n')
      error.value = [...result.failed.map((failure) => `${failure.date}：${failure.error}`), refreshError]
        .filter(Boolean)
        .join('\n')
      // Refresh errors do not turn successful writes into failed appends.
      return result
    } catch (cause) {
      if (current()) reportError(cause)
      return null
    } finally {
      appendingGit.value = false
      busy.value = false
    }
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
      applyDocument(switched, true)
      upsertEntry(switched)
      error.value = ''
      return true
    } catch (cause) {
      reportError(cause)
      return false
    } finally {
      busy.value = false
    }
  }

  async function retryBackup(): Promise<boolean> {
    if (!workspace.value || backupRetrying.value || busy.value) return false
    const path = workspace.value.path
    backupRetrying.value = true
    try {
      if (!(await flush())) return false
      await api.retryBackup(path)
      if (workspace.value?.path !== path) return false
      backupWarning.value = ''
      if (document.value) document.value = { ...document.value, backupWarning: undefined }
      return true
    } catch (cause) {
      if (workspace.value?.path === path) backupWarning.value = `文件已保存，数据库备份未完成：${String(cause)}`
      return false
    } finally {
      backupRetrying.value = false
    }
  }

  function dispose() {
    clearTimeout(timer)
  }

  return {
    workspace,
    entries,
    previews,
    selectedDate,
    document,
    isNewDraft,
    draft,
    dirty,
    canUndo,
    canRedo,
    busy,
    saving,
    appendingGit,
    error,
    backupWarning,
    backupRetrying,
    retryBackup,
    savedAt,
    edit,
    undo,
    redo,
    flush,
    select,
    setDefaultFormat,
    open,
    switchFormat,
    reload,
    refresh,
    appendGitReports,
    loadPreviews,
    reportError,
    dispose,
  }
}
