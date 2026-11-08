import { invoke, isTauri } from '@tauri-apps/api/core'
import { ref } from 'vue'
import { aiApi } from './ai'

export interface GitCommitItem {
  hash: string
  author: string
  email: string
  committedAt: string
  message: string
}

export interface GitRepositoryLogItem {
  id: string
  name: string
  commits: GitCommitItem[]
  truncated: boolean
  error?: string | null
  duplicateCommits: number
}

export interface GitDailyLogResult {
  date: string
  startDate: string
  endDate: string
  repositories: GitRepositoryLogItem[]
}

export const DEFAULT_GIT_JOURNAL_PROMPT =
  '你是一名专业的工作总结与日报助理。请根据以下 Git 提交记录，整理并生成一份清晰、条理分明、重点突出的中文工作日报草稿（Markdown 格式）。\n\n' +
  '输出结构要求：\n' +
  '### 今日工作进展与成果\n' +
  '- 按项目或业务模块分类归纳，概括具体的业务功能与改进，突出实际成果，不要简单堆砌提交说明；\n' +
  '### 技术攻关与问题修复\n' +
  '- 归纳关键的缺陷修复、性能调优或代码重构；\n' +
  '### 明日计划与待办事项\n' +
  '- 根据今日提交内容合理推导未完结事项或后续计划。\n\n' +
  '原则：\n' +
  '- 只基于提供的 Git 提交事实，不要凭空编造不存在的功能或测试结果；\n' +
  '- 语言简练专业，直接输出 Markdown 内容，不加任何前言或客套话。'

export function formatCommitsForJournalDraft(dailyLog: GitDailyLogResult): { text: string; commitCount: number } {
  const parts: string[] = []
  let totalCommits = 0

  for (const repo of dailyLog.repositories) {
    if (!repo.commits || repo.commits.length === 0) continue
    totalCommits += repo.commits.length

    const commitLines = repo.commits.map((c, idx) => {
      const msg = c.message.trim() || '未填写提交说明'
      return `  ${idx + 1}. [${c.hash.slice(0, 7)}] ${msg}`
    })

    parts.push(`【项目：${repo.name}】\n${commitLines.join('\n')}`)
  }

  return {
    text: parts.join('\n\n'),
    commitCount: totalCommits,
  }
}

export function useGitJournalDraft() {
  const generating = ref(false)
  const error = ref('')
  let currentRequestId = ''

  function cancel() {
    if (currentRequestId && isTauri()) {
      void aiApi.cancel(currentRequestId).catch(() => {})
    }
    currentRequestId = ''
    generating.value = false
  }

  async function generate(
    workspacePath: string,
    date: string,
    customPrompt?: string,
  ): Promise<{ content: string; commitCount: number } | null> {
    if (generating.value || !workspacePath || !date) return null
    cancel()

    error.value = ''
    generating.value = true
    const requestId = crypto.randomUUID()
    currentRequestId = requestId

    try {
      if (!isTauri()) {
        throw new Error('桌面环境下可用，浏览器预览模式暂不支持。')
      }

      const log = await invoke<GitDailyLogResult>('read_git_daily_log', {
        workspacePath,
        date,
        endDate: null,
      })

      if (currentRequestId !== requestId) return null

      const { text, commitCount } = formatCommitsForJournalDraft(log)
      if (commitCount === 0) {
        throw new Error('在已配置的 Git 仓库中未找到选定日期的提交记录。请先确认是否已提交代码或配置了本地仓库。')
      }

      const prompt = customPrompt || DEFAULT_GIT_JOURNAL_PROMPT
      const content = await aiApi.summarize(text, requestId, prompt)

      if (currentRequestId !== requestId) return null
      if (!content?.trim()) {
        throw new Error('AI 未返回生成内容，请重试。')
      }

      return {
        content: content.trim(),
        commitCount,
      }
    } catch (cause) {
      if (currentRequestId === requestId) {
        error.value = String(cause)
      }
      return null
    } finally {
      if (currentRequestId === requestId) {
        generating.value = false
        currentRequestId = ''
      }
    }
  }

  return {
    generating,
    error,
    generate,
    cancel,
  }
}
