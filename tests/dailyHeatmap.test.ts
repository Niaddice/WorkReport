// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { computed, ref } from 'vue'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import dayjs from 'dayjs'
import DailyHeatmap from '../src/components/DailyHeatmap.vue'
import OverviewModule from '../src/components/modules/OverviewModule.vue'
import { api, type DailyActivity, type Entry } from '../src/lib/workspace'
import { MODULE_CONTEXT, type ModuleContext } from '../src/modules/context'

vi.mock('../src/lib/workspace', () => ({ api: { dailyActivity: vi.fn() } }))
const wrappers: VueWrapper[] = []
const entry = (date: string): Entry => ({ date, filename: `${date}.md`, format: 'md' })

beforeEach(() => {
  vi.mocked(api.dailyActivity).mockReset().mockResolvedValue([])
})
afterEach(() => {
  wrappers.splice(0).forEach((wrapper) => wrapper.unmount())
})

function heatmap(entries: Entry[] = [], today = '20261101') {
  const wrapper = mount(DailyHeatmap, {
    props: { today, entries, workspacePath: '/tmp/activity' },
  })
  wrappers.push(wrapper)
  return wrapper
}
const cell = (wrapper: VueWrapper, date: string) => wrapper.get(`[data-date="${date}"]`)

describe('daily report heatmap', () => {
  it('shows exactly 365 days in Monday-first weeks, including leap day and excluding future dates', () => {
    const wrapper = heatmap([], '20280301')
    const days = wrapper.findAll('.heatmap-table [data-date]')
    expect(days).toHaveLength(365)
    expect(cell(wrapper, '20270303').attributes('aria-label')).toContain('未记录')
    expect(cell(wrapper, '20280229').attributes('aria-label')).toContain('2028年2月29日')
    expect(cell(wrapper, '20280301').classes()).toContain('is-today')
    expect(wrapper.find('[data-date="20280302"]').exists()).toBe(false)
    expect(wrapper.find('[data-date="20270302"]').exists()).toBe(false)
    const rows = wrapper.findAll('tbody tr')
    expect(rows.map((row) => row.get('th').text())).toEqual(['一', '二', '三', '四', '五', '六', '日'])
    expect(rows[1]!.find('[data-date="20280229"]').exists()).toBe(true)
    expect(api.dailyActivity).not.toHaveBeenCalled()
  })

  it('uses full-report character counts for color levels and retains empty and unreadable recorded days', async () => {
    const activity = [
      { date: '20261025', wordCount: 0 },
      { date: '20261026', wordCount: 199 },
      { date: '20261027', wordCount: 200 },
      { date: '20261028', wordCount: 500 },
      { date: '20261029', wordCount: 1000 },
      { date: '20261030', wordCount: null },
    ]
    vi.mocked(api.dailyActivity).mockResolvedValue(activity)
    const entries = [...activity.map((day) => entry(day.date)), entry('20241101'), entry('20261102')]
    const wrapper = heatmap(entries)
    await flushPromises()

    expect(api.dailyActivity).toHaveBeenCalledWith(
      '/tmp/activity',
      activity.map((day) => day.date),
    )
    expect(wrapper.get('.heatmap-heading').text()).toContain('已记录 6 天')
    for (const [date, level] of [
      ['20261025', '1'],
      ['20261026', '1'],
      ['20261027', '2'],
      ['20261028', '3'],
      ['20261029', '4'],
      ['20261030', '1'],
    ]) {
      expect(cell(wrapper, date!).attributes('data-level')).toBe(level)
    }
    expect(cell(wrapper, '20261029').attributes('aria-label')).toContain('1,000 字')
    expect(cell(wrapper, '20261030').attributes('aria-label')).toContain('字数暂不可用')
    expect(cell(wrapper, '20261101').attributes('data-level')).toBe('0')
    expect(wrapper.get('.heatmap-scroller').attributes('tabindex')).toBe('0')
  })

  it('immediately shows the hovered date and dismisses the hint on exit, Escape and scrolling', async () => {
    vi.mocked(api.dailyActivity).mockResolvedValue([{ date: '20261029', wordCount: 1000 }])
    const wrapper = heatmap([entry('20261029')])
    await flushPromises()

    await cell(wrapper, '20261029').trigger('mouseenter')
    await flushPromises()
    const tooltip = document.body.querySelector<HTMLElement>('.heatmap-tooltip')!
    expect(tooltip.getAttribute('role')).toBe('tooltip')
    expect(tooltip.textContent).toContain('2026年10月29日 星期四，已记录，1,000 字')
    expect(tooltip.style.visibility).toBe('visible')
    expect(cell(wrapper, '20261029').attributes('aria-describedby')).toBe(tooltip.id)
    expect(cell(wrapper, '20261029').attributes('title')).toBeUndefined()

    await cell(wrapper, '20261029').trigger('mouseleave')
    expect(document.body.querySelector('.heatmap-tooltip')).toBeNull()
    expect(cell(wrapper, '20261029').attributes('aria-describedby')).toBeUndefined()

    await cell(wrapper, '20261101').trigger('mouseenter')
    expect(document.body.querySelector('.heatmap-tooltip')?.textContent).toContain('2026年11月1日 星期日，今天，未记录')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()
    expect(document.body.querySelector('.heatmap-tooltip')).toBeNull()

    await cell(wrapper, '20261029').trigger('mouseenter')
    await wrapper.get('.heatmap-scroller').trigger('scroll')
    expect(document.body.querySelector('.heatmap-tooltip')).toBeNull()
  })

  it('refreshes after a report is saved and does not let a previous workspace overwrite current results', async () => {
    let finishOld!: (value: DailyActivity[]) => void
    vi.mocked(api.dailyActivity)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            finishOld = resolve
          }),
      )
      .mockResolvedValueOnce([{ date: '20261101', wordCount: 200 }])
      .mockResolvedValueOnce([{ date: '20261101', wordCount: 1500 }])
    const wrapper = heatmap([entry('20261101')])
    await wrapper.setProps({ workspacePath: '/tmp/new-workspace' })
    await flushPromises()
    expect(cell(wrapper, '20261101').attributes('data-level')).toBe('2')

    finishOld([{ date: '20261101', wordCount: 10 }])
    await flushPromises()
    expect(cell(wrapper, '20261101').attributes('data-level')).toBe('2')

    await wrapper.setProps({ entries: [entry('20261101')] })
    await flushPromises()
    expect(cell(wrapper, '20261101').attributes('data-level')).toBe('4')
    expect(api.dailyActivity).toHaveBeenLastCalledWith('/tmp/new-workspace', ['20261101'])
  })

  it('keeps recording dates visible if counts cannot be loaded and updates the window at midnight', async () => {
    vi.mocked(api.dailyActivity).mockRejectedValue(new Error('统计不可用'))
    const wrapper = heatmap([entry('20261101')])
    await flushPromises()
    expect(wrapper.get('[role="status"]').text()).toContain('仍显示记录日期')
    expect(cell(wrapper, '20261101').attributes('aria-label')).toContain('已记录，字数暂不可用')
    await wrapper.setProps({ today: '20261102' })
    await flushPromises()
    expect(wrapper.find('[data-date="20251102"]').exists()).toBe(false)
    expect(cell(wrapper, '20261102').classes()).toContain('is-today')
  })

  it('appears in data overview after its index is loaded and stays visible in an empty workspace', async () => {
    const journal = {
      workspace: ref({ path: '/tmp/activity', name: 'activity' }),
      entries: ref<Entry[]>([]),
      busy: ref(true),
    }
    const now = ref(dayjs('20261101'))
    const context = { journal, now, today: computed(() => now.value.format('YYYYMMDD')) } as unknown as ModuleContext
    const wrapper = mount(OverviewModule, { global: { provide: { [MODULE_CONTEXT as symbol]: context } } })
    wrappers.push(wrapper)
    expect(wrapper.find('.daily-heatmap').exists()).toBe(false)
    journal.busy.value = false
    await flushPromises()
    expect(wrapper.find('.daily-heatmap').exists()).toBe(true)
    expect(wrapper.get('.heatmap-heading').text()).toContain('已记录 0 天')
    expect(wrapper.get('.overview-empty').text()).toContain('第一篇日报')
    expect(wrapper.findAll('.heatmap-table [data-date]')).toHaveLength(365)
    expect(wrapper.findAll('button')).toHaveLength(0)
  })
})
