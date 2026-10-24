import { invoke, isTauri } from '@tauri-apps/api/core'
import { ref } from 'vue'
import { installHolidayData } from './chinaHolidays'

export interface HolidayDay {
  date: string
  name: string
  isOffDay: boolean
}
export interface HolidayYear {
  year: number
  days: HolidayDay[]
  syncedAt: string
  sourceUrl: string
}
export interface HolidayFailure {
  year: number
  message: string
  attemptedAt: string
}
export interface HolidayState {
  years: HolidayYear[]
  failures: HolidayFailure[]
}

export const HOLIDAY_STORAGE_KEY = 'work-report.holidays-preview'

export function emptyHolidayState(): HolidayState {
  return { years: [], failures: [] }
}

function validYear(year: unknown): year is number {
  return typeof year === 'number' && Number.isInteger(year) && year >= 2007 && year <= 2100
}

function validDate(date: unknown): date is string {
  if (typeof date !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(date)) return false
  const parsed = new Date(`${date}T00:00:00Z`)
  return Number.isFinite(parsed.getTime()) && parsed.toISOString().slice(0, 10) === date
}

function validTimestamp(value: unknown): value is string {
  return (
    typeof value === 'string' &&
    /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/.test(value) &&
    validDate(value.slice(0, 10)) &&
    Number.isFinite(Date.parse(value))
  )
}

function record(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === 'object' && !Array.isArray(value)
}

/** Reject a damaged cache as a whole, so incomplete data cannot replace a calendar year. */
export function parseHolidayState(value: unknown): HolidayState {
  if (!record(value) || !Array.isArray(value.years) || !Array.isArray(value.failures))
    throw new Error('放假信息缓存格式无效。')
  const years = new Set<number>()
  const parsedYears = value.years.map((entry): HolidayYear => {
    if (
      !record(entry) ||
      !validYear(entry.year) ||
      years.has(entry.year) ||
      !Array.isArray(entry.days) ||
      !entry.days.length ||
      entry.days.length > 400 ||
      !validTimestamp(entry.syncedAt) ||
      typeof entry.sourceUrl !== 'string' ||
      ![
        `https://raw.githubusercontent.com/NateScarlet/holiday-cn/master/${entry.year}.json`,
        `https://cdn.jsdelivr.net/gh/NateScarlet/holiday-cn@master/${entry.year}.json`,
      ].includes(entry.sourceUrl)
    )
      throw new Error('放假信息年度数据无效。')
    years.add(entry.year)
    const year = entry.year
    const dates = new Set<string>()
    const days = entry.days.map((day): HolidayDay => {
      if (
        !record(day) ||
        !validDate(day.date) ||
        dates.has(day.date) ||
        typeof day.name !== 'string' ||
        !day.name.trim() ||
        [...day.name.trim()].length > 100 ||
        /\p{Cc}/u.test(day.name) ||
        typeof day.isOffDay !== 'boolean'
      )
        throw new Error('放假信息日期数据无效。')
      // The following year's New Year notice may include dates in this December.
      if (!day.date.startsWith(`${year}-`) && !day.date.startsWith(`${year - 1}-12-`))
        throw new Error('放假信息日期不属于对应年度。')
      dates.add(day.date)
      return { date: day.date, name: day.name.trim(), isOffDay: day.isOffDay }
    })
    if (!days.some((day) => day.date.startsWith(`${year}-`))) throw new Error('放假信息缺少对应年度的日期。')
    return { year, days, syncedAt: entry.syncedAt, sourceUrl: entry.sourceUrl }
  })
  const failedYears = new Set<number>()
  const failures = value.failures.map((entry): HolidayFailure => {
    if (
      !record(entry) ||
      !validYear(entry.year) ||
      failedYears.has(entry.year) ||
      typeof entry.message !== 'string' ||
      !entry.message.trim() ||
      [...entry.message].length > 500 ||
      !validTimestamp(entry.attemptedAt)
    )
      throw new Error('放假信息同步记录无效。')
    failedYears.add(entry.year)
    return { year: entry.year, message: entry.message, attemptedAt: entry.attemptedAt }
  })
  return { years: parsedYears, failures }
}

