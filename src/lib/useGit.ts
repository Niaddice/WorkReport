import { invoke, isTauri } from '@tauri-apps/api/core'
import { ref } from 'vue'
import dayjs from 'dayjs'
import type { GitReportEntry } from './workspace'

export interface GitRepository {
  id: string
  name: string
  path: string
  author: string
  branch: string
}

export interface GitSettings {
  executable: string
  repositories: GitRepository[]
  includeMerges: boolean
  commitPrompt?: string
}

export interface GitCommit {
  hash: string
  author: string
  email: string
  committedAt: string
  message: string
}

export interface GitDailyLog {
  date: string
  startDate?: string
  endDate?: string
  repositories: {
    id: string
    name: string
    commits: GitCommit[]
    truncated: boolean
    error: string | null
    duplicateCommits?: number
  }[]
}

export function validateGitDateRange(startDate: string, endDate: string): string {
  const valid = (date: string) => /^\d{8}$/.test(date) && dayjs(date).format('YYYYMMDD') === date
  if (!valid(startDate) || !valid(endDate)) return '请选择有效的开始日期和结束日期。'
  if (startDate > endDate) return '开始日期不能晚于结束日期。'
  return ''
}

export const DEFAULT_GIT_COMMIT_PROMPT =
  '根据暂存区的更改生成简洁准确的中文 Git 提交说明。首行使用 Conventional Commits 格式（如 feat:、fix:、refactor:），概括主要改动；必要时空一行后列出关键变化。只描述实际更改，不编造测试结果或工作成果，只输出提交说明正文。'
export const DEFAULT_GIT_SETTINGS: GitSettings = {
  executable: 'git',
  repositories: [],
  includeMerges: false,
  commitPrompt: DEFAULT_GIT_COMMIT_PROMPT,
}

export function cloneGitSettings(value: GitSettings = DEFAULT_GIT_SETTINGS): GitSettings {
  return {
    executable: value.executable,
    repositories: value.repositories.map((repository) => ({ ...repository })),
    includeMerges: value.includeMerges,
    commitPrompt: value.commitPrompt ?? DEFAULT_GIT_COMMIT_PROMPT,
  }
}

export function validateGitSettings(settings: GitSettings): string {
  if ((settings.commitPrompt?.length ?? 0) > 2000 || settings.commitPrompt?.includes('\0'))
    return 'Git 提交说明提示词不能超过 2000 字，且不能包含空字符。'
  const absolute = (path: string) => /^(?:\/|[a-z]:[\\/]|\\\\)/i.test(path)
  if (
    !settings.executable.trim() ||
    /[\x00-\x1f\x7f]/.test(settings.executable) ||
    (settings.executable.trim() !== 'git' && !absolute(settings.executable.trim()))
  )
    return 'Git 程序请填写 git 或可执行文件的绝对路径。'
  if (settings.repositories.length > 20) return 'Git 仓库最多配置 20 个。'
  const paths = new Set<string>()
  for (const repository of settings.repositories) {
    if (!repository.name.trim() || repository.name.length > 100) return '请填写仓库名称，最多 100 字。'
    if (!absolute(repository.path.trim()) || /[\x00-\x1f\x7f]/.test(repository.path))
      return `${repository.name}：请填写仓库目录的绝对路径。`
    const path = repository.path.trim().replace(/[\\/]+$/, '')
    if (paths.has(path)) return '同一 Git 仓库目录不能重复添加。'
    paths.add(path)
    if (
      repository.author.length > 500 ||
      repository.branch.length > 500 ||
      /[\x00-\x1f\x7f]/.test(repository.author + repository.branch) ||
      repository.branch.trim().startsWith('-')
    )
      return `${repository.name}：作者或分支配置无效。`
  }
  return ''
}

