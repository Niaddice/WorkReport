import dayjs from 'dayjs'
import { computed, shallowRef } from 'vue'
import type { HolidayState } from './holidays'

export type ChinaDayKind = 'holiday' | 'workday'

export interface ChinaDayMeta {
  name: string
  kind: ChinaDayKind
}

// 2026 年安排来源：国务院办公厅关于 2026 年部分节假日安排的通知
// https://www.gov.cn/zhengce/zhengceku/202511/content_7047091.htm
// 首次同步之前的离线安排；已同步年度始终以本地同步数据为准。
const fixedFestivals: Record<string, string> = {
  '01-01': '元旦',
  '05-01': '劳动节',
  '05-02': '劳动节',
  '10-01': '国庆节',
  '10-02': '国庆节',
  '10-03': '国庆节',
}

const holidayOverrides: Record<string, ChinaDayMeta> = {
  '2026-01-01': { name: '元旦', kind: 'holiday' },
  '2026-01-02': { name: '元旦假期', kind: 'holiday' },
  '2026-01-03': { name: '元旦假期', kind: 'holiday' },
  '2026-01-04': { name: '补班', kind: 'workday' },

  '2026-02-14': { name: '补班', kind: 'workday' },
  '2026-02-15': { name: '春节假期', kind: 'holiday' },
  '2026-02-16': { name: '除夕', kind: 'holiday' },
  '2026-02-17': { name: '春节', kind: 'holiday' },
  '2026-02-18': { name: '春节假期', kind: 'holiday' },
  '2026-02-19': { name: '春节假期', kind: 'holiday' },
  '2026-02-20': { name: '春节假期', kind: 'holiday' },
  '2026-02-21': { name: '春节假期', kind: 'holiday' },
  '2026-02-22': { name: '春节假期', kind: 'holiday' },
  '2026-02-23': { name: '春节假期', kind: 'holiday' },
  '2026-02-28': { name: '补班', kind: 'workday' },

  '2026-04-04': { name: '清明假期', kind: 'holiday' },
  '2026-04-05': { name: '清明节', kind: 'holiday' },
  '2026-04-06': { name: '清明假期', kind: 'holiday' },

  '2026-05-01': { name: '劳动节', kind: 'holiday' },
  '2026-05-02': { name: '劳动节', kind: 'holiday' },
  '2026-05-03': { name: '劳动节假期', kind: 'holiday' },
  '2026-05-04': { name: '劳动节假期', kind: 'holiday' },
  '2026-05-05': { name: '劳动节假期', kind: 'holiday' },
  '2026-05-09': { name: '补班', kind: 'workday' },

  '2026-06-19': { name: '端午节', kind: 'holiday' },
  '2026-06-20': { name: '端午假期', kind: 'holiday' },
  '2026-06-21': { name: '端午假期', kind: 'holiday' },

  '2026-09-20': { name: '补班', kind: 'workday' },
  '2026-09-25': { name: '中秋节', kind: 'holiday' },
  '2026-09-26': { name: '中秋假期', kind: 'holiday' },
  '2026-09-27': { name: '中秋假期', kind: 'holiday' },

  '2026-10-01': { name: '国庆节', kind: 'holiday' },
  '2026-10-02': { name: '国庆节', kind: 'holiday' },
  '2026-10-03': { name: '国庆节', kind: 'holiday' },
  '2026-10-04': { name: '国庆假期', kind: 'holiday' },
  '2026-10-05': { name: '国庆假期', kind: 'holiday' },
  '2026-10-06': { name: '国庆假期', kind: 'holiday' },
  '2026-10-07': { name: '国庆假期', kind: 'holiday' },
  '2026-10-10': { name: '补班', kind: 'workday' },
}

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
