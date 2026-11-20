<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, useId, watch } from 'vue'
import {
  PhArrowClockwise,
  PhArrowsIn,
  PhArrowsOut,
  PhCaretDown,
  PhCaretRight,
  PhCheck,
  PhCircleNotch,
  PhFileText,
  PhFolder,
  PhFolderOpen,
  PhMicrosoftExcelLogo,
  PhTreeStructure,
  PhFilePlus,
  PhFolderPlus,
  PhTrashSimple,
} from '@phosphor-icons/vue'
import ContextMenu from '../ContextMenu.vue'
import DirectoryActionsPanel from '../DirectoryActionsPanel.vue'
import type { ContextMenuItem } from '../../lib/contextMenus'
import {
  workspaceFilesApi,
  fileNameError,
  type FileAction,
  type FilePlan,
  type FileRequest,
  type TrashEntry,
} from '../../lib/workspaceFiles'
import { api, type FileNode } from '../../lib/workspace'
import { useModuleContext } from '../../modules/context'
import { pushMessage } from '../../lib/messages'

const context = useModuleContext()
const { journal } = context
const { selectedDate } = journal

const operationPending = ref(false)
const rootElement = ref<HTMLElement>()
const selectedPath = ref('')
const creationParent = computed(() => {
  const selected = nodes.value.find((node) => node.path === selectedPath.value)
  return selected ? (selected.isDir ? selected.path : parentPath(selected.path)) : ''
})
const busy = computed(() => journal.busy.value || context.disabled.value || operationPending.value)
const newNode = ref<{
  action: 'createFile' | 'createDirectory' | 'rename'
  parent: string
  path: string
  workspacePath: string
  name: string
  error: string
  isDir: boolean
  journal: boolean
} | null>(null)
const newNodeInputs = ref<HTMLInputElement[]>([])
const newNodeHintId = useId()
const newNodeErrorId = useId()
let newNodeRevision = 0
let blurTimer: ReturnType<typeof setTimeout> | undefined

interface TreeNode {
  name: string
  path: string
  isDir: boolean
  size: number
  journalDate?: string | null
  protected?: boolean
  children: TreeNode[]
}

const nodes = ref<FileNode[]>([])
const loading = ref(false)
const error = ref('')
let loadRevision = 0

async function load() {
  const revision = ++loadRevision
  if (!journal.workspace.value) {
    loading.value = false
    return
  }
  loading.value = true
  error.value = ''
  try {
    const list = await api.listWorkspaceFiles()
    if (revision !== loadRevision) return
    nodes.value = list
  } catch (cause) {
    if (revision === loadRevision) error.value = `读取目录失败：${String(cause)}`
  } finally {
    if (revision === loadRevision) loading.value = false
  }
}

// 切换工作空间或日报增删 / 格式变化时重新读取目录。
watch([() => journal.workspace.value?.path, () => journal.entries.value], load, { immediate: true })
watch(
  () => context.workspaceDocument?.saving.value,
  (saving, wasSaving) => {
    if (wasSaving && !saving) void load()
  },
)

/** 后端按深度优先返回（父目录先于子节点），按 path 逐层挂载成树。 */
function buildTree(list: FileNode[]): TreeNode[] {
  const roots: TreeNode[] = []
  const byPath = new Map<string, TreeNode>()
  for (const node of list) {
    const item: TreeNode = { ...node, children: [] }
    const slash = node.path.lastIndexOf('/')
    const parent = slash >= 0 ? byPath.get(node.path.slice(0, slash)) : undefined
    if (parent) parent.children.push(item)
    else roots.push(item)
    if (node.isDir) byPath.set(node.path, item)
  }
  return roots
}

function sortTree(list: TreeNode[]): TreeNode[] {
  list.sort((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1
    return a.name.localeCompare(b.name, 'zh-Hans-CN', { numeric: true, sensitivity: 'base' })
  })
  for (const item of list) sortTree(item.children)
  return list
}

const tree = computed(() => sortTree(buildTree(nodes.value)))