/** Keep the original commit wording in a numbered report suitable for either journal format. */
export function gitDailyReport(log: GitDailyLog): string {
  const content = log.repositories
    .filter((repository) => repository.commits.length)
    .map((repository) => {
      const items = repository.commits.map((commit, index) => {
        const lines = commit.message.trim().split(/\r?\n/)
        const subject = lines.shift() || '未填写提交说明'
        const body = lines
          .filter((line) => line.trim())
          .map((line) => `   ${line}`)
          .join('\n')
        return `${index + 1}、${subject}${body ? `\n${body}` : ''}`
      })
      return `${repository.name}\n${items.join('\n')}`
    })
    .join('\n\n')
  const startDate = log.startDate ?? log.date
  const endDate = log.endDate ?? log.date
  return content && startDate !== endDate
    ? `Git 工作记录（${dayjs(startDate).format('YYYY-MM-DD')} ~ ${dayjs(endDate).format('YYYY-MM-DD')}）\n\n${content}`
    : content
}

/** Split by the same local commit date used when fetching the range. */
export function gitDailyReports(log: GitDailyLog, dates?: string[]): GitReportEntry[] {
  const startDate = log.startDate ?? log.date
  const endDate = log.endDate ?? log.date
  const allowed = dates ? new Set(dates) : null
  const days = new Map<string, GitDailyLog['repositories']>()
  for (const repository of log.repositories) {
    const commitsByDay = new Map<string, GitCommit[]>()
    for (const commit of repository.commits) {
      const date = dayjs(commit.committedAt).format('YYYYMMDD')
      if (validateGitDateRange(date, date) || date < startDate || date > endDate || (allowed && !allowed.has(date)))
        continue
      const commits = commitsByDay.get(date) ?? []
      commits.push(commit)
      commitsByDay.set(date, commits)
    }
    for (const [date, commits] of commitsByDay) {
      const repositories = days.get(date) ?? []
      repositories.push({ ...repository, commits })
      days.set(date, repositories)
    }
  }
  return [...days]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([date, repositories]) => ({ date, content: gitDailyReport({ date, repositories }) }))
}

export function useGit() {
  const settings = ref(cloneGitSettings())
  const loading = ref(false)
  const error = ref('')
  const reading = ref(false)
  const readError = ref('')
  const result = ref<GitDailyLog | null>(null)
  let settingsRevision = 0
  let readRevision = 0

  function clear() {
    readRevision++
    reading.value = false
    result.value = null
    readError.value = ''
  }

  function acceptSaved(value: GitSettings) {
    settingsRevision++
    settings.value = cloneGitSettings(value)
    loading.value = false
    error.value = ''
    clear()
  }

  async function load(): Promise<boolean> {
    const revision = ++settingsRevision
    loading.value = true
    error.value = ''
    settings.value = cloneGitSettings()
    clear()
    try {
      const value = isTauri() ? await invoke<GitSettings>('load_git_settings') : cloneGitSettings()
      if (revision !== settingsRevision) return false
      settings.value = cloneGitSettings(value)
      return true
    } catch (cause) {
      if (revision === settingsRevision) error.value = `读取 Git 设置失败：${String(cause)}`
      return false
    } finally {
      if (revision === settingsRevision) loading.value = false
    }
  }

  async function read(workspacePath: string, date: string, endDate = date): Promise<boolean> {
    clear()
    const revision = readRevision
    reading.value = true
    try {
      const validationError = validateGitDateRange(date, endDate)
      if (validationError) throw new Error(validationError)
      if (!isTauri()) throw new Error('请在桌面应用中读取本地 Git 提交。')
      const value = await invoke<GitDailyLog>('read_git_daily_log', {
        workspacePath,
        date,
        ...(endDate !== date ? { endDate } : {}),
      })
      if (revision !== readRevision) return false
      result.value = value
      return true
    } catch (cause) {
      if (revision === readRevision) readError.value = `获取提交失败：${String(cause)}`
      return false
    } finally {
      if (revision === readRevision) reading.value = false
    }
  }

  return { settings, loading, error, reading, readError, result, load, acceptSaved, clear, read }
}
