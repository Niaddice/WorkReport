import { invoke } from '@tauri-apps/api/core'

export class FileChangesCancelled extends Error {}

export type FileFeature = 'journal' | 'excel' | 'summary'
export interface FileRule {
  directory: string
  filenameTemplate: string
}
export interface FileSettings {
  journal: FileRule
  excel: FileRule
  summary: FileRule
}
export interface FileNameContext {
  date: string
  startDate?: string
  endDate?: string
  time?: string
}
export interface FileChangesPlan {
  token: string
  directories: {
    feature: FileFeature
    label: string
    from: string
    to: string
    fileCount: number
    targetFileCount?: number
  }[]
  filenames: {
    feature: FileFeature
    label: string
    from: string
    to: string
    fileCount: number
    unrecognizedCount: number
  }[]
  warnings: string[]
}
export interface FileChangesDecision {
  token: string
  renameExisting: Record<FileFeature, boolean>
}

export const FILE_FEATURES = [
  { id: 'journal', label: '日报' },
  { id: 'excel', label: 'Excel 报告' },
  { id: 'summary', label: '总结' },
] as const
export const DATE_TOKENS = ['date', 'YYYY', 'MM', 'DD'] as const
export const REPORT_TOKENS = [...DATE_TOKENS, 'startDate', 'endDate', 'time', 'HH', 'mm', 'ss'] as const
export const DEFAULT_FILE_SETTINGS: FileSettings = {
  journal: { directory: '', filenameTemplate: '{date}' },
  excel: { directory: 'excel', filenameTemplate: '总结_{startDate}-{endDate}_{date}_{time}' },
  summary: { directory: 'summary', filenameTemplate: '总结_{startDate}-{endDate}_{date}_{time}' },
}

export function cloneFileSettings(value?: FileSettings, legacySummaryDirectory?: string): FileSettings {
  return {
    journal: { ...DEFAULT_FILE_SETTINGS.journal, ...value?.journal },
    excel: { ...DEFAULT_FILE_SETTINGS.excel, ...value?.excel },
    summary: {
      ...DEFAULT_FILE_SETTINGS.summary,
      ...(legacySummaryDirectory?.trim() ? { directory: legacySummaryDirectory } : {}),
      ...value?.summary,
    },
  }
}

export function formatFileName(rule: FileRule | string, context: FileNameContext, extension: string): string {
  const template = typeof rule === 'string' ? rule : rule.filenameTemplate
  const date = context.date
  const time = context.time ?? '120000'
  const values: Record<string, string> = {
    date,
    YYYY: date.slice(0, 4),
    MM: date.slice(4, 6),
    DD: date.slice(6, 8),
    startDate: context.startDate ?? date,
    endDate: context.endDate ?? date,
    time,
    HH: time.slice(0, 2),
    mm: time.slice(2, 4),
    ss: time.slice(4, 6),
  }
  const name = template.trim().replace(/\{([^{}]+)\}/g, (token, key: string) => values[key] ?? token)
  return `${name}.${extension.replace(/^\./, '')}`
}

const reserved = /^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i
export function fileDirectoryError(feature: FileFeature, directory: string): string {
  const value = directory.trim()
  if (!value) return feature === 'journal' ? '' : '请填写目录名称。'
  if (value.includes('\\') || value.startsWith('/') || /^[a-z]:/i.test(value) || /[\x00-\x1f\x7f<>:"|?*]/.test(value))
    return '目录只能位于当前工作空间内，请填写目录名称或相对路径。'
  if (
    value
      .split('/')
      .some(
        (part) =>
          !part ||
          part.startsWith('.') ||
          reserved.test(part) ||
          /[. ]$/.test(part) ||
          part.toLowerCase() === 'workreport.json',
      )
  )
    return '目录不能包含上级路径、隐藏目录、保留名称或空白路径段。'
  if (/^\d{4}(?:\/|$)/.test(value)) return '工作空间根目录的年份目录保留给日报，请使用其他目录名称。'
  return ''
}

export function filenameTemplateError(feature: FileFeature, template: string): string {
  const value = template.trim()
  if (!value) return '请填写文件名规则。'
  if (value.length > 120) return '文件名规则不能超过 120 个字符。'
  if (/[\x00-\x1f\x7f<>:"/\\|?*]/.test(value) || value.startsWith('.') || /[. ]$/.test(value) || reserved.test(value))
    return '文件名不能包含路径、隐藏文件名、保留名称或特殊字符。'
  if (/\.(?:md|txt|xlsx)$/i.test(value)) return '文件扩展名由功能自动添加，不需要填写。'
  const allowed: readonly string[] = feature === 'journal' ? DATE_TOKENS : REPORT_TOKENS
  let unknown = false
  const remainder = value.replace(/\{([^{}]+)\}/g, (_token, key: string) => {
    if (!allowed.includes(key)) unknown = true
    return ''
  })
  if (unknown || /[{}]/.test(remainder)) return '存在不支持的日期占位符，请使用下方提供的占位符。'
  if (
    feature === 'journal' &&
    !value.includes('{date}') &&
    !['YYYY', 'MM', 'DD'].every((token) => value.includes(`{${token}}`))
  )
    return '日报文件名必须包含 {date}，或同时包含 {YYYY}、{MM}、{DD}，确保每天的文件名不同。'
  return ''
}

export function fileSettingsError(settings: FileSettings): { feature: FileFeature; message: string } | null {
  for (const { id } of FILE_FEATURES) {
    const message =
      fileDirectoryError(id, settings[id].directory) || filenameTemplateError(id, settings[id].filenameTemplate)
    if (message) return { feature: id, message }
  }
  const directories = FILE_FEATURES.map(({ id }) => ({
    id,
    value: settings[id].directory.trim().replace(/\\/g, '/').toLocaleLowerCase(),
  })).filter((item) => item.value)
  for (let first = 0; first < directories.length; first++) {
    for (let second = first + 1; second < directories.length; second++) {
      const a = directories[first]!,
        b = directories[second]!
      if (a.value === b.value || a.value.startsWith(`${b.value}/`) || b.value.startsWith(`${a.value}/`))
        return { feature: b.id, message: '三个功能的保存目录不能相同或互相包含，请使用独立目录。' }
    }
  }
  return null
}

export const fileSettingsApi = {
  load: () => invoke<FileSettings>('load_file_settings'),
  preview: (workspacePath: string, files: FileSettings) =>
    invoke<FileChangesPlan>('preview_file_settings', { workspacePath, files: cloneFileSettings(files) }),
}