/** 折叠状态按目录 path 记录，默认全部展开。 */
const collapsed = ref(new Set<string>())
function isCollapsed(path: string) {
  return collapsed.value.has(path)
}
function toggle(path: string) {
  selectedPath.value = path
  if (collapsed.value.has(path)) collapsed.value.delete(path)
  else {
    if (newNode.value && (newNode.value.parent === path || newNode.value.parent.startsWith(`${path}/`))) cancelNewNode()
    collapsed.value.add(path)
  }
}
const allExpanded = computed(() => collapsed.value.size === 0)
function toggleAll() {
  if (collapsed.value.size) {
    collapsed.value = new Set()
    return
  }
  const next = new Set<string>()
  if (newNode.value?.parent) cancelNewNode()
  const walk = (list: TreeNode[]) => {
    for (const node of list) {
      if (!node.isDir) continue
      next.add(node.path)
      walk(node.children)
    }
  }
  walk(tree.value)
  collapsed.value = next
}

/** 扁平化可见行：只展开未被折叠的目录，附带层级深度。 */
type Row = { kind: 'node'; node: TreeNode; depth: number } | { kind: 'create'; depth: number }
const rows = computed<Row[]>(() => {
  const result: Row[] = []
  if (newNode.value?.parent === '' && newNode.value.action !== 'rename') result.push({ kind: 'create', depth: 0 })
  const walk = (list: TreeNode[], depth: number) => {
    for (const node of list) {
      result.push(
        newNode.value?.action === 'rename' && newNode.value.path === node.path
          ? { kind: 'create', depth }
          : { kind: 'node', node, depth },
      )
      if (node.isDir && !collapsed.value.has(node.path)) {
        if (newNode.value?.parent === node.path && newNode.value.action !== 'rename')
          result.push({ kind: 'create', depth: depth + 1 })
        walk(node.children, depth + 1)
      }
    }
  }
  walk(tree.value, 0)
  return result
})

const fileCount = computed(() => nodes.value.filter((node) => !node.isDir).length)

/** 日期来自后端文件索引，重命名后仍按原日期打开日报。 */
const reportPattern = /^(\d{4})\/(\d{8})\.(md|txt)$/
function reportDate(node: TreeNode): string | null {
  if (node.isDir) return null
  if ('journalDate' in node) return node.journalDate ?? null
  // 兼容尚未提供文件身份信息的旧版接口。
  const match = reportPattern.exec(node.path)
  return match ? match[2] : null
}
function isCurrent(node: TreeNode) {
  const path = context.workspaceDocument?.path.value
  return path ? path === node.path : reportDate(node) === selectedDate.value
}
function isExcel(node: TreeNode) {
  return !node.isDir && /\.(?:xlsx|xlsm|xls|xlsb|csv|tsv)$/i.test(node.name)
}
function onFileClick(node: TreeNode) {
  if (busy.value) return
  selectedPath.value = node.path
  const date = reportDate(node)
  if (date) {
    if (date === selectedDate.value && !context.workspaceDocument?.path.value && !context.rangeDates?.value.length)
      return
    void context.actions.selectReport(date)
  } else void context.actions.openWorkspaceFile?.(node.path)
}

