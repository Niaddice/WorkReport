import { invoke, isTauri } from '@tauri-apps/api/core'
import { ref } from 'vue'

export interface GitFileChange {
  path: string
  originalPath: string | null
  indexStatus: string
  worktreeStatus: string
  conflicted: boolean
}

export interface GitBranch {
  name: string
  remote: boolean
  current: boolean
  upstream: string | null
}

export interface GitRepositoryState {
  root: string
  branch: string
  detached: boolean
  upstream: string | null
  ahead: number
  behind: number
  files: GitFileChange[]
  branches: GitBranch[]
  stashes: { reference: string; message: string }[]
  remotes: string[]
}

export interface GitHistoryCommit {
  hash: string
  parents: string[]
  author: string
  email: string
  committedAt: string
  message: string
  refs: string
}

export interface GitHistoryResult {
  commits: GitHistoryCommit[]
  hasMore: boolean
}

export interface GitDiffResult {
  text: string
  truncated: boolean
}

export type GitOperation =
  | 'stage'
  | 'unstage'
  | 'discard'
  | 'commit'
  | 'createBranch'
  | 'switchBranch'
  | 'deleteBranch'
  | 'mergeBranch'
  | 'stash'
  | 'applyStash'
  | 'popStash'
  | 'dropStash'
  | 'fetch'
  | 'pull'
  | 'push'

export interface GitActionRequest {
  operation: GitOperation
  paths: string[]
  message: string
  reference: string
}

export interface GitWorkbenchScope {
  workspacePath: string
  repositoryId: string
}

export function isGitStaged(file: GitFileChange): boolean {
  return !file.conflicted && ![' ', '?', '!'].includes(file.indexStatus)
}

export function isGitUntracked(file: GitFileChange): boolean {
  return file.indexStatus === '?' && file.worktreeStatus === '?'
}

export function isGitUnstaged(file: GitFileChange): boolean {
  return !file.conflicted && !isGitUntracked(file) && ![' ', '!'].includes(file.worktreeStatus)
}

/** Each view has its own revision; late responses never replace a newer selection. */
export function useGitWorkbench(scope: () => GitWorkbenchScope) {
  const repository = ref<GitRepositoryState | null>(null)
  const history = ref<GitHistoryResult | null>(null)
  const diff = ref<GitDiffResult | null>(null)
  const loading = ref(false)
  const historyLoading = ref(false)
  const diffLoading = ref(false)
  const busy = ref(false)
  const error = ref('')
  const notice = ref('')
  let scopeRevision = 0
  let repositoryRevision = 0
  let historyRevision = 0
  let diffRevision = 0
  let actionRevision = 0

  function context() {
    const value = { ...scope() }
    const revision = scopeRevision
    return {
      value,
      valid: () =>
        revision === scopeRevision &&
        value.workspacePath === scope().workspacePath &&
        value.repositoryId === scope().repositoryId,
    }
  }

  function assertDesktop(value: GitWorkbenchScope) {
    if (!isTauri()) throw new Error('请在桌面应用中管理本地 Git 仓库。')
    if (!value.workspacePath || !value.repositoryId) throw new Error('请先选择工作空间和 Git 仓库。')
  }

  function clear() {
    scopeRevision++
    repositoryRevision++
    historyRevision++
    diffRevision++
    actionRevision++
    repository.value = null
    history.value = null
    diff.value = null
    loading.value = false
    historyLoading.value = false
    diffLoading.value = false
    busy.value = false
    error.value = ''
    notice.value = ''
  }

  async function refresh(): Promise<boolean> {
    const target = context()
    const revision = ++repositoryRevision
    loading.value = true
    error.value = ''
    try {
      assertDesktop(target.value)
      const value = await invoke<GitRepositoryState>('read_git_repository', target.value)
      if (!target.valid() || revision !== repositoryRevision) return false
      repository.value = value
      return true
    } catch (cause) {
      if (target.valid() && revision === repositoryRevision) error.value = `读取仓库失败：${String(cause)}`
      return false
    } finally {
      if (target.valid() && revision === repositoryRevision) loading.value = false
    }
  }

  async function loadHistory(options: { query: string; reference: string; skip: number }): Promise<boolean> {
    const target = context()
    const revision = ++historyRevision
    historyLoading.value = true
    error.value = ''
    if (!options.skip) history.value = null
    try {
      assertDesktop(target.value)
      const value = await invoke<GitHistoryResult>('read_git_history', { ...target.value, ...options })
      if (!target.valid() || revision !== historyRevision) return false
      history.value = options.skip
        ? { commits: [...(history.value?.commits ?? []), ...value.commits], hasMore: value.hasMore }
        : value
      return true
    } catch (cause) {
      if (target.valid() && revision === historyRevision) error.value = `读取历史失败：${String(cause)}`
      return false
    } finally {
      if (target.valid() && revision === historyRevision) historyLoading.value = false
    }
  }

  async function showDiff(options: { path: string; staged: boolean; commit: string }): Promise<boolean> {
    const target = context()
    const revision = ++diffRevision
    diff.value = null
    diffLoading.value = true
    error.value = ''
    try {
      assertDesktop(target.value)
      const value = await invoke<GitDiffResult>('read_git_diff', { ...target.value, ...options })
      if (!target.valid() || revision !== diffRevision) return false
      diff.value = value
      return true
    } catch (cause) {
      if (target.valid() && revision === diffRevision) error.value = `读取差异失败：${String(cause)}`
      return false
    } finally {
      if (target.valid() && revision === diffRevision) diffLoading.value = false
    }
  }

  function clearDiff() {
    diffRevision++
    diff.value = null
    diffLoading.value = false
  }

  async function runAction(request: Partial<GitActionRequest> & { operation: GitOperation }): Promise<boolean> {
    if (busy.value || loading.value) return false
    const target = context()
    const revision = ++actionRevision
    const valid = () => target.valid() && revision === actionRevision
    busy.value = true
    error.value = ''
    notice.value = ''
    repositoryRevision++
    historyRevision++
    historyLoading.value = false
    clearDiff()
    try {
      assertDesktop(target.value)
      const output = await invoke<string>('git_repository_action', {
        ...target.value,
        request: { paths: [], message: '', reference: '', ...request },
      })
      if (!valid()) return false
      // A successful mutation is still successful if the subsequent status refresh fails.
      notice.value = output?.trim() || 'Git 操作已完成。'
      history.value = null
      await refresh()
      return valid()
    } catch (cause) {
      if (!valid()) return false
      const message = `Git 操作失败：${String(cause)}`
      // Merge, pull and stash application can leave conflicts even when Git exits with an error.
      await refresh()
      if (valid()) error.value = message
      return false
    } finally {
      if (valid()) busy.value = false
    }
  }

  return {
    repository,
    history,
    diff,
    loading,
    historyLoading,
    diffLoading,
    busy,
    error,
    notice,
    refresh,
    loadHistory,
    showDiff,
    clearDiff,
    runAction,
    clear,
  }
}
