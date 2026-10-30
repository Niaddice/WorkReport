<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useId, watch } from 'vue'
import dayjs from 'dayjs'
import {
  PhArrowClockwise,
  PhGitBranch,
  PhGitCommit,
  PhMagnifyingGlass,
  PhSparkle,
  PhStop,
  PhCloudArrowDown,
  PhArrowDown,
  PhArrowUp,
  PhX,
} from '@phosphor-icons/vue'
import AppSelect from './AppSelect.vue'
import GitDiffContent from './GitDiffContent.vue'
import { parseGitDiff } from '../lib/gitDiff'
import GitContextMenu from './ContextMenu.vue'
import type { ContextMenuItem } from '../lib/contextMenus'
import type { GitRepository } from '../lib/useGit'
import {
  isGitStaged,
  isGitUnstaged,
  isGitUntracked,
  useGitWorkbench,
  type GitActionRequest,
  type GitFileChange,
  type GitHistoryCommit,
  type GitBranch,
  type GitRepositoryState,
} from '../lib/useGitWorkbench'
import { api } from '../lib/workspace'

const props = withDefaults(
  defineProps<{
    workspacePath: string
    repositories: GitRepository[]
    disabled: boolean
    active?: boolean
    aiProviderName?: string | null
  }>(),
  { active: true },
)
const emit = defineEmits<{ changed: []; configureAi: [] }>()
const id = useId()
const repositoryId = ref(props.repositories[0]?.id ?? '')
const client = useGitWorkbench(() => ({ workspacePath: props.workspacePath, repositoryId: repositoryId.value }))
const { repository, history, diff, loading, historyLoading, diffLoading, busy, error, notice, generatingMessage } =
  client
const tab = ref<'changes' | 'history' | 'branches' | 'stashes'>('changes')
const tabs = [
  { id: 'changes', label: '本地更改' },
  { id: 'history', label: '提交历史' },
  { id: 'branches', label: '分支' },
  { id: 'stashes', label: 'Stash' },
] as const
const commitMessage = ref('')
const branchName = ref('')
const stashMessage = ref('')
const stashInput = ref<HTMLInputElement>()
const query = ref('')
const appliedQuery = ref('')
const historyReference = ref('')
const selectedFile = ref<{ path: string; staged: boolean } | null>(null)
const selectedCommit = ref<GitHistoryCommit | null>(null)
const compactDiff = ref(false)
const splitRoot = ref<HTMLElement>()
const compactBack = ref<HTMLButtonElement>()
const splitRatio = ref(32)
const resizing = ref(false)
let resizePointer: number | null = null
const confirmation = ref<{ title: string; detail: string; request: GitActionRequest } | null>(null)
const confirmationPanel = ref<HTMLElement>()
const contextMenu = ref<{ x: number; y: number; anchor: HTMLElement; items: ContextMenuItem[] } | null>(null)
let copyRevision = 0
const lock = computed(
  () => props.disabled || busy.value || loading.value || generatingMessage.value || !!confirmation.value,
)
const available = computed(() => !!props.workspacePath && !!repositoryId.value && !props.disabled)
const staged = computed(() => repository.value?.files.filter(isGitStaged) ?? [])
const unstaged = computed(() => repository.value?.files.filter(isGitUnstaged) ?? [])
const untracked = computed(() => repository.value?.files.filter(isGitUntracked) ?? [])
const conflicts = computed(() => repository.value?.files.filter((file) => file.conflicted) ?? [])
const groups = computed(() => [
  { id: 'conflict', label: '冲突', files: conflicts.value, staged: false },
  { id: 'staged', label: '已暂存', files: staged.value, staged: true },
  { id: 'unstaged', label: '未暂存', files: unstaged.value, staged: false },
  { id: 'untracked', label: '未跟踪', files: untracked.value, staged: false },
])
const canCommit = computed(
  () =>
    !lock.value &&
    !!repository.value &&
    staged.value.length > 0 &&
    !conflicts.value.length &&
    !repository.value.detached &&
    !!commitMessage.value.trim(),
)
const canGenerate = computed(
  () =>
    !lock.value &&
    available.value &&
    staged.value.length > 0 &&
    !conflicts.value.length &&
    props.aiProviderName !== null,
)
const aiHint = computed(() => {
  if (generatingMessage.value) return '取消生成提交说明'
  if (props.aiProviderName === null) return '请先配置默认 AI，再生成提交说明'
  if (conflicts.value.length) return '请先解决冲突，再生成提交说明'
  if (!staged.value.length) return '请先暂存要提交的文件'
  return `使用${props.aiProviderName || '默认 AI'}，根据 ${staged.value.length} 个已暂存文件生成提交说明`
})
let commitMessageRevision = 0
watch(
  commitMessage,
  () => {
    commitMessageRevision++
  },
  { flush: 'sync' },
)
watch(
  () => [props.active, props.disabled],
  () => {
    if (!props.active || props.disabled) client.cancelCommitMessage()
  },
)
watch(
  () => props.aiProviderName,
  () => client.cancelCommitMessage(),
)
const canSync = computed(() => !!repository.value?.upstream && !repository.value.detached && !conflicts.value.length)
const repositoryOptions = computed(() => props.repositories.map((repo) => ({ value: repo.id, label: repo.name })))
const historyOptions = computed(() => [
  { value: '', label: '当前分支' },
  { value: '__all__', label: '所有分支' },
  ...(repository.value?.branches.map((branch) => ({ value: branch.name, label: branch.name })) ?? []),
])
const localBranches = computed(() => repository.value?.branches.filter((branch) => !branch.remote) ?? [])
const remoteBranches = computed(() => repository.value?.branches.filter((branch) => branch.remote) ?? [])
const diffTitle = computed(() =>
  selectedCommit.value
    ? `${selectedCommit.value.hash.slice(0, 8)} · ${selectedCommit.value.message.split('\n')[0]}`
    : selectedFile.value
      ? `${selectedFile.value.staged ? '暂存区' : '工作区'} · ${selectedFile.value.path}`
      : '差异预览',
)
const parsedDiff = computed(() => parseGitDiff(diff.value?.text ?? ''))

function resizeStart(event: PointerEvent) {
  if (event.button !== 0) return
  resizing.value = true
  resizePointer = event.pointerId
  ;(event.currentTarget as HTMLElement).setPointerCapture?.(event.pointerId)
  event.preventDefault()
}
function resizeMove(event: PointerEvent) {
  if (!resizing.value || event.pointerId !== resizePointer || !splitRoot.value) return
  const bounds = splitRoot.value.getBoundingClientRect()
  if (bounds.width > 0)
    splitRatio.value = Math.min(60, Math.max(24, ((event.clientX - bounds.left) / bounds.width) * 100))
}
function resizeEnd() {
  resizing.value = false
  resizePointer = null
}
function resizeKey(event: KeyboardEvent) {
  const step = event.shiftKey ? 10 : 2
  if (event.key === 'Home') splitRatio.value = 24
  else if (event.key === 'End') splitRatio.value = 60
  else if (event.key === 'ArrowLeft') splitRatio.value = Math.max(24, splitRatio.value - step)
  else if (event.key === 'ArrowRight') splitRatio.value = Math.min(60, splitRatio.value + step)
  else return
  event.preventDefault()
}

