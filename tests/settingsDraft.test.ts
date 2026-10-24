import { afterEach, describe, expect, it, vi } from 'vitest'
import { effectScope, nextTick, ref, type EffectScope } from 'vue'
import { DEFAULT_DATABASE_SETTINGS, cloneDatabaseSettings } from '../src/lib/database'
import { cloneWorkspaceSettings, useSettingsDraft, type WorkspaceSettings } from '../src/lib/settingsDraft'

const scopes: EffectScope[] = []
afterEach(() => {
  scopes.splice(0).forEach((scope) => scope.stop())
})

function initialSettings(): WorkspaceSettings {
  return {
    appearance: { scale: 100 },
    database: cloneDatabaseSettings(DEFAULT_DATABASE_SETTINGS),
    ai: {
      providers: [
        {
          id: 'main',
          name: '默认 AI',
          kind: 'http',
          executable: '',
          model: 'example-model',
          baseUrl: 'https://example.test/v1',
          apiKey: 'local-key',
        },
      ],
      defaultProviderId: 'main',
      generationPreference: '',
      summaryPrompt: '',
    },
    excel: { enabled: false, templatePath: '', prompt: '', outputDirectory: '' },
    summary: { outputDirectory: '总结' },
  }
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

function createDraft(persist = vi.fn<(settings: WorkspaceSettings) => Promise<boolean>>().mockResolvedValue(true)) {
  const source = ref(initialSettings())
  const scope = effectScope()
  scopes.push(scope)
  const state = scope.run(() => useSettingsDraft(() => source.value, persist))!
  return { source, persist, ...state }
}

describe('manual settings draft', () => {
  it('keeps scale, AI selection and every section local until an explicit save', async () => {
    const state = createDraft()
    const original = initialSettings()
    state.draft.value.database.enabled = true
    state.draft.value.appearance.scale = 120
    state.draft.value.ai.providers[0]!.name = '修改后的 AI'
    state.draft.value.ai.providers.push({
      id: 'second',
      name: '备用 AI',
      kind: 'http',
      executable: '',
      model: 'second-model',
      baseUrl: 'https://second.example.test/v1',
      apiKey: '',
    })
    state.draft.value.ai.defaultProviderId = 'second'
    state.draft.value.ai.generationPreference = '改写时保留数字'
    state.draft.value.ai.summaryPrompt = '按项目整理'
    state.draft.value.summary.outputDirectory = '总结/周报'
    state.draft.value.excel = {
      enabled: true,
      templatePath: '/templates/week.xlsx',
      prompt: '填入工作明细',
      outputDirectory: '/exports',
    }
    await nextTick()
    expect(state.unchanged.value).toBe(false)
    expect(state.source.value).toEqual(original)
    expect(state.persist).not.toHaveBeenCalled()
  })

  it('submits one complete detached snapshot only when save is clicked', async () => {
    const state = createDraft()
    state.draft.value.appearance.scale = 90
    state.draft.value.ai.summaryPrompt = '用简短段落总结'
    state.draft.value.summary.outputDirectory = '输出/总结'
    state.draft.value.excel.outputDirectory = '/excel-output'
    state.draft.value.database.enabled = true
    state.draft.value.database.sqlitePath = 'backup/reports.sqlite'
    const expected = cloneWorkspaceSettings(state.draft.value)
    expect(await state.save()).toBe(true)
    expect(state.persist).toHaveBeenCalledExactlyOnceWith(expected)
    expect(state.unchanged.value).toBe(true)
    expect(state.saved.value).toBe(true)
    state.draft.value.ai.providers[0]!.name = '之后的修改'
    expect(state.persist.mock.calls[0]![0].ai.providers[0]!.name).toBe('默认 AI')
    expect(state.unchanged.value).toBe(false)
    expect(state.saved.value).toBe(false)
  })

  it.each(['false', 'rejection'] as const)(
    'retains edits and dirty state when persistence fails by %s',
    async (failure) => {
      const persist = vi.fn<(settings: WorkspaceSettings) => Promise<boolean>>()
      if (failure === 'false') persist.mockResolvedValueOnce(false)
      else persist.mockRejectedValueOnce(new Error('磁盘已满'))
      const state = createDraft(persist)
      state.draft.value.summary.outputDirectory = '新的总结目录'
      expect(await state.save()).toBe(false)
      expect(state.draft.value.summary.outputDirectory).toBe('新的总结目录')
      expect(state.source.value.summary.outputDirectory).toBe('总结')
      expect(state.unchanged.value).toBe(false)
      expect(state.saved.value).toBe(false)
      expect(state.saving.value).toBe(false)
      expect(state.error.value).toContain('保存设置失败')
      persist.mockResolvedValueOnce(true)
      expect(await state.save()).toBe(true)
      expect(persist).toHaveBeenCalledTimes(2)
      expect(state.error.value).toBe('')
    },
  )

  it('does not persist unchanged settings or a temporary edit reverted before saving', async () => {
    const state = createDraft()
    expect(await state.save()).toBe(true)
    state.draft.value.appearance.scale = 80
    state.draft.value.appearance.scale = 100
    expect(await state.save()).toBe(true)
    expect(state.persist).not.toHaveBeenCalled()
  })

  it('discards every unsaved section on reset without writing any settings', () => {
    const state = createDraft()
    state.draft.value.appearance.scale = 110
    state.draft.value.ai.providers.length = 0
    state.draft.value.ai.defaultProviderId = null
    state.draft.value.excel.enabled = true
    state.draft.value.summary.outputDirectory = '临时目录'
    state.reset()
    expect(state.draft.value).toEqual(initialSettings())
    expect(state.unchanged.value).toBe(true)
    expect(state.saved.value).toBe(false)
    expect(state.error.value).toBe('')
    expect(state.persist).not.toHaveBeenCalled()
    state.draft.value.ai.providers[0]!.name = '独立副本'
    expect(state.source.value.ai.providers[0]!.name).toBe('默认 AI')
  })

  it('accepts background source reloads while clean and preserves local edits while dirty', async () => {
    const state = createDraft()
    state.source.value = { ...initialSettings(), appearance: { scale: 90 } }
    await nextTick()
    expect(state.draft.value.appearance.scale).toBe(90)
    expect(state.unchanged.value).toBe(true)
    state.draft.value.ai.summaryPrompt = '我的未保存提示词'
    state.source.value = { ...initialSettings(), appearance: { scale: 120 }, summary: { outputDirectory: '后台目录' } }
    await nextTick()
    expect(state.draft.value.appearance.scale).toBe(90)
    expect(state.draft.value.ai.summaryPrompt).toBe('我的未保存提示词')
    expect(state.draft.value.summary.outputDirectory).toBe('总结')
    expect(state.unchanged.value).toBe(false)
    expect(state.persist).not.toHaveBeenCalled()
    state.reset()
    expect(state.draft.value).toEqual(state.source.value)
    expect(state.draft.value.summary.outputDirectory).toBe('后台目录')
  })

  it('shares concurrent save requests and persists the snapshot just once', async () => {
    const write = deferred<boolean>()
    const persist = vi.fn<(settings: WorkspaceSettings) => Promise<boolean>>().mockReturnValue(write.promise)
    const state = createDraft(persist)
    state.draft.value.ai.summaryPrompt = '合并重复保存'
    const first = state.save()
    const second = state.save()
    expect(first).toBe(second)
    expect(state.saving.value).toBe(true)
    expect(persist).toHaveBeenCalledTimes(1)
    write.resolve(true)
    expect(await Promise.all([first, second])).toEqual([true, true])
    expect(state.unchanged.value).toBe(true)
    expect(state.saving.value).toBe(false)
  })

  it('retains edits made during an in-flight save and requires another explicit save', async () => {
    const write = deferred<boolean>()
    const persist = vi
      .fn<(settings: WorkspaceSettings) => Promise<boolean>>()
      .mockReturnValueOnce(write.promise)
      .mockResolvedValue(true)
    const state = createDraft(persist)
    state.draft.value.appearance.scale = 110
    const first = state.save()
    state.draft.value.appearance.scale = 120
    state.draft.value.summary.outputDirectory = '保存中的新修改'
    write.resolve(true)
    expect(await first).toBe(false)
    expect(state.draft.value.appearance.scale).toBe(120)
    expect(state.draft.value.summary.outputDirectory).toBe('保存中的新修改')
    expect(state.unchanged.value).toBe(false)
    expect(state.saved.value).toBe(false)
    expect(state.error.value).toContain('未保存')
    expect(persist).toHaveBeenCalledTimes(1)
    expect(persist.mock.calls[0]![0].appearance.scale).toBe(110)
    expect(await state.save()).toBe(true)
    expect(persist).toHaveBeenCalledTimes(2)
    expect(persist.mock.calls[1]![0].appearance.scale).toBe(120)
    expect(state.unchanged.value).toBe(true)
  })

  it('uses the server-normalized result as the saved baseline and displayed draft', async () => {
    const write = deferred<boolean>()
    const persist = vi.fn<(settings: WorkspaceSettings) => Promise<boolean>>().mockReturnValue(write.promise)
    const state = createDraft(persist)
    state.draft.value.ai.providers[0]!.name = '  模型名称  '
    state.draft.value.summary.outputDirectory = '  总结/周报  '
    const saving = state.save()
    const committed = cloneWorkspaceSettings(persist.mock.calls[0]![0])
    committed.ai.providers[0]!.name = '模型名称'
    committed.summary.outputDirectory = '总结/周报'
    state.source.value = committed
    await nextTick()
    expect(state.draft.value.summary.outputDirectory).toBe('  总结/周报  ')
    write.resolve(true)
    expect(await saving).toBe(true)
    expect(state.draft.value).toEqual(committed)
    expect(state.unchanged.value).toBe(true)
    expect(state.saved.value).toBe(true)
    expect(await state.save()).toBe(true)
    expect(persist).toHaveBeenCalledTimes(1)
  })
})
