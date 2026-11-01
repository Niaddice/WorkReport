import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { api } from '../src/lib/workspace'
import { useJournal } from '../src/lib/useJournal'

vi.mock('../src/lib/workspace', () => ({
  api: {
    backupWarning: vi.fn(),
    open: vi.fn(),
    list: vi.fn(),
    previews: vi.fn(),
    read: vi.fn(),
    create: vi.fn(),
    save: vi.fn(),
    switchFormat: vi.fn(),
  },
}))

describe('journal file naming settings', () => {
  beforeEach(() => {
    vi.resetAllMocks()
    vi.useFakeTimers()
    vi.mocked(api.open).mockResolvedValue({ path: '/workspace', name: 'workspace' })
    vi.mocked(api.list).mockResolvedValue([])
    vi.mocked(api.previews).mockResolvedValue([])
  })
  afterEach(() => vi.useRealTimers())

  it('updates an unsaved draft name and its format using the configured date tokens', async () => {
    const journal = useJournal()
    journal.selectedDate.value = '20261031'
    await journal.open('/workspace')
    journal.setFileSettings({ journal: { directory: '日报', filenameTemplate: '工作_{YYYY}年{MM}月{DD}日' } })
    expect(journal.document.value?.filename).toBe('工作_2026年10月31日.txt')
    await journal.switchFormat('md')
    expect(journal.document.value?.filename).toBe('工作_2026年10月31日.md')
    expect(api.switchFormat).not.toHaveBeenCalled()
    journal.dispose()
  })

  it('uses the actual saved name after a rule change and the new rule for future dates', async () => {
    const saved = {
      date: '20261031',
      filename: '旧日报_20261031.md',
      format: 'md' as const,
      content: 'saved',
      revision: 'r1',
    }
    vi.mocked(api.list).mockResolvedValue([{ date: saved.date, filename: saved.filename, format: saved.format }])
    vi.mocked(api.read).mockResolvedValue(saved)
    const journal = useJournal()
    journal.selectedDate.value = saved.date
    await journal.open('/workspace')
    journal.setFileSettings({ journal: { directory: '日报', filenameTemplate: '新日报_{date}' } })
    expect(journal.document.value?.filename).toBe(saved.filename)
    await journal.select('20261101')
    expect(journal.document.value?.filename).toBe('新日报_20261101.txt')
    journal.dispose()
  })

  it('clears the previous workspace naming rule while the next settings load', async () => {
    const journal = useJournal()
    journal.selectedDate.value = '20261031'
    await journal.open('/workspace')
    journal.setFileSettings({ journal: { directory: '日报', filenameTemplate: '专属_{date}' } })
    vi.mocked(api.open).mockResolvedValue({ path: '/other', name: 'other' })
    await journal.open('/other')
    expect(journal.document.value?.filename).toBe('20261031.txt')
    journal.dispose()
  })
})
