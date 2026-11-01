import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { ExcelSheetPreviewData } from './excelPreview'

export interface WorkspaceDocument {
  path: string
  filename: string
  kind: 'text' | 'table' | 'unsupported'
  content: string
  revision: string
  encoding: string
  markdown: boolean
  journalDate: string | null
  sheets: ExcelSheetPreviewData[]
  message: string
}
export const workspaceDocumentApi = {
  read: (workspacePath: string, path: string) =>
    invoke<WorkspaceDocument>('read_workspace_document', { workspacePath, path }),
  save: (workspacePath: string, path: string, content: string, expectedRevision: string) =>
    invoke<WorkspaceDocument>('save_workspace_document', { workspacePath, path, content, expectedRevision }),
}
export function useWorkspaceDocument(getWorkspace: () => string | undefined) {
  const path = ref('')
  const document = ref<WorkspaceDocument | null>(null)
  const draft = ref('')
  const loading = ref(false)
  const saving = ref(false)
  const error = ref('')
  const dirty = computed(() => document.value?.kind === 'text' && draft.value !== document.value.content)
  const canUndo = ref(false),
    canRedo = ref(false)
  let undoStack: string[] = [],
    redoStack: string[] = []
  let lastEdit = 0,
    revision = 0,
    navigation = 0
  let workspacePath = ''
  let timer: ReturnType<typeof setTimeout> | undefined
  let pending: Promise<boolean> | undefined
  const flags = () => {
    canUndo.value = !!undoStack.length
    canRedo.value = !!redoStack.length
  }
  function clear() {
    revision++
    navigation++
    clearTimeout(timer)
    path.value = ''
    document.value = null
    draft.value = ''
    error.value = ''
    loading.value = false
    undoStack = []
    redoStack = []
    flags()
  }
  async function flush(): Promise<boolean> {
    clearTimeout(timer)
    if (pending) return pending.then((success) => (success && dirty.value ? flush() : success))
    if (!dirty.value || !document.value) return true
    const scope = revision,
      currentPath = path.value,
      currentWorkspace = workspacePath
    saving.value = true
    const task = (async () => {
      try {
        while (dirty.value && document.value && scope === revision) {
          const snapshot = draft.value
          const value = await workspaceDocumentApi.save(
            currentWorkspace,
            currentPath,
            snapshot,
            document.value.revision,
          )
          if (scope !== revision || getWorkspace() !== currentWorkspace) return false
          if (draft.value === snapshot) draft.value = value.content
          document.value = value
        }
        error.value = ''
        return scope === revision
      } catch (cause) {
        if (scope === revision) error.value = String(cause)
        return false
      } finally {
        saving.value = false
        pending = undefined
      }
    })()
    pending = task
    return task
  }
  async function open(relative: string, force = false): Promise<WorkspaceDocument | null> {
    const currentWorkspace = getWorkspace()
    if (!currentWorkspace) return null
    if (!force && path.value === relative && workspacePath === currentWorkspace && document.value && !error.value)
      return document.value
    // Reserve a navigation generation before saving, so the most recent click wins.
    const intent = ++navigation
    if (!(await flush()) || intent !== navigation || getWorkspace() !== currentWorkspace) return null
    const scope = ++revision
    workspacePath = currentWorkspace
    path.value = relative
    loading.value = true
    error.value = ''
    document.value = null
    draft.value = ''
    undoStack = []
    redoStack = []
    flags()
    lastEdit = 0
    try {
      const value = await workspaceDocumentApi.read(currentWorkspace, relative)
      if (scope !== revision || getWorkspace() !== currentWorkspace) return null
      document.value = value
      draft.value = value.content
      return value
    } catch (cause) {
      if (scope === revision) error.value = String(cause)
      return null
    } finally {
      if (scope === revision) loading.value = false
    }
  }
  function schedule() {
    clearTimeout(timer)
    if (!error.value) timer = setTimeout(() => void flush(), 650)
  }
  function edit(content: string, forceHistory = false) {
    if (document.value?.kind !== 'text' || content === draft.value) return
    if (forceHistory || Date.now() - lastEdit > 800) {
      undoStack.push(draft.value)
      if (undoStack.length > 100) undoStack.shift()
    }
    redoStack = []
    lastEdit = Date.now()
    draft.value = content
    flags()
    schedule()
  }
  function undo() {
    if (!undoStack.length) return
    redoStack.push(draft.value)
    draft.value = undoStack.pop()!
    lastEdit = 0
    flags()
    schedule()
  }
  function redo() {
    if (!redoStack.length) return
    undoStack.push(draft.value)
    draft.value = redoStack.pop()!
    lastEdit = 0
    flags()
    schedule()
  }
  async function reload(): Promise<WorkspaceDocument | null> {
    if (dirty.value || saving.value || !path.value || loading.value) return null
    const scope = revision,
      currentPath = path.value,
      currentWorkspace = workspacePath
    const original = document.value
    try {
      const value = await workspaceDocumentApi.read(currentWorkspace, currentPath)
      if (scope !== revision || dirty.value || getWorkspace() !== currentWorkspace) return null
      if (
        original?.kind !== value.kind ||
        original?.content !== value.content ||
        original?.markdown !== value.markdown
      ) {
        draft.value = value.content
        undoStack = []
        redoStack = []
        flags()
      }
      document.value = value
      error.value = ''
      return value
    } catch (cause) {
      if (scope === revision) error.value = String(cause)
      return null
    }
  }
  async function discardAndReload() {
    const current = path.value
    clearTimeout(timer)
    if (pending) await pending
    document.value = null
    if (current) await open(current, true)
  }
  function remap(from: string, to: string | null) {
    if (path.value !== from && !path.value.startsWith(`${from}/`)) return
    if (to === null) {
      clear()
      return
    }
    revision++
    navigation++
    loading.value = false
    const next = to + path.value.slice(from.length)
    path.value = next
    if (document.value) document.value = { ...document.value, path: next, filename: next.split('/').pop()! }
  }
  function dispose() {
    clearTimeout(timer)
    revision++
    navigation++
  }
  return {
    path,
    document,
    draft,
    dirty,
    loading,
    saving,
    error,
    canUndo,
    canRedo,
    open,
    flush,
    clear,
    edit,
    undo,
    redo,
    reload,
    discardAndReload,
    remap,
    dispose,
  }
}
