// @vitest-environment happy-dom
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { computed, nextTick, ref } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'

const { list, read } = vi.hoisted(() => ({ list: vi.fn(), read: vi.fn() }))
vi.mock('../src/lib/history', () => ({
  historyApi: { list, read },
}))

import HistoryModule from '../src/components/modules/HistoryModule.vue'
import SummaryModule from '../src/components/modules/SummaryModule.vue'
import ExcelModule from '../src/components/modules/ExcelModule.vue'
import { MODULE_CONTEXT, type ModuleContext } from '../src/modules/context'
import { useAi } from '../src/lib/ai'
import { useExcel } from '../src/lib/useExcel'
import { useSummaryOutput } from '../src/lib/useSummaryOutput'
import { clearMessages, messages } from '../src/lib/messages'

beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function showModal(this: HTMLDialogElement) {
    this.setAttribute('open', '')
  }
  HTMLDialogElement.prototype.close = function close(this: HTMLDialogElement) {
    this.removeAttribute('open')
  }
})

beforeEach(() => {
  clearMessages()
  list.mockReset()
  read.mockReset()
})

function context(overrides: Record<string, unknown> = {}) {
  const journal = {
    selectedDate: ref('20261025'),
    workspace: ref({ path: '/tmp/workreport-inline', name: 'inline' }),
  }
  const value = {
    journal,
    ai: useAi(),
    excel: useExcel(),
    summaryOutput: useSummaryOutput(),
    summarySelectionDates: ref<string[]>([]),
    excelBlockedReason: computed(() => ''),
    disabled: computed(() => false),
    rangeDates: ref<string[]>([]),
    rangeLoading: ref(false),
    actions: {
      requestSummary: vi.fn(async () => {}),
      prepareExcel: vi.fn(async () => {}),
      openAiSettings: vi.fn(),
      generateSummary: vi.fn(),
      directSummary: vi.fn(),
      closeSummary: vi.fn(),
      exportSummary: vi.fn(),
      confirmExcel: vi.fn(),
      discardExcelPreview: vi.fn(),
      openExcelOutputDirectory: vi.fn(),
      openExcelSettings: vi.fn(),
    },
    ...overrides,
  }
  return value as unknown as ModuleContext
}

function mountWithContext(component: typeof HistoryModule, value: ModuleContext) {
  return mount(component, {
    global: { provide: { [MODULE_CONTEXT as symbol]: value } },
  })
}

