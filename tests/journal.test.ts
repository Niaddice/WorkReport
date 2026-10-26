import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { api, type Document } from '../src/lib/workspace'
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
  },
}))
const original: Document = {
  date: '20261018',
  filename: '20261018.md',
  format: 'md',
  content: '# 日报\n',
  revision: 'r1',
}

async function setup() {
  vi.mocked(api.open).mockResolvedValue({ path: '/workspace', name: 'workspace' })
  vi.mocked(api.list).mockResolvedValue([{ date: original.date, filename: original.filename, format: 'md' }])
  vi.mocked(api.previews).mockResolvedValue([])
  vi.mocked(api.read).mockResolvedValue({ ...original })
  const journal = useJournal()
  journal.selectedDate.value = original.date
  await journal.open('/workspace')
  return journal
}

describe('workspace journal save lifecycle', () => {
  beforeEach(() => {
    vi.resetAllMocks()
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('requires choosing a workspace again on every startup', () => {
    const journal = useJournal()
    expect(journal.workspace.value).toBeNull()
    expect(api.open).not.toHaveBeenCalled()
  })

  it('starts a new draft in the configured default format', async () => {
    const journal = await setup()
    journal.setDefaultFormat('txt')
    expect(await journal.select('20261020')).toBe(true)
    expect(journal.isNewDraft.value).toBe(true)
    expect(journal.document.value?.format).toBe('txt')
    expect(journal.document.value?.filename).toBe('20261020.txt')
  })

  it('debounces typing and saves the literal Markdown content', async () => {
    const journal = await setup()
    vi.mocked(api.save).mockImplementation(async (date, content) => ({ ...original, date, content, revision: 'r2' }))
    journal.edit('第一版')
    await vi.advanceTimersByTimeAsync(300)
    journal.edit('# 第二版\n\n- 完成工作\n')
    await vi.advanceTimersByTimeAsync(650)
    expect(api.save).toHaveBeenCalledExactlyOnceWith(original.date, '# 第二版\n\n- 完成工作\n', 'r1', 'md')
    expect(journal.dirty.value).toBe(false)
    journal.dispose()
  })

  it('keeps autosaving files when the database backup needs retry', async () => {
    const journal = await setup()
    vi.mocked(api.save).mockImplementation(async (date, content) => ({
      ...original,
      date,
      content,
      revision: content,
      backupWarning: '文件已保存，数据库备份待重试',
    }))
    journal.edit('first')
    await vi.advanceTimersByTimeAsync(650)
    expect(journal.dirty.value).toBe(false)
    expect(journal.error.value).toBe('')
    expect(journal.backupWarning.value).toContain('备份待重试')
    journal.edit('second')
    await vi.advanceTimersByTimeAsync(650)
    expect(api.save).toHaveBeenCalledTimes(2)
    expect(journal.document.value?.content).toBe('second')
    expect(journal.dirty.value).toBe(false)
    journal.dispose()
  })

  it('retries a pending backup without turning it into a journal save error', async () => {
    const journal = await setup()
    journal.backupWarning.value = '备份待重试'
    journal.document.value = { ...original, backupWarning: '备份待重试' }
    vi.mocked(api.retryBackup).mockRejectedValueOnce('数据库无法连接').mockResolvedValueOnce({})
    expect(await journal.retryBackup()).toBe(false)
    expect(journal.backupWarning.value).toContain('数据库无法连接')
    expect(journal.error.value).toBe('')
    expect(journal.dirty.value).toBe(false)
    expect(await journal.retryBackup()).toBe(true)
    expect(api.retryBackup).toHaveBeenLastCalledWith('/workspace')
    expect(journal.backupWarning.value).toBe('')
    expect(journal.document.value?.backupWarning).toBeUndefined()
    journal.dispose()
  })

  it('keeps a disconnected database workspace accessible for settings recovery', async () => {
    const journal = await setup()
    vi.mocked(api.list).mockRejectedValueOnce('数据库连接失败')
    expect(await journal.open('/workspace')).toBe(false)
    expect(journal.workspace.value?.path).toBe('/workspace')
    expect(journal.document.value).toBeNull()
    expect(journal.error.value).toContain('数据库连接失败')
    expect(journal.dirty.value).toBe(false)
    journal.dispose()
  })

  it('serializes edits made while a save is in flight', async () => {
    const journal = await setup()
    let resolveFirst!: (value: Document) => void
    vi.mocked(api.save)
      .mockReturnValueOnce(
        new Promise((resolve) => {
          resolveFirst = resolve
        }),
      )
      .mockImplementationOnce(async (date, content, revision) => {
        expect(revision).toBe('r2')
        return { ...original, date, content, revision: 'r3' }
      })
    journal.edit('first')
    const pending = journal.flush()
    journal.edit('second')
    resolveFirst({ ...original, content: 'first', revision: 'r2' })
    expect(await pending).toBe(true)
    expect(api.save).toHaveBeenCalledTimes(2)
    expect(journal.document.value?.content).toBe('second')
    expect(journal.dirty.value).toBe(false)
    journal.dispose()
  })

  it('keeps unsaved content and blocks date and workspace changes after a save error', async () => {
    const journal = await setup()
    vi.mocked(api.save).mockRejectedValue(new Error('文件已被其他程序修改'))
    journal.edit('valuable draft')
    expect(await journal.select('20261019')).toBe(false)
    expect(journal.selectedDate.value).toBe(original.date)
    expect(journal.draft.value).toBe('valuable draft')
    expect(await journal.open('/other')).toBe(false)
    expect(journal.workspace.value?.path).toBe('/workspace')
    expect(api.open).toHaveBeenCalledTimes(1)
    journal.dispose()
  })

  it('saves typing continued during a slow calendar preview before changing date', async () => {
    const journal = await setup()
    let resolvePreview!: (value: []) => void
    vi.mocked(api.previews)
      .mockReturnValueOnce(
        new Promise((resolve) => {
          resolvePreview = resolve
        }),
      )
      .mockResolvedValue([])
    vi.mocked(api.save).mockImplementation(async (date, content) => ({ ...original, date, content, revision: content }))
    journal.edit('first save')
    const pending = journal.flush()
    await Promise.resolve()
    expect(api.previews).toHaveBeenCalledOnce()
    journal.edit('continued typing')
    const selecting = journal.select('20261019')
    resolvePreview([])
    await pending
    expect(await selecting).toBe(true)
    expect(api.save).toHaveBeenLastCalledWith(original.date, 'continued typing', 'first save', 'md')
    journal.dispose()
  })

  it('does not finish the first flush while newer typing still needs saving', async () => {
    const journal = await setup()
    let resolvePreview!: (value: []) => void
    vi.mocked(api.previews)
      .mockReturnValueOnce(
        new Promise((resolve) => {
          resolvePreview = resolve
        }),
      )
      .mockResolvedValue([])
    vi.mocked(api.save).mockImplementation(async (date, content) => ({ ...original, date, content, revision: content }))
    journal.edit('first save')
    const pending = journal.flush()
    await Promise.resolve()
    expect(api.previews).toHaveBeenCalledOnce()
    journal.edit('continued typing')
    resolvePreview([])
    expect(await pending).toBe(true)
    expect(api.save).toHaveBeenLastCalledWith(original.date, 'continued typing', 'first save', 'md')
    expect(journal.dirty.value).toBe(false)
    journal.dispose()
  })

  it('flushes the latest draft before converting the filename and uses the new revision', async () => {
    const journal = await setup()
    vi.mocked(api.save).mockImplementation(async (date, content) => ({ ...original, date, content, revision: 'r2' }))
    vi.mocked(api.switchFormat).mockResolvedValue({
      ...original,
      filename: '20261018.txt',
      format: 'txt',
      content: 'latest draft',
      revision: 'r2',
    })
    journal.edit('latest draft')
    expect(await journal.switchFormat('txt')).toBe(true)
    expect(api.switchFormat).toHaveBeenCalledWith(original.date, 'md', 'txt', 'r2')
    expect(journal.document.value?.filename).toBe('20261018.txt')
    expect(journal.dirty.value).toBe(false)
    journal.dispose()
  })

  it('keeps the current document when a format switch fails', async () => {
    const journal = await setup()
    vi.mocked(api.switchFormat).mockRejectedValue(new Error('target exists'))
    expect(await journal.switchFormat('txt')).toBe(false)
    expect(journal.document.value?.format).toBe('md')
    expect(journal.draft.value).toBe(original.content)
    journal.dispose()
  })

  it('shows an empty editable draft without creating a file', async () => {
    const journal = await setup()
    expect(await journal.select('20261019')).toBe(true)
    expect(journal.document.value).toMatchObject({
      date: '20261019',
      filename: '20261019.md',
      format: 'md',
      content: '',
      revision: '',
    })
    expect(journal.isNewDraft.value).toBe(true)
    expect(journal.draft.value).toBe('')
    expect(api.create).not.toHaveBeenCalled()
    expect(await journal.flush()).toBe(true)
    expect(api.create).not.toHaveBeenCalled()
    journal.dispose()
  })

  it('creates the report from the first typed content', async () => {
    const journal = await setup()
    await journal.select('20261019')
    const created: Document = {
      date: '20261019',
      filename: '20261019.md',
      format: 'md',
      content: '新的工作',
      revision: 'c1',
    }
    vi.mocked(api.create).mockResolvedValue(created)
    journal.edit('新的工作')
    await vi.advanceTimersByTimeAsync(650)
    expect(api.create).toHaveBeenCalledWith('20261019', 'md', '新的工作')
    expect(journal.isNewDraft.value).toBe(false)
    expect(journal.document.value).toEqual(created)
    expect(journal.dirty.value).toBe(false)
    expect(journal.entries.value.some((entry) => entry.date === '20261019')).toBe(true)
    expect(api.save).not.toHaveBeenCalled()
    journal.dispose()
  })

  it('creates in the format chosen before typing', async () => {
    const journal = await setup()
    await journal.select('20261019')
    expect(await journal.switchFormat('txt')).toBe(true)
    expect(journal.document.value?.format).toBe('txt')
    expect(api.switchFormat).not.toHaveBeenCalled()
    const created: Document = {
      date: '20261019',
      filename: '20261019.txt',
      format: 'txt',
      content: '纯文本内容',
      revision: 'c1',
    }
    vi.mocked(api.create).mockResolvedValue(created)
    journal.edit('纯文本内容')
    await vi.advanceTimersByTimeAsync(650)
    expect(api.create).toHaveBeenCalledWith('20261019', 'txt', '纯文本内容')
    expect(journal.document.value).toEqual(created)
    journal.dispose()
  })

  it('flushes before changing date and never creates a file by single-clicking', async () => {
    const journal = await setup()
    vi.mocked(api.save).mockImplementation(async (date, content) => ({ ...original, date, content, revision: 'r2' }))
    journal.edit('finished')
    expect(await journal.select('20261019')).toBe(true)
    expect(api.save).toHaveBeenCalledOnce()
    expect(api.create).not.toHaveBeenCalled()
    expect(journal.isNewDraft.value).toBe(true)
    expect(journal.document.value?.date).toBe('20261019')
    expect(journal.draft.value).toBe('')
    journal.dispose()
  })

  it('loads previews once per visible date set', async () => {
    const journal = await setup()
    vi.mocked(api.previews).mockResolvedValue([{ date: original.date, preview: '预览' }])
    await journal.loadPreviews([original.date, original.date])
    expect(api.previews).toHaveBeenCalledOnce()
    expect(api.previews).toHaveBeenCalledWith([original.date])
    expect(journal.previews.value[original.date]).toBe('预览')
    await journal.loadPreviews([original.date])
    expect(api.previews).toHaveBeenCalledOnce()
    journal.dispose()
  })

  it('updates the calendar entry and preview after saving without re-listing', async () => {
    const journal = await setup()
    const listCalls = vi.mocked(api.list).mock.calls.length
    vi.mocked(api.save).mockImplementation(async (date, content) => ({ ...original, date, content, revision: 'r2' }))
    vi.mocked(api.previews).mockResolvedValue([{ date: original.date, preview: '完成工作' }])
    journal.edit('完成工作')
    expect(await journal.flush()).toBe(true)
    expect(api.list).toHaveBeenCalledTimes(listCalls)
    expect(journal.entries.value).toEqual([{ date: original.date, filename: original.filename, format: 'md' }])
    expect(journal.previews.value[original.date]).toBe('完成工作')
    journal.dispose()
  })

  it('ignores a stale preview response after a save invalidates it', async () => {
    const journal = await setup()
    let resolveOld!: (value: { date: string; preview: string }[]) => void
    vi.mocked(api.previews)
      .mockReturnValueOnce(
        new Promise((resolve) => {
          resolveOld = resolve
        }),
      )
      .mockResolvedValueOnce([{ date: original.date, preview: '新预览' }])
    const old = journal.loadPreviews([original.date])
    vi.mocked(api.save).mockImplementation(async (date, content) => ({ ...original, date, content, revision: 'r2' }))
    journal.edit('新内容')
    expect(await journal.flush()).toBe(true)
    expect(journal.previews.value[original.date]).toBe('新预览')
    resolveOld([{ date: original.date, preview: '旧预览' }])
    await old
    expect(journal.previews.value[original.date]).toBe('新预览')
    journal.dispose()
  })

  it('keeps undo after autosave and an unchanged focus refresh, while accepting the latest revision', async () => {
    const journal = await setup()
    let stored = { ...original }
    vi.mocked(api.save).mockImplementation(async (date, content) => {
      stored = { ...stored, date, content, revision: 'saved' }
      return { ...stored }
    })
    vi.mocked(api.read).mockImplementation(async () => ({ ...stored, revision: 'refreshed' }))
    journal.edit('自动保存后的正文')
    await vi.advanceTimersByTimeAsync(650)
    expect(journal.dirty.value).toBe(false)

    await journal.refresh()

    expect(journal.document.value?.revision).toBe('refreshed')
    expect(journal.canUndo.value).toBe(true)
    expect(journal.undo()).toBe(true)
    expect(journal.draft.value).toBe(original.content)
    expect(await journal.flush()).toBe(true)
    expect(api.save).toHaveBeenLastCalledWith(original.date, original.content, 'refreshed', 'md')
    journal.dispose()
  })

  it('keeps both history stacks when refreshing after undo', async () => {
    const journal = await setup()
    let stored = { ...original }
    vi.mocked(api.save).mockImplementation(async (date, content) => {
      stored = { ...stored, date, content, revision: content }
      return { ...stored }
    })
    vi.mocked(api.read).mockImplementation(async () => ({ ...stored }))
    journal.edit('first', true)
    journal.edit('second', true)
    await journal.flush()
    expect(journal.undo()).toBe(true)

    await journal.refresh()

    expect(journal.draft.value).toBe('first')
    expect(journal.canUndo.value).toBe(true)
    expect(journal.canRedo.value).toBe(true)
    expect(journal.redo()).toBe(true)
    expect(journal.draft.value).toBe('second')
    expect(journal.undo()).toBe(true)
    expect(journal.draft.value).toBe('first')
    journal.dispose()
  })

  it('keeps redo for a new draft undone to empty without creating a file on refresh', async () => {
    const journal = await setup()
    await journal.select('20261019')
    journal.edit('未落盘的第一版')
    expect(journal.undo()).toBe(true)

    await journal.refresh()

    expect(journal.isNewDraft.value).toBe(true)
    expect(journal.canRedo.value).toBe(true)
    expect(api.create).not.toHaveBeenCalled()
    expect(journal.redo()).toBe(true)
    expect(journal.draft.value).toBe('未落盘的第一版')
    journal.dispose()
  })

  it('clears both history stacks when a refresh finds externally changed content', async () => {
    const journal = await setup()
    vi.mocked(api.save).mockImplementation(async (date, content) => ({ ...original, date, content, revision: 'saved' }))
    journal.edit('first', true)
    journal.edit('second', true)
    journal.undo()
    await journal.flush()
    vi.mocked(api.read).mockResolvedValue({ ...original, content: '另一程序修改的正文', revision: 'external' })

    await journal.refresh()

    expect(journal.draft.value).toBe('另一程序修改的正文')
    expect(journal.canUndo.value).toBe(false)
    expect(journal.canRedo.value).toBe(false)
    expect(journal.undo()).toBe(false)
    expect(journal.redo()).toBe(false)
    journal.dispose()
  })

  it('clears history when a refresh finds the report in a different format', async () => {
    const journal = await setup()
    vi.mocked(api.save).mockImplementation(async (date, content) => ({ ...original, date, content, revision: 'saved' }))
    journal.edit('same contents')
    await journal.flush()
    vi.mocked(api.list).mockResolvedValue([{ date: original.date, filename: '20261018.txt', format: 'txt' }])
    vi.mocked(api.read).mockResolvedValue({
      ...original,
      filename: '20261018.txt',
      format: 'txt',
      content: 'same contents',
      revision: 'external',
    })

    await journal.refresh()

    expect(journal.document.value?.format).toBe('txt')
    expect(journal.canUndo.value).toBe(false)
    expect(journal.canRedo.value).toBe(false)
    journal.dispose()
  })

  it('still discards local history on explicit reload even when saved contents are unchanged', async () => {
    const journal = await setup()
    const saved = { ...original, content: 'saved contents', revision: 'saved' }
    vi.mocked(api.save).mockResolvedValue(saved)
    journal.edit(saved.content)
    await journal.flush()
    vi.mocked(api.read).mockResolvedValue(saved)

    await journal.reload()

    expect(journal.draft.value).toBe(saved.content)
    expect(journal.canUndo.value).toBe(false)
    expect(journal.canRedo.value).toBe(false)
    journal.dispose()
  })

  it('clears local history when changing workspaces', async () => {
    const journal = await setup()
    vi.mocked(api.save).mockImplementation(async (date, content) => ({ ...original, date, content, revision: 'saved' }))
    journal.edit('saved contents')
    await journal.flush()
    vi.mocked(api.open).mockResolvedValue({ path: '/another-workspace', name: 'another' })

    await journal.open('/another-workspace')

    expect(journal.workspace.value?.path).toBe('/another-workspace')
    expect(journal.canUndo.value).toBe(false)
    expect(journal.canRedo.value).toBe(false)
    journal.dispose()
  })

  it('undoes and redoes edits, and resets history when changing date', async () => {
    const journal = await setup()
    vi.mocked(api.save).mockImplementation(async (date, content) => ({ ...original, date, content, revision: 'r2' }))
    expect(journal.canUndo.value).toBe(false)
    journal.edit('first')
    journal.edit('second', true)
    expect(journal.canUndo.value).toBe(true)
    expect(journal.undo()).toBe(true)
    expect(journal.draft.value).toBe('first')
    expect(journal.canRedo.value).toBe(true)
    expect(journal.redo()).toBe(true)
    expect(journal.draft.value).toBe('second')
    expect(journal.canRedo.value).toBe(false)
    expect(await journal.select('20261019')).toBe(true)
    expect(journal.canUndo.value).toBe(false)
    expect(journal.canRedo.value).toBe(false)
    journal.dispose()
  })
})
