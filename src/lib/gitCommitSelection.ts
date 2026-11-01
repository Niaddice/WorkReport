import { computed, ref, watch } from 'vue'
import dayjs from 'dayjs'
import type { GitRepository } from './useGit'
import type { GitHistoryCommit } from './useGitWorkbench'

export interface SelectedGitCommit {
  repository: Pick<GitRepository, 'id' | 'name' | 'path'>
  commit: GitHistoryCommit
}

export const MAX_SELECTED_COMMITS = 500
export const gitCommitKey = (repositoryId: string, hash: string) => `${repositoryId}\0${hash}`
export function cloneSelectedCommit(entry: SelectedGitCommit): SelectedGitCommit {
  return { repository: { ...entry.repository }, commit: { ...entry.commit, parents: [...entry.commit.parents] } }
}

export function groupSelectedCommits(entries: SelectedGitCommit[]) {
  const groups = new Map<string, { repository: SelectedGitCommit['repository']; commits: GitHistoryCommit[] }>()
  for (const entry of entries) {
    let group = groups.get(entry.repository.id)
    if (!group) {
      group = { repository: entry.repository, commits: [] }
      groups.set(entry.repository.id, group)
    }
    group.commits.push(entry.commit)
  }
  return [...groups.values()].map((group) => ({
    ...group,
    commits: [...group.commits].sort(
      (a, b) => Date.parse(b.committedAt) - Date.parse(a.committedAt) || a.hash.localeCompare(b.hash),
    ),
  }))
}

export function selectedCommitInformation(entries: SelectedGitCommit[]): string {
  return groupSelectedCommits(entries)
    .map(
      ({ repository, commits }) =>
        `${repository.name}\n${repository.path}\n\n${commits
          .map((commit) =>
            [
              `提交：${commit.hash}`,
              `作者：${commit.author} <${commit.email}>`,
              `时间：${dayjs(commit.committedAt).format('YYYY-MM-DD HH:mm:ss Z')}`,
              ...(commit.refs ? [`引用：${commit.refs}`] : []),
              ...(commit.parents.length ? [`父提交：${commit.parents.join(' ')}`] : []),
              '',
              commit.message.trim() || '未填写提交说明',
            ].join('\n'),
          )
          .join('\n\n────────────────────\n\n')}`,
    )
    .join('\n\n════════════════════\n\n')
}

export function selectedCommitReport(entries: SelectedGitCommit[]): string {
  return groupSelectedCommits(entries)
    .map(
      ({ repository, commits }) =>
        `${repository.name}\n${commits
          .map((commit, index) => {
            const [subject, ...body] = commit.message.trim().split(/\r?\n/)
            return `${index + 1}、${subject || '未填写提交说明'}${body.length ? `\n${body.map((line) => `   ${line}`).join('\n')}` : ''}`
          })
          .join('\n')}`,
    )
    .join('\n\n')
}

/** Selections survive repository switches, searches and pagination, but never cross workspaces or repository paths. */
export function useGitCommitSelection(scope: () => { workspacePath: string; repositories: GitRepository[] }) {
  const entries = ref<SelectedGitCommit[]>([])
  const error = ref('')
  const keys = computed(
    () => new Set(entries.value.map((entry) => gitCommitKey(entry.repository.id, entry.commit.hash))),
  )
  const repositoryCount = computed(() => new Set(entries.value.map((entry) => entry.repository.id)).size)
  const configurationKey = computed(() =>
    JSON.stringify([scope().workspacePath, scope().repositories.map(({ id, path }) => [id, path])]),
  )
  watch(
    scope,
    (value, previous) => {
      error.value = ''
      if (previous && value.workspacePath !== previous.workspacePath) {
        entries.value = []
        return
      }
      entries.value = entries.value.flatMap((entry) => {
        const repository = value.repositories.find(
          (repo) => repo.id === entry.repository.id && repo.path === entry.repository.path,
        )
        return repository
          ? [{ ...entry, repository: { id: repository.id, name: repository.name, path: repository.path } }]
          : []
      })
    },
    { deep: true, flush: 'sync' },
  )

  function has(repositoryId: string, hash: string) {
    return keys.value.has(gitCommitKey(repositoryId, hash))
  }
  function set(repositoryId: string, commits: GitHistoryCommit[], checked: boolean): boolean {
    const repository = scope().repositories.find((repo) => repo.id === repositoryId)
    if (!scope().workspacePath || !repository) return false
    error.value = ''
    const next = new Map(entries.value.map((entry) => [gitCommitKey(entry.repository.id, entry.commit.hash), entry]))
    for (const commit of commits) {
      const key = gitCommitKey(repositoryId, commit.hash)
      if (checked) next.set(key, cloneSelectedCommit({ repository, commit }))
      else next.delete(key)
    }
    if (next.size > MAX_SELECTED_COMMITS) {
      error.value = `最多同时选择 ${MAX_SELECTED_COMMITS} 条提交，请缩小选择范围。`
      return false
    }
    entries.value = [...next.values()]
    return true
  }
  function clear(repositoryId?: string) {
    entries.value = repositoryId ? entries.value.filter((entry) => entry.repository.id !== repositoryId) : []
    error.value = ''
  }
  return { entries, repositoryCount, configurationKey, error, has, set, clear }
}
