import { describe, expect, it } from 'vitest'
import { ref } from 'vue'
import { emptyHolidayState, type HolidayState, type HolidayYear } from '../src/lib/holidays'
import { useHolidayReminder } from '../src/lib/useHolidayReminder'

function syncedYear(year: number, syncedAt = `${year}-01-01T00:00:00+08:00`): HolidayYear {
  return {
    year,
    days: [{ date: `${year}-01-01`, name: '元旦', isOffDay: true }],
    syncedAt,
    sourceUrl: `https://example.com/${year}.json`,
  }
}

function setup(date = '2027-01-01', initial = emptyHolidayState(), loaded = true) {
  const state = ref<HolidayState>(initial)
  const localDate = ref(date)
  const ready = ref(loaded)
  return { state, localDate, ready, ...useHolidayReminder(state, localDate, ready) }
}

describe('holiday synchronization reminder lifecycle', () => {
  it('waits for the local cache before reporting missing data', () => {
    const reminder = setup('2027-01-01', emptyHolidayState(), false)
    expect(reminder.visible.value).toBe(false)
    reminder.state.value = { years: [syncedYear(2027)], failures: [] }
    reminder.ready.value = true
    expect(reminder.visible.value).toBe(false)
  })

  it('prompts on January 1 after a missing or unreadable cache finishes loading', () => {
    const reminder = setup('2027-01-01', emptyHolidayState(), false)
    expect(reminder.visible.value).toBe(false)
    // A read failure also finishes readiness while retaining the empty state.
    reminder.ready.value = true
    expect(reminder.visible.value).toBe(true)
  })

  it('never prompts on any other date, including leap day and failed synchronization', () => {
    const reminder = setup('2028-01-01', {
      years: [],
      failures: [{ year: 2028, message: 'Network unavailable', attemptedAt: '2028-01-01T00:00:00+08:00' }],
    })
    expect(reminder.visible.value).toBe(true)
    const day = new Date('2028-01-02T00:00:00Z')
    let daysChecked = 0
    while (day.getUTCFullYear() === 2028) {
      reminder.localDate.value = day.toISOString().slice(0, 10)
      expect(reminder.visible.value, reminder.localDate.value).toBe(false)
      day.setUTCDate(day.getUTCDate() + 1)
      daysChecked++
    }
    expect(daysChecked).toBe(365)
  })

  it('uses the current local year rather than a previous successful year', () => {
    const reminder = setup('2027-01-01', { years: [syncedYear(2026)], failures: [] })
    expect(reminder.visible.value).toBe(true)
    reminder.state.value = { years: [syncedYear(2026), syncedYear(2027)], failures: [] }
    expect(reminder.visible.value).toBe(false)
  })

  it('reacts to the newest failed attempt and clears after a successful retry', () => {
    const reminder = setup('2027-01-01', { years: [syncedYear(2027)], failures: [] })
    expect(reminder.visible.value).toBe(false)
    reminder.state.value.failures.push({ year: 2027, message: 'Request timed out', attemptedAt: '2027-01-01T00:02:00+08:00' })
    expect(reminder.visible.value).toBe(true)
    reminder.state.value = { years: [syncedYear(2027, '2027-01-01T00:03:00+08:00')], failures: [] }
    expect(reminder.visible.value).toBe(false)
  })

  it('does not repeat a dismissed reminder after clock updates or cache reloads that day', () => {
    const reminder = setup()
    expect(reminder.visible.value).toBe(true)
    reminder.dismiss()
    expect(reminder.visible.value).toBe(false)
    reminder.localDate.value = '2027-01-01'
    reminder.state.value = { years: [], failures: [{ year: 2027, message: 'Still offline', attemptedAt: '2027-01-01T12:00:00+08:00' }] }
    reminder.ready.value = false
    reminder.ready.value = true
    expect(reminder.visible.value).toBe(false)
  })

  it('dismisses for this session only and remains eligible the following January 1', () => {
    const reminder = setup()
    reminder.dismiss()
    reminder.localDate.value = '2027-01-02'
    expect(reminder.visible.value).toBe(false)
    reminder.localDate.value = '2028-01-01'
    expect(reminder.visible.value).toBe(true)
    reminder.dismiss()
    expect(reminder.visible.value).toBe(false)
    expect(setup('2028-01-01').visible.value).toBe(true)
  })

  it('appears across New Year midnight and disappears on January 2 without a restart', () => {
    const reminder = setup('2026-12-31')
    expect(reminder.visible.value).toBe(false)
    reminder.localDate.value = '2027-01-01'
    expect(reminder.visible.value).toBe(true)
    reminder.localDate.value = '2027-01-02'
    expect(reminder.visible.value).toBe(false)
    reminder.state.value = {
      years: [], failures: [{ year: 2027, message: 'Late failure', attemptedAt: '2027-01-02T00:00:00+08:00' }],
    }
    expect(reminder.visible.value).toBe(false)
  })
})
