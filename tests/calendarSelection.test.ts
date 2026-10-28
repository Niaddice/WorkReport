// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'
import { mount, type VueWrapper } from '@vue/test-utils'
import dayjs from 'dayjs'
import CalendarPane from '../src/components/CalendarPane.vue'

const wrappers: VueWrapper[] = []

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date'] })
  vi.setSystemTime(new Date(2026, 9, 23, 12))
})
afterEach(() => {
  for (const wrapper of wrappers) wrapper.unmount()
  wrappers.length = 0
  vi.useRealTimers()
})

function calendar() {
  const wrapper = mount(CalendarPane, {
    props: { today: '20261023', selected: '20261023', entries: [], previews: {}, disabled: false },
  })
  wrappers.push(wrapper)
  return wrapper
}

function day(wrapper: VueWrapper, date: string) {
  return wrapper.get<HTMLButtonElement>(`button[aria-label^="${dayjs(date).format('YYYY年M月D日 ')}"]`)
}

async function drag(wrapper: VueWrapper, start: string, end: string) {
  await day(wrapper, start).trigger('pointerdown', { button: 0 })
  await day(wrapper, end).trigger('pointerenter')
  window.dispatchEvent(new PointerEvent('pointerup'))
  await nextTick()
  // A drag's trailing click must not immediately reopen the final date.
  await day(wrapper, end).trigger('click')
}

function lastRange(wrapper: VueWrapper) {
  return wrapper.emitted('range')?.at(-1)?.[0]
}

describe('calendar continuous range selection', () => {
  it('shows the calendar status and drag hint without a multiple-selection switch', () => {
    const wrapper = calendar()
    expect(wrapper.find('[role="switch"]').exists()).toBe(false)
    expect(wrapper.get('.calendar-footer').text()).not.toContain('多选')
    expect(wrapper.get('.calendar-footer').text()).toContain('点击打开 · 拖拽框选')
  })

  it.each([
    ['月', '20261021', '20261023'],
    ['月', '20261023', '20261021'],
    ['周', '20261021', '20261023'],
    ['周', '20261023', '20261021'],
    ['年', '20261021', '20261023'],
    ['年', '20261023', '20261021'],
  ])('keeps one inclusive range in %s view when dragging %s to %s', async (view, start, end) => {
    const wrapper = calendar()
    await wrapper.get(`.view-switch button:nth-child(${view === '周' ? 1 : view === '月' ? 2 : 3})`).trigger('click')
    await drag(wrapper, start, end)

    expect(lastRange(wrapper)).toEqual(['20261021', '20261022', '20261023'])
    expect(wrapper.findAll('.range-selected')).toHaveLength(3)
    expect(day(wrapper, '20261021').classes()).toContain('range-start')
    expect(day(wrapper, '20261023').classes()).toContain('range-end')
    expect(wrapper.get('.calendar-toolbar .calendar-selection-status').text()).toBe('已选择 3 天')
    expect(wrapper.emitted('select')).toBeUndefined()
  })

  it('replaces the previous range when dragging a separate range', async () => {
    const wrapper = calendar()
    await drag(wrapper, '20261021', '20261023')
    await drag(wrapper, '20261025', '20261026')

    expect(lastRange(wrapper)).toEqual(['20261025', '20261026'])
    expect(wrapper.findAll('.range-selected')).toHaveLength(2)
    expect(day(wrapper, '20261021').classes()).not.toContain('range-selected')
  })

  it('opens a single date and clears the range instead of toggling an additional date', async () => {
    const wrapper = calendar()
    await drag(wrapper, '20261021', '20261023')
    await day(wrapper, '20261026').trigger('click', { ctrlKey: true, metaKey: true })

    expect(lastRange(wrapper)).toEqual([])
    expect(wrapper.emitted('select')).toEqual([['20261026']])
    expect(wrapper.find('.range-selected').exists()).toBe(false)
  })

  it.each(['Escape', 'next', 'today'])('clears the current range on %s', async (action) => {
    const wrapper = calendar()
    await drag(wrapper, '20261021', '20261023')
    if (action === 'Escape') window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    else if (action === 'next') await wrapper.get('[aria-label="下一时段"]').trigger('click')
    else await wrapper.get('.today-button').trigger('click')
    await nextTick()

    expect(lastRange(wrapper)).toEqual([])
    expect(wrapper.find('.range-selected').exists()).toBe(false)
  })
})
