import dayjs, { type Dayjs } from 'dayjs'
import 'dayjs/locale/zh-cn'

dayjs.locale('zh-cn')
export type ViewMode = 'week' | 'month' | 'year'
export const weekdays = ['一', '二', '三', '四', '五', '六', '日']
export const dateKey = (value: Dayjs | string = dayjs()) => dayjs(value).format('YYYYMMDD')

export function monthDays(anchor: Dayjs) {
  const first = anchor.startOf('month')
  const start = first.subtract((first.day() + 6) % 7, 'day')
  const total = Math.ceil((first.daysInMonth() + ((first.day() + 6) % 7)) / 7) * 7
  return Array.from({ length: total }, (_, index) => start.add(index, 'day'))
}

export function rangeDateKeys(start: string, end: string) {
  const first = dayjs(start)
  const last = dayjs(end)
  if (!first.isValid() || !last.isValid()) return []
  const from = first.isAfter(last) ? last : first
  const to = first.isAfter(last) ? first : last
  const dates: string[] = []
  for (let day = from; !day.isAfter(to) && dates.length < 370; day = day.add(1, 'day')) dates.push(dateKey(day))
  return dates
}

export function weekDays(anchor: Dayjs) {
  const start = anchor.subtract((anchor.day() + 6) % 7, 'day')
  return Array.from({ length: 7 }, (_, index) => start.add(index, 'day'))
}

export function movePeriod(anchor: Dayjs, view: ViewMode, step: number) {
  return anchor.add(step, view === 'week' ? 'week' : view === 'year' ? 'year' : 'month')
}

// WebKit may spell the day in Chinese for zh-CN, even with day: 'numeric'.
// Request Latin digits explicitly, and localize the resulting day ourselves.
const lunarFormatter = new Intl.DateTimeFormat('en-US-u-ca-chinese', {
  day: 'numeric',
  month: 'numeric',
  numberingSystem: 'latn',
})
export function lunarDay(date: Dayjs) {
  const parts = lunarFormatter.formatToParts(date.toDate())
  const value = Number(parts.find((part) => part.type === 'day')?.value)
  if (!Number.isInteger(value) || value < 1 || value > 30) return ''
  if (value === 1) {
    const month = parts.find((part) => part.type === 'month')?.value ?? ''
    const monthNumber = Number.parseInt(month, 10)
    const name = ['正月', '二月', '三月', '四月', '五月', '六月', '七月', '八月', '九月', '十月', '冬月', '腊月'][
      monthNumber - 1
    ]
    return name ? `${month.includes('bis') ? '闰' : ''}${name}` : '初一'
  }
  const digits = ['一', '二', '三', '四', '五', '六', '七', '八', '九', '十']
  if (value <= 10) return `初${digits[value - 1]}`
  if (value < 20) return `十${digits[value - 11]}`
  if (value === 20) return '二十'
  if (value < 30) return `廿${digits[value - 21]}`
  return '三十'
}