watch(
  () => [props.workspacePath, props.repositories, repositoryId.value, tab.value, props.active, lock.value],
  () => {
    contextMenu.value = null
  },
  { flush: 'sync', deep: true },
)
watch(
  () => `${props.workspacePath}\0${JSON.stringify(props.repositories)}\0${repositoryId.value}`,
  () => {
    if (!props.repositories.some((repo) => repo.id === repositoryId.value)) {
      repositoryId.value = props.repositories[0]?.id ?? ''
    }
    client.clear()
    copyRevision++
    commitMessage.value = ''
    branchName.value = ''
    stashMessage.value = ''
    query.value = ''
    appliedQuery.value = ''
    historyReference.value = ''
    selectedFile.value = null
    compactDiff.value = false
    selectedCommit.value = null
    confirmation.value = null
    if (available.value && props.active) void refresh()
  },
  { immediate: true, flush: 'sync' },
)
watch(
  () => props.disabled,
  (value) => {
    if (!value && props.active && available.value && !repository.value && !loading.value) void refresh()
  },
)
watch(
  () => props.active,
  (active) => {
    if (active && available.value && !lock.value) void refresh()
  },
)
watch(tab, () => {
  client.cancelCommitMessage()
  selectedFile.value = null
  compactDiff.value = false
  selectedCommit.value = null
  client.clearDiff()
  if (tab.value === 'history' && !history.value && available.value) void searchHistory()
})
watch(historyReference, () => {
  if (tab.value === 'history' && available.value && !lock.value) void searchHistory()
})
function onFocus() {
  if (props.active && available.value && !lock.value) void refresh()
}
onMounted(() => window.addEventListener('focus', onFocus))
onBeforeUnmount(() => {
  copyRevision++
  window.removeEventListener('focus', onFocus)
  client.clear()
})

async function refresh() {
  if (!available.value || busy.value || confirmation.value) return
  selectedFile.value = null
  selectedCommit.value = null
  client.clearDiff()
  if (await client.refresh()) {
    if (tab.value === 'history') await searchHistory()
  }
}

async function searchHistory(more = false) {
  if (lock.value || !available.value || historyLoading.value) return
  if (!more) {
    appliedQuery.value = query.value.trim()
    selectedCommit.value = null
    compactDiff.value = false
    client.clearDiff()
  }
  await client.loadHistory({
    query: appliedQuery.value,
    reference: historyReference.value,
    skip: more ? (history.value?.commits.length ?? 0) : 0,
  })
}

async function selectFile(file: GitFileChange, staged: boolean) {
  if (lock.value) return
  selectedCommit.value = null
  selectedFile.value = { path: file.path, staged }
  compactDiff.value = true
  const pending = client.showDiff({ path: file.path, staged, commit: '' })
  await nextTick()
  if (
    selectedFile.value?.path === file.path &&
    splitRoot.value &&
    splitRoot.value.clientWidth > 0 &&
    splitRoot.value.clientWidth <= 560
  ) {
    compactBack.value?.focus({ preventScroll: true })
  }
  await pending
}

async function returnToFiles() {
  compactDiff.value = false
  await nextTick()
  splitRoot.value?.querySelector<HTMLButtonElement>('.file-row.selected .file-select')?.focus({ preventScroll: true })
}

async function selectCommit(commit: GitHistoryCommit) {
  if (lock.value) return
  selectedFile.value = null
  selectedCommit.value = commit
  compactDiff.value = true
  const pending = client.showDiff({ path: '', staged: false, commit: commit.hash })
  await nextTick()
  if (splitRoot.value && splitRoot.value.clientWidth > 0 && splitRoot.value.clientWidth <= 560)
    compactBack.value?.focus({ preventScroll: true })
  await pending
}

async function returnToHistory() {
  compactDiff.value = false
  await nextTick()
  splitRoot.value?.querySelector<HTMLButtonElement>('.history-entry.selected')?.focus({ preventScroll: true })
}
function resetHistorySearch() {
  query.value = ''
  if (historyReference.value) historyReference.value = ''
  else void searchHistory()
}
async function openStashView() {
  tab.value = 'stashes'
  await nextTick()
  stashInput.value?.focus({ preventScroll: true })
}
async function openChangesView() {
  tab.value = 'changes'
  await nextTick()
  splitRoot.value?.querySelector<HTMLElement>('.file-select, .commit-form textarea')?.focus({ preventScroll: true })
}
function branchActionHint(branch: GitBranch, operation: 'switch' | 'merge' | 'delete') {
  if (branch.current) return '当前已在此分支'
  if (operation === 'delete') return '只删除已合并的本地分支'
  if (conflicts.value.length) return '请先解决工作区冲突'
  if (operation === 'merge' && repository.value?.detached) return '请先切换到本地分支'
  if (operation === 'merge' && repository.value?.files.length) return '请先提交更改或保存到 Stash'
  return operation === 'merge'
    ? '仅以快进方式合并到当前分支'
    : branch.remote
      ? '创建并切换到跟踪此远程分支的本地分支'
      : `切换到 ${branch.name}`
}

async function act(request: Partial<GitActionRequest> & { operation: GitActionRequest['operation'] }) {
  if (lock.value || !available.value) return false
  const scope = `${props.workspacePath}\0${repositoryId.value}`
  const result = await client.runAction(request)
  if (scope !== `${props.workspacePath}\0${repositoryId.value}`) return false
  selectedFile.value = null
  selectedCommit.value = null
  emit('changed')
  if (result && tab.value === 'history') await searchHistory()
  return result
}

function filePaths(file: GitFileChange) {
  // The backend expands the original path for renames using the fresh status.
  return [file.path]
}

async function commit(push = false) {
  if (!canCommit.value || (push && !canSync.value)) return
  if (!(await act({ operation: 'commit', message: commitMessage.value.trim() }))) return
  commitMessage.value = ''
  if (push && !(await act({ operation: 'push' }))) {
    error.value = `提交已保留在本地，推送失败；可直接点击“Push 推送”重试。${error.value ? `\n${error.value}` : ''}`
  }
}

async function generateMessage() {
  if (generatingMessage.value) {
    client.cancelCommitMessage()
    return
  }
  if (!canGenerate.value) return
  const revision = commitMessageRevision
  const message = await client.generateCommitMessage()
  if (!message || !props.active) return
  if (revision !== commitMessageRevision) {
    notice.value = '提交说明已被编辑，已保留你的修改。'
    return
  }
  commitMessage.value = message
  notice.value = 'AI 提交说明已生成，可修改后再提交。'
}

async function createBranch() {
  if (!branchName.value.trim() || lock.value) return
  if (await act({ operation: 'createBranch', reference: branchName.value.trim() })) branchName.value = ''
}

async function stash() {
  if (!repository.value?.files.length || lock.value || conflicts.value.length) return
  if (await act({ operation: 'stash', message: stashMessage.value.trim() })) stashMessage.value = ''
}

async function ask(
  title: string,
  detail: string,
  request: Partial<GitActionRequest> & { operation: GitActionRequest['operation'] },
) {
  if (lock.value) return
  confirmation.value = { title, detail, request: { paths: [], message: '', reference: '', ...request } }
  await nextTick()
  confirmationPanel.value?.focus()
}

