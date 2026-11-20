import { describe, expect, it, vi } from 'vitest'
import {
  createBackupBundle,
  executeRestore,
  parseBackupJson,
  planRestore,
  type WorkspaceBackupBundle,
} from '../src/lib/backupMigration'
import type { Document, Entry } from '../src/lib/workspace'
import type { WorkspaceSettings } from '../src/lib/settingsDraft'

describe('backupMigration', () => {
  const sampleEntries: Entry[] = [
    { date: '20261026', filename: '20261026.md', format: 'md' },
    { date: '20261027', filename: '20261027.txt', format: 'txt' },
  ]

  const mockDocs: Record<string, Document> = {
    '20261026': {
      date: '20261026',
      filename: '20261026.md',
      format: 'md',
      content: '# 10月26日 工作总结',
      revision: 'r1',
    },
    '20261027': {
      date: '20261027',
      filename: '20261027.txt',
      format: 'txt',
      content: '10月27日 文本日志',
      revision: 'r2',
    },
  }

  it('creates backup bundle with all journals and settings', async () => {
    const readSpy = vi.fn().mockImplementation((date: string) => Promise.resolve(mockDocs[date]))
    const settingsSpy = vi.fn().mockResolvedValue({
      editor: { defaultTemplateId: 'standard', customTemplate: '', autoApplyTemplate: true },
    } as unknown as WorkspaceSettings)

    const bundle = await createBackupBundle({
      workspaceName: 'DevTeam',
      workspacePath: '/path/to/dev',
      includeSettings: true,
      entries: sampleEntries,
      readEntry: readSpy,
      getSettings: settingsSpy,
    })

    expect(bundle.version).toBe(1)
    expect(bundle.workspaceName).toBe('DevTeam')
    expect(bundle.totalJournals).toBe(2)
    expect(bundle.journals).toHaveLength(2)
    expect(bundle.journals[0]?.content).toBe('# 10月26日 工作总结')
    expect(bundle.journals[1]?.format).toBe('txt')
    expect(bundle.settings).toBeDefined()
  })

  it('parses valid backup JSON and throws on malformed JSON', () => {
    const validJson = JSON.stringify({
      version: 1,
      workspaceName: 'MyWork',
      journals: [{ date: '20261028', format: 'md', content: '测试内容' }],
    })

    const parsed = parseBackupJson(validJson)
    expect(parsed.workspaceName).toBe('MyWork')
    expect(parsed.totalJournals).toBe(1)
    expect(parsed.journals[0]?.date).toBe('20261028')

    expect(() => parseBackupJson('invalid json')).toThrow('备份文件格式解析失败')
    expect(() => parseBackupJson(JSON.stringify({ notJournals: [] }))).toThrow('缺少 journals')
  })

  it('plans restore by categorizing new and conflicting dates', () => {
    const bundle: WorkspaceBackupBundle = {
      version: 1,
      exportedAt: '2026-11-08T10:00:00Z',
      workspaceName: 'BackupSpace',
      totalJournals: 3,
      journals: [
        { date: '20261026', format: 'md', content: '新版26日' }, // 冲突
        { date: '20261028', format: 'md', content: '28日内容' }, // 新增
        { date: '20261029', format: 'txt', content: '29日内容' }, // 新增
      ],
    }

    const plan = planRestore(bundle, sampleEntries)
    expect(plan.totalInBackup).toBe(3)
    expect(plan.newEntriesCount).toBe(2)
    expect(plan.conflictingEntriesCount).toBe(1)
    expect(plan.conflictingDates).toEqual(['20261026'])
    expect(plan.newDates).toEqual(['20261028', '20261029'])
  })

  it('executes restore with skip policy: only creates non-conflicting entries', async () => {
    const createSpy = vi.fn().mockResolvedValue({} as Document)
    const saveSpy = vi.fn().mockResolvedValue({} as Document)

    const bundle: WorkspaceBackupBundle = {
      version: 1,
      exportedAt: '2026-11-08T10:00:00Z',
      workspaceName: 'BackupSpace',
      totalJournals: 2,
      journals: [
        { date: '20261026', format: 'md', content: '冲突被跳过' },
        { date: '20261028', format: 'md', content: '新增28日' },
      ],
    }

    const result = await executeRestore({
      bundle,
      policy: 'skip',
      existingEntries: sampleEntries,
      createEntry: createSpy,
      saveEntry: saveSpy,
    })

    expect(result.totalRestored).toBe(1)
    expect(result.createdCount).toBe(1)
    expect(result.skippedCount).toBe(1)
    expect(result.overwrittenCount).toBe(0)
    expect(createSpy).toHaveBeenCalledWith('20261028', 'md', '新增28日')
    expect(saveSpy).not.toHaveBeenCalled()
  })

  it('executes restore with overwrite policy: updates conflicting entries', async () => {
    const createSpy = vi.fn().mockResolvedValue({} as Document)
    const saveSpy = vi.fn().mockResolvedValue({} as Document)
    const readSpy = vi.fn().mockResolvedValue({ revision: 'rev-existing' } as Document)

    const bundle: WorkspaceBackupBundle = {
      version: 1,
      exportedAt: '2026-11-08T10:00:00Z',
      workspaceName: 'BackupSpace',
      totalJournals: 2,
      journals: [
        { date: '20261026', format: 'md', content: '覆盖已有内容' },
        { date: '20261028', format: 'md', content: '新增28日' },
      ],
    }

    const result = await executeRestore({
      bundle,
      policy: 'overwrite',
      existingEntries: sampleEntries,
      createEntry: createSpy,
      saveEntry: saveSpy,
      readEntry: readSpy,
    })

    expect(result.totalRestored).toBe(2)
    expect(result.createdCount).toBe(1)
    expect(result.overwrittenCount).toBe(1)
    expect(result.skippedCount).toBe(0)
    expect(saveSpy).toHaveBeenCalledWith('20261026', '覆盖已有内容', 'rev-existing', 'md')
    expect(createSpy).toHaveBeenCalledWith('20261028', 'md', '新增28日')
  })
})
