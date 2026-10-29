<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useId, watch } from 'vue'
import dayjs from 'dayjs'
import { PhArrowClockwise, PhGitBranch, PhGitCommit, PhMagnifyingGlass } from '@phosphor-icons/vue'
import AppSelect from './AppSelect.vue'
import type { GitRepository } from '../lib/useGit'
import {
  isGitStaged,
  isGitUnstaged,
  isGitUntracked,
  useGitWorkbench,
  type GitActionRequest,
  type GitFileChange,
  type GitHistoryCommit,
} from '../lib/useGitWorkbench'
import { api } from '../lib/workspace'

const props = withDefaults(
  defineProps<{ workspacePath: string; repositories: GitRepository[]; disabled: boolean; active?: boolean }>(),
  { active: true },
)
const emit = defineEmits<{ changed: [] }>()
const id = useId()
const repositoryId = ref(props.repositories[0]?.id ?? '')
const client = useGitWorkbench(() => ({ workspacePath: props.workspacePath, repositoryId: repositoryId.value }))
const { repository, history, diff, loading, historyLoading, diffLoading, busy, error, notice } = client
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
const query = ref('')
const appliedQuery = ref('')
const historyReference = ref('')
const selectedFile = ref<{ path: string; staged: boolean } | null>(null)
const selectedCommit = ref<GitHistoryCommit | null>(null)
const confirmation = ref<{ title: string; detail: string; request: GitActionRequest } | null>(null)
const confirmationPanel = ref<HTMLElement>()
let copyRevision = 0
const lock = computed(() => props.disabled || busy.value || loading.value || !!confirmation.value)
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
const diffLines = computed(() => (diff.value?.text ?? '').split('\n'))

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
  selectedFile.value = null
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
  await client.showDiff({ path: file.path, staged, commit: '' })
}

