import { invoke } from '@tauri-apps/api/core'
import type { FileFormat } from './workspace'

export type DatabaseEngine = 'sqlite' | 'mysql' | 'postgres'
export type DatabaseStorageMode = 'dual' | 'database'
export type DatabaseTlsMode = 'preferred' | 'required' | 'disabled' | 'verifyFull'
export type DatabaseTransferDirection = 'filesToDatabase' | 'databaseToFiles'
export type DatabaseTransferAction = 'copy' | 'migrate'
export type DatabaseConflictPolicy = 'abort' | 'overwrite'

/** 历史版本上限的可配置范围：1 GB – 10 GB，滑块步进 1 GB，默认 1 GB。 */
export const MIN_HISTORY_LIMIT_BYTES = 1024 * 1024 * 1024
export const MAX_HISTORY_LIMIT_BYTES = 10 * 1024 * 1024 * 1024
export const DEFAULT_HISTORY_LIMIT_BYTES = 1024 * 1024 * 1024

export function clampHistoryLimit(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return DEFAULT_HISTORY_LIMIT_BYTES
  return Math.min(MAX_HISTORY_LIMIT_BYTES, Math.max(MIN_HISTORY_LIMIT_BYTES, Math.round(value)))
}

export interface DatabaseSettings {
  enabled: boolean
  engine: DatabaseEngine
  mode: DatabaseStorageMode
  sqlitePath: string
  host: string
  port: number
  database: string
  user: string
  password: string
  tlsMode: DatabaseTlsMode
  workspaceId: string
  /** 新建日报的默认文件格式，仅影响之后创建的日报。 */
  defaultFormat: FileFormat
  /** 历史版本最大占用空间；超出后从最旧的一天开始清理。 */
  historyLimitBytes: number
}

export interface DatabaseConnectionResult {
  message: string
  recordCount?: number
}

export interface DatabaseTransferResult {
  copied: number
  skipped: number
  mode: DatabaseStorageMode
  enabled: boolean
  message: string
  settings: DatabaseSettings
}

export const DEFAULT_DATABASE_SETTINGS: DatabaseSettings = {
  enabled: false,
  engine: 'sqlite',
  mode: 'dual',
  sqlitePath: 'workreport.sqlite',
  host: '127.0.0.1',
  port: 0,
  database: 'workreport',
  user: '',
  password: '',
  tlsMode: 'preferred',
  workspaceId: '',
  defaultFormat: 'txt',
  historyLimitBytes: DEFAULT_HISTORY_LIMIT_BYTES,
}

export function cloneDatabaseSettings(settings: DatabaseSettings): DatabaseSettings {
  return {
    enabled: settings.enabled,
    engine: settings.engine,
    mode: settings.mode,
    sqlitePath: settings.sqlitePath,
    host: settings.host,
    port: settings.port,
    database: settings.database,
    user: settings.user,
    password: settings.password,
    tlsMode: settings.tlsMode,
    workspaceId: settings.workspaceId,
    defaultFormat: settings.defaultFormat === 'md' ? 'md' : 'txt',
    historyLimitBytes: clampHistoryLimit(settings.historyLimitBytes),
  }
}

/** Validate drafts without connecting or writing anything. The backend validates again. */
export function validateDatabaseSettings(settings: DatabaseSettings): string {
  if (!settings.enabled) return ''
  if (!['sqlite', 'mysql', 'postgres'].includes(settings.engine)) return '请选择支持的数据库类型。'
  if (!['dual', 'database'].includes(settings.mode)) return '请选择数据存储方式。'
  if (settings.engine === 'sqlite') {
    const path = settings.sqlitePath.trim()
    if (!path) return '' // The backend normalizes an empty path to workreport.sqlite.
    if (
      path.startsWith('/') ||
      /^[a-z]:/i.test(path) ||
      /[\\\x00-\x1f\x7f]/.test(path) ||
      path.split('/').some((part) => !part || part === '.' || part === '..') ||
      !/\.(sqlite|sqlite3|db)$/.test(path)
    )
      return '请输入工作空间内的 SQLite 相对路径，扩展名为 .sqlite、.sqlite3 或 .db，不能使用上级目录或外部路径。'
    return ''
  }
  if (!settings.host.trim() || settings.host.trim().length > 253 || /[\x00-\x20\x7f/\\@?#]/.test(settings.host))
    return '请填写数据库主机名或 IP 地址，不包含协议、路径或空格。'
  if (!Number.isInteger(settings.port) || settings.port < 0 || settings.port > 65535)
    return '数据库端口必须是 1–65535 的整数，填 0 使用默认端口。'
  if (
    !settings.database.trim() ||
    new TextEncoder().encode(settings.database.trim()).length > 128 ||
    /[\x00-\x1f\x7f]/.test(settings.database)
  )
    return '请填写有效的数据库名称（最多 128 字节）。'
  if (
    !settings.user.trim() ||
    new TextEncoder().encode(settings.user.trim()).length > 128 ||
    /[\x00-\x1f\x7f]/.test(settings.user)
  )
    return '请填写数据库用户名（最多 128 字节）。'
  if (settings.password.includes('\0')) return '数据库密码包含无效字符。'
  if (!['preferred', 'required', 'disabled', 'verifyFull'].includes(settings.tlsMode))
    return '请选择有效的 TLS 连接方式。'
  return ''
}

/** Connection errors must never display credentials, even if a driver includes a URL. */
export function databaseError(cause: unknown, settings: DatabaseSettings): string {
  let message = String(cause)
  if (settings.password) {
    message = message.split(settings.password).join('••••••')
    message = message.split(encodeURIComponent(settings.password)).join('••••••')
  }
  return message.replace(/(mysql|postgres(?:ql)?):\/\/[^\s/@]+:[^\s/@]*@/gi, '$1://••••••@')
}

export const databaseApi = {
  load: () => invoke<DatabaseSettings>('load_database_settings'),
  test: (workspacePath: string, settings: DatabaseSettings) =>
    invoke<DatabaseConnectionResult>('test_database_connection', {
      workspacePath,
      settings: cloneDatabaseSettings(settings),
    }),
  transfer: (
    workspacePath: string,
    direction: DatabaseTransferDirection,
    action: DatabaseTransferAction,
    conflictPolicy: DatabaseConflictPolicy = 'abort',
  ) =>
    invoke<DatabaseTransferResult>('transfer_workspace_storage', { workspacePath, direction, action, conflictPolicy }),
}