const menu = ref<{ node: TreeNode | null; x: number; y: number; anchor: HTMLElement } | null>(null)
const panelAction = ref<'delete' | 'trash' | null>(null)
const panelPath = ref('')
const panelLoading = ref(false)
const panelError = ref('')
const panel = ref<InstanceType<typeof DirectoryActionsPanel>>()
const panelId = useId()
const plan = ref<FilePlan | null>(null)
const trashEntries = ref<TrashEntry[]>([])
let panelRevision = 0
let panelWorkspace = ''
function parentPath(path: string) {
  return path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : ''
}
function expandParents(path: string) {
  const parts = path.split('/').slice(0, -1)
  for (let index = 1; index <= parts.length; index++) collapsed.value.delete(parts.slice(0, index).join('/'))
}
async function focusRow(path: string) {
  await nextTick()
  const row = [...(rootElement.value?.querySelectorAll<HTMLElement>('[data-file-path]') ?? [])].find(
    (row) => row.dataset.filePath === path,
  )
  ;(row ?? rootElement.value)?.focus({ preventScroll: true })
}
async function focusNewNode(select = false) {
  const revision = newNodeRevision
  await nextTick()
  if (revision !== newNodeRevision || !newNode.value) return
  const input = newNodeInputs.value[0]
  input?.focus()
  if (select && input) {
    const lastDot = input.value.lastIndexOf('.')
    input.setSelectionRange(
      0,
      newNode.value.action === 'rename' && !newNode.value.isDir && lastDot > 0 ? lastDot : input.value.length,
    )
  }
  input?.scrollIntoView?.({ block: 'nearest' })
}
function cancelNewNode(restoreFocus = false) {
  if (operationPending.value) return
  const parent = newNode.value?.action === 'rename' ? newNode.value.path : (newNode.value?.parent ?? '')
  clearTimeout(blurTimer)
  newNodeRevision++
  newNode.value = null
  if (restoreFocus) void focusRow(parent)
}
async function startNewNode(action: 'createFile' | 'createDirectory' | 'rename', parent: string, node?: TreeNode) {
  newNodeRevision++
  newNode.value = {
    action,
    parent,
    path: node?.path ?? '',
    workspacePath: journal.workspace.value!.path,
    name: node?.name ?? '',
    error: '',
    isDir: node?.isDir ?? action === 'createDirectory',
    journal: !!node && !!reportDate(node),
  }
  expandParents(`${parent}/`)
  await focusNewNode(true)
}
function validateNewNode() {
  const draft = newNode.value
  if (!draft) return ''
  const validation = fileNameError(draft.name)
  if (validation) return validation
  if (draft.journal && !/\.(md|txt)$/.test(draft.name.trim())) return '日报名称需保留 .md 或 .txt 扩展名。'
  const path = `${draft.parent ? `${draft.parent}/` : ''}${draft.name.trim()}`
  return nodes.value.some((node) => node.path === path && node.path !== draft.path)
    ? '同名文件或目录已存在，请使用其他名称。'
    : ''
}
function onNewNodeInput() {
  if (newNode.value) newNode.value.error = newNode.value.name ? validateNewNode() : ''
}
function onNewNodeBlur(event: FocusEvent) {
  const input = event.target as HTMLInputElement
  const revision = newNodeRevision
  clearTimeout(blurTimer)
  // Wait for DOM focus to settle. App switching and OS input-method popups keep the input active.
  blurTimer = setTimeout(() => {
    if (revision !== newNodeRevision || !newNode.value || busy.value || document.activeElement === input) return
    if (!newNode.value.name.trim() || validateNewNode() || newNode.value.error) cancelNewNode()
    else void createNode(true)
  }, 0)
}
function onNewNodeKeydown(event: KeyboardEvent) {
  if (event.isComposing || event.keyCode === 229) return
  if (
    event.key === 'F2' &&
    newNode.value?.action === 'rename' &&
    !newNode.value.isDir &&
    event.target instanceof HTMLInputElement
  ) {
    event.preventDefault()
    const input = event.target
    const dot = input.value.lastIndexOf('.')
    if (dot > 0) {
      if (input.selectionStart === 0 && input.selectionEnd === dot) input.select()
      else if (input.selectionStart === 0 && input.selectionEnd === input.value.length)
        input.setSelectionRange(dot + 1, input.value.length)
      else input.setSelectionRange(0, dot)
    }
  } else if (event.key === 'Escape') {
    event.preventDefault()
    cancelNewNode(true)
  } else if (event.key === 'Enter' && event.target instanceof HTMLInputElement) {
    event.preventDefault()
    void createNode()
  }
}
async function createNode(fromBlur = false) {
  const draft = newNode.value
  if (!draft || busy.value) return
  clearTimeout(blurTimer)
  draft.error = validateNewNode()
  if (draft.error) {
    void focusNewNode()
    return
  }
  if (draft.action === 'rename' && draft.name.trim() === nodes.value.find((node) => node.path === draft.path)?.name) {
    cancelNewNode(!fromBlur)
    return
  }
  const revision = newNodeRevision
  operationPending.value = true
  try {
    const result = await performChange(
      { action: draft.action, path: draft.action === 'rename' ? draft.path : draft.parent, name: draft.name.trim() },
      undefined,
      draft.workspacePath,
    )
    if (revision !== newNodeRevision) return
    if (draft.action === 'rename' && draft.isDir) {
      collapsed.value = new Set(
        [...collapsed.value].map((path) =>
          path === draft.path || path.startsWith(`${draft.path}/`) ? result.path + path.slice(draft.path.length) : path,
        ),
      )
    }
    expandParents(result.path)
    await load()
    if (revision !== newNodeRevision) return
    newNode.value = null
    selectedPath.value = result.path
    operationPending.value = false
    if (draft.action === 'createFile') {
      const node = nodes.value.find((node) => node.path === result.path)
      if (node?.journalDate) await context.actions.selectReport(node.journalDate)
      else await context.actions.openWorkspaceFile?.(result.path)
      if (revision === newNodeRevision) {
        await nextTick()
        context.editor?.value?.focus()
      }
    } else if (!fromBlur) await focusRow(result.path)
  } catch (cause) {
    if (revision === newNodeRevision) draft.error = String(cause)
  } finally {
    operationPending.value = false
    if (revision === newNodeRevision && newNode.value && !fromBlur) void focusNewNode()
  }
}
function onTreePointerDown(event: PointerEvent) {
  const target = event.target
  if (
    !(target instanceof Element) ||
    target.closest('[data-file-path], .directory-new-node, .directory-module-actions, .directory-actions-panel')
  )
    return
  selectedPath.value = ''
  rootElement.value?.focus({ preventScroll: true })
}
function closeMenu(restoreFocus = false) {
  const anchor = menu.value?.anchor
  menu.value = null
  if (restoreFocus && anchor?.isConnected) anchor.focus({ preventScroll: true })
}
function openMenu(event: MouseEvent | KeyboardEvent, node: TreeNode | null = null) {
  if (event instanceof KeyboardEvent && event.key !== 'ContextMenu' && !(event.shiftKey && event.key === 'F10')) return
  event.preventDefault()
  event.stopPropagation()
  if (busy.value || !journal.workspace.value) return
  selectedPath.value = node?.path ?? ''
  const anchor = event.currentTarget as HTMLElement
  const rect = anchor.getBoundingClientRect()
  const mouse = event instanceof MouseEvent && (event.clientX || event.clientY)
  menu.value = { node, anchor, x: mouse ? event.clientX : rect.left, y: mouse ? event.clientY : rect.bottom }
}
const menuItems = computed<ContextMenuItem[]>(() => {
  const node = menu.value?.node
  const parent = node ? (node.isDir ? node.path : parentPath(node.path)) : ''
  const items: ContextMenuItem[] = [
    { id: 'createFile', label: '新建文件…', run: () => beginAction('createFile', parent) },
    { id: 'createDirectory', label: '新建文件夹…', run: () => beginAction('createDirectory', parent) },
  ]
  if (node) {
    items.push(
      {
        id: 'open',
        label: node.isDir ? '打开文件夹' : '打开',
        run: () => (node.isDir ? openItem(node.path) : onFileClick(node)),
      },
      {
        id: 'rename',
        label: node.protected ? '重命名（数据库正在使用）' : '重命名…',
        disabled: !!node.protected,
        run: () => beginAction('rename', node.path, node),
      },
      {
        id: 'delete',
        label: node.protected ? '删除（数据库正在使用）' : '删除…',
        disabled: !!node.protected,
        danger: true,
        run: () => beginAction('delete', node.path, node),
      },
      { id: 'copy', label: '复制相对路径', run: () => copyPath(node.path) },
      { id: 'parent', label: '打开所在文件夹', run: () => openItem(node.path, true) },
    )
  } else items.push({ id: 'open', label: '打开工作空间文件夹', run: () => openItem('') })
  items.push(
    { id: 'trash', label: '回收站…', run: () => beginAction('trash', '') },
    { id: 'refresh', label: '刷新目录', run: load },
  )
  return items
})
function chooseMenu(id: string) {
  const item = menuItems.value.find((item) => item.id === id)
  if (!item || item.disabled || busy.value) return
  closeMenu(true)
  void item.run()
}
function onRowKeydown(event: KeyboardEvent, node: TreeNode) {
  if (['ArrowDown', 'ArrowUp', 'Home', 'End', 'ArrowRight', 'ArrowLeft'].includes(event.key)) {
    if (busy.value) return
    event.preventDefault()
    event.stopPropagation()
    const visible = rows.value.flatMap((row) => (row.kind === 'node' ? [row.node] : []))
    const index = visible.findIndex((item) => item.path === node.path)
    let target = node.path
    if (event.key === 'Home') target = visible[0]?.path ?? target
    else if (event.key === 'End') target = visible.at(-1)?.path ?? target
    else if (event.key === 'ArrowDown') target = visible[index + 1]?.path ?? target
    else if (event.key === 'ArrowUp') target = visible[index - 1]?.path ?? target
    else if (event.key === 'ArrowRight' && node.isDir) {
      if (isCollapsed(node.path)) collapsed.value.delete(node.path)
      else target = visible[index + 1]?.path.startsWith(`${node.path}/`) ? visible[index + 1]!.path : target
    } else if (event.key === 'ArrowLeft') {
      if (node.isDir && !isCollapsed(node.path)) collapsed.value.add(node.path)
      else target = parentPath(node.path) || target
    }
    selectedPath.value = target
    void focusRow(target)
    return
  }
  if (event.key === 'F2' || event.key === 'Delete') {
    if (busy.value || node.protected) return
    event.preventDefault()
    event.stopPropagation()
    void beginAction(event.key === 'F2' ? 'rename' : 'delete', node.path, node)
  } else openMenu(event, node)
}
async function copyPath(path: string) {
  try {
    await api.copy(path)
    pushMessage({ tone: 'success', text: '相对路径已复制' })
  } catch (cause) {
    error.value = `复制路径失败：${String(cause)}`
  }
}
async function openItem(path: string, parent = false) {
  const workspacePath = journal.workspace.value?.path
  if (!workspacePath || busy.value) return
  try {
    await workspaceFilesApi.open(workspacePath, path, parent)
  } catch (cause) {
    if (journal.workspace.value?.path === workspacePath) error.value = String(cause)
  }
}

