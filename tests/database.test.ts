import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  cloneDatabaseSettings,
  databaseApi,
  databaseError,
  DEFAULT_DATABASE_SETTINGS,
  DEFAULT_HISTORY_LIMIT_BYTES,
  MAX_HISTORY_LIMIT_BYTES,
  MIN_HISTORY_LIMIT_BYTES,
  validateDatabaseSettings,
  type DatabaseSettings,
} from '../src/lib/database'

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }))

beforeEach(() => {
  invokeMock.mockReset()
})

function activeDatabase(value: Partial<DatabaseSettings> = {}): DatabaseSettings {
  return { ...cloneDatabaseSettings(DEFAULT_DATABASE_SETTINGS), enabled: true, ...value }
}

describe('database configuration', () => {
  it('keeps defaults and snapshots detached from edits and excludes unrelated state', () => {
    const original = activeDatabase({ password: 'keep-local' })
    const copy = cloneDatabaseSettings({ ...original, unexpectedToken: 'never-send' } as DatabaseSettings)
    copy.password = 'changed'
    copy.sqlitePath = 'backups/new.sqlite'
    expect(original.password).toBe('keep-local')
    expect(original.sqlitePath).toBe('workreport.sqlite')
    expect(DEFAULT_DATABASE_SETTINGS.enabled).toBe(false)
    expect(copy).not.toHaveProperty('unexpectedToken')
  })

  it('uses TXT by default while preserving a selected Markdown format', () => {
    expect(DEFAULT_DATABASE_SETTINGS.defaultFormat).toBe('txt')
    expect(cloneDatabaseSettings({ ...DEFAULT_DATABASE_SETTINGS, defaultFormat: 'txt' }).defaultFormat).toBe('txt')
    expect(cloneDatabaseSettings({ ...DEFAULT_DATABASE_SETTINGS, defaultFormat: 'md' }).defaultFormat).toBe('md')
    expect(
      cloneDatabaseSettings({
        ...DEFAULT_DATABASE_SETTINGS,
        defaultFormat: 'bogus',
      } as unknown as DatabaseSettings).defaultFormat,
    ).toBe('txt')
    expect(
      cloneDatabaseSettings({ ...DEFAULT_DATABASE_SETTINGS, defaultFormat: undefined } as unknown as DatabaseSettings)
        .defaultFormat,
    ).toBe('txt')
  })

  it('defaults the history limit to 1 GB and clamps custom values', () => {
    expect(DEFAULT_DATABASE_SETTINGS.historyLimitBytes).toBe(DEFAULT_HISTORY_LIMIT_BYTES)
    expect(
      cloneDatabaseSettings({ ...DEFAULT_DATABASE_SETTINGS, historyLimitBytes: 32 * 1024 * 1024 }).historyLimitBytes,
    ).toBe(MIN_HISTORY_LIMIT_BYTES)
    expect(
      cloneDatabaseSettings({ ...DEFAULT_DATABASE_SETTINGS, historyLimitBytes: 500 * 1024 * 1024 * 1024 })
        .historyLimitBytes,
    ).toBe(MAX_HISTORY_LIMIT_BYTES)
    expect(
      cloneDatabaseSettings({ ...DEFAULT_DATABASE_SETTINGS, historyLimitBytes: Number.NaN }).historyLimitBytes,
    ).toBe(DEFAULT_HISTORY_LIMIT_BYTES)
  })

  it('validates drafts without creating a database or invoking the backend', () => {
    expect(validateDatabaseSettings(activeDatabase())).toBe('')
    expect(validateDatabaseSettings(activeDatabase({ sqlitePath: '' }))).toBe('')
    expect(validateDatabaseSettings(activeDatabase({ sqlitePath: '备份/日报.sqlite3' }))).toBe('')
    expect(
      validateDatabaseSettings(activeDatabase({ engine: 'postgres', port: 0, user: 'writer', tlsMode: 'verifyFull' })),
    ).toBe('')
    // A disabled draft may retain incomplete connection details for later editing.
    expect(validateDatabaseSettings(activeDatabase({ enabled: false, engine: 'mysql', host: '', user: '' }))).toBe('')
    expect(invokeMock).not.toHaveBeenCalled()
  })

  it.each([
    '../outside.sqlite',
    '/tmp/outside.sqlite',
    'C:\\outside.sqlite',
    'a/../b.sqlite',
    'a//b.sqlite',
    'a\\b.sqlite',
    'workreport.json',
    'bad\0.sqlite',
  ])('rejects a SQLite path that cannot be safely created in the workspace: %s', (sqlitePath) => {
    expect(validateDatabaseSettings(activeDatabase({ sqlitePath }))).not.toBe('')
    expect(invokeMock).not.toHaveBeenCalled()
  })

  it.each([-1, 1.5, 65536, Number.NaN])('rejects invalid server port %s before a connection is attempted', (port) => {
    expect(validateDatabaseSettings(activeDatabase({ engine: 'mysql', user: 'writer', port }))).not.toBe('')
    expect(invokeMock).not.toHaveBeenCalled()
  })

  it('requires remote connection fields and rejects URLs, embedded paths and invalid credentials', () => {
    const remote = activeDatabase({ engine: 'postgres', user: 'writer' })
    for (const host of ['', 'https://db.example', 'db.example/path', 'user@db.example', 'db\\socket'])
      expect(validateDatabaseSettings({ ...remote, host })).not.toBe('')
    expect(validateDatabaseSettings({ ...remote, user: '' })).not.toBe('')
    expect(validateDatabaseSettings({ ...remote, database: '' })).not.toBe('')
    expect(validateDatabaseSettings({ ...remote, database: '库'.repeat(43) })).not.toBe('')
    expect(validateDatabaseSettings({ ...remote, password: 'bad\0password' })).not.toBe('')
  })

  it('redacts raw and URL encoded passwords as well as driver URL credentials', () => {
    const settings = activeDatabase({ engine: 'mysql', password: 'secret ?#@/百分比%' })
    const message = `Cannot connect: mysql://writer:${encodeURIComponent(settings.password)}@db.example/workreport; password=${settings.password}`
    const result = databaseError(new Error(message), settings)
    expect(result).not.toContain(settings.password)
    expect(result).not.toContain(encodeURIComponent(settings.password))
    expect(result).not.toContain('writer:')
    expect(result).toContain('db.example')
    expect(databaseError('postgresql://other:unknown-secret@db.example/data', activeDatabase())).not.toContain(
      'unknown-secret',
    )
  })
})