async function confirm() {
  if (!confirmation.value || props.disabled || busy.value || loading.value) return
  const request = confirmation.value.request
  confirmation.value = null
  await act(request)
}

function discard(file: GitFileChange) {
  void ask(
    '丢弃未暂存的更改？',
    `将 ${file.path} 的工作区内容恢复为暂存区版本。未暂存的修改将丢失，请先复制或 Stash 需要保留的内容。`,
    { operation: 'discard', paths: [file.path] },
  )
}

async function copyValue(value: string, label: string) {
  const revision = ++copyRevision
  const scope = `${props.workspacePath}\0${repositoryId.value}`
  const current = () => revision === copyRevision && scope === `${props.workspacePath}\0${repositoryId.value}`
  try {
    await api.copy(value)
    if (current()) notice.value = `${label}已复制。`
  } catch (cause) {
    if (current()) error.value = `复制失败：${String(cause)}`
  }
}

function copyHash(hash: string) {
  return copyValue(hash, '提交哈希')
}

function showMenu(event: MouseEvent | KeyboardEvent, items: ContextMenuItem[]) {
  if (event instanceof KeyboardEvent && event.key !== 'ContextMenu' && !(event.shiftKey && event.key === 'F10')) return
  if (lock.value || !available.value || !props.active) return
  event.preventDefault()
  event.stopPropagation()
  const row = event.currentTarget as HTMLElement
  const anchor =
    (event.target as Element).closest<HTMLElement>('button:not(:disabled)') ??
    row.querySelector<HTMLElement>('button:not(:disabled)') ??
    row
  if (anchor === row && row.tagName !== 'BUTTON' && !row.hasAttribute('tabindex')) row.tabIndex = -1
  const bounds = anchor.getBoundingClientRect()
  const mouse = event instanceof MouseEvent && (event.clientX !== 0 || event.clientY !== 0)
  contextMenu.value = {
    x: mouse ? event.clientX : bounds.left,
    y: mouse ? event.clientY : bounds.bottom,
    anchor,
    items,
  }
}
function closeMenu(restoreFocus = false) {
  const anchor = contextMenu.value?.anchor
  contextMenu.value = null
  if (restoreFocus && anchor?.isConnected) anchor.focus({ preventScroll: true })
}
async function chooseMenu(id: string) {
  const item = contextMenu.value?.items.find((item) => item.id === id)
  if (!item || item.disabled || lock.value || !props.active) return
  closeMenu(true)
  try {
    await item.run()
  } catch (cause) {
    error.value = `Git 操作失败：${String(cause)}`
  }
}
function fileMenu(event: MouseEvent | KeyboardEvent, file: GitFileChange, group: { id: string; staged: boolean }) {
  const items: ContextMenuItem[] = [
    { id: 'diff', label: '查看差异', run: () => selectFile(file, group.staged) },
    {
      id: 'stage',
      label: group.staged ? '取消暂存' : '暂存文件',
      run: () => act({ operation: group.staged ? 'unstage' : 'stage', paths: filePaths(file) }),
    },
    { id: 'copy', label: '复制文件路径', run: () => copyValue(file.path, '文件路径') },
  ]
  if (group.id === 'unstaged')
    items.push({
      id: 'discard',
      label: '丢弃未暂存的更改…',
      danger: true,
      disabled: !!file.originalPath,
      run: () => discard(file),
    })
  showMenu(event, items)
}
function commitMenu(event: MouseEvent | KeyboardEvent, entry: GitHistoryCommit) {
  showMenu(event, [
    { id: 'diff', label: '查看提交差异', run: () => selectCommit(entry) },
    { id: 'hash', label: '复制提交哈希', run: () => copyHash(entry.hash) },
    { id: 'message', label: '复制提交说明', run: () => copyValue(entry.message, '提交说明') },
  ])
}
function branchMenu(event: MouseEvent | KeyboardEvent, branch: GitBranch) {
  const items: ContextMenuItem[] = [
    {
      id: 'switch',
      label: branch.remote ? '检出跟踪分支' : '切换分支',
      disabled: branch.current || !!conflicts.value.length,
      run: () => act({ operation: 'switchBranch', reference: branch.name }),
    },
    {
      id: 'merge',
      label: '快进合并到当前分支',
      disabled:
        branch.current || repository.value?.detached || !!conflicts.value.length || !!repository.value?.files.length,
      run: () => act({ operation: 'mergeBranch', reference: branch.name }),
    },
    { id: 'copy', label: '复制分支名称', run: () => copyValue(branch.name, '分支名称') },
  ]
  if (!branch.remote)
    items.push({
      id: 'delete',
      label: '删除本地分支…',
      danger: true,
      disabled: branch.current,
      run: () =>
        ask('删除本地分支？', `将删除本地分支 ${branch.name}。只有已合并的分支才能删除。`, {
          operation: 'deleteBranch',
          reference: branch.name,
        }),
    })
  showMenu(event, items)
}
function stashMenu(event: MouseEvent | KeyboardEvent, item: GitRepositoryState['stashes'][number]) {
  showMenu(event, [
    {
      id: 'apply',
      label: '应用并保留',
      disabled: !!conflicts.value.length,
      run: () => act({ operation: 'applyStash', reference: item.reference }),
    },
    {
      id: 'pop',
      label: '恢复并移除',
      disabled: !!conflicts.value.length,
      run: () => act({ operation: 'popStash', reference: item.reference }),
    },
    { id: 'copy', label: '复制 Stash 引用', run: () => copyValue(item.reference, 'Stash 引用') },
    {
      id: 'delete',
      label: '删除 Stash…',
      danger: true,
      run: () =>
        ask('删除 Stash？', `${item.reference}：${item.message}\n删除后将无法从列表恢复这些修改，请确认已不再需要。`, {
          operation: 'dropStash',
          reference: item.reference,
        }),
    },
  ])
}

function statusLabel(file: GitFileChange, staged: boolean) {
  if (file.conflicted) return '冲突'
  if (isGitUntracked(file)) return '新文件'
  const status = staged ? file.indexStatus : file.worktreeStatus
  return (
    ({ A: '新增', M: '修改', D: '删除', R: '重命名', C: '复制', T: '类型变化' } as Record<string, string>)[status] ??
    status
  )
}
</script>