async function beginAction(action: Exclude<FileAction, 'restore'> | 'trash', path: string, node?: TreeNode) {
  if (busy.value || !journal.workspace.value) return
  if ((action === 'rename' || action === 'delete') && node?.protected) return
  const startingWorkspace = journal.workspace.value.path
  closeMenu()
  if (newNode.value?.name.trim() && !validateNewNode() && !newNode.value.error) {
    const previousName = newNode.value
    await createNode(true)
    if (newNode.value || busy.value || journal.workspace.value?.path !== startingWorkspace) return
    if (
      previousName.action === 'rename' &&
      previousName.isDir &&
      (path === previousName.path || path.startsWith(`${previousName.path}/`))
    ) {
      path = selectedPath.value + path.slice(previousName.path.length)
      if (node) node = { ...node, path, name: path.split('/').pop()! }
    }
  } else cancelNewNode()
  closePanel(false)
  if (action === 'createFile' || action === 'createDirectory' || action === 'rename') {
    await startNewNode(action, action === 'rename' ? parentPath(path) : path, node)
    return
  }
  const revision = ++panelRevision
  panelWorkspace = startingWorkspace
  panelAction.value = action
  panelPath.value = path
  panelError.value = ''
  plan.value = null
  trashEntries.value = []
  panelLoading.value = true
  await nextTick()
  if (revision !== panelRevision) return
  panel.value?.focus()
  try {
    if (action === 'delete') {
      if (!(await journal.flush())) throw new Error('请先保存当前内容，再删除文件。')
      if (revision !== panelRevision) return
      const value = await workspaceFilesApi.preview(startingWorkspace, { action, path })
      if (revision === panelRevision) plan.value = value
    } else if (action === 'trash') {
      const value = await workspaceFilesApi.trash(startingWorkspace)
      if (revision === panelRevision) trashEntries.value = value
    }
  } catch (cause) {
    if (revision === panelRevision) panelError.value = String(cause)
  } finally {
    if (revision === panelRevision) panelLoading.value = false
  }
}
function closePanel(restoreFocus = true) {
  if (operationPending.value) return
  const path = panelAction.value === 'delete' ? panelPath.value : selectedPath.value
  panelRevision++
  panelAction.value = null
  panelLoading.value = false
  panelError.value = ''
  if (restoreFocus) void focusRow(path)
}
async function performChange(request: FileRequest, token?: string, workspacePath = panelWorkspace) {
  if (!context.actions.changeWorkspaceFiles) throw new Error('文件操作暂不可用，请重新打开工作空间。')
  return context.actions.changeWorkspaceFiles(workspacePath, request, token)
}
async function confirmDelete() {
  if (busy.value || panelLoading.value || panelAction.value !== 'delete' || !plan.value) return
  const revision = panelRevision
  const request: FileRequest = { action: 'delete', path: panelPath.value }
  operationPending.value = true
  panelError.value = ''
  try {
    await performChange(request, plan.value.token)
    if (revision !== panelRevision) return
    await load()
    if (revision !== panelRevision) return
    selectedPath.value = parentPath(request.path)
    operationPending.value = false
    closePanel(false)
    pushMessage({ tone: 'info', text: '已移入工作空间回收站，可通过回收站恢复。' })
    await focusRow(selectedPath.value)
  } catch (cause) {
    if (revision === panelRevision) {
      panelError.value = String(cause)
      plan.value = null
      try {
        const nextPlan = await workspaceFilesApi.preview(panelWorkspace, request)
        if (revision === panelRevision) plan.value = nextPlan
      } catch {
        /* Keep the operation error visible; retry refreshes the snapshot. */
      }
    }
  } finally {
    operationPending.value = false
  }
}
async function restore(entry: TrashEntry) {
  if (busy.value || panelLoading.value || panelAction.value !== 'trash') return
  const revision = panelRevision
  operationPending.value = true
  panelError.value = ''
  try {
    await performChange({ action: 'restore', path: '', trashId: entry.id })
    if (revision !== panelRevision) return
    await load()
    if (revision !== panelRevision) return
    const entries = await workspaceFilesApi.trash(panelWorkspace)
    if (revision !== panelRevision) return
    trashEntries.value = entries
    pushMessage({ tone: 'success', text: `已恢复 ${entry.name}` })
    panel.value?.focus()
  } catch (cause) {
    if (revision === panelRevision) panelError.value = String(cause)
  } finally {
    operationPending.value = false
  }
}
watch(busy, (value) => {
  if (value) closeMenu()
})
watch(
  () => journal.workspace.value?.path,
  () => {
    clearTimeout(blurTimer)
    selectedPath.value = ''
    newNodeRevision++
    newNode.value = null
    panelRevision++
    menu.value = null
    panelAction.value = null
    panelLoading.value = false
    nodes.value = []
    collapsed.value.clear()
  },
)
onBeforeUnmount(() => {
  clearTimeout(blurTimer)
  newNodeRevision++
  loadRevision++
  panelRevision++
})

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}
</script>