describe('inline modules', () => {
  it('lists the selected day and shows a hover tooltip with version content', async () => {
    list.mockResolvedValue([
      { id: 'v2', date: '20261025', savedAt: Date.now(), format: 'md', bytes: 1024 },
      { id: 'v1', date: '20261025', savedAt: Date.now() - 1000, format: 'md', bytes: 800 },
    ])
    read.mockResolvedValue('# 旧版本内容')
    const wrapper = mountWithContext(HistoryModule, context())
    await flushPromises()

    expect(list).toHaveBeenCalledWith('20261025')
    expect(wrapper.findAll('.history-module-item')).toHaveLength(2)
    const item = wrapper.findAll('.history-module-item')[0]!
    await item.trigger('mouseenter', { clientX: 120, clientY: 180 })
    await flushPromises()

    expect(read).toHaveBeenCalledWith('20261025', 'v2')
    const tooltip = document.body.querySelector('.history-tooltip')
    expect(tooltip?.textContent).toContain('旧版本内容')
    expect((tooltip as HTMLElement).style.left).not.toBe('')

    await item.trigger('mouseleave')
    await nextTick()
    expect(document.body.querySelector('.history-tooltip')).toBeNull()
    wrapper.unmount()
  })

  it('groups history by day when a date range is selected', async () => {
    list.mockResolvedValue([
      { id: 'c', date: '20261025', savedAt: Date.now(), format: 'md', bytes: 300 },
      { id: 'b', date: '20261024', savedAt: Date.now() - 1000, format: 'txt', bytes: 200 },
      { id: 'a', date: '20261022', savedAt: Date.now() - 2000, format: 'md', bytes: 100 },
    ])
    read.mockResolvedValue('# 内容')
    const rangeDates = ref(['20261023', '20261024', '20261025'])
    const value = context({ rangeDates })
    const wrapper = mountWithContext(HistoryModule, value)
    await flushPromises()

    expect(list).toHaveBeenCalledWith()
    const groups = wrapper.findAll('.history-module-group')
    expect(groups).toHaveLength(3)
    expect(groups[0]!.find('.history-module-group-title').text()).toContain('2026 年 10 月 25 日')
    expect(groups[0]!.findAll('.history-module-item')).toHaveLength(1)
    expect(groups[1]!.find('.history-module-group-title').text()).toContain('2026 年 10 月 24 日')
    expect(groups[0]!.find('.history-module-group-count').text()).toBe('1 个版本')
    expect(groups[2]!.find('.history-module-group-title').text()).toContain('2026 年 10 月 23 日')
    expect(groups[2]!.find('.history-module-group-count').text()).toBe('0 个版本')
    expect(groups[2]!.find('.history-module-group-empty').text()).toBe('暂无历史版本')
    // 范围外的 10 月 22 日不会出现。
    expect(wrapper.text()).not.toContain('10 月 22 日')

    rangeDates.value = []
    value.journal.selectedDate.value = '20261024'
    await flushPromises()
    expect(list).toHaveBeenLastCalledWith('20261024')
    expect(wrapper.findAll('.history-module-group')).toHaveLength(1)
    expect(wrapper.find('.history-module-group-title').exists()).toBe(false)
    expect(wrapper.findAll('.history-module-item')).toHaveLength(1)
    wrapper.unmount()
  })

  it('waits for range preparation so a newly saved history version is included', async () => {
    list.mockResolvedValue([])
    const value = context()
    const wrapper = mountWithContext(HistoryModule, value)
    await flushPromises()
    list.mockClear()

    value.rangeDates.value = ['20261024', '20261025']
    value.rangeLoading.value = true
    await flushPromises()
    expect(list).not.toHaveBeenCalled()
    expect(wrapper.attributes('aria-busy')).toBe('true')

    list.mockResolvedValue([{ id: 'new', date: '20261025', savedAt: Date.now(), format: 'md', bytes: 300 }])
    value.rangeLoading.value = false
    await flushPromises()
    expect(list).toHaveBeenCalledExactlyOnceWith()
    expect(wrapper.findAll('.history-module-group')).toHaveLength(2)
    expect(wrapper.findAll('.history-module-item')).toHaveLength(1)
    expect(wrapper.find('.history-module-group-count').text()).toBe('1 个版本')
    wrapper.unmount()
  })

  it('automatically prepares the summary and shows the merged text inline', async () => {
    const value = context()
    vi.mocked(value.actions.requestSummary).mockImplementation(async () => {
      value.ai.summaryVisible.value = true
      value.ai.summaryDates.value = ['20261025']
      value.ai.summaryScopeDates.value = ['20261025']
    })
    const wrapper = mountWithContext(SummaryModule, value)
    await flushPromises()

    // 打开即显示总结面板，自动读取日报原文并直接合并。
    expect(value.actions.requestSummary).toHaveBeenCalledWith(['20261025'])
    expect(wrapper.find('.summary-module-panel .summary-dialog.is-inline').exists()).toBe(true)
    expect(wrapper.find('.ai-dialog-heading').exists()).toBe(false)
    expect(wrapper.find('.summary-header .ai-dialog-description').text()).toBe('2026 年 10 月 25 日 · 1 天有日报')
    expect(wrapper.find('.summary-run-button').text()).toContain('总结')
    expect(value.actions.directSummary).toHaveBeenCalledTimes(1)
    expect(value.actions.generateSummary).not.toHaveBeenCalled()

    // 未配置 AI：点击总结按钮只弹确认 popover，并引导前往配置。
    await wrapper.find('.summary-run-button').trigger('click')
    expect(wrapper.find('.summary-confirm-menu').exists()).toBe(true)
    expect(wrapper.find('.summary-confirm-copy').text()).toContain('尚未配置 AI 设置')
    await wrapper.find('.summary-confirm-primary').trigger('click')
    expect(value.actions.openAiSettings).toHaveBeenCalledTimes(1)
    expect(value.actions.generateSummary).not.toHaveBeenCalled()
    expect(wrapper.find('.summary-confirm-menu').exists()).toBe(false)

    value.ai.summaryResult.value = '已合并的总结'
    value.ai.summaryMode.value = 'direct'
    await nextTick()
    expect(wrapper.text()).toContain('已合并的总结')
    // 复制与总结按钮共用日期行，文本区使用下方全部宽度。
    expect(wrapper.find('.summary-copy-icon').exists()).toBe(true)
    expect(wrapper.find('.summary-result .summary-result-actions').exists()).toBe(false)

    wrapper.unmount()
  })

  it('asks for confirmation before generating an AI summary when configured', async () => {
    const value = context()
    value.ai.settings.value = {
      providers: [
        {
          id: 'p1',
          name: '默认 AI',
          kind: 'http',
          executable: '',
          model: 'test',
          baseUrl: 'https://api.example.com',
          apiKey: '',
        },
      ],
      defaultProviderId: 'p1',
      generationPreference: '',
      summaryPrompt: '',
    }
    vi.mocked(value.actions.requestSummary).mockImplementation(async () => {
      value.ai.summaryVisible.value = true
      value.ai.summaryDates.value = ['20261025']
      value.ai.summaryScopeDates.value = ['20261025']
    })
    const wrapper = mountWithContext(SummaryModule, value)
    await flushPromises()

    await wrapper.find('.summary-run-button').trigger('click')
    expect(wrapper.find('.summary-confirm-menu').exists()).toBe(true)
    expect(wrapper.find('.summary-confirm-copy').text()).toContain('默认 AI')
    await wrapper.find('.summary-confirm-primary').trigger('click')
    expect(value.actions.generateSummary).toHaveBeenCalledTimes(1)
    expect(value.actions.openAiSettings).not.toHaveBeenCalled()
    expect(wrapper.find('.summary-confirm-menu').exists()).toBe(false)
    wrapper.unmount()
  })

  it('keeps the export menu and summary confirmation exclusive and dismissible', async () => {
    const value = context()
    vi.mocked(value.actions.requestSummary).mockImplementation(async () => {
      value.ai.summaryVisible.value = true
      value.ai.summaryDates.value = ['20261025']
      value.ai.summaryScopeDates.value = ['20261025']
    })
    value.ai.summaryResult.value = '总结内容'
    value.ai.summaryMode.value = 'direct'
    const wrapper = mount(SummaryModule, {
      attachTo: document.body,
      global: { provide: { [MODULE_CONTEXT as symbol]: value } },
    })
    await flushPromises()

    await wrapper.find('.summary-export-icon').trigger('click')
    expect(wrapper.find('#summary-export-menu').exists()).toBe(true)
    await wrapper.find('.summary-run-button').trigger('click')
    expect(wrapper.find('#summary-export-menu').exists()).toBe(false)
    expect(wrapper.find('.summary-confirm-menu').exists()).toBe(true)
    await wrapper.find('.summary-ai-control').trigger('keydown', { key: 'Escape' })
    expect(wrapper.find('.summary-confirm-menu').exists()).toBe(false)
    expect(document.activeElement).toBe(wrapper.find('.summary-run-button').element)

    await wrapper.find('.summary-run-button').trigger('click')
    document.body.dispatchEvent(new Event('pointerdown', { bubbles: true }))
    await nextTick()
    expect(wrapper.find('.summary-confirm-menu').exists()).toBe(false)
    await wrapper.find('.summary-export-icon').trigger('click')
    await wrapper.findAll('#summary-export-menu [role="menuitem"]')[0]!.trigger('click')
    expect(value.actions.exportSummary).toHaveBeenCalledWith('md')
    expect(wrapper.find('#summary-export-menu').exists()).toBe(false)
    wrapper.unmount()
  })

  it('prepares the Excel preview and renders it inline', async () => {
    const value = context()
    value.excel.settings.value = {
      enabled: true,
      templatePath: '/tmp/workreport-inline/template.xlsx',
      prompt: '填充本周工作',
      outputDirectory: '/tmp/workreport-inline/excel',
    }
    value.ai.settings.value = {
      providers: [
        {
          id: 'p1',
          name: '默认 AI',
          kind: 'http',
          executable: '',
          model: 'test',
          baseUrl: 'https://api.example.com',
          apiKey: '',
        },
      ],
      defaultProviderId: 'p1',
      generationPreference: '',
      summaryPrompt: '',
    }
    const wrapper = mountWithContext(ExcelModule, value)
    await wrapper.find('.excel-generate-button').trigger('click')
    expect(value.actions.prepareExcel).toHaveBeenCalledTimes(1)
    expect(value.actions.prepareExcel).toHaveBeenCalledWith([
      '20261019',
      '20261020',
      '20261021',
      '20261022',
      '20261023',
      '20261024',
      '20261025',
    ])

    value.excel.preview.value = {
      id: 'p1',
      templateName: '周报.xlsx',
      outputDirectory: '/tmp/workreport-inline/excel',
      start: '20261019',
      end: '20261025',
      cells: [],
      styles: [],
      sheets: [],
    }
    await nextTick()
    expect(wrapper.find('.excel-module-panel .excel-preview-dialog.is-inline').exists()).toBe(true)

    wrapper.unmount()
  })

  it('guides to Excel settings when the module is not configured', async () => {
    const value = context()
    const wrapper = mountWithContext(ExcelModule, value)
    await wrapper.find('.excel-generate-button').trigger('click')
    expect(value.actions.prepareExcel).not.toHaveBeenCalled()
    expect(messages.value.at(-1)?.text).toContain('请先')
    expect(messages.value.at(-1)?.actions[0]?.label).toBe('前往 Excel 设置')
    messages.value.at(-1)?.actions[0]?.run()
    expect(value.actions.openExcelSettings).toHaveBeenCalledTimes(1)
    wrapper.unmount()
  })
})
