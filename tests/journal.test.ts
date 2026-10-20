import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { api, type Document } from '../src/lib/workspace'
import { useJournal } from '../src/lib/useJournal'

vi.mock('../src/lib/workspace', () => ({ api: { open: vi.fn(), list: vi.fn(), previews: vi.fn(), read: vi.fn(), create: vi.fn(), save: vi.fn(), switchFormat: vi.fn() } }))
const original: Document = { date: '20261018', filename: '20261018.md', format: 'md', content: '# 日报\n', revision: 'r1' }

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
  beforeEach(() => { vi.resetAllMocks(); vi.useFakeTimers() })
  afterEach(() => { vi.useRealTimers() })

  it('requires choosing a workspace again on every startup', () => {
    const journal = useJournal()
    expect(journal.workspace.value).toBeNull()
    expect(api.open).not.toHaveBeenCalled()
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

  it('serializes edits made while a save is in flight', async () => {
    const journal = await setup()
    let resolveFirst!: (value: Document) => void
    vi.mocked(api.save).mockReturnValueOnce(new Promise((resolve) => { resolveFirst = resolve }))
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

  it('flushes the latest draft before converting the filename and uses the new revision', async () => {
    const journal = await setup()
    vi.mocked(api.save).mockImplementation(async (date, content) => ({ ...original, date, content, revision: 'r2' }))
    vi.mocked(api.switchFormat).mockResolvedValue({ ...original, filename: '20261018.txt', format: 'txt', content: 'latest draft', revision: 'r2' })
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
    expect(journal.document.value).toMatchObject({ date: '20261019', filename: '20261019.md', format: 'md', content: '', revision: '' })
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
    const created: Document = { date: '20261019', filename: '20261019.md', format: 'md', content: '新的工作', revision: 'c1' }
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
    const created: Document = { date: '20261019', filename: '20261019.txt', format: 'txt', content: '纯文本内容', revision: 'c1' }
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
    vi.mocked(api.previews).mockReturnValueOnce(new Promise((resolve) => { resolveOld = resolve }))
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
})