describe('database command boundaries', () => {
  it('loads only the current workspace configuration', async () => {
    const settings = activeDatabase()
    invokeMock.mockResolvedValueOnce(settings)
    expect(await databaseApi.load()).toBe(settings)
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('load_database_settings')
  })

  it('tests a detached copy of the current draft and never saves it', async () => {
    const draft = activeDatabase({ engine: 'mysql', user: 'writer', password: 'for-test-only' })
    const expected = cloneDatabaseSettings(draft)
    const response = { message: '连接成功', recordCount: 4 }
    invokeMock.mockResolvedValueOnce(response)
    const pending = databaseApi.test('/workspace', { ...draft, unrelatedKey: 'excluded' } as DatabaseSettings)
    draft.password = 'edited-after-test'
    expect(await pending).toBe(response)
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('test_database_connection', {
      workspacePath: '/workspace',
      settings: expected,
    })
  })

  it('transfers using saved credentials in the backend and defaults to stopping on conflict', async () => {
    invokeMock.mockResolvedValue(undefined)
    await databaseApi.transfer('/workspace', 'filesToDatabase', 'copy')
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('transfer_workspace_storage', {
      workspacePath: '/workspace',
      direction: 'filesToDatabase',
      action: 'copy',
      conflictPolicy: 'abort',
    })
    const args = invokeMock.mock.calls[0]![1]
    expect(args).not.toHaveProperty('settings')
    expect(args).not.toHaveProperty('password')
  })

  it('sends explicit overwrite intent for a confirmed migration in either direction', async () => {
    invokeMock.mockResolvedValue(undefined)
    await databaseApi.transfer('/workspace', 'databaseToFiles', 'migrate', 'overwrite')
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('transfer_workspace_storage', {
      workspacePath: '/workspace',
      direction: 'databaseToFiles',
      action: 'migrate',
      conflictPolicy: 'overwrite',
    })
  })
})
