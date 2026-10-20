import { beforeEach, describe, expect, it, vi } from 'vitest'
import { aiApi, useAi, type AiProvider, type AiSettings } from '../src/lib/ai'
import { useAiSettingsDraft } from '../src/lib/useAiSettingsDraft'
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
const provider: AiProvider = { id: 'one', name: 'AI', kind: 'http', executable: '', model: 'model', baseUrl: 'https://example.com/v1', apiKey: 'test-key' }
describe('AI preview lifecycle', () => {
  beforeEach(() => { vi.restoreAllMocks() })
  it('does not automatically choose a provider', async () => {
    const ai = useAi()
    vi.spyOn(aiApi, 'load').mockResolvedValue({ providers: [provider], defaultProviderId: null, generationPreference: '' })
    await ai.load()
    expect(ai.defaultProvider.value).toBeUndefined()
  })
  it('keeps original text unchanged when regenerating and accepting a result is explicit', async () => {
    const ai = useAi()
    vi.spyOn(aiApi, 'polish').mockResolvedValueOnce('first result').mockResolvedValueOnce('second result')
    ai.open('original text', 'txt')
    await vi.waitFor(() => expect(ai.generating.value).toBe(false))
    expect(ai.result.value).toBe('first result')
    await ai.generate()
    expect(aiApi.polish).toHaveBeenLastCalledWith('original text', 'txt', expect.any(String))
    expect(ai.original.value).toBe('original text')
    expect(ai.result.value).toBe('second result')
  })
  it('cancels requests and ignores late results after the preview is closed', async () => {
    const ai = useAi()
    let resolve!: (text: string) => void
    vi.spyOn(aiApi, 'polish').mockReturnValue(new Promise((r) => { resolve = r }))
    vi.spyOn(aiApi, 'cancel').mockResolvedValue()
    ai.open('original', 'md'); ai.close(); resolve('late result')
    await Promise.resolve()
    expect(aiApi.cancel).toHaveBeenCalledOnce()
    expect(ai.visible.value).toBe(false)
    expect(ai.result.value).toBe('')
  })
  it('keeps saved settings intact when a configuration save fails', async () => {
    const ai = useAi()
    ai.settings.value = { providers: [provider], defaultProviderId: 'one', generationPreference: '' }
    vi.spyOn(aiApi, 'save').mockRejectedValue(new Error('invalid endpoint'))
    expect(await ai.save({ providers: [], defaultProviderId: null, generationPreference: '' })).toBe(false)
    expect(ai.defaultProvider.value?.id).toBe('one')
  })
  it('saves the selected default immediately so home and the next app launch use it', async () => {
    let stored: AiSettings = { providers: [provider], defaultProviderId: null, generationPreference: '' }
    vi.spyOn(aiApi, 'load').mockImplementation(async () => structuredClone(stored))
    vi.spyOn(aiApi, 'save').mockImplementation(async (settings) => { stored = settings })
    const ai = useAi()
    await ai.load()
    const form = useAiSettingsDraft(() => ai.settings.value, ai.save)
    expect(await form.selectDefault('one')).toBe(true)
    expect(stored.defaultProviderId).toBe('one')
    expect(ai.defaultProvider.value?.id).toBe('one')
    const restarted = useAi()
    await restarted.load()
    expect(restarted.defaultProvider.value?.id).toBe('one')
  })
  it('waits for an automatic default save before leaving settings', async () => {
    const ai = useAi()
    ai.settings.value = { providers: [provider], defaultProviderId: null, generationPreference: '' }
    let finish!: () => void
    vi.spyOn(aiApi, 'save').mockImplementation(() => new Promise((resolve) => { finish = resolve }))
    const form = useAiSettingsDraft(() => ai.settings.value, ai.save)
    const selection = form.selectDefault('one')
    let leftSettings = false
    const leaving = form.flush().then((saved) => { leftSettings = saved })
    await Promise.resolve()
    expect(leftSettings).toBe(false)
    finish()
    await Promise.all([selection, leaving])
    expect(leftSettings).toBe(true)
    expect(ai.defaultProvider.value?.id).toBe('one')
    expect(aiApi.save).toHaveBeenCalledOnce()
  })
  it('rolls back a failed default selection and blocks leaving with invalid changes', async () => {
    const ai = useAi()
    ai.settings.value = { providers: [provider], defaultProviderId: null, generationPreference: '' }
    vi.spyOn(aiApi, 'save').mockRejectedValue(new Error('invalid endpoint'))
    const form = useAiSettingsDraft(() => ai.settings.value, ai.save)
    expect(await form.selectDefault('one')).toBe(false)
    expect(form.draft.value.defaultProviderId).toBeNull()
    expect(ai.defaultProvider.value).toBeUndefined()
    form.draft.value.providers[0]!.baseUrl = 'invalid'
    expect(await form.flush()).toBe(false)
    expect(ai.error.value).toContain('invalid endpoint')
  })
  it('saves other form edits before returning home', async () => {
    const ai = useAi()
    ai.settings.value = { providers: [provider], defaultProviderId: 'one', generationPreference: '' }
    vi.spyOn(aiApi, 'save').mockResolvedValue()
    const form = useAiSettingsDraft(() => ai.settings.value, ai.save)
    form.draft.value.providers[0]!.model = 'new-model'
    expect(await form.flush()).toBe(true)
    expect(ai.defaultProvider.value?.model).toBe('new-model')
  })
  it('saves provider removal even when Vue array filtering retains nested proxies', async () => {
    const ai = useAi()
    ai.settings.value = { providers: [provider, { ...provider, id: 'remove' }], defaultProviderId: 'one', generationPreference: '保留原意' }
    vi.spyOn(aiApi, 'save').mockResolvedValue()
    const form = useAiSettingsDraft(() => ai.settings.value, ai.save)
    form.draft.value.providers = form.draft.value.providers.filter(item => item.id !== 'remove')
    expect(await form.flush()).toBe(true)
    expect(ai.settings.value.providers.map(item => item.id)).toEqual(['one'])
    expect(ai.defaultProvider.value?.id).toBe('one')
    expect(ai.settings.value.generationPreference).toBe('保留原意')
  })
  it('persists generation preferences through the settings form and app restart', async () => {
    let stored: AiSettings = { providers: [provider], defaultProviderId: 'one', generationPreference: '' }
    vi.spyOn(aiApi, 'load').mockImplementation(async () => structuredClone(stored))
    vi.spyOn(aiApi, 'save').mockImplementation(async (settings) => { stored = settings })
    const ai = useAi()
    await ai.load()
    const form = useAiSettingsDraft(() => ai.settings.value, ai.save)
    form.draft.value.generationPreference = '简洁客观，保留条目结构'
    expect(form.unchanged.value).toBe(false)
    expect(await form.flush()).toBe(true)
    expect(stored.generationPreference).toBe('简洁客观，保留条目结构')
    expect(stored.defaultProviderId).toBe('one')
    const restarted = useAi()
    await restarted.load()
    expect(restarted.settings.value.generationPreference).toBe('简洁客观，保留条目结构')
    const restartedForm = useAiSettingsDraft(() => restarted.settings.value, restarted.save)
    restartedForm.draft.value.generationPreference = ''
    expect(await restartedForm.flush()).toBe(true)
    expect(stored.generationPreference).toBe('')
    expect(stored.defaultProviderId).toBe('one')
  })
  it('does not let a late startup load overwrite a newly saved default', async () => {
    const ai = useAi()
    let finish!: (settings: AiSettings) => void
    vi.spyOn(aiApi, 'load').mockReturnValue(new Promise((resolve) => { finish = resolve }))
    vi.spyOn(aiApi, 'save').mockResolvedValue()
    const loading = ai.load()
    await ai.save({ providers: [provider], defaultProviderId: 'one', generationPreference: '' })
    finish({ providers: [provider], defaultProviderId: null, generationPreference: '' })
    await loading
    expect(ai.defaultProvider.value?.id).toBe('one')
  })
  it('reports elapsed time while generating', async () => {
    vi.useFakeTimers()
    try {
      const ai = useAi()
      let resolve!: (text: string) => void
      vi.spyOn(aiApi, 'polish').mockReturnValue(new Promise((done) => { resolve = done }))
      ai.open('original', 'md')
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
  it('refreshes the saved default and reports configuration read failures', async () => {
    const ai = useAi()
    vi.spyOn(aiApi, 'load')
      .mockResolvedValueOnce({ providers: [provider], defaultProviderId: 'one', generationPreference: '' })
      .mockRejectedValueOnce(new Error('configuration unreadable'))
    expect(await ai.load()).toBe(true)
    expect(ai.defaultProvider.value?.id).toBe('one')
    expect(await ai.load()).toBe(false)
    expect(ai.error.value).toContain('configuration unreadable')
  })
})
