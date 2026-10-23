import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { invoke, isTauri } from '@tauri-apps/api/core'
import { DEFAULT_SUMMARY_OUTPUT_SETTINGS, SUMMARY_OUTPUT_STORAGE_KEY, useSummaryOutput, type SummaryExportResult, type SummaryOutputSettings } from '../src/lib/useSummaryOutput'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }))

const configured: SummaryOutputSettings = { outputDirectory: '归档/工作总结' }
const exported: SummaryExportResult = { path: '/workspace/归档/工作总结/20261019-20261022.md', filename: '20261019-20261022.md' }
function deferred<T>() {
  let resolve!: (value: T) => void; let reject!: (cause: unknown) => void
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail })
  return { promise, resolve, reject }
}
const tick = async () => { for (let index = 0; index < 8; index++) await Promise.resolve() }

beforeEach(() => { vi.resetAllMocks(); vi.mocked(isTauri).mockReturnValue(true) })
afterEach(() => { vi.unstubAllGlobals() })

describe('summary output settings', () => {
  it('loads the workspace directory without writing defaults', async () => {
    const output = useSummaryOutput()
    expect(output.settings.value).toEqual(DEFAULT_SUMMARY_OUTPUT_SETTINGS)
    expect(await output.flush()).toBe(true)
    expect(invoke).not.toHaveBeenCalled()
    vi.mocked(invoke).mockResolvedValueOnce(configured)
    expect(await output.load()).toBe(true)
    expect(output.settings.value).toEqual(configured)
    expect(invoke).toHaveBeenCalledExactlyOnceWith('load_summary_settings')
  })

  it('saves the latest directory after edits made during an in-flight write', async () => {
    const write = deferred<void>()
    vi.mocked(invoke).mockReturnValueOnce(write.promise).mockResolvedValue(undefined)
    const output = useSummaryOutput()
    const first = output.save(configured)
    const second = output.save({ outputDirectory: '最终目录' })
    expect(invoke).toHaveBeenCalledTimes(1)
    write.resolve()
    expect(await Promise.all([first, second])).toEqual([true, true])
    expect(output.settings.value.outputDirectory).toBe('最终目录')
    expect(invoke).toHaveBeenLastCalledWith('save_summary_settings', { settings: { outputDirectory: '最终目录' } })
    expect(output.saving.value).toBe(false)
  })

  it('retains a failed directory change for retry instead of treating the old directory as saved', async () => {
    vi.mocked(invoke).mockRejectedValueOnce('disk full').mockResolvedValueOnce(undefined)
    const output = useSummaryOutput()
    expect(await output.save(configured)).toBe(false)
    expect(output.settings.value).toEqual(DEFAULT_SUMMARY_OUTPUT_SETTINGS)
    expect(output.error.value).toContain('disk full')
    expect(await output.flush()).toBe(true)
    expect(output.settings.value).toEqual(configured)
    expect(output.error.value).toBe('')
  })

  it('does not let a slow settings read replace a newer saved directory', async () => {
    const read = deferred<SummaryOutputSettings>()
    vi.mocked(invoke).mockReturnValueOnce(read.promise).mockResolvedValueOnce(undefined)
    const output = useSummaryOutput()
    const loading = output.load()
    await output.save(configured)
    read.resolve(DEFAULT_SUMMARY_OUTPUT_SETTINGS)
    expect(await loading).toBe(true)
    expect(output.settings.value).toEqual(configured)
  })

  it('isolates a new workspace from old reads, writes and queued retries', async () => {
    const oldRead = deferred<SummaryOutputSettings>(); const oldWrite = deferred<void>()
    vi.mocked(invoke).mockReturnValueOnce(oldRead.promise).mockReturnValueOnce(oldWrite.promise).mockResolvedValueOnce({ outputDirectory: '新空间' })
    const output = useSummaryOutput()
    const loading = output.load()
    const saving = output.save(configured)
    const queued = output.save({ outputDirectory: '旧空间待保存' })
    expect(await output.reload()).toBe(true)
    oldRead.resolve({ outputDirectory: '旧空间' }); oldWrite.reject('old failure')
    expect(await Promise.all([loading, saving, queued])).toEqual([false, false, false])
    expect(output.settings.value.outputDirectory).toBe('新空间')
    expect(output.error.value).toBe('')
    expect(output.loading.value).toBe(false)
    expect(await output.flush()).toBe(true)
    expect(invoke).toHaveBeenCalledTimes(3)
  })
})

