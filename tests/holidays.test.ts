import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { invoke, isTauri } from '@tauri-apps/api/core'
import { computed } from 'vue'
import { getChinaDayMeta, installHolidayData, isWeekend } from '../src/lib/chinaHolidays'
import {
  emptyHolidayState,
  HOLIDAY_STORAGE_KEY,
  parseHolidayState,
  shouldRemindHolidaySync,
  useHolidays,
  type HolidayState,
  type HolidayYear,
} from '../src/lib/holidays'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }))

function yearData(year = 2027, values: Partial<HolidayYear> = {}): HolidayYear {
  return {
    year,
    days: [{ date: `${year}-01-01`, name: '元旦', isOffDay: true }],
    syncedAt: `${year}-01-01T00:00:00+08:00`,
    sourceUrl: `https://raw.githubusercontent.com/NateScarlet/holiday-cn/master/${year}.json`,
    ...values,
  }
}

function stateWith(...years: HolidayYear[]): HolidayState {
  return { years, failures: [] }
}

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
  installHolidayData(emptyHolidayState())
})
afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
  installHolidayData(emptyHolidayState())
})

describe('manual holiday synchronization', () => {
  it('loads only the local cache and applies holidays reactively to the calendar', async () => {
    const holidays = useHolidays()
    const date = computed(() => getChinaDayMeta('2027-01-02'))
    expect(date.value).toBeNull()
    vi.mocked(invoke).mockResolvedValue(
      stateWith(
        yearData(2027, {
          days: [
            { date: '2027-01-02', name: '元旦假期', isOffDay: true },
            { date: '2027-01-03', name: '元旦', isOffDay: false },
          ],
        }),
      ),
    )
    expect(await holidays.load()).toBe(true)
    expect(invoke).toHaveBeenCalledExactlyOnceWith('load_holiday_data')
    expect(date.value).toEqual({ name: '元旦假期', kind: 'holiday' })
    expect(isWeekend('2027-01-03')).toBe(false)
    expect(getChinaDayMeta('2027-01-03')).toEqual({ name: '补班', kind: 'workday' })
  })

  it('downloads only after a manual request, deduplicates clicks, and installs the persisted result', async () => {
    const response = deferred<HolidayState>()
    vi.mocked(invoke).mockReturnValueOnce(response.promise)
    const holidays = useHolidays()
    expect(invoke).not.toHaveBeenCalled()
    const first = holidays.sync(2027)
    const repeated = holidays.sync(2027)
    expect(holidays.syncing.value).toBe(true)
    expect(invoke).toHaveBeenCalledExactlyOnceWith('sync_holiday_year', { year: 2027 })
    const next = stateWith(yearData())
    response.resolve(next)
    expect(await Promise.all([first, repeated])).toEqual([true, true])
    expect(holidays.state.value).toEqual(next)
    expect(holidays.syncing.value).toBe(false)
    expect(holidays.error.value).toBe('')
  })

  it('keeps working calendar data after failed sync and reloads the persisted failure before a later successful retry', async () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2027-01-01T01:00:00+08:00'))
    const cached = stateWith(yearData())
    const failed = {
      ...cached,
      failures: [{ year: 2027, message: 'offline', attemptedAt: '2027-01-01T01:00:00+08:00' }],
    }
    vi.mocked(invoke)
      .mockResolvedValueOnce(cached)
      .mockRejectedValueOnce('offline')
      .mockResolvedValueOnce(failed)
      .mockResolvedValueOnce({ ...cached, years: [yearData(2027, { syncedAt: '2027-01-01T01:01:00+08:00' })] })
    const holidays = useHolidays()
    expect(await holidays.load()).toBe(true)
    expect(await holidays.sync(2027)).toBe(false)
    expect(holidays.state.value.years).toEqual(cached.years)
    expect(holidays.state.value.failures).toEqual(failed.failures)
    expect(holidays.error.value).toContain('offline')
    expect(shouldRemindHolidaySync(holidays.state.value, '2027-01-01')).toBe(true)
    expect(getChinaDayMeta('2027-01-01')?.name).toBe('元旦')
    expect(await holidays.sync(2027)).toBe(true)
    expect(shouldRemindHolidaySync(holidays.state.value, '2027-01-01')).toBe(false)
    expect(holidays.state.value.failures).toEqual([])
  })

  it('retains current calendar data and newest failed attempt even when the failure cannot be persisted or read', async () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2027-01-01T02:00:00+08:00'))
    vi.mocked(invoke)
      .mockResolvedValueOnce(stateWith(yearData()))
      .mockRejectedValueOnce('disk full')
      .mockRejectedValueOnce('cannot read')
    const holidays = useHolidays()
    await holidays.load()
    expect(await holidays.sync(2027)).toBe(false)
    expect(holidays.state.value.years).toHaveLength(1)
    expect(holidays.state.value.failures).toEqual([
      { year: 2027, message: 'disk full', attemptedAt: '2026-12-31T18:00:00.000Z' },
    ])
    expect(shouldRemindHolidaySync(holidays.state.value, '2027-01-01')).toBe(true)
  })

  it('replaces an older retained failure if saving the latest failed attempt also failed', async () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2027-01-01T02:00:00+08:00'))
    const cached = {
      ...stateWith(yearData()),
      failures: [{ year: 2027, message: 'old failure', attemptedAt: '2026-12-31T23:00:00+08:00' }],
    }
    vi.mocked(invoke)
      .mockResolvedValueOnce(cached)
      .mockRejectedValueOnce('new failure; disk full')
      .mockResolvedValueOnce(cached)
    const holidays = useHolidays()
    await holidays.load()
    expect(shouldRemindHolidaySync(holidays.state.value, '2027-01-01')).toBe(false)
    expect(await holidays.sync(2027)).toBe(false)
    expect(holidays.state.value.failures).toEqual([
      { year: 2027, message: 'new failure; disk full', attemptedAt: '2026-12-31T18:00:00.000Z' },
    ])
    expect(shouldRemindHolidaySync(holidays.state.value, '2027-01-01')).toBe(true)
  })

  it('does not let a slow startup cache read overwrite a completed manual sync', async () => {
    const oldRead = deferred<HolidayState>()
    const synced = stateWith(yearData(2027, { days: [{ date: '2027-01-02', name: '新安排', isOffDay: true }] }))
    vi.mocked(invoke).mockReturnValueOnce(oldRead.promise).mockResolvedValueOnce(synced)
    const holidays = useHolidays()
    const initial = holidays.load()
    expect(await holidays.sync(2027)).toBe(true)
    oldRead.resolve(emptyHolidayState())
    expect(await initial).toBe(false)
    expect(holidays.state.value).toEqual(synced)
    expect(getChinaDayMeta('2027-01-02')?.name).toBe('新安排')
  })

  it('does not start another cache read during sync', async () => {
    const request = deferred<HolidayState>()
    vi.mocked(invoke).mockReturnValueOnce(request.promise)
    const holidays = useHolidays()
    const pending = holidays.sync(2027)
    const reloaded = holidays.load()
    expect(invoke).toHaveBeenCalledTimes(1)
    request.resolve(stateWith(yearData()))
    expect(await pending).toBe(true)
    expect(await reloaded).toBe(true)
  })

  it('reports unreadable caches and invalid years without downloading or writing defaults', async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ years: [{ year: 2027 }], failures: [] })
    const holidays = useHolidays()
    expect(await holidays.load()).toBe(false)
    expect(holidays.error.value).toContain('读取放假信息失败')
    expect(holidays.state.value).toEqual(emptyHolidayState())
    for (const year of [2006, 2101, NaN, 2026.5]) expect(await holidays.sync(year)).toBe(false)
    expect(invoke).toHaveBeenCalledExactlyOnceWith('load_holiday_data')
  })

  it('reads browser preview cache but explains that syncing requires the desktop app', async () => {
    const cached = stateWith(yearData())
    const getItem = vi.fn(() => JSON.stringify(cached))
    const fetch = vi.fn()
    vi.stubGlobal('localStorage', { getItem })
    vi.stubGlobal('fetch', fetch)
    vi.mocked(isTauri).mockReturnValue(false)
    const holidays = useHolidays()
    expect(await holidays.load()).toBe(true)
    expect(holidays.state.value).toEqual(cached)
    expect(getItem).toHaveBeenCalledExactlyOnceWith(HOLIDAY_STORAGE_KEY)
    expect(await holidays.sync(2027)).toBe(false)
    expect(holidays.error.value).toContain('桌面应用')
    expect(invoke).not.toHaveBeenCalled()
    expect(fetch).not.toHaveBeenCalled()
  })
})

