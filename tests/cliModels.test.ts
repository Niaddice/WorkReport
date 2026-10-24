import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { effectScope, reactive, type EffectScope } from 'vue'
import { aiApi, type AiModel, type ProviderKind } from '../src/lib/ai'
import { useCliModels } from '../src/lib/useCliModels'

const available: AiModel[] = [{ id: 'gpt-a', name: 'GPT A' }]
let scope: EffectScope
beforeEach(() => {
  vi.restoreAllMocks()
  scope = effectScope()
})
afterEach(() => {
  scope.stop()
})

describe('CLI model catalog', () => {
  it('fetches with the current kind and executable', async () => {
    const source = reactive<{ kind: ProviderKind; executable: string }>({
      kind: 'codex',
      executable: ' /opt/homebrew/bin/codex ',
    })
    const catalog = scope.run(() => useCliModels(() => source))!
    vi.spyOn(aiApi, 'cliModels').mockResolvedValue(available)
    expect(await catalog.fetchModels()).toBe(true)
    expect(aiApi.cliModels).toHaveBeenCalledWith('codex', '/opt/homebrew/bin/codex')
    expect(catalog.models.value).toEqual(available)
    expect(catalog.queried.value).toBe(true)
  })

  it('requires a CLI path and supports opencode', async () => {
    const source = reactive<{ kind: ProviderKind; executable: string }>({ kind: 'opencode', executable: '' })
    const catalog = scope.run(() => useCliModels(() => source))!
    vi.spyOn(aiApi, 'cliModels').mockResolvedValue(available)
    expect(await catalog.fetchModels()).toBe(false)
    expect(aiApi.cliModels).not.toHaveBeenCalled()
    expect(catalog.error.value).toContain('CLI 路径')
    source.executable = '/opt/homebrew/bin/opencode'
    expect(await catalog.fetchModels()).toBe(true)
    expect(aiApi.cliModels).toHaveBeenCalledWith('opencode', '/opt/homebrew/bin/opencode')
  })

  it('discards stale results after the executable changes', async () => {
    const source = reactive<{ kind: ProviderKind; executable: string }>({ kind: 'codex', executable: '/opt/codex' })
    const catalog = scope.run(() => useCliModels(() => source))!
    let finish!: (models: AiModel[]) => void
    vi.spyOn(aiApi, 'cliModels')
      .mockReturnValueOnce(
        new Promise((resolve) => {
          finish = resolve
        }),
      )
      .mockResolvedValueOnce(available)
    const old = catalog.fetchModels()
    source.executable = '/opt/codex-new'
    expect(await catalog.fetchModels()).toBe(true)
    finish([{ id: 'stale', name: 'Stale' }])
    expect(await old).toBe(false)
    expect(catalog.models.value).toEqual(available)
    expect(catalog.loading.value).toBe(false)
  })

  it('clears errors and retries', async () => {
    const source = reactive<{ kind: ProviderKind; executable: string }>({ kind: 'codex', executable: '/opt/codex' })
    const catalog = scope.run(() => useCliModels(() => source))!
    vi.spyOn(aiApi, 'cliModels')
      .mockRejectedValueOnce(new Error('CLI 获取模型列表失败'))
      .mockResolvedValueOnce(available)
    expect(await catalog.fetchModels()).toBe(false)
    expect(catalog.error.value).toContain('获取模型列表失败')
    expect(await catalog.fetchModels()).toBe(true)
    expect(catalog.error.value).toBe('')
    expect(catalog.models.value).toEqual(available)
  })

  it('ignores pending results after the editor is removed', async () => {
    const source = reactive<{ kind: ProviderKind; executable: string }>({ kind: 'codex', executable: '/opt/codex' })
    const catalog = scope.run(() => useCliModels(() => source))!
    let finish!: (models: AiModel[]) => void
    vi.spyOn(aiApi, 'cliModels').mockReturnValue(
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