<template>
  <div
    ref="rootElement"
    class="directory-module"
    :aria-busy="busy || loading"
    tabindex="0"
    @pointerdown="onTreePointerDown"
    @contextmenu="openMenu($event)"
    @keydown.self="openMenu($event)"
  >
    <header class="directory-module-toolbar">
      <span class="directory-module-count">
        <PhTreeStructure :size="15" aria-hidden="true" />{{ fileCount }} 个文件
      </span>
      <span class="directory-module-actions">
        <button
          type="button"
          class="text-action directory-module-refresh"
          aria-label="新建文件"
          title="新建文件"
          :disabled="busy"
          @pointerdown.prevent
          @click.stop="beginAction('createFile', creationParent)"
        >
          <PhFilePlus :size="15" aria-hidden="true" />
        </button>
        <button
          type="button"
          class="text-action directory-module-refresh"
          aria-label="新建文件夹"
          title="新建文件夹"
          :disabled="busy"
          @pointerdown.prevent
          @click.stop="beginAction('createDirectory', creationParent)"
        >
          <PhFolderPlus :size="15" aria-hidden="true" />
        </button>
        <button
          type="button"
          class="text-action directory-module-refresh"
          aria-label="工作空间回收站"
          title="工作空间回收站"
          :aria-expanded="panelAction === 'trash'"
          :aria-controls="panelAction === 'trash' ? panelId : undefined"
          :disabled="busy"
          @click="panelAction === 'trash' ? closePanel() : beginAction('trash', '')"
        >
          <PhTrashSimple :size="15" aria-hidden="true" />
        </button>
        <button
          type="button"
          class="text-action directory-module-refresh"
          aria-label="刷新目录"
          title="刷新目录"
          :disabled="loading"
          @click="load"
        >
          <PhCircleNotch v-if="loading" class="spin" :size="14" aria-hidden="true" />
          <PhArrowClockwise v-else :size="14" aria-hidden="true" />
        </button>
        <button
          v-if="nodes.length"
          type="button"
          class="text-action directory-module-toggle"
          :aria-label="allExpanded ? '全部折叠' : '全部展开'"
          :title="allExpanded ? '全部折叠' : '全部展开'"
          @click="toggleAll"
        >
          <PhArrowsIn v-if="allExpanded" :size="14" aria-hidden="true" />
          <PhArrowsOut v-else :size="14" aria-hidden="true" />
        </button>
      </span>
    </header>

    <p v-if="error" class="directory-empty error-text" role="alert">{{ error }}</p>
    <p v-else-if="loading && !nodes.length" class="directory-empty" role="status">正在读取目录…</p>
    <p v-else-if="!nodes.length && !newNode" class="directory-empty">这个工作空间还没有文件。</p>
    <div v-if="nodes.length || newNode" class="directory-module-tree" role="tree" aria-label="工作空间文件">
      <template v-for="row in rows" :key="row.kind === 'create' ? 'new-node' : row.node.path">
        <div
          v-if="row.kind === 'create' && newNode"
          class="directory-new-node"
          :class="{ 'directory-rename-node': newNode.action === 'rename' }"
          role="treeitem"
          :aria-level="row.depth + 1"
          :aria-label="
            newNode.action === 'rename'
              ? newNode.isDir
                ? '重命名文件夹'
                : '重命名文件'
              : newNode.isDir
                ? '新建文件夹'
                : '新建文件'
          "
          :data-file-path="newNode.action === 'rename' ? newNode.path : undefined"
          :style="{ paddingLeft: `${6 + row.depth * 14}px` }"
          @contextmenu.prevent.stop
          @keydown.stop="onNewNodeKeydown"
        >
          <form class="directory-new-node-form" @submit.prevent="createNode()">
            <PhFolder v-if="newNode.isDir" class="directory-tree-folder" :size="15" aria-hidden="true" />
            <PhFileText v-else class="directory-item-file" :size="15" aria-hidden="true" />
            <input
              ref="newNodeInputs"
              v-model="newNode.name"
              class="directory-new-node-name"
              type="text"
              maxlength="120"
              autocomplete="off"
              spellcheck="false"
              :aria-label="newNode.isDir ? '文件夹名称' : '文件名称（含扩展名）'"
              :aria-invalid="!!newNode.error"
              :aria-describedby="newNode.error ? `${newNodeHintId} ${newNodeErrorId}` : newNodeHintId"
              :disabled="busy"
              @input="onNewNodeInput"
              @blur="onNewNodeBlur"
            />
            <PhCircleNotch
              v-if="operationPending"
              class="spin"
              :size="12"
              :aria-label="newNode.action === 'rename' ? '正在重命名' : '正在创建'"
            />
          </form>
          <span :id="newNodeHintId" class="sr-only">输入名称后按 Enter 确认，按 Esc 取消。</span>
          <p v-if="newNode.action === 'rename' && newNode.journal" class="directory-name-note">
            重命名后仍关联原日报日期，请保留 .md 或 .txt 扩展名。
          </p>
          <p v-else-if="newNode.action === 'rename' && newNode.isDir" class="directory-name-note">
            文件夹中的内容一起移动，关联的功能保存目录同步更新。
          </p>
          <p v-if="newNode.error" :id="newNodeErrorId" class="directory-new-node-error" role="alert">
            {{ newNode.error }}
          </p>
        </div>
        <button
          v-else-if="row.kind === 'node' && row.node.isDir"
          type="button"
          class="directory-tree-node"
          :class="{ 'is-selected': selectedPath === row.node.path }"
          role="treeitem"
          :aria-expanded="!isCollapsed(row.node.path)"
          :aria-level="row.depth + 1"
          :aria-selected="selectedPath === row.node.path"
          :style="{ paddingLeft: `${6 + row.depth * 14}px` }"
          :data-file-path="row.node.path"
          aria-haspopup="menu"
          :disabled="busy"
          @contextmenu.stop="openMenu($event, row.node)"
          @keydown="onRowKeydown($event, row.node)"
          @focus="selectedPath = row.node.path"
          @click="toggle(row.node.path)"
        >
          <PhCaretDown v-if="!isCollapsed(row.node.path)" class="directory-tree-caret" :size="13" aria-hidden="true" />
          <PhCaretRight v-else class="directory-tree-caret" :size="13" aria-hidden="true" />
          <PhFolderOpen
            v-if="!isCollapsed(row.node.path)"
            class="directory-tree-folder"
            :size="15"
            aria-hidden="true"
          />
          <PhFolder v-else class="directory-tree-folder" :size="15" aria-hidden="true" />
          <span class="directory-tree-label">{{ row.node.name }}</span>
        </button>
        <button
          v-else-if="row.kind === 'node'"
          type="button"
          class="directory-item"
          :class="{
            'is-current': isCurrent(row.node),
            'is-selected': selectedPath === row.node.path,
            'is-report': !!reportDate(row.node),
            'is-excel': isExcel(row.node),
          }"
          role="treeitem"
          :aria-level="row.depth + 1"
          :aria-selected="selectedPath === row.node.path"
          :disabled="busy"
          :data-file-path="row.node.path"
          aria-haspopup="menu"
          :style="{ paddingLeft: `${6 + row.depth * 14}px` }"
          :title="row.node.path"
          @contextmenu.stop="openMenu($event, row.node)"
          @keydown="onRowKeydown($event, row.node)"
          @focus="selectedPath = row.node.path"
          @click="onFileClick(row.node)"
        >
          <PhMicrosoftExcelLogo v-if="isExcel(row.node)" class="directory-item-file" :size="15" aria-hidden="true" />
          <PhFileText v-else class="directory-item-file" :size="15" aria-hidden="true" />
          <span class="directory-item-name">{{ row.node.name }}</span>
          <span class="directory-item-size">{{ formatSize(row.node.size) }}</span>
          <PhCheck v-if="isCurrent(row.node)" class="directory-item-check" :size="15" aria-hidden="true" />
        </button>
      </template>
    </div>
    <ContextMenu
      v-if="menu"
      kind="directory"
      :x="menu.x"
      :y="menu.y"
      :items="menuItems"
      @close="closeMenu"
      @choose="chooseMenu"
    />
    <DirectoryActionsPanel
      v-if="panelAction"
      :id="panelId"
      ref="panel"
      :action="panelAction"
      :path="panelPath"
      :plan="plan"
      :loading="panelLoading"
      :busy="busy"
      :error="panelError"
      :entries="trashEntries"
      :database-mode="context.storageMode?.value !== 'files' && !!context.storageMode"
      @close="closePanel()"
      @confirm="confirmDelete"
      @retry="beginAction('delete', panelPath)"
      @restore="restore"
    />
  </div>
</template>