/** localDate must come from the app's local calendar, not a UTC date string. */
export function shouldRemindHolidaySync(state: HolidayState, localDate: string): boolean {
  if (!validDate(localDate) || !localDate.endsWith('-01-01')) return false
  const year = Number(localDate.slice(0, 4))
  const synced = state.years.find((entry) => entry.year === year)
  if (!synced) return true
  return state.failures.some(
    (entry) => entry.year === year && Date.parse(entry.attemptedAt) >= Date.parse(synced.syncedAt),
  )
}

async function readHolidayState(): Promise<HolidayState> {
  if (isTauri()) return parseHolidayState(await invoke<unknown>('load_holiday_data'))
  const saved = globalThis.localStorage.getItem(HOLIDAY_STORAGE_KEY)
  return saved ? parseHolidayState(JSON.parse(saved)) : emptyHolidayState()
}

/** App-wide public calendar metadata; independent from journal workspace settings. */
export function useHolidays() {
  const state = ref<HolidayState>(emptyHolidayState())
  const loading = ref(false)
  const syncing = ref(false)
  const error = ref('')
  let revision = 0
  let loadTask: Promise<boolean> | undefined
  let syncTask: Promise<boolean> | undefined

  function accept(next: HolidayState) {
    state.value = next
    installHolidayData(next)
  }

  function load(): Promise<boolean> {
    if (syncTask) return syncTask
    if (loadTask) return loadTask
    const currentRevision = revision
    loading.value = true
    if (!syncTask) error.value = ''
    loadTask = (async () => {
      try {
        const next = await readHolidayState()
        if (revision !== currentRevision) return false
        accept(next)
        return true
      } catch (cause) {
        if (revision === currentRevision) error.value = `读取放假信息失败：${String(cause)}`
        return false
      } finally {
        loading.value = false
        loadTask = undefined
      }
    })()
    return loadTask
  }

  function sync(year: number): Promise<boolean> {
    if (syncTask) return syncTask
    if (!validYear(year)) {
      error.value = '暂不支持同步该年度的放假信息。'
      return Promise.resolve(false)
    }
    if (!isTauri()) {
      error.value = '请在桌面应用中同步放假信息。'
      return Promise.resolve(false)
    }
    revision++
    const attemptedAt = new Date().toISOString()
    syncing.value = true
    error.value = ''
    syncTask = (async () => {
      try {
        const next = parseHolidayState(await invoke<unknown>('sync_holiday_year', { year }))
        if (!next.years.some((entry) => entry.year === year)) throw new Error('同步结果缺少所选年度的放假信息。')
        accept(next)
        return true
      } catch (cause) {
        // A failed download never removes the last working calendar. The backend
        // records the failed attempt locally, which also survives app restarts.
        try {
          accept(await readHolidayState())
        } catch {
          /* Keep the working in-memory calendar when the cache is unavailable. */
        }
        const message = String(cause)
        const persistedFailure = state.value.failures.find((entry) => entry.year === year)
        // Rust records second precision at the beginning of the attempt. Keep
        // that persisted record if it belongs to this request, otherwise expose
        // a fresh in-memory failure even when writing the cache also failed.
        if (
          !persistedFailure ||
          Date.parse(persistedFailure.attemptedAt) < Math.floor(Date.parse(attemptedAt) / 1000) * 1000
        ) {
          accept({
            ...state.value,
            failures: [...state.value.failures.filter((entry) => entry.year !== year), { year, message, attemptedAt }],
          })
        }
        error.value = `同步 ${year} 年放假信息失败：${message}`
        return false
      } finally {
        syncing.value = false
        syncTask = undefined
      }
    })()
    return syncTask
  }

  return { state, loading, syncing, error, load, sync }
}
