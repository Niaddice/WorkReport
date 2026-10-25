import { beforeEach, describe, expect, it, vi } from 'vitest'
import { aiApi, useAi, type AiProvider, type AiSettings } from '../src/lib/ai'
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
const provider: AiProvider = {
  id: 'one',
  name: 'AI',
  kind: 'http',
  executable: '',
  model: 'model',
  baseUrl: 'https://example.com/v1',
  apiKey: 'test-key',
}
describe('AI preview lifecycle', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })
  it('does not automatically choose a provider', async () => {
    const ai = useAi()
    vi.spyOn(aiApi, 'load').mockResolvedValue({
      providers: [provider],
      defaultProviderId: null,
      summaryPrompt: '',
      generationPreference: '',
    })
    await ai.load()
    expect(ai.defaultProvider.value).toBeUndefined()
  })
  it('keeps original text unchanged when regenerating and accepting a result is explicit', async () => {
    const ai = useAi()
    const polish = vi
      .spyOn(aiApi, 'polish')
      .mockResolvedValueOnce('first result')
      .mockResolvedValueOnce('second result')
    ai.open('original text', 'txt')
    expect(ai.visible.value).toBe(true)
    expect(polish).not.toHaveBeenCalled()
    await ai.generate()
    expect(ai.result.value).toBe('first result')
    await ai.generate()
    expect(aiApi.polish).toHaveBeenLastCalledWith('original text', 'txt', expect.any(String))
    expect(ai.original.value).toBe('original text')
    expect(ai.result.value).toBe('second result')
  })
  it('cancels requests and ignores late results after the preview is closed', async () => {
    const ai = useAi()
    let resolve!: (text: string) => void
    vi.spyOn(aiApi, 'polish').mockReturnValue(
      new Promise((r) => {
        resolve = r
      }),
    )
    vi.spyOn(aiApi, 'cancel').mockResolvedValue()
    ai.open('original', 'md')
    void ai.generate()
    ai.close()
    resolve('late result')
    await Promise.resolve()
    expect(aiApi.cancel).toHaveBeenCalledOnce()
    expect(ai.visible.value).toBe(false)
    expect(ai.result.value).toBe('')
  })
  it('reports elapsed time while generating', async () => {
    vi.useFakeTimers()
    try {
      const ai = useAi()
      let resolve!: (text: string) => void
      vi.spyOn(aiApi, 'polish').mockReturnValue(
        new Promise((done) => {
          resolve = done
        }),
      )
      ai.open('original', 'md')
      void ai.generate()
      vi.advanceTimersByTime(3000)
      expect(ai.generating.value).toBe(true)
      expect(ai.elapsedSeconds.value).toBe(3)
      resolve('result')
      await Promise.resolve()
      await Promise.resolve()
      expect(ai.generating.value).toBe(false)
      expect(ai.elapsedSeconds.value).toBe(3)
    } finally {
      vi.useRealTimers()
    }
  })

  it('isolates a new workspace from an earlier slow configuration read', async () => {
    const ai = useAi()
    let finishFirst!: (value: AiSettings) => void
    let finishSecond!: (value: AiSettings) => void
    vi.spyOn(aiApi, 'load')
      .mockReturnValueOnce(
        new Promise((resolve) => {
          finishFirst = resolve
        }),
      )
      .mockReturnValueOnce(
        new Promise((resolve) => {
          finishSecond = resolve
        }),
      )
    const previous = ai.load()
    const current = ai.reload()
    expect(aiApi.load).toHaveBeenCalledTimes(2)
    finishFirst({ providers: [provider], defaultProviderId: 'one', summaryPrompt: '', generationPreference: '旧空间' })
    expect(await previous).toBe(false)
    expect(ai.defaultProvider.value).toBeUndefined()
    finishSecond({ providers: [], defaultProviderId: null, summaryPrompt: '', generationPreference: '新空间' })
    expect(await current).toBe(true)
    expect(ai.settings.value.generationPreference).toBe('新空间')
    expect(ai.defaultProvider.value).toBeUndefined()
  })

  it('keeps the new workspace unconfigured when its settings cannot be read', async () => {
    const ai = useAi()
    vi.spyOn(aiApi, 'load')
      .mockResolvedValueOnce({
        providers: [provider],
        defaultProviderId: 'one',
        summaryPrompt: '',
        generationPreference: '',
      })
      .mockRejectedValueOnce('new workspace unreadable')
    await ai.load()
    expect(await ai.reload()).toBe(false)
    expect(ai.defaultProvider.value).toBeUndefined()
    expect(ai.settings.value.providers).toEqual([])
    expect(ai.error.value).toContain('new workspace unreadable')
  })

  it('refreshes the saved default and reports configuration read failures', async () => {
    const ai = useAi()
    vi.spyOn(aiApi, 'load')
      .mockResolvedValueOnce({
        providers: [provider],
        defaultProviderId: 'one',
        summaryPrompt: '',
        generationPreference: '',
      })
      .mockRejectedValueOnce(new Error('configuration unreadable'))
    expect(await ai.load()).toBe(true)
    expect(ai.defaultProvider.value?.id).toBe('one')
    expect(await ai.load()).toBe(false)
    expect(ai.error.value).toContain('configuration unreadable')
  })

  it('opens the summary dialog without calling the AI', () => {
    const ai = useAi()
    const summarize = vi.spyOn(aiApi, 'summarize').mockResolvedValue('result')
    ai.openSummary({ ai: 'reports', direct: 'plain' }, ['20261019', '20261018'])
    expect(ai.summaryVisible.value).toBe(true)
    expect(ai.summaryResult.value).toBe('')
    expect(ai.summaryMode.value).toBe('')
    expect(ai.summaryDates.value).toEqual(['20261018', '20261019'])
    expect(ai.summaryScopeDates.value).toEqual(['20261018', '20261019'])
    expect(summarize).not.toHaveBeenCalled()
  })
  it('summarizes the selected reports only when the AI action is chosen', async () => {
    const ai = useAi()
    vi.spyOn(aiApi, 'summarize').mockResolvedValue('## 总结\n\n完成事项')
    vi.spyOn(aiApi, 'cancel').mockResolvedValue()
    ai.openSummary({ ai: 'reports', direct: 'plain' }, ['20261019', '20261018'])
    await ai.generateSummary()
    expect(aiApi.summarize).toHaveBeenCalledWith('reports', expect.any(String))
    expect(ai.summaryMode.value).toBe('ai')
    expect(ai.summaryResult.value).toContain('完成事项')
    ai.closeSummary()
    expect(ai.summaryVisible.value).toBe(false)
    expect(ai.summaryMode.value).toBe('')
  })
  it('outputs the merged reports directly as plain text without calling the AI', () => {
    const ai = useAi()
    const summarize = vi.spyOn(aiApi, 'summarize').mockResolvedValue('ai result')
    const direct = '2026年10月18日\n\n完成事项\n\n2026年10月19日\n\n继续推进'
    ai.openSummary({ ai: '### 2026年10月18日 星期日\n\n完成事项', direct }, ['20261018', '20261019'])
    ai.directSummary()
    expect(ai.summaryResult.value).toBe(direct)
    expect(ai.summaryResult.value).not.toContain('###')
    expect(ai.summaryResult.value).not.toContain('---')
    expect(ai.summaryResult.value).not.toContain('星期')
    expect(ai.summaryMode.value).toBe('direct')
    expect(ai.summaryGenerating.value).toBe(false)
    expect(summarize).not.toHaveBeenCalled()
  })
  it('ignores a late summary after it is cancelled', async () => {
    const ai = useAi()
    let resolve!: (text: string) => void
    vi.spyOn(aiApi, 'summarize').mockReturnValue(
      new Promise((done) => {
        resolve = done
      }),
    )
    vi.spyOn(aiApi, 'cancel').mockResolvedValue()
    ai.openSummary({ ai: 'reports', direct: 'plain' }, ['20261018'])
    const pending = ai.generateSummary()
    ai.closeSummary()
    resolve('late summary')
    await pending
    await Promise.resolve()
    expect(ai.summaryVisible.value).toBe(false)
    expect(ai.summaryResult.value).toBe('')
  })
})
