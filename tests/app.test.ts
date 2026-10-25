// @vitest-environment happy-dom
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import dayjs from 'dayjs'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => {
    throw new Error('IPC 不可用')
  }),
  isTauri: () => false,
}))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ onCloseRequested: async () => () => {}, destroy: async () => {} }),
}))
vi.mock('@tauri-apps/api/event', () => ({ listen: async () => () => {} }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: async () => null }))
import App from '../src/App.vue'
import AppRail from '../src/components/AppRail.vue'
import CalendarPane from '../src/components/CalendarPane.vue'
import JournalEditor from '../src/components/JournalEditor.vue'
import ConfirmDialog from '../src/components/ConfirmDialog.vue'
import AiPolishDialog from '../src/components/AiPolishDialog.vue'
import { installHolidayData } from '../src/lib/chinaHolidays'
import { emptyHolidayState } from '../src/lib/holidays'
import { DEFAULT_UI_PREFERENCES, normalizeUiPreferences } from '../src/lib/uiPreferences'
import SkinDialog from '../src/components/SkinDialog.vue'

beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function showModal(this: HTMLDialogElement) {
    this.setAttribute('open', '')
  }
  HTMLDialogElement.prototype.close = function close(this: HTMLDialogElement) {
    this.removeAttribute('open')
  }
  // AppSelect 用 ResizeObserver 跟随触发按钮定位，happy-dom 未实现。
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver
})

afterEach(() => {
  installHolidayData(emptyHolidayState())
})

describe('app shell', () => {
  it('renders the greeting toolbar and the workspace gate before a workspace is chosen', async () => {
    const wrapper = mount(App)
    await flushPromises()

    expect(wrapper.find('.app-toolbar').exists()).toBe(true)
    expect(wrapper.find('.home-greeting').text()).toContain('今天是')
    expect(wrapper.find('.workspace-gate').exists()).toBe(true)
    expect(wrapper.find('.home').exists()).toBe(false)

    wrapper.unmount()
  })

  it('reports that choosing a workspace needs the desktop app', async () => {
    const wrapper = mount(App)
    await flushPromises()

    await wrapper.find('.workspace-gate md-filled-button').trigger('click')
    await flushPromises()

    expect(wrapper.find('.app-message').text()).toContain('Tauri 桌面应用')

    wrapper.unmount()
  })

  it('moves the workspace actions out of the toolbar into the rail', async () => {
    const wrapper = mount(App)
    await flushPromises()

    const labels = wrapper.findAll('.app-toolbar button').map((button) => button.attributes('aria-label'))
    expect(labels).not.toContain('设置')
    expect(labels).not.toContain('刷新')
    // 没有工作空间时功能栏不出现。
    expect(wrapper.find('.app-rail').exists()).toBe(false)

    wrapper.unmount()
  })
})

describe('calendar month cells', () => {
  function mountCalendar() {
    return mount(CalendarPane, {
      props: { today: '20261024', selected: '20261024', entries: [], previews: {}, disabled: false },
    })
  }

  it('makes the adjacent-month cells inert in month view', async () => {
    const wrapper = mountCalendar()
    const cells = wrapper.findAll('.day-cell')
    const outside = cells.filter((cell) => cell.classes('outside'))

    expect(outside.length).toBeGreaterThan(0)
    for (const cell of outside) {
      expect(cell.attributes('disabled')).toBeDefined()
      expect(cell.attributes('aria-label')).toContain('非本月')
    }

    await outside[0]!.trigger('click')
    expect(wrapper.emitted('select')).toBeUndefined()

    wrapper.unmount()
  })

  it('keeps the days of the displayed month selectable', async () => {
    const wrapper = mountCalendar()
    const cell = wrapper.findAll('.day-cell').find((item) => !item.classes('outside'))

    expect(cell).toBeDefined()
    expect(cell!.attributes('disabled')).toBeUndefined()
    await cell!.trigger('click')
    expect(wrapper.emitted('select')).toHaveLength(1)
    expect(wrapper.emitted('select')![0]![0]).toMatch(/^\d{8}$/)

    wrapper.unmount()
  })

  it('places the lunar label on the same row as the day number', () => {
    const wrapper = mountCalendar()
    const cell = wrapper.findAll('.day-cell').find((item) => !item.classes('outside'))!

    expect(cell.find('.day-top .lunar-label').exists()).toBe(true)

    wrapper.unmount()
  })

  it('keeps a festival name on its own row while the lunar day stays inline', () => {
    const first = dayjs().startOf('month')
    installHolidayData({
      years: [
        {
          year: first.year(),
          days: [{ date: first.format('YYYY-MM-DD'), name: '测试节日', isOffDay: true }],
          syncedAt: '2026-01-01T00:00:00+08:00',
          sourceUrl: 'https://example.com/holiday.json',
        },
      ],
      failures: [],
    })

    const wrapper = mountCalendar()
    const cell = wrapper
      .findAll('.day-cell')
      .find((item) => item.attributes('aria-label')?.startsWith(first.format('YYYY年M月D日')))!

    const inline = cell.find('.day-top .lunar-label')
    expect(inline.exists()).toBe(true)
    expect(inline.text()).not.toBe('测试节日')

    const below = Array.from(cell.element.children).find((child) => child.classList.contains('lunar-label'))
    expect(below?.textContent).toBe('测试节日')

    wrapper.unmount()
  })
})