async function selectCommit(commit: GitHistoryCommit) {
  if (lock.value) return
  selectedFile.value = null
  selectedCommit.value = commit
  await client.showDiff({ path: '', staged: false, commit: commit.hash })
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

async function copyHash(hash: string) {
  const revision = ++copyRevision
  const scope = `${props.workspacePath}\0${repositoryId.value}`
  const current = () => revision === copyRevision && scope === `${props.workspacePath}\0${repositoryId.value}`
  try {
    await api.copy(hash)
    if (current()) notice.value = '提交哈希已复制。'
  } catch (cause) {
    if (current()) error.value = `复制失败：${String(cause)}`
  }
}

function lineClass(line: string) {
  if (line.startsWith('+++') || line.startsWith('---') || line.startsWith('diff ') || line.startsWith('index '))
    return 'diff-meta'
  if (line.startsWith('+')) return 'diff-add'
  if (line.startsWith('-')) return 'diff-remove'
  if (line.startsWith('@@')) return 'diff-hunk'
  return ''
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
    <div class="workbench-toolbar">
      <AppSelect
        v-model="repositoryId"
        :options="repositoryOptions"
        :disabled="lock || !repositories.length"
        aria-label="Git 仓库"
      />
      <div class="workbench-sync">
        <button type="button" :disabled="lock || !repository?.remotes.length" @click="act({ operation: 'fetch' })">
          Fetch 获取
        </button>
        <button
          type="button"
          :disabled="lock || !canSync"
          title="仅快进更新当前分支"
          @click="act({ operation: 'pull' })"
        >
          Pull 更新
        </button>
        <button type="button" :disabled="lock || !canSync" @click="act({ operation: 'push' })">Push 推送</button>
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
    <div v-if="repository" class="repository-caption">
      <span><PhGitBranch :size="15" />{{ repository.detached ? '分离 HEAD' : repository.branch || '尚无提交' }}</span>
      <span v-if="repository.upstream"
        >{{ repository.upstream }} · ↑{{ repository.ahead }} ↓{{ repository.behind }}</span
      >
      <span v-else>未配置跟踪分支</span>
      <code :title="repository.root">{{ repository.root }}</code>
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
      <div v-if="tab === 'changes'" class="workbench-split">
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
                ><span
                  >{{ file.path }}<small v-if="file.originalPath">原路径：{{ file.originalPath }}</small></span
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
          <form class="commit-form" @submit.prevent="commit()">
            <label :for="`${id}-message`">提交说明</label>
            <textarea
              :id="`${id}-message`"
              v-model="commitMessage"
              :disabled="lock"
              aria-label="提交说明"
              placeholder="概述这次更改…"
              rows="4"
              @keydown.enter.ctrl.prevent="commit()"
              @keydown.enter.meta.prevent="commit()"
            />
            <p class="workbench-hint">仅提交已暂存的 {{ staged.length }} 个文件。⌘ / Ctrl + Enter 提交。</p>
            <p v-if="repository.detached" class="workbench-hint">当前处于分离 HEAD，请先创建或切换本地分支。</p>
            <div class="button-row">
              <button type="submit" class="primary-button" :disabled="!canCommit">提交</button
              ><button type="button" :disabled="!canCommit || !canSync" @click="commit(true)">提交并推送</button>
            </div>
          </form>
        </div>
        <section class="diff-pane" :aria-label="diffTitle">
          <h4>{{ diffTitle }}</h4>
          <p v-if="diffLoading" class="workbench-empty" role="status">正在读取差异…</p>
          <template v-else-if="diff"
            ><p v-if="diff.truncated" class="workbench-hint">差异过大，仅显示部分内容。</p>
            <pre
              v-if="diff.text"
              class="diff-content"
            ><code v-for="(line, index) in diffLines" :key="index" :class="lineClass(line)">{{ line || ' ' }}</code></pre>
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
            />
          </div>
          <AppSelect
            v-model="historyReference"
            :options="historyOptions"
            :disabled="lock || historyLoading"
            aria-label="历史分支"
          /><button type="submit" :disabled="lock || historyLoading">搜索</button>
        </form>
        <div class="workbench-split">
          <div class="history-list">
            <p v-if="historyLoading && !history" class="workbench-empty" role="status">正在读取提交历史…</p>
            <p v-else-if="!history?.commits.length" class="workbench-empty">没有符合条件的提交。</p>
            <button
              v-for="entry in history?.commits"
              :key="entry.hash"
              type="button"
              class="history-entry"
              :class="{ selected: selectedCommit?.hash === entry.hash }"
              :disabled="lock"
              @click="selectCommit(entry)"
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
          <section class="diff-pane" aria-label="提交差异">
            <template v-if="selectedCommit"
              ><h4>{{ diffTitle }}</h4>
              <div class="commit-details">
                <span>{{ selectedCommit.author }} &lt;{{ selectedCommit.email }}&gt;</span
                ><span>{{ dayjs(selectedCommit.committedAt).format('YYYY-MM-DD HH:mm:ss') }}</span
                ><button type="button" @click="copyHash(selectedCommit.hash)">
                  复制哈希 {{ selectedCommit.hash.slice(0, 8) }}
                </button>
                <pre>{{ selectedCommit.message }}</pre>
              </div></template
            >
            <p v-if="diffLoading" class="workbench-empty" role="status">正在读取提交差异…</p>
            <template v-else-if="diff"
              ><p v-if="diff.truncated" class="workbench-hint">差异过大，仅显示部分内容。</p>
              <pre
                v-if="diff.text"
                class="diff-content"
              ><code v-for="(line, index) in diffLines" :key="index" :class="lineClass(line)">{{ line || ' ' }}</code></pre>
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
          <div v-for="branch in group.branches" :key="branch.name" class="branch-row">
            <div>
              <strong
                ><PhGitBranch :size="16" />{{ branch.name
                }}<span v-if="branch.current" class="current-badge">当前</span></strong
              ><small v-if="branch.upstream">跟踪 {{ branch.upstream }}</small>
            </div>
            <div class="button-row">
              <button
                type="button"
                :disabled="lock || branch.current || !!conflicts.length"
                @click="act({ operation: 'switchBranch', reference: branch.name })"
              >
                {{ branch.remote ? '检出跟踪分支' : '切换' }}</button
              ><button
                type="button"
                :disabled="
                  lock || branch.current || repository.detached || !!conflicts.length || !!repository.files.length
                "
                title="快进合并到当前分支"
                @click="act({ operation: 'mergeBranch', reference: branch.name })"
              >
                合并</button
              ><button
                v-if="!branch.remote"
                type="button"
                :disabled="lock || branch.current"
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
            v-model="stashMessage"
            aria-label="Stash 说明"
            placeholder="为暂存的工作添加说明（可选）"
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
        <div v-for="item in repository.stashes" :key="item.reference" class="stash-row">
          <div>
            <code>{{ item.reference }}</code
            ><strong>{{ item.message }}</strong>
          </div>
          <div class="button-row">
            <button
              type="button"
              :disabled="lock || !!conflicts.length"
              @click="act({ operation: 'applyStash', reference: item.reference })"
            >
              应用并保留</button
            ><button
              type="button"
              :disabled="lock || !!conflicts.length"
              @click="act({ operation: 'popStash', reference: item.reference })"
            >
              恢复并移除</button
            ><button
              type="button"
              :disabled="lock"
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
.git-workbench {
  display: flex;
  min-height: 0;
  height: 100%;
  flex-direction: column;
  overflow: auto;
  color: #39485d;
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
  border: 1px solid #e1e7f0;
  border-radius: 8px;
  background: var(--skin-surface, #fff);
  color: #52647c;
  cursor: pointer;
  white-space: nowrap;
}
button:hover:not(:disabled) {
  border-color: #b7cef1;
  background: #f0f5fe;
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
  border: 1px solid #dde5ef;
  border-radius: 8px;
  padding: 9px 10px;
  background: var(--skin-surface, #fff);
  color: inherit;
}
input {
  width: 100%;
  box-sizing: border-box;
}
textarea {
  resize: vertical;
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
  padding: 14px 16px 9px;
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
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 7px 14px;
  padding: 3px 16px 12px;
  color: #8190a5;
  font-size: var(--text-small);
}
.repository-caption > span:first-child {
  display: inline-flex;
  gap: 6px;
  align-items: center;
  color: var(--skin-accent);
}
.repository-caption code {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 100%;
}
.workbench-tabs {
  display: flex;
  gap: 8px;
  border-bottom: 1px solid #e6ecf4;
  padding: 0 16px;
}
.workbench-tabs button {
  border: 0;
  border-radius: 0;
  border-bottom: 2px solid transparent;
  padding: 10px 3px;
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
  background: #edf2f9;
  padding: 1px 5px;
  font-size: var(--text-small);
  color: #8190a5;
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
  background: #fff1ef;
  color: #ad4d42;
}
.workbench-notice {
  background: #eff7ef;
  color: #4c7453;
}
.workbench-confirm {
  margin: 12px 16px;
  border: 1px solid #efcea8;
  background: #fff9ef;
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
  color: #b04c42;
  border-color: #e9b7ae;
}
.primary-button {
  background: #e7f0fe;
  border-color: #d3e3fd;
  color: var(--skin-accent);
}
.workbench-split {
  display: grid;
  grid-template-columns: minmax(210px, 38%) minmax(0, 1fr);
  flex: 1;
  min-height: 220px;
  overflow: auto;
}
.change-pane,
.history-list {
  min-width: 0;
  border-right: 1px solid #e6ecf4;
  overflow: auto;
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
  color: #8190a5;
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
  background: #edf4ff;
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
  color: #8190a5;
  font-size: 10px;
  overflow: hidden;
  text-overflow: ellipsis;
}
.file-status {
  min-width: 32px;
  color: #4f8b6f;
}
.file-status.conflict {
  color: #bd6958;
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
  color: #a07166;
}
.commit-form {
  display: grid;
  gap: 8px;
  border-top: 1px solid #e6ecf4;
  padding: 13px 12px;
}
.commit-form label {
  font-weight: 500;
  font-size: var(--text-small);
}
.workbench-hint {
  font-size: var(--text-small);
  color: #8190a5;
  line-height: 1.7;
  margin: 0;
  overflow-wrap: anywhere;
}
.diff-pane {
  min-width: 0;
  overflow: auto;
  background: #fbfcfe;
}
.diff-pane > h4 {
  padding: 12px 14px;
  border-bottom: 1px solid #e6ecf4;
  overflow-wrap: anywhere;
  line-height: 1.6;
}
.diff-pane > .workbench-hint {
  padding: 10px 14px;
  background: #fff8e8;
  color: #946f38;
}
.diff-content {
  font-size: var(--text-small);
  line-height: 1.7;
  margin: 0;
  padding: 10px 0;
  tab-size: 4;
}
.diff-content code {
  display: block;
  box-sizing: border-box;
  width: max-content;
  min-width: 100%;
  padding: 0 14px;
}
.diff-add {
  color: #35714b;
  background: #eaf7ee;
}
.diff-remove {
  color: #a84b4b;
  background: #fff0ee;
}
.diff-hunk {
  color: #677faa;
  background: #edf2fc;
}
.diff-meta {
  color: #8190a5;
}
.history-view {
  flex: 1;
  min-height: 280px;
  display: flex;
  flex-direction: column;
}
.history-filters {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  padding: 12px 16px;
  border-bottom: 1px solid #e6ecf4;
}
.search-input {
  display: flex;
  align-items: center;
  flex: 1;
  min-width: 120px;
  border: 1px solid #dde5ef;
  border-radius: 8px;
  padding-left: 9px;
  color: #8190a5;
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
  border-bottom: 1px solid #edf0f6;
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
  color: #8190a5;
  font-size: var(--text-small);
}
.load-more {
  display: flex;
  margin: 12px auto;
}
.commit-details {
  display: grid;
  gap: 6px;
  padding: 13px 14px;
  border-bottom: 1px solid #e6ecf4;
  color: #8190a5;
  font-size: var(--text-small);
  overflow-wrap: anywhere;
}
.commit-details button {
  justify-self: start;
  font-size: var(--text-small);
}
.commit-details pre {
  margin: 5px 0 0;
  font: inherit;
  color: #52647c;
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
  border-bottom: 1px solid #edf0f6;
}
.branch-row > div:first-child,
.stash-row > div:first-child {
  display: grid;
  gap: 5px;
  min-width: 0;
}
.branch-row strong {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-weight: 500;
  overflow-wrap: anywhere;
}
.branch-row small,
.stash-row code {
  font-size: var(--text-small);
  color: #8190a5;
}
.current-badge {
  font-size: var(--text-small);
  background: #e7f0fe;
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
  background: #edf4ff;
  border-top: 1px solid #d3e3fd;
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
    border-top: 1px solid #e6ecf4;
    min-height: 180px;
  }
  .repository-caption code {
    display: none;
  }
  .workbench-toolbar {
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
