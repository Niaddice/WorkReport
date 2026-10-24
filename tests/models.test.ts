import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { effectScope, reactive, type EffectScope } from 'vue'
import { aiApi, type AiModel } from '../src/lib/ai'
import { useAiModels } from '../src/lib/useAiModels'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
const available: AiModel[] = [{ id: 'text-model', name: 'Text model' }]
let scope: EffectScope
beforeEach(() => {
  vi.restoreAllMocks()
  scope = effectScope()
})
afterEach(() => {
  scope.stop()
})

describe('model catalog', () => {
  it('fetches with the current unsaved endpoint and key, without needing a model ID', async () => {
    const source = reactive({ baseUrl: ' https://example.com/v1 ', apiKey: 'test-key' })
    const catalog = scope.run(() => useAiModels(() => source))!
    vi.spyOn(aiApi, 'models').mockResolvedValue(available)
    expect(await catalog.fetchModels()).toBe(true)
    expect(aiApi.models).toHaveBeenCalledWith('https://example.com/v1', 'test-key')
    expect(catalog.models.value).toEqual(available)
    expect(catalog.queried.value).toBe(true)
  })
  it('requires an endpoint and allows an unauthenticated local endpoint', async () => {
    const source = reactive({ baseUrl: '', apiKey: '' })
    const catalog = scope.run(() => useAiModels(() => source))!
    vi.spyOn(aiApi, 'models').mockResolvedValue(available)
    expect(await catalog.fetchModels()).toBe(false)
    expect(aiApi.models).not.toHaveBeenCalled()
    source.baseUrl = 'http://localhost:1234/v1'
    expect(await catalog.fetchModels()).toBe(true)
    expect(aiApi.models).toHaveBeenCalledWith(source.baseUrl, '')
    expect(catalog.error.value).toBe('')
  })
  it('discards stale results when the endpoint or API key changes', async () => {
    const source = reactive({ baseUrl: 'https://first.example/v1', apiKey: 'old-key' })
    const catalog = scope.run(() => useAiModels(() => source))!
    let finish!: (models: AiModel[]) => void
    vi.spyOn(aiApi, 'models')
      .mockReturnValueOnce(
        new Promise((resolve) => {
          finish = resolve
        }),
      )
      .mockResolvedValueOnce(available)
    const old = catalog.fetchModels()
    source.apiKey = 'new-key'
    source.baseUrl = 'https://second.example/v1'
    expect(await catalog.fetchModels()).toBe(true)
    finish([{ id: 'stale', name: 'Stale model' }])
    expect(await old).toBe(false)
    expect(catalog.models.value).toEqual(available)
    expect(catalog.loading.value).toBe(false)
    source.apiKey = 'another-key'
    expect(catalog.models.value).toEqual([])
    expect(catalog.queried.value).toBe(false)
  })
  it('clears stale errors, handles empty catalogs, and permits retry', async () => {
    const source = reactive({ baseUrl: 'https://example.com/v1', apiKey: '' })
    const catalog = scope.run(() => useAiModels(() => source))!
    vi.spyOn(aiApi, 'models').mockRejectedValueOnce(new Error('HTTP 401')).mockResolvedValueOnce([])
    expect(await catalog.fetchModels()).toBe(false)
    expect(catalog.error.value).toContain('401')
    expect(await catalog.fetchModels()).toBe(true)
    expect(catalog.error.value).toBe('')
    expect(catalog.models.value).toEqual([])
    expect(catalog.queried.value).toBe(true)
  })
  it('ignores pending results after the provider editor is removed', async () => {
    const catalog = scope.run(() => useAiModels(() => ({ baseUrl: 'https://example.com/v1', apiKey: '' })))!
    let finish!: (models: AiModel[]) => void
    vi.spyOn(aiApi, 'models').mockReturnValue(
      new Promise((resolve) => {
        finish = resolve
      }),
    )
    const pending = catalog.fetchModels()
    scope.stop()
    finish(available)
    expect(await pending).toBe(false)
    expect(catalog.models.value).toEqual([])
  })
})