describe('holiday calendar authority and cache validation', () => {
  it('keeps the built-in offline calendar until that year has been synced', () => {
    installHolidayData(stateWith(yearData(2027)))
    expect(getChinaDayMeta('2026-10-10')).toEqual({ name: '补班', kind: 'workday' })
    expect(getChinaDayMeta('2028-01-01')).toEqual({ name: '元旦', kind: 'holiday' })
  })

  it('replaces an entire synced year, removing stale fallback and previous sync dates', () => {
    installHolidayData(stateWith(yearData(2026, { days: [{ date: '2026-10-11', name: '新假期', isOffDay: true }] })))
    expect(getChinaDayMeta('2026-10-10')).toBeNull()
    expect(getChinaDayMeta('2026-10-01')).toBeNull()
    expect(getChinaDayMeta('2026-10-11')?.name).toBe('新假期')
    installHolidayData(stateWith(yearData(2026)))
    expect(getChinaDayMeta('2026-10-11')).toBeNull()
    expect(isWeekend('2026-10-10')).toBe(true)
  })

  it('gives the newer annual notice precedence for explicit dates in the preceding December', () => {
    const previous = yearData(2026, { days: [{ date: '2026-12-31', name: '旧安排', isOffDay: false }] })
    const next = yearData(2027, {
      days: [
        { date: '2027-01-01', name: '元旦', isOffDay: true },
        { date: '2026-12-31', name: '元旦假期', isOffDay: true },
      ],
    })
    installHolidayData(parseHolidayState(stateWith(next, previous)))
    expect(getChinaDayMeta('2026-12-31')).toEqual({ name: '元旦假期', kind: 'holiday' })
  })

  it('rejects incomplete, duplicate, impossible, or wrong-year date records', () => {
    const invalidDays: unknown[] = [
      [],
      [{ date: '2027-02-29', name: '假期', isOffDay: true }],
      [{ date: '2027-01-01', name: ' ', isOffDay: true }],
      [{ date: '2027-01-01', name: '元旦', isOffDay: 'true' }],
      [
        { date: '2027-01-01', name: '元旦', isOffDay: true },
        { date: '2027-01-01', name: '元旦', isOffDay: true },
      ],
      [{ date: '2026-11-30', name: '假期', isOffDay: true }],
      [{ date: '2026-12-31', name: '假期', isOffDay: true }],
    ]
    for (const days of invalidDays)
      expect(() => parseHolidayState({ years: [{ ...yearData(), days }], failures: [] })).toThrow()
    expect(() => parseHolidayState(stateWith(yearData(), yearData()))).toThrow()
    expect(() => parseHolidayState(stateWith(yearData(2027, { syncedAt: 'yesterday' })))).toThrow()
    expect(() => parseHolidayState(stateWith(yearData(2027, { sourceUrl: 'javascript:alert(1)' })))).toThrow()
  })
})