describe('summary export and directory opening', () => {
  it('reads settings, exports sorted unique dates and leaves Markdown intact', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(configured).mockResolvedValueOnce(exported)
    const output = useSummaryOutput()
    const content = '# 总结\n\n**完成**\n'
    expect(await output.exportSummary('/workspace', ['20261022', '20261019', '20261022'], content, 'md', 'ai')).toBe(true)
    expect(invoke).toHaveBeenNthCalledWith(1, 'load_summary_settings')
    expect(invoke).toHaveBeenLastCalledWith('export_summary', { workspacePath: '/workspace', dates: ['20261019', '20261022'], content, format: 'md' })
    expect(output.result.value).toEqual(exported)
    expect(output.exporting.value).toBe(false)
  })

  it('exports readable AI text while keeping direct text unchanged', async () => {
    vi.mocked(invoke).mockImplementation(command => Promise.resolve(command === 'load_summary_settings' ? configured : exported))
    const output = useSummaryOutput()
    expect(await output.exportSummary('/workspace', ['20261022'], '# 总结\n\n**完成**', 'txt', 'ai')).toBe(true)
    expect(invoke).toHaveBeenLastCalledWith('export_summary', { workspacePath: '/workspace', dates: ['20261022'], content: '总结\n\n完成', format: 'txt' })
    const direct = '1. 保留 **原文**\n   第二行\n'
    expect(await output.exportSummary('/workspace', ['20261022'], direct, 'txt', 'direct')).toBe(true)
    expect(invoke).toHaveBeenLastCalledWith('export_summary', { workspacePath: '/workspace', dates: ['20261022'], content: direct, format: 'txt' })
  })

  it('never exports or opens a stale directory when settings cannot be read', async () => {
    vi.mocked(invoke).mockRejectedValue('cannot read config')
    const output = useSummaryOutput()
    expect(await output.exportSummary('/workspace', ['20261022'], '总结', 'md')).toBe(false)
    expect(output.exportError.value).toContain('cannot read config')
    expect(await output.openDirectory('/workspace')).toBe(false)
    expect(output.openError.value).toContain('cannot read config')
    expect(vi.mocked(invoke).mock.calls.every(call => call[0] === 'load_summary_settings')).toBe(true)
  })

  it('blocks export and opening until a pending directory change saves successfully', async () => {
    vi.mocked(invoke).mockImplementation(command => command === 'save_summary_settings' ? Promise.reject('disk full') : Promise.resolve(configured))
    const output = useSummaryOutput()
    expect(await output.save({ outputDirectory: '新目录' })).toBe(false)
    expect(await output.exportSummary('/workspace', ['20261022'], '总结', 'md')).toBe(false)
    expect(await output.openDirectory('/workspace')).toBe(false)
    expect(output.exportError.value).toContain('disk full')
    expect(output.openError.value).toContain('disk full')
    expect(vi.mocked(invoke).mock.calls.some(call => call[0] === 'export_summary' || call[0] === 'open_summary_output_directory')).toBe(false)
    vi.mocked(invoke).mockImplementation(command => Promise.resolve(command === 'load_summary_settings' ? configured : exported))
    expect(await output.exportSummary('/workspace', ['20261022'], '总结', 'md')).toBe(true)
    const commands = vi.mocked(invoke).mock.calls.map(call => call[0])
    expect(commands.slice(-3)).toEqual(['load_summary_settings', 'save_summary_settings', 'export_summary'])
    expect(output.settings.value.outputDirectory).toBe('新目录')
  })

  it('rejects empty directory changes rather than exporting with the previous directory', async () => {
    vi.mocked(invoke).mockResolvedValue(configured)
    const output = useSummaryOutput()
    expect(await output.save({ outputDirectory: '   ' })).toBe(false)
    expect(await output.exportSummary('/workspace', ['20261022'], '总结', 'md')).toBe(false)
    expect(output.exportError.value).toContain('不能为空')
    expect(invoke).toHaveBeenCalledExactlyOnceWith('load_summary_settings')
  })

  it('deduplicates exports while writing and prevents an old dialog result from appearing in a new one', async () => {
    const write = deferred<SummaryExportResult>()
    vi.mocked(invoke).mockResolvedValueOnce(configured).mockReturnValueOnce(write.promise)
    const output = useSummaryOutput()
    const pending = output.exportSummary('/workspace', ['20261022'], '总结', 'md')
    await tick()
    expect(invoke).toHaveBeenLastCalledWith('export_summary', expect.any(Object))
    expect(await output.exportSummary('/workspace', ['20261022'], '重复', 'md')).toBe(false)
    output.clearExport()
    expect(output.exporting.value).toBe(true)
    write.resolve(exported)
    expect(await pending).toBe(false)
    expect(output.result.value).toBeNull()
    expect(output.exporting.value).toBe(false)
    expect(output.exportError.value).toBe('')
  })

  it('does not start a hidden export if the dialog closes during settings loading', async () => {
    const read = deferred<SummaryOutputSettings>()
    vi.mocked(invoke).mockReturnValueOnce(read.promise)
    const output = useSummaryOutput()
    const pending = output.exportSummary('/workspace', ['20261022'], '总结', 'md')
    output.clearExport()
    read.resolve(configured)
    expect(await pending).toBe(false)
    expect(invoke).toHaveBeenCalledExactlyOnceWith('load_summary_settings')
  })

  it('discards late export and directory errors after changing workspace', async () => {
    const write = deferred<SummaryExportResult>(); const open = deferred<void>()
    vi.mocked(invoke).mockImplementation(command => {
      if (command === 'export_summary') return write.promise
      if (command === 'open_summary_output_directory') return open.promise
      return Promise.resolve(configured)
    })
    const output = useSummaryOutput()
    const exporting = output.exportSummary('/old', ['20261022'], '总结', 'md')
    await tick()
    const opening = output.openDirectory('/old')
    await tick()
    expect(await output.reload()).toBe(true)
    write.reject('old write failure'); open.reject('old open failure')
    expect(await Promise.all([exporting, opening])).toEqual([false, false])
    expect(output.exportError.value).toBe(''); expect(output.openError.value).toBe('')
    expect(output.result.value).toBeNull()
    expect(output.exporting.value).toBe(false); expect(output.opening.value).toBe(false)
  })

  it('loads the configured directory and opens it with the workspace snapshot', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(configured).mockResolvedValueOnce(undefined)
    const output = useSummaryOutput()
    expect(await output.openDirectory('/workspace')).toBe(true)
    expect(invoke).toHaveBeenLastCalledWith('open_summary_output_directory', { workspacePath: '/workspace' })
    expect(output.opening.value).toBe(false)
  })

  it('allows browser preference previews but never invokes native output or directory opening', async () => {
    const storage = new Map<string, string>([[SUMMARY_OUTPUT_STORAGE_KEY, JSON.stringify(configured)]])
    vi.stubGlobal('localStorage', { getItem: vi.fn((key: string) => storage.get(key) ?? null), setItem: vi.fn((key: string, value: string) => storage.set(key, value)) })
    vi.mocked(isTauri).mockReturnValue(false)
    const output = useSummaryOutput()
    expect(await output.load()).toBe(true)
    expect(output.settings.value).toEqual(configured)
    expect(await output.save({ outputDirectory: '预览总结' })).toBe(true)
    expect(JSON.parse(storage.get(SUMMARY_OUTPUT_STORAGE_KEY)!)).toEqual({ outputDirectory: '预览总结' })
    expect(await output.exportSummary('/workspace', ['20261022'], '总结', 'md')).toBe(false)
    expect(await output.openDirectory('/workspace')).toBe(false)
    expect(output.exportError.value).toContain('桌面应用'); expect(output.openError.value).toContain('桌面应用')
    expect(invoke).not.toHaveBeenCalled()
  })
})
