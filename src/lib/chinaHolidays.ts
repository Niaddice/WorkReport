import dayjs from 'dayjs'
import { computed, shallowRef } from 'vue'
import type { HolidayState } from './holidays'
import calendarFallback from '../../shared/calendar/holidays.json'

export type ChinaDayKind = 'holiday' | 'workday'

export interface ChinaDayMeta {
  name: string
  kind: ChinaDayKind
}

// 2026 年安排来源：国务院办公厅关于 2026 年部分节假日安排的通知
// https://www.gov.cn/zhengce/zhengceku/202511/content_7047091.htm
// 首次同步之前的离线安排；已同步年度始终以本地同步数据为准。
const fixedFestivals: Record<string, string> = calendarFallback.fixedFestivals
const holidayOverrides = calendarFallback.holidayOverrides as Record<string, ChinaDayMeta>

const syncedHolidayState = shallowRef<HolidayState>({ years: [], failures: [] })
const syncedCalendar = computed(() => {
  const years = new Set<number>()
  const dates = new Map<string, ChinaDayMeta>()
  // Newer notices take precedence for overlapping dates around New Year.
  for (const entry of [...syncedHolidayState.value.years].sort((a, b) => a.year - b.year)) {
    years.add(entry.year)
    for (const day of entry.days)
      dates.set(day.date, { name: day.isOffDay ? day.name : '补班', kind: day.isOffDay ? 'holiday' : 'workday' })
  }
  return { years, dates }
})

export function installHolidayData(state: HolidayState) {
  syncedHolidayState.value = state
}

export function getChinaDayMeta(date: string): ChinaDayMeta | null {
  const synced = syncedCalendar.value
  const current = synced.dates.get(date)
  if (current) return current
  // A synced schedule is authoritative. Do not resurrect removed adjustment
  // dates or assume additional days off from the built-in fallback calendar.
  if (synced.years.has(Number(date.slice(0, 4)))) return null
  const override = holidayOverrides[date]
  if (override) return override

  const fixedName = fixedFestivals[dayjs(date).format('MM-DD')]
  return fixedName ? { name: fixedName, kind: 'holiday' } : null
}

export function isWeekend(date: string) {
  const weekDay = dayjs(date).day()
  // 调休补班日即使是周六/周日，也不应使用周末样式。
  return (weekDay === 0 || weekDay === 6) && getChinaDayMeta(date)?.kind !== 'workday'
}
