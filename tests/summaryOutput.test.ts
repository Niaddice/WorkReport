import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { invoke, isTauri } from '@tauri-apps/api/core'
import {
  DEFAULT_SUMMARY_OUTPUT_SETTINGS,
  SUMMARY_OUTPUT_STORAGE_KEY,
  useSummaryOutput,
  type SummaryExportResult,
  type SummaryOutputSettings,
} from '../src/lib/useSummaryOutput'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }))

const configured: SummaryOutputSettings = { outputDirectory: '归档/工作总结' }
const exported: SummaryExportResult = {
  path: '/workspace/归档/工作总结/总结_20261019-20261022_20261028_160500.md',
  filename: '总结_20261019-20261022_20261028_160500.md',
}
function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (cause: unknown) => void
  const promise = new Promise<T>((done, fail) => {
    resolve = done
    reject = fail
  })
  return { promise, resolve, reject }
}
const tick = async () => {
  for (let index = 0; index < 8; index++) await Promise.resolve()
}

beforeEach(() => {
  vi.resetAllMocks()
  vi.mocked(isTauri).mockReturnValue(true)
})
afterEach(() => {
  vi.unstubAllGlobals()
})

describe('summary output settings', () => {
  it('defaults to summary and migrates the old default directory', async () => {
    const output = useSummaryOutput()
    expect(output.settings.value.outputDirectory).toBe('summary')
    vi.mocked(invoke).mockResolvedValueOnce({ outputDirectory: '总结' })
    expect(await output.load()).toBe(true)
    expect(output.settings.value.outputDirectory).toBe('summary')
  })
  it('loads the workspace directory without writing defaults', async () => {
    const output = useSummaryOutput()
    expect(output.settings.value).toEqual(DEFAULT_SUMMARY_OUTPUT_SETTINGS)
    expect(invoke).not.toHaveBeenCalled()
    vi.mocked(invoke).mockResolvedValueOnce(configured)
    expect(await output.load()).toBe(true)
    expect(output.settings.value).toEqual(configured)
    expect(invoke).toHaveBeenCalledExactlyOnceWith('load_summary_settings')
  })

  it('reads settings, exports sorted unique dates and leaves Markdown intact', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(configured).mockResolvedValueOnce(exported)
    const output = useSummaryOutput()
    const content = '# 总结\n\n**完成**\n'
    expect(await output.exportSummary('/workspace', ['20261022', '20261019', '20261022'], content, 'md', 'ai')).toBe(
      true,
    )
    expect(invoke).toHaveBeenNthCalledWith(1, 'load_summary_settings')
    expect(invoke).toHaveBeenLastCalledWith('export_summary', {
      workspacePath: '/workspace',
      dates: ['20261019', '20261022'],
      content,
      format: 'md',
    })
    expect(output.result.value).toEqual(exported)
    expect(output.exporting.value).toBe(false)
  })

  it('exports readable AI text while keeping direct text unchanged', async () => {
    vi.mocked(invoke).mockImplementation((command) =>
      Promise.resolve(command === 'load_summary_settings' ? configured : exported),
    )
    const output = useSummaryOutput()
    expect(await output.exportSummary('/workspace', ['20261022'], '# 总结\n\n**完成**', 'txt', 'ai')).toBe(true)
    expect(invoke).toHaveBeenLastCalledWith('export_summary', {
      workspacePath: '/workspace',
      dates: ['20261022'],
      content: '总结\n\n完成',
      format: 'txt',
    })
    const direct = '1. 保留 **原文**\n   第二行\n'
    expect(await output.exportSummary('/workspace', ['20261022'], direct, 'txt', 'direct')).toBe(true)
    expect(invoke).toHaveBeenLastCalledWith('export_summary', {
      workspacePath: '/workspace',
      dates: ['20261022'],
      content: direct,
      format: 'txt',
    })
  })

  it('never exports or opens a stale directory when settings cannot be read', async () => {
    vi.mocked(invoke).mockRejectedValue('cannot read config')
    const output = useSummaryOutput()
    expect(await output.exportSummary('/workspace', ['20261022'], '总结', 'md')).toBe(false)
    expect(output.exportError.value).toContain('cannot read config')
    expect(await output.openDirectory('/workspace')).toBe(false)
    expect(output.openError.value).toContain('cannot read config')
    expect(vi.mocked(invoke).mock.calls.every((call) => call[0] === 'load_summary_settings')).toBe(true)
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
    const write = deferred<SummaryExportResult>()
    const open = deferred<void>()
    vi.mocked(invoke).mockImplementation((command) => {
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
    write.reject('old write failure')
    open.reject('old open failure')
    expect(await Promise.all([exporting, opening])).toEqual([false, false])
    expect(output.exportError.value).toBe('')
    expect(output.openError.value).toBe('')
    expect(output.result.value).toBeNull()
    expect(output.exporting.value).toBe(false)
    expect(output.opening.value).toBe(false)
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
    vi.stubGlobal('localStorage', {
      getItem: vi.fn((key: string) => storage.get(key) ?? null),
      setItem: vi.fn((key: string, value: string) => storage.set(key, value)),
    })
    vi.mocked(isTauri).mockReturnValue(false)
    const output = useSummaryOutput()
    expect(await output.load()).toBe(true)
    expect(output.settings.value).toEqual(configured)
    expect(await output.exportSummary('/workspace', ['20261022'], '总结', 'md')).toBe(false)
    expect(await output.openDirectory('/workspace')).toBe(false)
    expect(output.exportError.value).toContain('桌面应用')
    expect(output.openError.value).toContain('桌面应用')
    expect(invoke).not.toHaveBeenCalled()
  })
})
