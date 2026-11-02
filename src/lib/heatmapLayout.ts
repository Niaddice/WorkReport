import dayjs from 'dayjs'
import { dateKey, rangeDateKeys, weekdays } from './calendar'

export interface HeatmapGrid {
  days: number
  start: string
  rowLabels: boolean
  columns: { key: string; label: string; description: string }[]
  rows: { key: string; label: string; days: (string | null)[] }[]
}

export function heatmapGrid(today: string, days: number): HeatmapGrid {
  const end = dayjs(today).startOf('day')
  const start = end.subtract(days - 1, 'day')
  const dates = rangeDateKeys(dateKey(start), dateKey(end))
  if (days <= 7) {
    return {
      days,
      start: dateKey(start),
      rowLabels: false,
      columns: dates.map((date) => ({
        key: date,
        label: weekdays[(dayjs(date).day() + 6) % 7],
        description: dayjs(date).format('M月D日 dddd'),
      })),
      rows: [{ key: today, label: `近 ${days} 天`, days: dates }],
    }
  }
  const first = start.subtract((start.day() + 6) % 7, 'day')
  const length = Math.ceil((end.diff(first, 'day') + 1) / 7)
  const weeks = Array.from({ length }, (_, column) => {
    const weekStart = first.add(column * 7, 'day')
    const values = Array.from({ length: 7 }, (_, row) => {
      const day = weekStart.add(row, 'day')
      return day.isBefore(start) || day.isAfter(end) ? null : dateKey(day)
    })
    const monthStart = values.find((date) => date && dayjs(date).date() === 1)
    return { key: dateKey(weekStart), days: values, label: monthStart ? dayjs(monthStart).format('M月') : '' }
  })
  if (days === 30) {
    return {
      days,
      start: dateKey(start),
      rowLabels: false,
      columns: weekdays.map((label) => ({ key: label, label, description: `星期${label}` })),
      rows: weeks.map((week) => ({ ...week, label: `${dayjs(week.key).format('M月D日')}开始的一周` })),
    }
  }
  return {
    days,
    start: dateKey(start),
    rowLabels: true,
    columns: weeks.map((week) => ({
      key: week.key,
      label: week.label,
      description: `${dayjs(week.key).format('M月D日')}开始的一周`,
    })),
    rows: weekdays.map((label, row) => ({ key: label, label, days: weeks.map((week) => week.days[row]) })),
  }
}
