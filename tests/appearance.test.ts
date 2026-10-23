import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { invoke, isTauri } from '@tauri-apps/api/core'
import { APPEARANCE_STORAGE_KEY, useAppearance } from '../src/lib/useAppearance'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }))

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail })
  return { promise, resolve, reject }
}

beforeEach(() => { vi.resetAllMocks(); vi.mocked(isTauri).mockReturnValue(true) })
afterEach(() => { vi.unstubAllGlobals() })

describe('appearance settings', () => {
  it('starts at 100% and reads saved settings without writing defaults', async () => {
    const appearance = useAppearance()
    expect(appearance.scale.value).toBe(100)
    expect(await appearance.flush()).toBe(true)
    expect(invoke).not.toHaveBeenCalled()
    vi.mocked(invoke).mockResolvedValue({ scale: 80 })
    expect(await appearance.load()).toBe(true)
    expect(appearance.scale.value).toBe(80)
    expect(invoke).toHaveBeenCalledExactlyOnceWith('load_appearance_settings')
    expect(appearance.loading.value).toBe(false)
  })

  it('reloads the scale saved in another workspace', async () => {
    const appearance = useAppearance()
    vi.mocked(invoke).mockResolvedValueOnce({ scale: 80 })
    await appearance.load()
    expect(appearance.scale.value).toBe(80)
    vi.mocked(invoke).mockResolvedValueOnce({ scale: 120 })
    expect(await appearance.reload()).toBe(true)
    expect(appearance.scale.value).toBe(120)
    vi.mocked(invoke).mockClear()
    expect(await appearance.flush()).toBe(true)
    expect(invoke).not.toHaveBeenCalled()
  })

  it('accepts the five supported steps and rejects invalid values', async () => {
    const appearance = useAppearance()
    vi.mocked(invoke).mockResolvedValue(undefined)
    for (const scale of [80, 90, 100, 110, 120]) {
      expect(await appearance.setScale(scale)).toBe(true)
      expect(appearance.scale.value).toBe(scale)
      expect(invoke).toHaveBeenLastCalledWith('save_appearance_settings', { settings: { scale } })
    }
    vi.mocked(invoke).mockClear()
    for (const scale of [70, 85, 130, NaN, Infinity]) expect(await appearance.setScale(scale)).toBe(false)
    expect(appearance.scale.value).toBe(120)
    expect(invoke).not.toHaveBeenCalled()
  })

  it('starts a fresh workspace read and ignores the previous slow read', async () => {
    const first = deferred<{ scale: number }>(); const second = deferred<{ scale: number }>()
    vi.mocked(invoke).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)
    const appearance = useAppearance()
    const previous = appearance.load()
    const current = appearance.reload()
    expect(invoke).toHaveBeenCalledTimes(2)
    first.resolve({ scale: 80 })
    expect(await previous).toBe(false)
    expect(appearance.loading.value).toBe(true)
    expect(appearance.scale.value).toBe(100)
    second.resolve({ scale: 120 })
    expect(await current).toBe(true)
    expect(appearance.scale.value).toBe(120)
    expect(appearance.loading.value).toBe(false)
  })

  it('discards a previous workspace save and its queued retries', async () => {
    const oldWrite = deferred<void>()
    vi.mocked(invoke).mockReturnValueOnce(oldWrite.promise).mockResolvedValueOnce({ scale: 90 })
    const appearance = useAppearance()
    const previous = appearance.setScale(80)
    const queued = appearance.setScale(120)
    expect(await appearance.reload()).toBe(true)
    oldWrite.reject('old write failed')
    expect(await previous).toBe(false)
    expect(await queued).toBe(false)
    expect(appearance.scale.value).toBe(90)
    expect(appearance.saving.value).toBe(false)
    expect(appearance.error.value).toBe('')
    expect(await appearance.flush()).toBe(true)
    expect(invoke).toHaveBeenCalledTimes(2)
  })

  it('applies immediately and serializes rapid changes, keeping the latest value', async () => {
    const first = deferred<void>()
    const last = deferred<void>()
    vi.mocked(invoke).mockReturnValueOnce(first.promise).mockReturnValueOnce(last.promise)
    const appearance = useAppearance()
    const initialSave = appearance.setScale(80)
    const intermediateSave = appearance.setScale(90)
    const finalSave = appearance.setScale(120)
    expect(appearance.scale.value).toBe(120)
    expect(appearance.saving.value).toBe(true)
    expect(invoke).toHaveBeenCalledTimes(1)
    first.resolve()
    await Promise.resolve()
    await Promise.resolve()
    expect(invoke).toHaveBeenCalledTimes(2)
    expect(invoke).toHaveBeenLastCalledWith('save_appearance_settings', { settings: { scale: 120 } })
    last.resolve()
    expect(await Promise.all([initialSave, intermediateSave, finalSave])).toEqual([true, true, true])
    expect(appearance.saving.value).toBe(false)
  })

  it('persists a quick return to the previous saved value after an in-flight write', async () => {
    const first = deferred<void>()
    vi.mocked(invoke).mockResolvedValueOnce({ scale: 100 }).mockReturnValueOnce(first.promise).mockResolvedValue(undefined)
    const appearance = useAppearance()
    await appearance.load()
    const pending = appearance.setScale(110)
    const returned = appearance.setScale(100)
    first.resolve()
    expect(await pending).toBe(true)
    expect(await returned).toBe(true)
    expect(invoke).toHaveBeenLastCalledWith('save_appearance_settings', { settings: { scale: 100 } })
  })

  it('persists a choice made while the previous save promise is settling', async () => {
    const first = deferred<void>()
    vi.mocked(invoke).mockReturnValueOnce(first.promise).mockResolvedValueOnce(undefined)
    const appearance = useAppearance()
    const pending = appearance.setScale(80)
    first.resolve()
    await Promise.resolve()
    await Promise.resolve()
    const latest = appearance.setScale(120)
    await pending
    expect(await latest).toBe(true)
    expect(invoke).toHaveBeenLastCalledWith('save_appearance_settings', { settings: { scale: 120 } })
  })

  it('preserves the selected scale on failure and retries through flush', async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error('disk full')).mockResolvedValueOnce(undefined)
    const appearance = useAppearance()
    expect(await appearance.setScale(120)).toBe(false)
    expect(appearance.scale.value).toBe(120)
    expect(appearance.error.value).toContain('disk full')
    expect(appearance.saving.value).toBe(false)
    expect(await appearance.flush()).toBe(true)
    expect(appearance.error.value).toBe('')
    expect(invoke).toHaveBeenNthCalledWith(2, 'save_appearance_settings', { settings: { scale: 120 } })
  })

  it('saves the latest choice after an older pending save fails', async () => {
    const first = deferred<void>()
    vi.mocked(invoke).mockReturnValueOnce(first.promise).mockResolvedValueOnce(undefined)
    const appearance = useAppearance()
    const pending = appearance.setScale(80)
    appearance.setScale(120)
    first.reject(new Error('older write failed'))
    expect(await pending).toBe(true)
    expect(appearance.error.value).toBe('')
    expect(invoke).toHaveBeenLastCalledWith('save_appearance_settings', { settings: { scale: 120 } })
  })

  it('does not let a slow startup read replace a newer user selection', async () => {
    const read = deferred<{ scale: number }>()
    vi.mocked(invoke).mockReturnValueOnce(read.promise).mockResolvedValueOnce(undefined)
    const appearance = useAppearance()
    const loading = appearance.load()
    expect(appearance.loading.value).toBe(true)
    await appearance.setScale(110)
    read.resolve({ scale: 80 })
    expect(await loading).toBe(true)
    expect(appearance.scale.value).toBe(110)
    expect(await appearance.flush()).toBe(true)
    expect(invoke).toHaveBeenCalledTimes(2)
  })

  it('keeps defaults for invalid saved settings and exposes read errors', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ scale: 85 }).mockRejectedValueOnce(new Error('cannot read'))
    const appearance = useAppearance()
    expect(await appearance.load()).toBe(true)
    expect(appearance.scale.value).toBe(100)
    expect(await appearance.load()).toBe(false)
    expect(appearance.error.value).toContain('cannot read')
    expect(appearance.loading.value).toBe(false)
  })

  it('stores only appearance preferences under a separate browser-preview key', async () => {
    const storage = new Map<string, string>([[APPEARANCE_STORAGE_KEY, JSON.stringify({ scale: 90 })]])
    const getItem = vi.fn((key: string) => storage.get(key) ?? null)
    const setItem = vi.fn((key: string, value: string) => { storage.set(key, value) })
    vi.stubGlobal('localStorage', { getItem, setItem })
    vi.mocked(isTauri).mockReturnValue(false)
    const appearance = useAppearance()
    expect(await appearance.load()).toBe(true)
    expect(appearance.scale.value).toBe(90)
    expect(await appearance.setScale(120)).toBe(true)
    expect(getItem).toHaveBeenCalledExactlyOnceWith(APPEARANCE_STORAGE_KEY)
    expect(setItem).toHaveBeenCalledExactlyOnceWith(APPEARANCE_STORAGE_KEY, JSON.stringify({ scale: 120 }))
    expect(invoke).not.toHaveBeenCalled()
  })
})
