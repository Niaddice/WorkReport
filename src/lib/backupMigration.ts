import dayjs from 'dayjs'
import type { Document, Entry, FileFormat } from './workspace'
import type { WorkspaceSettings } from './settingsDraft'

export interface BackupJournalEntry {
  date: string
  format: FileFormat
  content: string
}

export interface WorkspaceBackupBundle {
  version: number
  exportedAt: string
  workspaceName: string
  workspacePath?: string
  totalJournals: number
  journals: BackupJournalEntry[]
  settings?: Partial<WorkspaceSettings>
}

export interface RestorePlan {
  totalInBackup: number
  newEntriesCount: number
  conflictingEntriesCount: number
  hasSettings: boolean
  newDates: string[]
  conflictingDates: string[]
}

export interface RestoreResult {
  totalRestored: number
  createdCount: number
  overwrittenCount: number
  skippedCount: number
  settingsRestored: boolean
}

/**
 * 创建完整备份包，汇总所有日报内容与工作空间配置。
 */
export async function createBackupBundle(params: {
  workspaceName: string
  workspacePath?: string
  includeSettings?: boolean
  entries: Entry[]
  readEntry: (date: string, format: FileFormat) => Promise<Document>
  getSettings?: () => Promise<WorkspaceSettings | null>
}): Promise<WorkspaceBackupBundle> {
  const journals: BackupJournalEntry[] = []

  for (const entry of params.entries) {
    try {
      const doc = await params.readEntry(entry.date, entry.format)
      journals.push({
        date: doc.date,
        format: doc.format,
        content: doc.content,
      })
    } catch {
      // 容错处理：单篇读取异常不阻断整包备份
    }
  }

  let settings: Partial<WorkspaceSettings> | undefined
  if (params.includeSettings && params.getSettings) {
    try {
      const loaded = await params.getSettings()
      if (loaded) settings = loaded
    } catch {
      // 设置读取异常继续生成日报包
    }
  }

  return {
    version: 1,
    exportedAt: new Date().toISOString(),
    workspaceName: params.workspaceName || '未命名工作空间',
    workspacePath: params.workspacePath,
    totalJournals: journals.length,
    journals,
    settings,
  }
}

/**
 * 解析并校验备份 JSON 字符串。
 */
export function parseBackupJson(jsonString: string): WorkspaceBackupBundle {
  let parsed: unknown
  try {
    parsed = JSON.parse(jsonString)
  } catch (err) {
    throw new Error(`备份文件格式解析失败: ${err instanceof Error ? err.message : String(err)}`, { cause: err })
  }

  if (!parsed || typeof parsed !== 'object') {
    throw new Error('无效的备份文件结构')
  }

  const obj = parsed as Record<string, unknown>
  if (!Array.isArray(obj.journals)) {
    throw new Error('备份文件中缺少 journals 日报数据列表')
  }

  const journals: BackupJournalEntry[] = []
  for (const item of obj.journals) {
    if (item && typeof item === 'object') {
      const j = item as Record<string, unknown>
      if (typeof j.date === 'string' && typeof j.content === 'string') {
        journals.push({
          date: j.date,
          format: (j.format === 'txt' ? 'txt' : 'md') as FileFormat,
          content: j.content,
        })
      }
    }
  }

  return {
    version: typeof obj.version === 'number' ? obj.version : 1,
    exportedAt: typeof obj.exportedAt === 'string' ? obj.exportedAt : new Date().toISOString(),
    workspaceName: typeof obj.workspaceName === 'string' ? obj.workspaceName : '备份工作空间',
    workspacePath: typeof obj.workspacePath === 'string' ? obj.workspacePath : undefined,
    totalJournals: journals.length,
    journals,
    settings: (obj.settings && typeof obj.settings === 'object' ? obj.settings : undefined) as
      Partial<WorkspaceSettings> | undefined,
  }
}

/**
 * 分析备份包在当前工作空间中的冲突与恢复计划。
 */
export function planRestore(bundle: WorkspaceBackupBundle, existingEntries: Entry[]): RestorePlan {
  const existingMap = new Map<string, Entry>()
  for (const e of existingEntries) {
    existingMap.set(e.date, e)
  }

  const newDates: string[] = []
  const conflictingDates: string[] = []

  for (const j of bundle.journals) {
    if (existingMap.has(j.date)) {
      conflictingDates.push(j.date)
    } else {
      newDates.push(j.date)
    }
  }

  return {
    totalInBackup: bundle.journals.length,
    newEntriesCount: newDates.length,
    conflictingEntriesCount: conflictingDates.length,
    hasSettings: !!bundle.settings,
    newDates,
    conflictingDates,
  }
}

/**
 * 执行数据恢复，根据策略导入或覆盖日报，并可选同步配置。
 */
export async function executeRestore(params: {
  bundle: WorkspaceBackupBundle
  policy: 'skip' | 'overwrite'
  restoreSettings?: boolean
  existingEntries: Entry[]
  createEntry: (date: string, format: FileFormat, content: string) => Promise<Document>
  saveEntry: (date: string, content: string, expectedRevision: string, format: FileFormat) => Promise<Document>
  readEntry?: (date: string, format: FileFormat) => Promise<Document>
  saveSettings?: (settings: WorkspaceSettings) => Promise<unknown>
}): Promise<RestoreResult> {
  const existingMap = new Map<string, Entry>()
  for (const e of params.existingEntries) {
    existingMap.set(e.date, e)
  }

  let createdCount = 0
  let overwrittenCount = 0
  let skippedCount = 0

  for (const j of params.bundle.journals) {
    const existing = existingMap.get(j.date)
    if (!existing) {
      // 本地无此日期记录，创建新日报
      try {
        await params.createEntry(j.date, j.format, j.content)
        createdCount++
      } catch {
        // 创建失败跳过
      }
    } else if (params.policy === 'overwrite') {
      // 本地有此日期记录且策略为覆盖
      try {
        let rev = '1'
        if (params.readEntry) {
          const currentDoc = await params.readEntry(existing.date, existing.format)
          rev = currentDoc.revision
        }
        await params.saveEntry(j.date, j.content, rev, j.format)
        overwrittenCount++
      } catch {
        // 覆盖失败尝试重新创建或跳过
      }
    } else {
      // 冲突且策略为跳过
      skippedCount++
    }
  }

  let settingsRestored = false
  if (params.restoreSettings && params.bundle.settings && params.saveSettings) {
    try {
      await params.saveSettings(params.bundle.settings as WorkspaceSettings)
      settingsRestored = true
    } catch {
      // 设置同步异常不影响已恢复的日报数据
    }
  }

  return {
    totalRestored: createdCount + overwrittenCount,
    createdCount,
    overwrittenCount,
    skippedCount,
    settingsRestored,
  }
}

/**
 * 触发备份文件的浏览器本地下载。
 */
export function downloadBackupFile(bundle: WorkspaceBackupBundle): void {
  const jsonStr = JSON.stringify(bundle, null, 2)
  const blob = new Blob([jsonStr], { type: 'application/json' })
  const url = URL.createObjectURL(blob)

  const dateTag = dayjs(bundle.exportedAt).format('YYYYMMDD-HHmmss')
  const cleanName = (bundle.workspaceName || 'workspace').replace(/[^\w\u4e00-\u9fa5-_]/g, '_')
  const filename = `WorkReport-Backup-${cleanName}-${dateTag}.json`

  const link = document.createElement('a')
  link.href = url
  link.download = filename
  document.body.appendChild(link)
  link.click()
  document.body.removeChild(link)
  URL.revokeObjectURL(url)
}