<template>
  <div class="git-workbench" :aria-busy="busy || loading">
    <GitContextMenu
      v-if="contextMenu"
      kind="git"
      :x="contextMenu.x"
      :y="contextMenu.y"
      :items="contextMenu.items"
      @close="closeMenu"
      @choose="chooseMenu"
    />
    <div class="workbench-toolbar">
      <AppSelect
        v-model="repositoryId"
        :options="repositoryOptions"
        :disabled="lock || !repositories.length"
        aria-label="Git 仓库"
      />
      <div v-if="repository" class="repository-caption" :title="repository.root">
        <span><PhGitBranch :size="15" />{{ repository.detached ? '分离 HEAD' : repository.branch || '尚无提交' }}</span>
        <span v-if="repository.upstream"
          >{{ repository.upstream }} · ↑{{ repository.ahead }} ↓{{ repository.behind }}</span
        >
        <span v-else>未配置跟踪分支</span>
        <code :title="repository.root">{{ repository.root }}</code>
      </div>
      <div class="workbench-sync">
        <button
          type="button"
          :disabled="lock || !repository?.remotes.length"
          aria-label="Fetch 获取"
          title="获取远程引用（Fetch）"
          @click="act({ operation: 'fetch' })"
        >
          <PhCloudArrowDown :size="17" aria-hidden="true" /><span>Fetch 获取</span>
        </button>
        <button
          type="button"
          :disabled="lock || !canSync"
          aria-label="Pull 更新"
          title="仅快进更新当前分支"
          @click="act({ operation: 'pull' })"
        >
          <PhArrowDown :size="17" aria-hidden="true" /><span>Pull 更新</span>
        </button>
        <button
          type="button"
          :disabled="lock || !canSync"
          aria-label="Push 推送"
          title="推送当前分支（Push）"
          @click="act({ operation: 'push' })"
        >
          <PhArrowUp :size="17" aria-hidden="true" /><span>Push 推送</span>
        </button>
        <button
          type="button"
          class="refresh-button"
          :disabled="lock || !available"
          aria-label="刷新 Git 仓库"
          title="刷新仓库状态"
          @click="refresh"
        >
          <PhArrowClockwise :size="17" />
        </button>
      </div>
    </div>
    <nav class="workbench-tabs" aria-label="Git 工作台视图">
      <button
        v-for="item in tabs"
        :key="item.id"
        type="button"
        :class="{ active: tab === item.id }"
        :aria-pressed="tab === item.id"
        :disabled="busy || !!confirmation"
        @click="tab = item.id"
      >
        {{ item.label
        }}<span v-if="item.id === 'changes' && repository?.files.length">{{ repository.files.length }}</span>
      </button>
    </nav>
    <p v-if="error" class="workbench-message workbench-error" role="alert">{{ error }}</p>
    <p v-if="notice" class="workbench-message workbench-notice" role="status">{{ notice }}</p>
    <section
      v-if="confirmation"
      ref="confirmationPanel"
      class="workbench-confirm"
      role="alertdialog"
      :aria-labelledby="`${id}-confirm-title`"
      :aria-describedby="`${id}-confirm-detail`"
      tabindex="-1"
      @keydown.esc.prevent="confirmation = null"
    >
      <strong :id="`${id}-confirm-title`">{{ confirmation.title }}</strong>
      <p :id="`${id}-confirm-detail`">{{ confirmation.detail }}</p>
      <div class="button-row">
        <button type="button" @click="confirmation = null">取消</button
        ><button type="button" class="danger-button" @click="confirm">确认操作</button>
      </div>
    </section>
    <p v-if="loading" class="workbench-empty" role="status">正在读取仓库…</p>
    <p v-else-if="!repositories.length" class="workbench-empty">请先在 Git 配置中添加本地仓库。</p>
    <template v-else-if="repository">
      <div
        v-if="tab === 'changes'"
        ref="splitRoot"
        class="workbench-split changes-view"
        :class="{ 'show-diff': compactDiff, resizing }"
        :style="{ '--git-sidebar-width': `${splitRatio}%` }"
      >
        <div class="change-pane">
          <div class="bulk-actions">
            <button
              type="button"
              :disabled="lock || (!unstaged.length && !untracked.length)"
              @click="act({ operation: 'stage', paths: [...unstaged, ...untracked].flatMap(filePaths) })"
            >
              全部暂存</button
            ><button
              type="button"
              :disabled="lock || !staged.length"
              @click="act({ operation: 'unstage', paths: staged.flatMap(filePaths) })"
            >
              全部取消暂存
            </button>
          </div>
          <p v-if="!repository.files.length" class="workbench-empty"><PhGitCommit :size="25" />工作区干净</p>
          <p v-if="conflicts.length" class="workbench-message workbench-error">
            请先在文件中解决冲突，再暂存对应文件。所有冲突解决后才能提交。
          </p>
          <section v-for="group in groups.filter((group) => group.files.length)" :key="group.id" class="file-group">
            <h4>
              {{ group.label }}<span>{{ group.files.length }}</span>
            </h4>
            <div
              v-for="file in group.files"
              :key="file.path"
              class="file-row"
              :class="{ selected: selectedFile?.path === file.path && selectedFile.staged === group.staged }"
              @contextmenu="fileMenu($event, file, group)"
              @keydown="fileMenu($event, file, group)"
            >
              <button
                type="button"
                class="file-select"
                :disabled="lock"
                :aria-label="`查看${group.staged ? '暂存' : '工作区'}差异 ${file.path}`"
                :title="file.originalPath ? `${file.originalPath} → ${file.path}` : file.path"
                @click="selectFile(file, group.staged)"
              >
                <span class="file-status" :class="{ conflict: file.conflicted }">{{
                  statusLabel(file, group.staged)
                }}</span
                ><span class="file-path"
                  ><strong>{{ file.path.split('/').at(-1) }}</strong
                  ><small v-if="file.path.includes('/')">{{ file.path.slice(0, file.path.lastIndexOf('/')) }}</small
                  ><small v-if="file.originalPath">原路径：{{ file.originalPath }}</small></span
                >
              </button>
              <button
                v-if="group.staged"
                type="button"
                class="file-action"
                :disabled="lock"
                :aria-label="`取消暂存 ${file.path}`"
                @click="act({ operation: 'unstage', paths: filePaths(file) })"
              >
                −
              </button>
              <button
                v-else
                type="button"
                class="file-action"
                :disabled="lock"
                :aria-label="`暂存 ${file.path}`"
                :title="file.conflicted ? '将编辑后的冲突文件标记为已解决' : '暂存文件'"
                @click="act({ operation: 'stage', paths: filePaths(file) })"
              >
                +
              </button>
              <button
                v-if="group.id === 'unstaged'"
                type="button"
                class="file-action discard-action"
                :disabled="lock || !!file.originalPath"
                :aria-label="`丢弃 ${file.path}`"
                title="丢弃未暂存的修改"
                @click="discard(file)"
              >
                ↶
              </button>
            </div>
          </section>
        </div>
        <form class="commit-form" @submit.prevent="commit()">
          <div class="commit-message-heading">
            <label :for="`${id}-message`">提交说明</label>
            <span v-if="aiProviderName" class="ai-provider" :title="`默认 AI：${aiProviderName}`">{{
              aiProviderName
            }}</span>
            <span class="generate-action" :title="aiHint"
              ><button
                type="button"
                class="generate-commit-message"
                :aria-label="generatingMessage ? '取消生成提交说明' : 'AI 生成提交说明'"
                :title="aiHint"
                :aria-describedby="`${id}-ai-hint`"
                :aria-busy="generatingMessage"
                :disabled="!generatingMessage && !canGenerate"
                @click="generateMessage"
              >
                <PhStop v-if="generatingMessage" :size="17" aria-hidden="true" /><PhSparkle
                  v-else
                  :size="17"
                  aria-hidden="true"
                /></button
            ></span>
          </div>
          <textarea
            :id="`${id}-message`"
            v-model="commitMessage"
            :disabled="disabled || busy || loading || !!confirmation"
            aria-label="提交说明"
            placeholder="概述这次更改…"
            rows="3"
            @keydown.enter.ctrl.prevent="commit()"
            @keydown.enter.meta.prevent="commit()"
          />
          <p v-if="generatingMessage" class="workbench-hint" role="status">
            正在根据暂存区更改生成说明，可继续编辑或取消生成…
          </p>
          <p :id="`${id}-ai-hint`" class="workbench-hint">
            <template v-if="aiProviderName === null"
              ><button type="button" class="configure-ai" :disabled="disabled || busy" @click="emit('configureAi')">
                配置默认 AI</button
              >后可生成提交说明。</template
            ><template v-else-if="!staged.length || conflicts.length">{{ aiHint }}</template
            ><template v-else>仅提交已暂存的 {{ staged.length }} 个文件。⌘ / Ctrl + Enter 提交。</template>
          </p>
          <p v-if="repository.detached" class="workbench-hint">当前处于分离 HEAD，请先创建或切换本地分支。</p>
          <div class="button-row">
            <button type="submit" class="primary-button" :disabled="!canCommit">提交</button
            ><button type="button" :disabled="!canCommit || !canSync" @click="commit(true)">提交并推送</button>
          </div>
        </form>
        <div
          class="git-splitter"
          role="separator"
          aria-label="调整文件列表与差异宽度"
          aria-orientation="vertical"
          tabindex="0"
          :aria-valuenow="Math.round(splitRatio)"
          :aria-valuemin="24"
          :aria-valuemax="60"
          @pointerdown="resizeStart"
          @pointermove="resizeMove"
          @pointerup="resizeEnd"
          @pointercancel="resizeEnd"
          @lostpointercapture="resizeEnd"
          @keydown="resizeKey"
        />
        <section class="diff-pane" :aria-label="diffTitle">
          <div class="diff-heading">
            <button
              ref="compactBack"
              type="button"
              class="compact-back"
              aria-label="返回文件列表"
              @click="returnToFiles"
            >
              ← 文件
            </button>
            <h4 :title="diffTitle">{{ diffTitle }}</h4>
            <span
              v-if="diff && !diffLoading"
              class="diff-stats"
              :aria-label="`新增 ${parsedDiff.additions} 行，删除 ${parsedDiff.deletions} 行`"
              ><span>+{{ parsedDiff.additions }}</span
              ><span>−{{ parsedDiff.deletions }}</span></span
            >
          </div>
          <p v-if="diffLoading" class="workbench-empty" role="status">正在读取差异…</p>
          <template v-else-if="diff"
            ><p v-if="diff.truncated" class="workbench-hint">差异过大，仅显示部分内容。</p>
            <GitDiffContent v-if="diff.text" :lines="parsedDiff.lines" />
            <p v-else class="workbench-empty">没有文本差异，可能是二进制文件或文件权限变化。</p></template
          >
          <p v-else class="workbench-empty">选择文件查看工作区或暂存区差异。</p>
        </section>
      </div>
      <div v-else-if="tab === 'history'" class="history-view">
        <form class="history-filters" @submit.prevent="searchHistory()">
          <div class="search-input">
            <PhMagnifyingGlass :size="17" /><input
              v-model="query"
              aria-label="搜索提交说明"
              placeholder="搜索提交说明"
              :disabled="lock"
            /><button
              v-if="query || historyReference"
              type="button"
              class="clear-search"
              aria-label="清除提交筛选"
              title="清除提交筛选"
              :disabled="lock || historyLoading"
              @click="resetHistorySearch"
            >
              <PhX :size="15" aria-hidden="true" />
            </button>
          </div>
          <AppSelect
            v-model="historyReference"
            :options="historyOptions"
            :disabled="lock || historyLoading"
            aria-label="历史分支"
          /><button type="submit" :disabled="lock || historyLoading">搜索</button>
        </form>
        <div
          ref="splitRoot"
          class="workbench-split history-split"
          :class="{ 'show-diff': compactDiff, resizing }"
          :style="{ '--git-sidebar-width': `${splitRatio}%` }"
        >
          <div class="history-list">
            <p v-if="historyLoading && !history" class="workbench-empty" role="status">正在读取提交历史…</p>
            <div v-else-if="!history?.commits.length" class="workbench-empty history-empty">
              <PhGitCommit :size="28" aria-hidden="true" /><strong>{{
                appliedQuery || historyReference ? '未找到相关提交' : '暂无提交记录'
              }}</strong>
              <p>
                {{
                  appliedQuery || historyReference
                    ? '可调整搜索说明或分支范围后重试。'
                    : '提交已暂存的更改后，可在这里查看历史。'
                }}
              </p>
              <button
                v-if="appliedQuery || historyReference"
                type="button"
                :disabled="lock || historyLoading"
                @click="resetHistorySearch"
              >
                清除筛选</button
              ><button v-else type="button" :disabled="lock" @click="openChangesView">前往本地更改</button>
            </div>
            <button
              v-for="entry in history?.commits"
              :key="entry.hash"
              type="button"
              class="history-entry"
              :class="{ selected: selectedCommit?.hash === entry.hash }"
              :disabled="lock"
              @click="selectCommit(entry)"
              @contextmenu="commitMenu($event, entry)"
              @keydown="commitMenu($event, entry)"
            >
              <span class="history-subject">{{ entry.message.split('\n')[0] || '未填写提交说明' }}</span
              ><span v-if="entry.refs" class="history-refs">{{ entry.refs }}</span
              ><span class="history-meta"
                ><code>{{ entry.hash.slice(0, 8) }}</code
                >{{ entry.author }} · {{ dayjs(entry.committedAt).format('MM-DD HH:mm') }}</span
              >
            </button>
            <button
              v-if="history?.hasMore"
              type="button"
              class="load-more"
              :disabled="lock || historyLoading"
              @click="searchHistory(true)"
            >
              {{ historyLoading ? '正在加载…' : '加载更多提交' }}
            </button>
          </div>
          <div
            class="git-splitter"
            role="separator"
            aria-label="调整提交列表与差异宽度"
            aria-orientation="vertical"
            tabindex="0"
            :aria-valuenow="Math.round(splitRatio)"
            :aria-valuemin="24"
            :aria-valuemax="60"
            @pointerdown="resizeStart"
            @pointermove="resizeMove"
            @pointerup="resizeEnd"
            @pointercancel="resizeEnd"
            @lostpointercapture="resizeEnd"
            @keydown="resizeKey"
          />
          <section class="diff-pane" aria-label="提交差异">
            <template v-if="selectedCommit"
              ><div class="diff-heading">
                <button
                  ref="compactBack"
                  type="button"
                  class="compact-back"
                  aria-label="返回提交列表"
                  @click="returnToHistory"
                >
                  ← 历史
                </button>
                <h4 :title="diffTitle">{{ diffTitle }}</h4>
                <span
                  v-if="diff && !diffLoading"
                  class="diff-stats"
                  :aria-label="`新增 ${parsedDiff.additions} 行，删除 ${parsedDiff.deletions} 行`"
                  ><span>+{{ parsedDiff.additions }}</span
                  ><span>−{{ parsedDiff.deletions }}</span></span
                >
              </div>
              <div class="commit-details">
                <span>{{ selectedCommit.author }} &lt;{{ selectedCommit.email }}&gt;</span
                ><span>{{ dayjs(selectedCommit.committedAt).format('YYYY-MM-DD HH:mm:ss') }}</span
                ><button type="button" @click="copyHash(selectedCommit.hash)">
                  复制哈希 {{ selectedCommit.hash.slice(0, 8) }}
                </button>
                <details class="commit-full-message">
                  <summary>完整提交说明</summary>
                  <pre>{{ selectedCommit.message }}</pre>
                </details>
              </div></template
            >
            <p v-if="diffLoading" class="workbench-empty" role="status">正在读取提交差异…</p>
            <template v-else-if="diff"
              ><p v-if="diff.truncated" class="workbench-hint">差异过大，仅显示部分内容。</p>
              <GitDiffContent v-if="diff.text" :lines="parsedDiff.lines" />
              <p v-else class="workbench-empty">此提交没有可显示的文本差异。</p></template
            >
            <p v-else-if="!selectedCommit" class="workbench-empty">选择提交查看完整说明和文件差异。</p>
          </section>
        </div>
      </div>
      <div v-else-if="tab === 'branches'" class="branch-view">
        <form class="inline-form" @submit.prevent="createBranch">
          <input v-model="branchName" aria-label="新分支名称" placeholder="新分支名称" :disabled="lock" /><button
            type="submit"
            class="primary-button"
            :disabled="lock || !branchName.trim()"
          >
            创建并切换
          </button>
        </form>
        <p class="workbench-hint">从当前 HEAD 创建分支。合并只允许快进；删除分支前会检查是否已合并。</p>
        <p v-if="repository.files.length && !conflicts.length" class="workbench-hint branch-notice">
          工作区有 {{ repository.files.length }} 个变更，合并前请先提交或<button
            type="button"
            class="configure-ai"
            :disabled="lock"
            @click="openStashView"
          >
            保存到 Stash</button
          >。
        </p>
        <section
          v-for="group in [
            { label: '本地分支', branches: localBranches },
            { label: '远程分支', branches: remoteBranches },
          ]"
          :key="group.label"
          class="branch-group"
        >
          <h4>
            {{ group.label }} <span>{{ group.branches.length }}</span>
          </h4>
          <p v-if="!group.branches.length" class="workbench-hint">暂无{{ group.label }}</p>
          <div
            v-for="branch in group.branches"
            :key="branch.name"
            class="branch-row"
            @contextmenu="branchMenu($event, branch)"
            @keydown="branchMenu($event, branch)"
          >
            <div>
              <strong
                ><PhGitBranch :size="16" /><span class="branch-name" :title="branch.name">{{ branch.name }}</span
                ><span v-if="branch.current" class="current-badge">当前</span></strong
              ><small v-if="branch.upstream">跟踪 {{ branch.upstream }}</small>
            </div>
            <div class="button-row">
              <button
                type="button"
                :disabled="lock || branch.current || !!conflicts.length"
                :title="branchActionHint(branch, 'switch')"
                @click="act({ operation: 'switchBranch', reference: branch.name })"
              >
                {{ branch.remote ? '检出跟踪分支' : '切换' }}</button
              ><button
                type="button"
                :disabled="
                  lock || branch.current || repository.detached || !!conflicts.length || !!repository.files.length
                "
                :title="branchActionHint(branch, 'merge')"
                @click="act({ operation: 'mergeBranch', reference: branch.name })"
              >
                合并</button
              ><button
                v-if="!branch.remote"
                type="button"
                :disabled="lock || branch.current"
                :title="branchActionHint(branch, 'delete')"
                class="danger-text"
                @click="
                  ask('删除本地分支？', `将删除本地分支 ${branch.name}。只有已合并的分支才能删除。`, {
                    operation: 'deleteBranch',
                    reference: branch.name,
                  })
                "
              >
                删除
              </button>
            </div>
          </div>
        </section>
      </div>
      <div v-else class="stash-view">
        <form class="inline-form" @submit.prevent="stash">
          <input
            ref="stashInput"
            v-model="stashMessage"
            aria-label="Stash 说明"
            placeholder="例如：切换分支前保存工作（可选）"
            :disabled="lock"
          /><button
            type="submit"
            class="primary-button"
            :disabled="lock || !repository.files.length || !!conflicts.length"
          >
            保存到 Stash
          </button>
        </form>
        <p class="workbench-hint">
          保存已跟踪文件的修改及未跟踪的新文件。恢复后请检查工作区与暂存区；发生冲突时会保留 Stash。
        </p>
        <p v-if="!repository.stashes.length" class="workbench-empty">暂无 Stash。可先保存手头工作，再切换分支。</p>
        <div
          v-for="item in repository.stashes"
          :key="item.reference"
          class="stash-row"
          @contextmenu="stashMenu($event, item)"
          @keydown="stashMenu($event, item)"
        >
          <div>
            <code>{{ item.reference }}</code
            ><strong :title="item.message">{{ item.message }}</strong>
          </div>
          <div class="button-row">
            <button
              type="button"
              :disabled="lock || !!conflicts.length"
              :title="conflicts.length ? '请先解决工作区冲突' : '恢复到工作区，并保留此 Stash 记录'"
              @click="act({ operation: 'applyStash', reference: item.reference })"
            >
              应用并保留</button
            ><button
              type="button"
              :disabled="lock || !!conflicts.length"
              :title="conflicts.length ? '请先解决工作区冲突' : '恢复成功后移除此记录；发生冲突时保留'"
              @click="act({ operation: 'popStash', reference: item.reference })"
            >
              恢复并移除</button
            ><button
              type="button"
              :disabled="lock"
              class="danger-text"
              title="删除此 Stash 记录，需要确认"
              @click="
                ask(
                  '删除 Stash？',
                  `${item.reference}：${item.message}\n删除后将无法从列表恢复这些修改，请确认已不再需要。`,
                  { operation: 'dropStash', reference: item.reference },
                )
              "
            >
              删除
            </button>
          </div>
        </div>
      </div>
      <p v-if="busy" class="workbench-progress" role="status">正在执行 Git 操作，请稍候…</p>
    </template>
  </div>
