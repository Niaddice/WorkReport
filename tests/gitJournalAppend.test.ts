import { beforeEach, describe, expect, it, vi } from 'vitest'
import { api, type Document, type GitAppendResult } from '../src/lib/workspace'
import { useJournal } from '../src/lib/useJournal'

vi.mock('../src/lib/workspace', () => ({
  api: {
    backupWarning: vi.fn(),
    retryBackup: vi.fn(),
    open: vi.fn(),
    list: vi.fn(),
    previews: vi.fn(),
    read: vi.fn(),
    create: vi.fn(),
    save: vi.fn(),
    switchFormat: vi.fn(),
    appendGitReports: vi.fn(),
  },
}))

const original: Document = {
  date: '20261028',
  filename: '20261028.md',
  format: 'md',
  content: '已有工作记录',
  revision: 'v1',
}
const reports = [{ date: original.date, content: '日报项目\n1、Git 工作记录' }]
const written: Document = { ...original, content: `${original.content}\n\n${reports[0]!.content}`, revision: 'v2' }
const outcome: GitAppendResult = { documents: [written], skippedDates: [], failed: [] }

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

async function setup() {
  vi.mocked(api.open).mockResolvedValue({ path: '/workspace', name: 'workspace' })
  vi.mocked(api.list).mockResolvedValue([{ date: original.date, filename: original.filename, format: original.format }])
  vi.mocked(api.previews).mockResolvedValue([])
  vi.mocked(api.read).mockResolvedValue({ ...original })
  const journal = useJournal()
  journal.selectedDate.value = original.date
  await journal.open('/workspace')
  return journal
}

beforeEach(() => vi.resetAllMocks())

describe('Git journal batch append', () => {
  it('flushes unsaved work before appending and keeps the current document synchronized with undo history', async () => {
    const journal = await setup()
    const events: string[] = []
    vi.mocked(api.save).mockImplementation(async (date, content) => {
      events.push('save draft')
      return { ...original, date, content, revision: 'draft-saved' }
    })
    const combined = { ...written, content: `未保存的用户修改\n\n${reports[0]!.content}` }
    vi.mocked(api.appendGitReports).mockImplementation(async () => {
      events.push('append reports')
      return { ...outcome, documents: [combined] }
    })
    journal.edit('未保存的用户修改')
    try {
      expect(await journal.appendGitReports('/workspace', reports)).toEqual({ ...outcome, documents: [combined] })
      expect(events).toEqual(['save draft', 'append reports'])
      expect(api.appendGitReports).toHaveBeenCalledExactlyOnceWith('/workspace', reports)
      expect(journal.selectedDate.value).toBe(original.date)
      expect(journal.draft.value).toBe(combined.content)
      expect(journal.document.value?.revision).toBe(combined.revision)
      expect(journal.dirty.value).toBe(false)
      expect(journal.undo()).toBe(true)
      expect(journal.draft.value).toBe('未保存的用户修改')
    } finally {
      journal.dispose()
    }
  })

  it('locks editing, selection and duplicate append while the batch is pending', async () => {
    const journal = await setup()
    const pending = deferred<GitAppendResult>()
    vi.mocked(api.appendGitReports).mockReturnValueOnce(pending.promise)
    const running = journal.appendGitReports('/workspace', reports)
    await vi.waitFor(() => expect(api.appendGitReports).toHaveBeenCalledTimes(1))
    try {
      expect(journal.busy.value).toBe(true)
      expect(await journal.select('20261029')).toBe(false)
      expect(await journal.open('/other-workspace')).toBe(false)
      expect(await journal.appendGitReports('/workspace', reports)).toBeNull()
      journal.edit('不应覆盖待保存内容')
      expect(journal.draft.value).toBe(original.content)
      expect(api.appendGitReports).toHaveBeenCalledTimes(1)
      pending.resolve(outcome)
      expect(await running).toEqual(outcome)
      expect(journal.busy.value).toBe(false)
      expect(journal.draft.value).toBe(written.content)
    } finally {
      journal.dispose()
    }
  })

  it('does not append when flushing the current draft fails', async () => {
    const journal = await setup()
    vi.mocked(api.save).mockRejectedValue(new Error('外部修改冲突'))
    journal.edit('保留当前草稿')
    try {
      expect(await journal.appendGitReports('/workspace', reports)).toBeNull()
      expect(api.appendGitReports).not.toHaveBeenCalled()
      expect(journal.draft.value).toBe('保留当前草稿')
      expect(journal.error.value).toContain('外部修改冲突')
      expect(journal.busy.value).toBe(false)
    } finally {
      journal.dispose()
    }
  })

  it('returns persisted successes even when refreshing the journal index fails', async () => {
    const journal = await setup()
    vi.mocked(api.appendGitReports).mockResolvedValue(outcome)
    vi.mocked(api.list).mockRejectedValueOnce(new Error('索引刷新失败'))
    try {
      expect(await journal.appendGitReports('/workspace', reports)).toEqual(outcome)
      expect(journal.draft.value).toBe(written.content)
      expect(journal.error.value).toContain('刷新')
      expect(journal.busy.value).toBe(false)
    } finally {
      journal.dispose()
    }
  })

  it('refreshes the selected journals even when optional calendar snippets fail', async () => {
    const journal = await setup()
    const afterSave = vi.fn(async () => {})
    vi.mocked(api.appendGitReports).mockResolvedValue(outcome)
    vi.mocked(api.previews).mockRejectedValueOnce(new Error('日历预览读取失败'))
    try {
      expect(await journal.appendGitReports('/workspace', reports, { afterSave })).toEqual(outcome)
      expect(afterSave).toHaveBeenCalledOnce()
      expect(journal.draft.value).toBe(written.content)
      expect(journal.error.value).toBe('')
    } finally {
      journal.dispose()
    }
  })

  it('does not invoke append for another workspace or an invalidated calendar scope', async () => {
    const journal = await setup()
    try {
      expect(await journal.appendGitReports('/other-workspace', reports)).toBeNull()
      expect(await journal.appendGitReports('/workspace', reports, { isCurrent: () => false })).toBeNull()
      expect(api.appendGitReports).not.toHaveBeenCalled()
      expect(journal.draft.value).toBe(original.content)
    } finally {
      journal.dispose()
    }
  })
})
