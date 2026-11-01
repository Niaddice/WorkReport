import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { invoke, isTauri } from '@tauri-apps/api/core'
import { APPEARANCE_STORAGE_KEY, normalizeAppearanceSettings, useAppearance } from '../src/lib/useAppearance'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }))

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((done, fail) => {
    resolve = done
    reject = fail
  })
  return { promise, resolve, reject }
}

beforeEach(() => {
  vi.resetAllMocks()
  vi.mocked(isTauri).mockReturnValue(true)
})
afterEach(() => {
  vi.unstubAllGlobals()
})

describe('appearance settings', () => {
  it('defaults old settings to two seconds and adopts, reloads and resets saved gesture durations', async () => {
    const appearance = useAppearance()
    expect(appearance.railHoldDelayMs.value).toBe(2000)
    expect(appearance.railHoldDurationMs.value).toBe(2000)
    vi.mocked(invoke).mockResolvedValueOnce({ scale: 100, railHoldDelayMs: 700, railHoldDurationMs: 3800 })
    await appearance.load()
    expect(appearance.railHoldDelayMs.value).toBe(700)
    expect(appearance.railHoldDurationMs.value).toBe(3800)
    appearance.acceptSaved({ scale: 100, rail: 'right', railHoldDelayMs: 500, railHoldDurationMs: 10_000 })
    expect(appearance.railHoldDelayMs.value).toBe(500)
    expect(appearance.railHoldDurationMs.value).toBe(10_000)
    vi.mocked(invoke).mockResolvedValueOnce({ scale: 100 })
    await appearance.reload()
    expect(appearance.railHoldDelayMs.value).toBe(2000)
    expect(appearance.railHoldDurationMs.value).toBe(2000)
  })

  it.each([undefined, null, '2000', 0, 499, 501, 10_100, NaN, Infinity])(
    'falls back to safe gesture defaults for %s',
    (value) => {
      const settings = normalizeAppearanceSettings({ railHoldDelayMs: value, railHoldDurationMs: value })
      expect(settings.railHoldDelayMs).toBe(2000)
      expect(settings.railHoldDurationMs).toBe(2000)
    },
  )
  it('starts at 100% and reads saved settings without writing defaults', async () => {
    const appearance = useAppearance()
    expect(appearance.scale.value).toBe(100)
    expect(appearance.rail.value).toBe('right')
    expect(invoke).not.toHaveBeenCalled()
    vi.mocked(invoke).mockResolvedValue({ scale: 80 })
    expect(await appearance.load()).toBe(true)
    expect(appearance.scale.value).toBe(80)
    expect(invoke).toHaveBeenCalledExactlyOnceWith('load_appearance_settings')
    expect(appearance.loading.value).toBe(false)
  })

  it('keeps the rail on the right unless a valid side was saved', async () => {
    const appearance = useAppearance()
    vi.mocked(invoke).mockResolvedValueOnce({ scale: 100, rail: 'left' })
    await appearance.load()
    expect(appearance.rail.value).toBe('left')
    vi.mocked(invoke).mockResolvedValueOnce({ scale: 100, rail: 'top' })
    await appearance.reload()
    expect(appearance.rail.value).toBe('right')
    appearance.acceptSaved({ scale: 100, rail: 'left' })
    expect(appearance.rail.value).toBe('left')
  })

  it('reloads the scale saved in another workspace', async () => {
    const appearance = useAppearance()
    vi.mocked(invoke).mockResolvedValueOnce({ scale: 80 })
    await appearance.load()
    expect(appearance.scale.value).toBe(80)
    vi.mocked(invoke).mockResolvedValueOnce({ scale: 120 })
    expect(await appearance.reload()).toBe(true)
    expect(appearance.scale.value).toBe(120)
    expect(appearance.loading.value).toBe(false)
  })

  it('starts a fresh workspace read and ignores the previous slow read', async () => {
    const first = deferred<{ scale: number }>()
    const second = deferred<{ scale: number }>()
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

  it('adopts the values the unified save persisted and drops an in-flight read', async () => {
    const read = deferred<{ scale: number }>()
    vi.mocked(invoke).mockReturnValueOnce(read.promise)
    const appearance = useAppearance()
    const loading = appearance.load()
    expect(appearance.loading.value).toBe(true)
    appearance.acceptSaved({ scale: 110, rail: 'right' })
    expect(appearance.scale.value).toBe(110)
    read.resolve({ scale: 80 })
    expect(await loading).toBe(false)
    expect(appearance.scale.value).toBe(110)
    expect(appearance.loading.value).toBe(false)
    expect(appearance.error.value).toBe('')
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

  it('reads browser-preview preferences under a separate key without touching IPC', async () => {
    const storage = new Map<string, string>([
      [APPEARANCE_STORAGE_KEY, JSON.stringify({ scale: 90, railHoldDelayMs: 600, railHoldDurationMs: 900 })],
    ])
    const getItem = vi.fn((key: string) => storage.get(key) ?? null)
    vi.stubGlobal('localStorage', { getItem })
    vi.mocked(isTauri).mockReturnValue(false)
    const appearance = useAppearance()
    expect(await appearance.load()).toBe(true)
    expect(appearance.scale.value).toBe(90)
    expect(appearance.railHoldDelayMs.value).toBe(600)
    expect(appearance.railHoldDurationMs.value).toBe(900)
    expect(getItem).toHaveBeenCalledExactlyOnceWith(APPEARANCE_STORAGE_KEY)
    expect(invoke).not.toHaveBeenCalled()
  })
})
