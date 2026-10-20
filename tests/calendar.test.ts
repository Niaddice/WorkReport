import { describe, expect, it } from 'vitest'
import dayjs from 'dayjs'
import { dateKey, lunarDay, monthDays, movePeriod, weekDays } from '../src/lib/calendar'
import { getChinaDayMeta, isWeekend } from '../src/lib/chinaHolidays'

describe('preserved calendar behavior', () => {
  it('starts on Monday and includes neighboring days in full weeks', () => {
    const days = monthDays(dayjs('2026-07-01'))
    expect(days).toHaveLength(35)
    expect(dateKey(days[0])).toBe('20260629')
    expect(dateKey(days.at(-1)!)).toBe('20260802')
    expect(days.filter((date) => date.month() === 6)).toHaveLength(31)
  })

  it('handles leap days, weeks spanning years, and month clamping', () => {
    expect(monthDays(dayjs('2028-02-01')).some((date) => dateKey(date) === '20280229')).toBe(true)
    expect(weekDays(dayjs('2027-01-01')).map(dateKey)).toEqual(['20261228', '20261229', '20261230', '20261231', '20270101', '20270102', '20270103'])
    expect(dateKey(movePeriod(dayjs('2026-01-31'), 'month', 1))).toBe('20260228')
  })

  it('keeps original holiday and compensatory workday metadata and lunar labels', () => {
    expect(getChinaDayMeta('2026-10-10')?.kind).toBe('workday')
    expect(getChinaDayMeta('2026-10-01')?.name).toBe('国庆节')
    expect(lunarDay(dayjs('2026-02-17'))).toMatch(/月/)
    expect(lunarDay(dayjs('2026-10-18'))).toBe('初九')
  })

  it('marks the Spring Festival holiday start and weekend makeup workdays', () => {
    expect(getChinaDayMeta('2026-02-15')).toEqual({ name: '春节假期', kind: 'holiday' })
    expect(getChinaDayMeta('2026-09-20')).toEqual({ name: '补班', kind: 'workday' })
    expect(getChinaDayMeta('2026-05-09')).toEqual({ name: '补班', kind: 'workday' })
    expect(getChinaDayMeta('2026-04-26')).toBeNull()
    expect(isWeekend('2026-09-20')).toBe(false)
    expect(isWeekend('2026-10-10')).toBe(false)
    expect(isWeekend('2026-10-11')).toBe(true)
    expect(isWeekend('2026-04-26')).toBe(true)
    expect(getChinaDayMeta('2027-01-01')?.name).toBe('元旦')
  })
})
