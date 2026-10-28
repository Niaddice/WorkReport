// @vitest-environment happy-dom
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { computed, nextTick, ref } from 'vue'
import { DOMWrapper, flushPromises, mount } from '@vue/test-utils'

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
import { clearMessages } from '../src/lib/messages'

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
      generateSummary: vi.fn(async () => {}),
      cancelSummary: vi.fn(),
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
    global: { provide: { [MODULE_CONTEXT as symbol]: value }, stubs: { teleport: component === ExcelModule } },
  })
}

function configuredExcelContext() {
  const value = context()
  value.excel.settings.value = {
    enabled: true,
    templatePath: '/tmp/workreport-inline/template.xlsx',
    prompt: '填充本周工作',
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
  return value
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

    // 未配置 AI：同一个生成窗口显示配置入口，不发出 AI 请求。
    await wrapper.find('.summary-run-button').trigger('click')
    const modal = new DOMWrapper(document.body)
    expect(modal.get('#summary-generation-dialog').attributes('open')).toBeDefined()
    expect(modal.get('.summary-generation-editor').text()).toContain('配置并选择默认 AI')
    await modal.get('.summary-generation-editor .text-action').trigger('click')
    expect(value.actions.openAiSettings).toHaveBeenCalledTimes(1)
    expect(value.actions.generateSummary).not.toHaveBeenCalled()
    expect(modal.get('#summary-generation-dialog').attributes('open')).toBeUndefined()

    value.ai.summaryResult.value = '已合并的总结'
    value.ai.summaryMode.value = 'direct'
    await nextTick()
    expect(wrapper.text()).toContain('已合并的总结')
    // 复制与总结按钮共用日期行，文本区使用下方全部宽度。
    expect(wrapper.find('.summary-copy-icon').exists()).toBe(true)
    expect(wrapper.find('.summary-result .summary-result-actions').exists()).toBe(false)

    wrapper.unmount()
  })

  it('uses one AI summary window for temporary prompt editing, progress and results', async () => {
    const value = configuredExcelContext()
    value.ai.settings.value.summaryPrompt = '设置中的总结规则'
    vi.mocked(value.actions.requestSummary).mockImplementation(async () => {
      value.ai.openSummary({ ai: '日报原文', direct: '合并原文' }, ['20261025'])
    })
    let complete!: () => void
    vi.mocked(value.actions.generateSummary).mockImplementation(async () => {
      value.ai.summaryMode.value = 'ai'
      value.ai.summaryGenerating.value = true
      value.ai.summaryResult.value = ''
      await new Promise<void>((resolve) => {
        complete = resolve
      })
      value.ai.summaryResult.value = '## 项目成果\n\n完成本次功能'
      value.ai.summaryGenerating.value = false
    })
    const opening = vi.spyOn(HTMLDialogElement.prototype, 'showModal')
    const wrapper = mountWithContext(SummaryModule, value)
    const body = new DOMWrapper(document.body)
    try {
      await flushPromises()
      await wrapper.get('.summary-run-button').trigger('click')
      const dialog = body.get('#summary-generation-dialog').element
      expect(body.get<HTMLTextAreaElement>('#summary-session-prompt').element.value).toBe('设置中的总结规则')
      expect(value.actions.generateSummary).not.toHaveBeenCalled()
      await body.get('#summary-session-prompt').setValue('仅本次按项目提炼成果')
      await body.get('.summary-start-generation').trigger('click')
      expect(value.actions.generateSummary).toHaveBeenCalledExactlyOnceWith('仅本次按项目提炼成果')
      expect(body.find('#summary-generation-dialog [role="progressbar"]').exists()).toBe(true)
      expect(value.ai.settings.value.summaryPrompt).toBe('设置中的总结规则')
      complete()
      await flushPromises()
      expect(body.get('#summary-generation-dialog').element).toBe(dialog)
      expect(body.get('.summary-generation-result').text()).toContain('完成本次功能')
      expect(body.get('.summary-session-prompt').text()).toContain('仅本次按项目提炼成果')
      await body.get('.summary-generation-actions md-filled-button').trigger('click')
      expect(value.actions.exportSummary).toHaveBeenCalledWith('md')
      expect(opening).toHaveBeenCalledTimes(1)
      await body.get('.summary-edit-generation').trigger('click')
      expect(body.get<HTMLTextAreaElement>('#summary-session-prompt').element.value).toBe('仅本次按项目提炼成果')
      await body.get('.summary-generation-actions md-text-button').trigger('click')
      await wrapper.get('.summary-run-button').trigger('click')
      expect(body.get<HTMLTextAreaElement>('#summary-session-prompt').element.value).toBe('设置中的总结规则')
      expect(value.actions.openAiSettings).not.toHaveBeenCalled()
    } finally {
      wrapper.unmount()
      opening.mockRestore()
    }
  })

  it('closes the inline export menu when opening the AI window and can cancel without generating', async () => {
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
    const body = new DOMWrapper(document.body)
    await flushPromises()
    await wrapper.get('.summary-export-icon').trigger('click')
    expect(wrapper.find('#summary-export-menu').exists()).toBe(true)
    await wrapper.get('.summary-run-button').trigger('click')
    expect(wrapper.find('#summary-export-menu').exists()).toBe(false)
    expect(body.get('#summary-generation-dialog').attributes('open')).toBeDefined()
    await body.get('#summary-generation-dialog').trigger('cancel')
    expect(body.get('#summary-generation-dialog').attributes('open')).toBeUndefined()
    expect(value.actions.generateSummary).not.toHaveBeenCalled()
    await wrapper.get('.summary-export-icon').trigger('click')
    await wrapper.findAll('#summary-export-menu [role="menuitem"]')[0]!.trigger('click')
    expect(value.actions.exportSummary).toHaveBeenCalledWith('md')
    expect(wrapper.find('#summary-export-menu').exists()).toBe(false)
    wrapper.unmount()
  })

  it('opens one window to edit the prompt, confirm generation, show progress and preview the result', async () => {
    const value = configuredExcelContext()
    let complete!: () => void
    vi.mocked(value.actions.prepareExcel).mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          complete = resolve
        }),
    )
    const opening = vi.spyOn(HTMLDialogElement.prototype, 'showModal')
    const mounted = mount(ExcelModule, {
      attachTo: document.body,
      global: { provide: { [MODULE_CONTEXT as symbol]: value } },
    })
    const wrapper = new DOMWrapper(document.body)
    try {
      await wrapper.find('.excel-generate-button').trigger('click')
      expect(value.actions.prepareExcel).not.toHaveBeenCalled()
      expect(wrapper.find('.excel-confirm-popover').exists()).toBe(false)
      expect(wrapper.get('.excel-session-overview').text()).toContain('默认 AI')
      const dialog = wrapper.get('.excel-preview-dialog').element
      expect(wrapper.get<HTMLTextAreaElement>('#excel-session-prompt').element.value).toBe('填充本周工作')
      await wrapper.get('#excel-session-prompt').setValue('本次按项目分类，突出交付成果')
      expect(value.excel.settings.value.prompt).toBe('填充本周工作')
      await wrapper.get('.excel-start-generation').trigger('click')
      expect(value.actions.prepareExcel).toHaveBeenCalledExactlyOnceWith(['20261025'], '本次按项目分类，突出交付成果')
      expect(wrapper.get('.excel-preview-dialog').element).toBe(dialog)
      expect(wrapper.get('.excel-preview-dialog').attributes('open')).toBeDefined()
      expect(wrapper.find('.excel-preview-dialog [role="progressbar"]').exists()).toBe(true)
      expect(wrapper.get('.excel-preview-dialog').text()).toContain('AI 正在处理所选日报')
      expect(wrapper.find('.excel-exports').exists()).toBe(true)
      expect(wrapper.findAll('.excel-exports-header .excel-module-actions button')).toHaveLength(3)

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
      complete()
      await flushPromises()
      expect(wrapper.get('.excel-preview-dialog').attributes('open')).toBeDefined()
      expect(wrapper.find('.excel-preview-dialog.is-inline').exists()).toBe(false)
      expect(wrapper.find('.excel-preview-dialog [role="progressbar"]').exists()).toBe(false)
      expect(wrapper.get('.excel-preview-dialog').text()).toContain('确认输出')
      expect(wrapper.get('.excel-preview-dialog').element).toBe(dialog)
      expect(wrapper.get('.excel-session-prompt').text()).toContain('本次按项目分类，突出交付成果')
      expect(value.actions.confirmExcel).not.toHaveBeenCalled()
      await wrapper.get('.excel-preview-actions .text-action').trigger('click')
      expect(wrapper.get<HTMLTextAreaElement>('#excel-session-prompt').element.value).toBe(
        '本次按项目分类，突出交付成果',
      )
      expect(wrapper.find('.excel-preview-actions').exists()).toBe(false)
      await wrapper.get('#excel-session-prompt').setValue('本次补充项目风险')
      await wrapper.get('.excel-start-generation').trigger('click')
      expect(value.actions.prepareExcel).toHaveBeenLastCalledWith(['20261025'], '本次补充项目风险')
      value.excel.preview.value = { ...value.excel.preview.value!, id: 'p2' }
      complete()
      await flushPromises()
      expect(wrapper.get('.excel-preview-dialog').element).toBe(dialog)
      await wrapper.get('.excel-preview-dialog md-filled-button').trigger('click')
      expect(value.actions.confirmExcel).toHaveBeenCalledTimes(1)

      expect(opening).toHaveBeenCalledTimes(1)
    } finally {
      mounted.unmount()
      opening.mockRestore()
    }
  })

  it('filters by the exact selected day and by inclusive overlap for a range', async () => {
    const value = context()
    vi.spyOn(value.excel, 'loadExports').mockResolvedValue(true)
    const wrapper = mountWithContext(ExcelModule, value)
    await flushPromises()
    const filenames = [
      '周报_20261019-20261024_before.xlsx',
      '周报_20261025-20261101_current.xlsx',
      '报告_20261101-20261109_next.xlsx',
      '报告_20261001-20261231_quarter.xlsx',
    ]
    value.excel.exports.value = filenames.map((filename) => ({
      filename,
      path: `/reports/${filename}`,
      modified: 0,
      size: 100,
    }))
    await nextTick()
    const visible = () => wrapper.findAll('.excel-export-name').map((item) => item.text())
    expect(visible()).toEqual([filenames[1], filenames[3]])

    value.rangeDates.value = ['20261101', '20261031', '20261030']
    await nextTick()
    expect(visible()).toEqual([filenames[1], filenames[2], filenames[3]])

    value.rangeDates.value = []
    value.journal.selectedDate.value = '20261102'
    await nextTick()
    expect(visible()).toEqual([filenames[2], filenames[3]])

    value.journal.selectedDate.value = '20270101'
    await nextTick()
    expect(visible()).toEqual([])
    expect(wrapper.get('.excel-exports-state').text()).toContain('没有对应的报告')
    wrapper.unmount()
  })

  it('generates the full selected range across weeks after confirmation', async () => {
    const value = configuredExcelContext()
    const dates = [
      '20261227',
      '20261228',
      '20261229',
      '20261230',
      '20261231',
      '20270101',
      '20270102',
      '20270103',
      '20270104',
    ]
    value.rangeDates.value = dates
    const wrapper = mountWithContext(ExcelModule, value)
    await wrapper.get('.excel-generate-button').trigger('click')
    expect(value.actions.prepareExcel).not.toHaveBeenCalled()
    expect(wrapper.get('.excel-session-overview').text()).toContain('2026 年 12 月 27 日')
    await wrapper.get('.excel-start-generation').trigger('click')
    expect(value.actions.prepareExcel).toHaveBeenCalledExactlyOnceWith(dates, '填充本周工作')
    wrapper.unmount()
  })

  it('discards temporary edits when closing and restores saved settings on the next opening', async () => {
    const value = configuredExcelContext()
    const wrapper = mountWithContext(ExcelModule, value)
    await wrapper.get('.excel-generate-button').trigger('click')
    await wrapper.get('#excel-session-prompt').setValue('仅用于这一次')
    await wrapper.get('.excel-generation-actions md-text-button').trigger('click')
    expect(wrapper.get('.excel-preview-dialog').attributes('open')).toBeUndefined()
    expect(value.actions.prepareExcel).not.toHaveBeenCalled()
    expect(value.excel.settings.value.prompt).toBe('填充本周工作')
    await wrapper.get('.excel-generate-button').trigger('click')
    expect(wrapper.get<HTMLTextAreaElement>('#excel-session-prompt').element.value).toBe('填充本周工作')
    await wrapper.get('#excel-session-prompt').setValue('  ')
    expect(wrapper.get('.excel-start-generation').attributes('disabled')).toBeDefined()
    expect(value.actions.prepareExcel).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('cancels generation from the progress window before the AI finishes', async () => {
    const value = configuredExcelContext()
    let complete!: () => void
    vi.mocked(value.actions.prepareExcel).mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          complete = resolve
        }),
    )
    const wrapper = mountWithContext(ExcelModule, value)
    await wrapper.get('.excel-generate-button').trigger('click')
    await wrapper.get('.excel-start-generation').trigger('click')
    await wrapper.get('.excel-preview-dialog md-text-button').trigger('click')
    expect(value.actions.discardExcelPreview).toHaveBeenCalledExactlyOnceWith()
    expect(wrapper.get('.excel-preview-dialog').attributes('open')).toBeUndefined()
    complete()
    await flushPromises()
    expect(wrapper.get('.excel-preview-dialog').attributes('open')).toBeUndefined()
    wrapper.unmount()
  })

  it('keeps a generation failure visible and retries with the original dates', async () => {
    const value = configuredExcelContext()
    vi.mocked(value.actions.prepareExcel).mockRejectedValueOnce('AI 请求失败')
    const wrapper = mountWithContext(ExcelModule, value)
    await wrapper.get('.excel-generate-button').trigger('click')
    await wrapper.get('.excel-start-generation').trigger('click')
    await flushPromises()
    expect(wrapper.get('.excel-preview-dialog [role="alert"]').text()).toContain('AI 请求失败')
    value.journal.selectedDate.value = '20261026'
    await nextTick()
    await wrapper.get('.excel-start-generation').trigger('click')
    expect(value.actions.prepareExcel).toHaveBeenLastCalledWith(['20261025'], '填充本周工作')
    wrapper.unmount()
  })

  it('guides to Excel settings when the module is not configured', async () => {
    const value = context()
    const wrapper = mountWithContext(ExcelModule, value)
    await wrapper.find('.excel-generate-button').trigger('click')
    expect(value.actions.prepareExcel).not.toHaveBeenCalled()
    expect(wrapper.get('.excel-preview-dialog').text()).toContain('请先')
    await wrapper.get('.excel-generation-editor .text-action').trigger('click')
    expect(value.actions.openExcelSettings).toHaveBeenCalledTimes(1)
    wrapper.unmount()
  })
})