</template>

<style scoped>
.commit-message-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.generate-commit-message {
  width: 30px;
  height: 30px;
  padding: 0;
  color: var(--skin-accent);
}
.git-workbench {
  display: flex;
  min-height: 0;
  flex: 1;
  flex-direction: column;
  overflow: hidden;
  color: var(--skin-text);
  font-size: var(--text-body);
  container-type: inline-size;
}
.workbench-toolbar,
.repository-caption,
.workbench-tabs,
.git-workbench > .workbench-message,
.workbench-confirm {
  flex-shrink: 0;
}
button,
input,
textarea {
  font: inherit;
}
button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 7px 10px;
  border: 1px solid var(--skin-border);
  border-radius: 8px;
  background: var(--skin-surface);
  color: var(--skin-text-body);
  cursor: pointer;
  white-space: nowrap;
}
button:hover:not(:disabled) {
  border-color: var(--skin-accent-border);
  background: var(--skin-canvas);
  color: var(--skin-accent);
}
button:disabled {
  opacity: 0.45;
  cursor: default;
}
button:focus-visible,
input:focus-visible,
textarea:focus-visible,
.workbench-confirm:focus-visible {
  outline: 2px solid var(--skin-accent);
  outline-offset: 2px;
}
input,
textarea {
  min-width: 0;
  border: 1px solid var(--skin-border);
  border-radius: 8px;
  padding: 9px 10px;
  background: var(--skin-surface);
  color: inherit;
}
input {
  width: 100%;
  box-sizing: border-box;
}
textarea {
  resize: vertical;
  max-height: 140px;
  width: 100%;
  box-sizing: border-box;
  line-height: 1.6;
}
.workbench-toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
  justify-content: space-between;
  padding: 8px 12px 4px;
}
.workbench-toolbar > .app-select {
  width: 180px;
  min-width: 120px;
  flex: 1;
  max-width: 280px;
}
.workbench-sync,
.button-row,
.bulk-actions {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px;
}
.workbench-sync {
  gap: 5px;
}
.workbench-sync button {
  padding: 7px 8px;
  font-size: var(--text-small);
}
.refresh-button {
  width: 31px;
  height: 31px;
  padding: 0;
}
.repository-caption {
  flex: 1 1 170px;
  min-width: 0;
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 7px 14px;
  padding: 0;
  color: var(--skin-text-muted);
  font-size: var(--text-small);
}
.repository-caption > span:first-child {
  display: inline-flex;
  gap: 6px;
  align-items: center;
  color: var(--skin-accent);
}
.repository-caption code {
  display: none;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 100%;
}
.workbench-tabs {
  display: flex;
  gap: 8px;
  border-bottom: 1px solid var(--skin-border);
  padding: 0 16px;
}
.workbench-tabs button {
  border: 0;
  border-radius: 0;
  border-bottom: 2px solid transparent;
  padding: 7px 3px;
  background: transparent;
  gap: 6px;
}
.workbench-tabs button.active {
  color: var(--skin-accent);
  border-bottom-color: var(--skin-accent);
}
.workbench-tabs button span,
h4 > span {
  border-radius: 5px;
  background: var(--skin-accent-soft);
  padding: 1px 5px;
  font-size: var(--text-small);
  color: var(--skin-text-muted);
}
.workbench-message {
  margin: 10px 16px 0;
  padding: 10px 12px;
  border-radius: 8px;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  max-height: 130px;
  overflow: auto;
  font-size: var(--text-small);
  line-height: 1.6;
}
.workbench-error {
  background: var(--skin-danger-soft);
  color: var(--skin-danger);
}
.workbench-notice {
  background: var(--skin-subtle);
  color: var(--skin-success);
}
.workbench-confirm {
  margin: 12px 16px;
  border: 1px solid var(--skin-warning-border);
  background: var(--skin-warning-soft);
  padding: 14px;
  border-radius: 10px;
}
.workbench-confirm p {
  line-height: 1.7;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
.workbench-confirm .button-row {
  justify-content: flex-end;
}
.danger-button {
  color: var(--skin-danger);
  border-color: var(--skin-danger-border);
}
.primary-button {
  background: var(--skin-accent-soft);
  border-color: var(--skin-accent-border);
  color: var(--skin-accent);
}
.workbench-split {
  display: grid;
  grid-template-columns: minmax(210px, 38%) minmax(0, 1fr);
  flex: 1;
  min-height: 0;
  overflow: hidden;
}
.change-pane,
.history-list {
  min-width: 0;
  border-right: 1px solid var(--skin-border);
  overflow: auto;
}
.changes-view {
  grid-template-columns: minmax(210px, var(--git-sidebar-width, 32%)) 6px minmax(0, 1fr);
  grid-template-rows: minmax(0, 1fr) auto;
  grid-template-areas: 'files gutter diff' 'commit gutter diff';
}
.changes-view > .change-pane {
  grid-area: files;
  min-height: 0;
  border-right: 0;
}
.changes-view > .diff-pane {
  grid-area: diff;
  min-height: 0;
}
.git-splitter {
  grid-area: gutter;
  cursor: col-resize;
  touch-action: none;
  background: var(--skin-canvas);
  border-inline: 1px solid var(--skin-border);
}
.git-splitter:hover,
.git-splitter:focus-visible,
.resizing > .git-splitter {
  background: var(--skin-accent-border);
  outline: 0;
}
.resizing {
  user-select: none;
  cursor: col-resize;
}
.diff-heading {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  position: sticky;
  top: 0;
  z-index: 1;
  background: var(--skin-surface);
  border-bottom: 1px solid var(--skin-border);
}
.diff-heading h4 {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  flex: 1;
  min-width: 0;
  overflow-wrap: anywhere;
}
.diff-stats {
  display: flex;
  gap: 9px;
  font:
    var(--text-small) 'SFMono-Regular',
    Consolas,
    monospace;
  white-space: nowrap;
}
.diff-stats > span:first-child {
  color: var(--skin-success);
}
.diff-stats > span:last-child {
  color: var(--skin-danger);
}
.compact-back {
  display: none;
}
.ai-provider {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 45%;
  color: var(--skin-text-muted);
  font-size: var(--text-caption);
}
.commit-message-heading label {
  flex: 1;
}
.generate-action {
  display: inline-flex;
}
.configure-ai {
  padding: 0;
  border: 0;
  min-height: auto;
  color: var(--skin-accent);
  background: transparent;
  font-size: inherit;
}
.file-path {
  display: block;
}
.file-path strong {
  font-weight: 500;
}
.bulk-actions {
  padding: 12px;
}
.bulk-actions button {
  font-size: var(--text-small);
  padding: 6px 7px;
}
.workbench-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 9px;
  min-height: 85px;
  margin: 0;
  padding: 20px;
  color: var(--skin-text-muted);
  line-height: 1.8;
  text-align: center;
}
.file-group {
  margin: 0 0 14px;
}
h4 {
  font-size: var(--text-small);
  font-weight: 500;
  margin: 0;
}
.file-group h4 {
  padding: 5px 12px 7px;
  display: flex;
  gap: 8px;
  align-items: center;
}
.file-row {
  display: flex;
  align-items: center;
  padding: 0 7px 0 5px;
  gap: 2px;
}
.file-row.selected,
.history-entry.selected {
  background: var(--skin-canvas);
}
.file-select {
  flex: 1;
  min-width: 0;
  border: 0;
  border-radius: 4px;
  padding: 8px 5px;
  background: transparent;
  justify-content: flex-start;
  font-size: var(--text-small);
  text-align: left;
}
.file-select > span:last-child {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.file-select small {
  display: block;
  color: var(--skin-text-muted);
  font-size: var(--text-caption);
  overflow: hidden;
  text-overflow: ellipsis;
}
.file-status {
  min-width: 32px;
  color: var(--skin-success);
}
.file-status.conflict {
  color: var(--skin-danger);
}
.file-action {
  border: 0;
  background: transparent;
  width: 27px;
  height: 27px;
  padding: 0;
  font-size: 18px;
}
.discard-action {
  font-size: 16px;
  color: var(--skin-danger);
}
.commit-form {
  grid-area: commit;
  min-width: 0;
  display: grid;
  gap: 8px;
  border-top: 1px solid var(--skin-border);
  padding: 10px 12px;
}
.commit-form label {
  font-weight: 500;
  font-size: var(--text-small);
}
.workbench-hint {
  font-size: var(--text-small);
  color: var(--skin-text-muted);
  line-height: 1.7;
  margin: 0;
  overflow-wrap: anywhere;
}
.diff-pane {
  min-width: 0;
  overflow: auto;
  background: var(--skin-surface);
}
.diff-pane > h4 {
  padding: 12px 14px;
  border-bottom: 1px solid var(--skin-border);
  overflow-wrap: anywhere;
  line-height: 1.6;
}
.diff-pane > .workbench-hint {
  padding: 10px 14px;
  background: var(--skin-warning-soft);
  color: var(--skin-warning);
}
.history-split {
  grid-template-columns: minmax(210px, var(--git-sidebar-width, 32%)) 6px minmax(0, 1fr);
  grid-template-areas: 'history gutter diff';
}
.history-split > .history-list {
  grid-area: history;
  min-height: 0;
  border-right: 0;
}
.history-split > .diff-pane {
  grid-area: diff;
  min-height: 0;
}
.history-empty {
  flex-direction: column;
}
.history-empty p {
  margin: 0;
}
.history-empty strong {
  font-weight: 500;
  color: var(--skin-text);
}
.clear-search {
  flex-shrink: 0;
  width: 26px;
  height: 26px;
  padding: 0;
  border: 0;
}
.commit-full-message summary {
  cursor: pointer;
  color: var(--skin-text-body);
  padding: 4px 0;
}
.branch-notice {
  margin-top: 10px;
  color: var(--skin-warning);
}
button.danger-text {
  color: var(--skin-danger);
}
button.danger-text:hover:not(:disabled) {
  color: var(--skin-danger);
  border-color: var(--skin-danger-border);
  background: var(--skin-danger-soft);
}
.branch-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.current-badge {
  flex-shrink: 0;
}
.history-view {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
.history-filters {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  padding: 12px 16px;
  border-bottom: 1px solid var(--skin-border);
}
.search-input {
  display: flex;
  align-items: center;
  flex: 1;
  min-width: 120px;
  border: 1px solid var(--skin-border);
  border-radius: 8px;
  padding-left: 9px;
  color: var(--skin-text-muted);
}
.search-input input {
  border: 0;
  padding-left: 6px;
}
.history-filters > .app-select {
  width: 170px;
}
.history-entry {
  display: grid;
  justify-content: stretch;
  width: 100%;
  gap: 6px;
  padding: 13px 14px;
  text-align: left;
  border: 0;
  border-radius: 0;
  border-bottom: 1px solid var(--skin-surface);
  background: transparent;
  white-space: normal;
}
.history-subject {
  font-weight: 500;
  overflow-wrap: anywhere;
  line-height: 1.5;
}
.history-refs {
  color: var(--skin-accent);
  font-size: var(--text-small);
  overflow-wrap: anywhere;
}
.history-meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  color: var(--skin-text-muted);
  font-size: var(--text-small);
}
.load-more {
  display: flex;
  margin: 12px auto;
}
.commit-details {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 12px;
  padding: 13px 14px;
  border-bottom: 1px solid var(--skin-border);
  color: var(--skin-text-muted);
  font-size: var(--text-small);
  overflow-wrap: anywhere;
}
.commit-details button {
  margin-left: auto;
  font-size: var(--text-small);
}
.commit-full-message {
  flex-basis: 100%;
  min-width: 0;
}
.commit-details pre {
  margin: 5px 0 0;
  font: inherit;
  color: var(--skin-text-body);
  white-space: pre-wrap;
}
.branch-view,
.stash-view {
  overflow: auto;
  padding: 16px;
}
.inline-form {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 9px;
}
.inline-form input {
  flex: 1;
}
.branch-group {
  margin-top: 23px;
}
.branch-group > h4 {
  margin-bottom: 9px;
}
.branch-row,
.stash-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 10px;
  padding: 12px 0;
  border-bottom: 1px solid var(--skin-surface);
}
.branch-row > div:first-child,
.stash-row > div:first-child {
  flex: 1 1 160px;
  display: grid;
  gap: 5px;
  min-width: 0;
}
.branch-row strong {
  min-width: 0;
  max-width: 100%;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-weight: 500;
  overflow-wrap: anywhere;
}
.branch-row strong > svg {
  flex-shrink: 0;
}
.branch-row > .button-row,
.stash-row > .button-row {
  flex-shrink: 0;
}
.branch-row small,
.stash-row code {
  font-size: var(--text-small);
  color: var(--skin-text-muted);
}
.current-badge {
  font-size: var(--text-small);
  background: var(--skin-accent-soft);
  color: var(--skin-accent);
  border-radius: 5px;
  padding: 2px 6px;
}
.branch-row button,
.stash-row button {
  font-size: var(--text-small);
}
.stash-row strong {
  font-weight: 500;
  overflow-wrap: anywhere;
  line-height: 1.6;
}
.workbench-progress {
  color: var(--skin-accent);
  background: var(--skin-canvas);
  border-top: 1px solid var(--skin-accent-border);
  margin: 0;
  padding: 10px 16px;
  font-size: var(--text-small);
}
@container (max-width: 560px) {
  .workbench-split {
    grid-template-columns: 1fr;
    overflow: auto;
    display: block;
  }
  .change-pane,
  .history-list {
    border-right: 0;
    max-height: 380px;
  }
  .diff-pane {
    border-top: 1px solid var(--skin-border);
    min-height: 180px;
  }
  .history-split {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    grid-template-areas: 'history';
    overflow: hidden;
  }
  .history-split > .history-list {
    max-height: none;
  }
  .history-split > .diff-pane {
    display: none;
    grid-area: history;
    min-height: 0;
  }
  .history-split.show-diff > .history-list {
    display: none;
  }
  .history-split.show-diff > .diff-pane {
    display: block;
  }
  .changes-view {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr) auto;
    grid-template-areas: 'files' 'commit';
    overflow: hidden;
  }
  .changes-view > .change-pane {
    max-height: none;
  }
  .changes-view > .diff-pane {
    display: none;
    grid-area: files;
    min-height: 0;
  }
  .changes-view.show-diff > .change-pane {
    display: none;
  }
  .changes-view.show-diff > .diff-pane {
    display: block;
  }
  .git-splitter {
    display: none;
  }
  .compact-back {
    display: inline-flex;
    flex-shrink: 0;
    padding: 4px 6px;
  }
  .repository-caption code {
    display: none;
  }
  .workbench-sync button span {
    display: none;
  }
  .workbench-sync button {
    width: 32px;
    height: 32px;
    padding: 0;
  }
  .repository-caption {
    order: 3;
    flex-basis: 100%;
  }
  .workbench-toolbar {
    gap: 6px 8px;
    padding-left: 12px;
    padding-right: 12px;
  }
  .workbench-tabs {
    padding: 0 12px;
  }
  .inline-form {
    flex-wrap: wrap;
  }
  .history-filters > .app-select {
    flex: 1;
  }
}
</style>
