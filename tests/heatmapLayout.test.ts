import { describe, expect, it } from 'vitest'
import { heatmapGrid } from '../src/lib/heatmapLayout'

describe('heatmap date windows', () => {
  it.each(['20261101', '20261102', '20280301'])(
    'keeps each window ending on %s with no future or duplicate dates',
    (today) => {
      for (const days of [365, 90, 30, 7, 1]) {
        const grid = heatmapGrid(today, days)
        const dates = grid.rows.flatMap((row) => row.days.filter((date): date is string => !!date))
        expect(dates).toHaveLength(days)
        expect(new Set(dates).size).toBe(days)
        const sorted = [...dates].sort()
        expect(sorted[0]).toBe(grid.start)
        expect(sorted.at(-1)).toBe(today)
        expect(grid.rows.every((row) => row.days.length === grid.columns.length)).toBe(true)
      }
    },
  )

  it('uses calendar rows for a month and a single rolling row for a week', () => {
    const month = heatmapGrid('20261102', 30)
    expect(month.columns.map((column) => column.label)).toEqual(['一', '二', '三', '四', '五', '六', '日'])
    expect(month.rows).toHaveLength(6)
    expect(month.rows.at(-1)?.days).toEqual(['20261102', null, null, null, null, null, null])
    const week = heatmapGrid('20261102', 7)
    expect(week.rows).toHaveLength(1)
    expect(week.columns.map((column) => column.label)).toEqual(['二', '三', '四', '五', '六', '日', '一'])
    expect(week.rows[0].days).toEqual([
      '20261027',
      '20261028',
      '20261029',
      '20261030',
      '20261031',
      '20261101',
      '20261102',
    ])
  })
})