describe('January 1 holiday reminder eligibility', () => {
  it('prompts only on January 1 when the exact current year has not been synced', () => {
    const cached = stateWith(yearData(2026))
    expect(shouldRemindHolidaySync(cached, '2026-12-31')).toBe(false)
    expect(shouldRemindHolidaySync(cached, '2027-01-01')).toBe(true)
    expect(shouldRemindHolidaySync(cached, '2027-01-02')).toBe(false)
    expect(shouldRemindHolidaySync(cached, '2027-10-23')).toBe(false)
    expect(shouldRemindHolidaySync(stateWith(yearData()), '2027-01-01')).toBe(false)
    expect(shouldRemindHolidaySync(cached, '20270101')).toBe(false)
  })

  it('requires a later failed attempt for the same year and clears after successful retry', () => {
    const cached = stateWith(yearData())
    cached.failures.push({ year: 2027, message: 'failed', attemptedAt: '2027-01-01T00:30:00+08:00' })
    expect(shouldRemindHolidaySync(cached, '2027-01-01')).toBe(true)
    expect(shouldRemindHolidaySync(cached, '2027-01-02')).toBe(false)
    cached.years[0]!.syncedAt = '2027-01-01T01:00:00+08:00'
    expect(shouldRemindHolidaySync(cached, '2027-01-01')).toBe(false)
    cached.failures = [{ year: 2026, message: 'last year', attemptedAt: '2027-01-01T02:00:00+08:00' }]
    expect(shouldRemindHolidaySync(cached, '2027-01-01')).toBe(false)
  })
})
