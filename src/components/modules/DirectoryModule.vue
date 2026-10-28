<script setup lang="ts">
import { computed, ref, watch } from 'vue'
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
} from '@phosphor-icons/vue'
import { invoke } from '@tauri-apps/api/core'
import ExcelExportPreviewDialog from '../ExcelExportPreviewDialog.vue'
import type { ExcelExportPreview } from '../../lib/useExcel'
import { api, type FileNode } from '../../lib/workspace'
import { useModuleContext } from '../../modules/context'

const context = useModuleContext()
const { journal } = context
const { selectedDate } = journal

const busy = computed(() => journal.busy.value || context.disabled.value)

interface TreeNode {
  name: string
  path: string
  isDir: boolean
  size: number
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

/** 后端按深度优先返回（父目录先于子节点），按 path 逐层挂载成树。 */
function buildTree(list: FileNode[]): TreeNode[] {
  const roots: TreeNode[] = []
  const byPath = new Map<string, TreeNode>()
  for (const node of list) {
    const item: TreeNode = { name: node.name, path: node.path, isDir: node.isDir, size: node.size, children: [] }
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
  if (collapsed.value.has(path)) collapsed.value.delete(path)
  else collapsed.value.add(path)
}
const allExpanded = computed(() => collapsed.value.size === 0)
function toggleAll() {
  if (collapsed.value.size) {
    collapsed.value = new Set()
    return
  }
  const next = new Set<string>()
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
interface Row {
  node: TreeNode
  depth: number
}
const rows = computed<Row[]>(() => {
  const result: Row[] = []
  const walk = (list: TreeNode[], depth: number) => {
    for (const node of list) {
      result.push({ node, depth })
      if (node.isDir && !collapsed.value.has(node.path)) walk(node.children, depth + 1)
    }
  }
  walk(tree.value, 0)
  return result
})

const fileCount = computed(() => nodes.value.filter((node) => !node.isDir).length)

/** 只有形如 `<年份>/<yyyymmdd>.<md|txt>` 的文件才是日报，可点击切换日期。 */
const reportPattern = /^(\d{4})\/(\d{8})\.(md|txt)$/
function reportDate(node: TreeNode): string | null {
  if (node.isDir) return null
  const match = reportPattern.exec(node.path)
  return match ? match[2] : null
}
function isCurrent(node: TreeNode) {
  return reportDate(node) === selectedDate.value
}
function isExcel(node: TreeNode) {
  return !node.isDir && /\.xlsx$/i.test(node.name)
}
function isClickable(node: TreeNode) {
  return !!reportDate(node) || isExcel(node)
}

/** Excel 文件点击后在弹窗中预览，样式与「Excel报告」导出预览一致。 */
const excelPreview = ref<ExcelExportPreview | null>(null)
const excelOpening = ref(false)
const excelOpenError = ref('')
const previewFilename = ref('')
async function openExcel(node: TreeNode) {
  const workspacePath = journal.workspace.value?.path
  if (!workspacePath || excelOpening.value) return
  excelOpening.value = true
  excelOpenError.value = ''
  excelPreview.value = null
  previewFilename.value = node.name
  try {
    const value = await invoke<ExcelExportPreview>('read_workspace_excel', { workspacePath, path: node.path })
    excelPreview.value = value && Array.isArray(value.sheets) ? value : null
  } catch (cause) {
    excelOpenError.value = `读取 Excel 内容失败：${String(cause)}`
  } finally {
    excelOpening.value = false
  }
}
function closeExcelPreview() {
  excelPreview.value = null
  excelOpening.value = false
  excelOpenError.value = ''
}

function onFileClick(node: TreeNode) {
  const date = reportDate(node)
  if (date) {
    if (busy.value || date === selectedDate.value) return
    void context.actions.selectReport(date)
    return
  }
  if (isExcel(node)) void openExcel(node)
}

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}
</script>

<template>
  <div class="directory-module" :aria-busy="busy || loading">
    <header class="directory-module-toolbar">
      <span class="directory-module-count">
        <PhTreeStructure :size="15" aria-hidden="true" />{{ fileCount }} 个文件
      </span>
      <span class="directory-module-actions">
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
    <p v-else-if="!nodes.length" class="directory-empty">这个工作空间还没有文件。</p>
    <div v-else class="directory-module-tree" role="tree" aria-label="工作空间文件">
      <template v-for="row in rows" :key="row.node.path">
        <button
          v-if="row.node.isDir"
          type="button"
          class="directory-tree-node"
          role="treeitem"
          :aria-expanded="!isCollapsed(row.node.path)"
          :aria-level="row.depth + 1"
          :style="{ paddingLeft: `${6 + row.depth * 14}px` }"
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
          v-else
          type="button"
          class="directory-item"
          :class="{
            'is-current': isCurrent(row.node),
            'is-report': !!reportDate(row.node),
            'is-excel': isExcel(row.node),
          }"
          role="treeitem"
          :aria-level="row.depth + 1"
          :aria-selected="isCurrent(row.node)"
          :disabled="busy || !isClickable(row.node)"
          :style="{ paddingLeft: `${6 + row.depth * 14}px` }"
          :title="row.node.path"
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
    <ExcelExportPreviewDialog
      :visible="!!excelPreview || excelOpening || !!excelOpenError"
      :filename="excelPreview?.filename ?? previewFilename"
      :sheets="excelPreview?.sheets ?? []"
      :loading="excelOpening"
      :error="excelOpenError"
      @close="closeExcelPreview"
    />
  </div>
</template>