describe('function rail', () => {
  it('offers the workspace actions including the module layout entry', async () => {
    const wrapper = mount(AppRail, { props: { disabled: false, active: [] } })
    const buttons = wrapper.findAll('.rail-button')
    expect(buttons.map((button) => button.attributes('aria-label'))).toEqual([
      '日历',
      '日报编辑',
      '数据概览',
      '历史记录',
      'Excel 周报',
      'AI 总结',
      '布局',
      '切换皮肤',
      '刷新',
      '设置',
    ])

    await buttons[0]!.trigger('click')
    await buttons[2]!.trigger('click')
    await buttons[6]!.trigger('click')
    await buttons[7]!.trigger('click')
    await buttons[8]!.trigger('click')
    await buttons[9]!.trigger('click')
    expect(wrapper.emitted('module')).toEqual([['calendar'], ['overview']])
    expect(wrapper.emitted('layout')).toHaveLength(1)
    expect(wrapper.emitted('skin')).toHaveLength(1)
    expect(wrapper.emitted('refresh')).toHaveLength(1)
    expect(wrapper.emitted('settings')).toHaveLength(1)
    // 设置固定在侧边栏最下方。
    expect(wrapper.find('.rail-bottom .rail-button').attributes('aria-label')).toBe('设置')

    wrapper.unmount()
  })

  it('disables every action while the workspace is busy', () => {
    const wrapper = mount(AppRail, { props: { disabled: true, active: [] } })
    for (const button of wrapper.findAll('.rail-button')) {
      expect(button.attributes('disabled')).toBeDefined()
    }
    wrapper.unmount()
  })
})

describe('layout preferences', () => {
  it('normalizes the split layout and falls back for damaged values', () => {
    const tree = {
      type: 'split',
      direction: 'column',
      ratio: 0.4,
      first: { type: 'leaf', id: 'editor' },
      second: { type: 'leaf', id: 'calendar' },
    }
    expect(normalizeUiPreferences({ layout: tree, skin: 'default' })).toEqual({
      layout: tree,
      skin: 'default',
    })
    // 旧 schema 会迁移成树，不再以 columns/modules 出现在运行时模型中。
    const migrated = normalizeUiPreferences({
      columns: 1,
      modules: [{ id: 'editor' }, { id: 'overview' }, { id: 'calendar' }],
      skin: 'default',
    })
    expect(migrated.skin).toBe('default')
    expect(migrated.layout.type).toBe('split')
    expect(normalizeUiPreferences({ columns: 9, modules: 'broken', skin: 'neon' })).toEqual(DEFAULT_UI_PREFERENCES)
    expect(normalizeUiPreferences(null)).toEqual(DEFAULT_UI_PREFERENCES)
  })
})

describe('skin dialog', () => {
  it('shows the available skins and emits the chosen one', async () => {
    const wrapper = mount(SkinDialog, {
      props: { active: 'default' },
      global: { stubs: { teleport: true } },
      attachTo: document.body,
    })

    const cards = wrapper.findAll('.skin-card:not(.skin-card-muted)')
    expect(cards).toHaveLength(1)
    expect(cards[0]!.text()).toContain('默认')
    expect(cards[0]!.classes()).toContain('active')

    await cards[0]!.trigger('click')
    expect(wrapper.emitted('select')).toEqual([['default']])

    wrapper.unmount()
  })
})

