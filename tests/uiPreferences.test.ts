import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { invoke, isTauri } from '@tauri-apps/api/core'
import { UI_PREFERENCES_STORAGE_KEY, normalizeUiPreferences, uiPreferencesApi } from '../src/lib/uiPreferences'
import { flattenLayout } from '../src/modules/layout'
import { SKINS } from '../src/lib/skins'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }))

const storage = new Map<string, string>()

beforeEach(() => {
  vi.resetAllMocks()
  storage.clear()
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => storage.get(key) ?? null,
    setItem: (key: string, value: string) => storage.set(key, value),
  })
})
afterEach(() => {
  vi.unstubAllGlobals()
})

describe('ui preferences persistence', () => {
  it.each(SKINS)('restores the $id skin after a browser-preview save', async ({ id }) => {
    vi.mocked(isTauri).mockReturnValue(false)
    const preferences = normalizeUiPreferences({ skin: id })
    await uiPreferencesApi.save(preferences, '/tmp/preview-workspace')
    expect((await uiPreferencesApi.load()).skin).toBe(id)
    expect(invoke).not.toHaveBeenCalled()
  })

  it.each(SKINS)('preserves the $id skin through desktop persistence', async ({ id }) => {
    vi.mocked(isTauri).mockReturnValue(true)
    const preferences = normalizeUiPreferences({ skin: id })
    await uiPreferencesApi.save(preferences, '/tmp/demo-workspace')
    expect(invoke).toHaveBeenLastCalledWith('save_ui_preferences', {
      workspacePath: '/tmp/demo-workspace',
      preferences,
    })
    vi.mocked(invoke).mockResolvedValueOnce(preferences)
    expect((await uiPreferencesApi.load()).skin).toBe(id)
  })

  it('reads browser-preview preferences without IPC', async () => {
    vi.mocked(isTauri).mockReturnValue(false)
    storage.set(
      UI_PREFERENCES_STORAGE_KEY,
      JSON.stringify({ columns: 1, modules: [{ id: 'history', span: 'full' }], skin: 'default' }),
    )
    const loaded = await uiPreferencesApi.load()
    // 旧缓存会迁移成布局树；只有一个模块时就是单叶子，不再补核心模块。
    expect(flattenLayout(loaded.layout).map((item) => item.id)).toEqual(['history'])
    expect(loaded.layout).toEqual({ type: 'leaf', id: 'history' })
    expect(invoke).not.toHaveBeenCalled()
  })

  it('falls back to defaults for damaged browser caches', async () => {
    vi.mocked(isTauri).mockReturnValue(false)
    storage.set(UI_PREFERENCES_STORAGE_KEY, '{ not json')
    const loaded = await uiPreferencesApi.load()
    expect(flattenLayout(loaded.layout).map((item) => item.id)).toEqual(['calendar', 'editor'])
  })

  it('normalizes and writes browser-preview preferences', async () => {
    vi.mocked(isTauri).mockReturnValue(false)
    const layout = normalizeUiPreferences({
      columns: 1,
      modules: [{ id: 'overview' }],
      skin: 'default',
    }).layout
    await uiPreferencesApi.save({ layout, skin: 'default', savedLayouts: [] }, '/tmp/preview-workspace')
    const saved = JSON.parse(storage.get(UI_PREFERENCES_STORAGE_KEY) ?? '{}')
    expect(flattenLayout(saved.layout).map((item: { id: string }) => item.id)).toEqual(['overview'])
    expect(saved.skin).toBe('default')
  })

  it('loads the legacy IPC payload through the modular normalizer', async () => {
    vi.mocked(isTauri).mockReturnValue(true)
    vi.mocked(invoke).mockResolvedValueOnce({ axis: 'column', order: ['editor', 'calendar'], skin: 'default' })
    const loaded = await uiPreferencesApi.load()
    expect(invoke).toHaveBeenCalledExactlyOnceWith('load_ui_preferences')
    expect(loaded.skin).toBe('default')
    expect(flattenLayout(loaded.layout).map((item) => item.id)).toEqual(['editor', 'calendar'])
  })

  it('sends the normalized modular payload to the backend', async () => {
    vi.mocked(isTauri).mockReturnValue(true)
    vi.mocked(invoke).mockResolvedValue(undefined)
    const layout = normalizeUiPreferences({
      layout: { type: 'leaf', id: 'calendar' },
      skin: 'default',
    }).layout
    await uiPreferencesApi.save({ layout, skin: 'default', savedLayouts: [] }, '/tmp/demo-workspace')
    expect(vi.mocked(invoke)).toHaveBeenCalledOnce()
    const [command, payload] = vi.mocked(invoke).mock.calls[0] as unknown as [
      string,
      { workspacePath: string; preferences: { layout: unknown; skin: string } },
    ]
    expect(command).toBe('save_ui_preferences')
    expect(payload.workspacePath).toBe('/tmp/demo-workspace')
    expect(payload.preferences.skin).toBe('default')
    expect(JSON.stringify(payload.preferences.layout)).toContain('"id":"calendar"')
  })
})
