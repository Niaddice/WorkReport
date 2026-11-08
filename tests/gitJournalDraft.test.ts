import { describe, expect, it, vi } from 'vitest'
import { formatCommitsForJournalDraft, useGitJournalDraft, type GitDailyLogResult } from '../src/lib/useGitJournalDraft'

const { invoke, isTauri } = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauri: vi.fn(() => true),
}))

vi.mock('@tauri-apps/api/core', () => {
  class MockChannel {
    onmessage: (chunk: unknown) => void = () => {}
  }
  return {
    invoke,
    isTauri,
    Channel: MockChannel,
  }
})

describe('useGitJournalDraft', () => {
  it('formats commits from multiple repositories into readable text', () => {
    const dailyLog: GitDailyLogResult = {
      date: '20261108',
      startDate: '20261108',
      endDate: '20261108',
      repositories: [
        {
          id: 'repo1',
          name: '前端项目',
          truncated: false,
          duplicateCommits: 0,
          commits: [
            {
              hash: 'abcdef123456',
              author: 'Alice',
              email: 'alice@example.com',
              committedAt: '2026-11-08T10:00:00Z',
              message: 'feat: 支持从 Git 提交生成日报',
            },
          ],
        },
        {
          id: 'repo2',
          name: '后端项目',
          truncated: false,
          duplicateCommits: 0,
          commits: [
            {
              hash: '789012345678',
              author: 'Bob',
              email: 'bob@example.com',
              committedAt: '2026-11-08T10:30:00Z',
              message: 'fix: 优化 Git Diff 降噪管道',
            },
          ],
        },
      ],
    }

    const { text, commitCount } = formatCommitsForJournalDraft(dailyLog)
    expect(commitCount).toBe(2)
    expect(text).toContain('【项目：前端项目】')
    expect(text).toContain('[abcdef1] feat: 支持从 Git 提交生成日报')
    expect(text).toContain('【项目：后端项目】')
    expect(text).toContain('[7890123] fix: 优化 Git Diff 降噪管道')
  })

  it('generates structured journal using read_git_daily_log and summarize_entries', async () => {
    invoke.mockImplementation(async (command: string) => {
      if (command === 'read_git_daily_log') {
        return {
          date: '20261108',
          startDate: '20261108',
          endDate: '20261108',
          repositories: [
            {
              id: 'repo1',
              name: 'WorkReport',
              truncated: false,
              duplicateCommits: 0,
              commits: [
                {
                  hash: '111111111111',
                  author: 'Dev',
                  email: 'dev@test.com',
                  committedAt: '2026-11-08T09:00:00Z',
                  message: 'feat: 完成第二阶段开发',
                },
              ],
            },
          ],
        }
      }
      if (command === 'summarize_entries') {
        return '### 今日工作进展与成果\n- 完成第二阶段开发'
      }
      return null
    })

    const { generate, generating, error } = useGitJournalDraft()
    expect(generating.value).toBe(false)

    const result = await generate('/path/to/workspace', '20261108')
    expect(result).not.toBeNull()
    expect(result?.commitCount).toBe(1)
    expect(result?.content).toContain('### 今日工作进展与成果')
    expect(error.value).toBe('')
    expect(generating.value).toBe(false)
  })

  it('reports error when no commits are found', async () => {
    invoke.mockImplementation(async (command: string) => {
      if (command === 'read_git_daily_log') {
        return {
          date: '20261108',
          startDate: '20261108',
          endDate: '20261108',
          repositories: [
            {
              id: 'repo1',
              name: 'EmptyRepo',
              truncated: false,
              duplicateCommits: 0,
              commits: [],
            },
          ],
        }
      }
      return null
    })

    const { generate, error } = useGitJournalDraft()
    const result = await generate('/path/to/workspace', '20261108')
    expect(result).toBeNull()
    expect(error.value).toContain('未找到选定日期的提交记录')
  })
})