describe('journal editor actions', () => {
  function mountEditor(content: string, format: 'md' | 'txt' = 'md') {
    return mount(JournalEditor, {
      props: {
        date: '20261024',
        document: { date: '20261024', filename: `20261024.${format}`, format, content, revision: '1' },
        isNew: true,
        content,
        dirty: false,
        saving: false,
        disabled: false,
        error: '',
        savedAt: null,
        canRestore: false,
        canUndo: false,
        canRedo: false,
        preview: false,
        storageMode: 'files' as const,
      },
    })
  }

  it('hides the undo and redo buttons', () => {
    const wrapper = mountEditor('')
    const labels = wrapper.findAll('button').map((button) => button.text())

    expect(labels).not.toContain('撤销')
    expect(labels).not.toContain('重做')
    expect(wrapper.find('.history-action').exists()).toBe(false)

    wrapper.unmount()
  })

  it('hides the copy icon while empty and disables AI', () => {
    const wrapper = mountEditor('')

    expect(wrapper.find('.copy-icon').exists()).toBe(false)
    const ai = wrapper.find('.ai-toggle')
    expect(ai.text()).toBe('AI')
    expect(ai.attributes('disabled')).toBeDefined()

    wrapper.unmount()
  })

  it('shows the copy icon and enables AI once there is content', () => {
    const wrapper = mountEditor('今天完成了接口联调')

    expect(wrapper.find('.editor-body .copy-icon').exists()).toBe(true)
    expect(wrapper.find('.editor-file-row .copy-icon').exists()).toBe(false)
    expect(wrapper.find('.copy-icon').attributes('aria-label')).toBe('复制日报')
    expect(wrapper.find('.ai-toggle').attributes('disabled')).toBeUndefined()

    const actions = wrapper.find('.editor-toolbar-actions')
    const [first, second] = Array.from(actions.element.children)
    expect(first.classList.contains('preview-toggle')).toBe(true)
    expect(second.classList.contains('ai-toggle')).toBe(true)

    wrapper.unmount()
  })

  it('keeps the AI button in plain-text mode without a preview toggle', () => {
    const wrapper = mountEditor('纯文本内容', 'txt')

    expect(wrapper.find('.preview-toggle').exists()).toBe(false)
    expect(wrapper.find('.ai-toggle').exists()).toBe(true)

    wrapper.unmount()
  })
})

describe('AI polish dialog', () => {
  function mountDialog(result: string) {
    return mount(AiPolishDialog, {
      props: {
        visible: true,
        generating: false,
        elapsedSeconds: 0,
        original: '原始内容',
        result,
        error: '',
        provider: '测试模型',
        providerKind: 'http',
      },
      global: { stubs: { teleport: true } },
      attachTo: document.body,
    })
  }

  it('waits for an explicit start before requesting', async () => {
    const wrapper = mountDialog('')
    const primary = wrapper.get('.ai-dialog-confirm-actions md-filled-button')

    expect(primary.text()).toBe('开始润色')
    await primary.trigger('click')
    expect(wrapper.emitted('start')).toHaveLength(1)
    expect(wrapper.emitted('apply')).toBeUndefined()

    wrapper.unmount()
  })

  it('switches the primary action to confirmation once a result exists', async () => {
    const wrapper = mountDialog('润色结果')
    const primary = wrapper.get('.ai-dialog-confirm-actions md-filled-button')

    expect(primary.text()).toBe('确认使用')
    await primary.trigger('click')
    expect(wrapper.emitted('apply')).toHaveLength(1)
    expect(wrapper.emitted('start')).toBeUndefined()

    wrapper.unmount()
  })
})

describe('confirm dialog', () => {
  it('opens a native modal and closes through the exposed handle', async () => {
    const wrapper = mount(ConfirmDialog, {
      props: {
        kind: 'format',
        targetFormat: 'txt',
        filename: '20261023.md',
        date: '20261023',
        storageMode: 'files',
        error: '',
        busy: false,
      },
      attachTo: document.body,
    })

    const dialog = wrapper.find('dialog')
    expect(dialog.attributes('open')).toBeUndefined()

    ;(wrapper.vm as unknown as { open: () => void }).open()
    await flushPromises()
    expect(dialog.attributes('open')).toBeDefined()
    expect(dialog.text()).toContain('20261023.txt')

    await (wrapper.vm as unknown as { close: () => Promise<boolean> }).close()
    expect(dialog.attributes('open')).toBeUndefined()

    wrapper.unmount()
  })
})
