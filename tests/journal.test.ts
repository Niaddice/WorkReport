import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { api, type Document } from '../src/lib/workspace'
import { useJournal } from '../src/lib/useJournal'

vi.mock('../src/lib/workspace', () => ({ api: { open: vi.fn(), list: vi.fn(), read: vi.fn(), create: vi.fn(), save: vi.fn(), switchFormat: vi.fn() } }))
const original: Document = { date: '20261018', filename: '20261018.md', format: 'md', content: '# 日报\n', revision: 'r1' }

async function setup() {
  vi.mocked(api.open).mockResolvedValue({ path: '/workspace', name: 'workspace' })
  vi.mocked(api.list).mockResolvedValue([{ date: original.date, filename: original.filename, format: 'md', preview: '日报' }])
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

  it('creates a TXT document when the selected format is txt', async () => {
    const journal = await setup()
    vi.mocked(api.create).mockResolvedValue({ ...original, date: '20261019', filename: '20261019.txt', format: 'txt' })
    expect(await journal.create('20261019', 'txt')).toBe(true)
    expect(api.create).toHaveBeenCalledWith('20261019', 'txt')
    expect(journal.document.value?.format).toBe('txt')
    journal.dispose()
  })

  it('flushes before changing date and never creates a file by single-clicking', async () => {
    const journal = await setup()
    vi.mocked(api.save).mockImplementation(async (date, content) => ({ ...original, date, content, revision: 'r2' }))
    journal.edit('finished')
    expect(await journal.select('20261019')).toBe(true)
    expect(api.save).toHaveBeenCalledOnce()
    expect(api.create).not.toHaveBeenCalled()
    expect(journal.document.value).toBeNull()
    expect(journal.draft.value).toBe('')
    journal.dispose()
  })
})
