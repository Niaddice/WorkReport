// @vitest-environment happy-dom
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import dayjs from 'dayjs'

const { nativeRuntime, ipcInvoke } = vi.hoisted(() => ({
  nativeRuntime: {
    enabled: false,
    listeners: new Map<string, (event: { payload: string }) => void>(),
  },
  ipcInvoke: vi.fn<(command: string, args?: Record<string, unknown>) => Promise<unknown>>(async () => {
    throw new Error('IPC 不可用')
  }),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: ipcInvoke,
  isTauri: () => nativeRuntime.enabled,
}))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ onCloseRequested: async () => () => {}, destroy: async () => {} }),
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: async (name: string, callback: (event: { payload: string }) => void) => {
    nativeRuntime.listeners.set(name, callback)
    return () => nativeRuntime.listeners.delete(name)
  },
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: async () => null }))
import App from '../src/App.vue'
import AppRail from '../src/components/AppRail.vue'
import CalendarPane from '../src/components/CalendarPane.vue'
import JournalEditor from '../src/components/JournalEditor.vue'
import ConfirmDialog from '../src/components/ConfirmDialog.vue'
import { installHolidayData } from '../src/lib/chinaHolidays'
import { emptyHolidayState } from '../src/lib/holidays'
import { DEFAULT_UI_PREFERENCES, normalizeUiPreferences } from '../src/lib/uiPreferences'
import SkinDialog from '../src/components/SkinDialog.vue'
import { DEFAULT_DATABASE_SETTINGS } from '../src/lib/database'

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
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockImplementation(function (this: HTMLElement) {
    return this.classList.contains('module-board') ? 1200 : 0
  })
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockImplementation(function (this: HTMLElement) {
    return this.classList.contains('module-board') ? 800 : 0
  })
})

afterEach(() => {
  installHolidayData(emptyHolidayState())
  nativeRuntime.enabled = false
  nativeRuntime.listeners.clear()
  ipcInvoke.mockReset()
  ipcInvoke.mockRejectedValue(new Error('IPC 不可用'))
})

describe('journal history routing', () => {
  async function mountHistoryApp() {
    nativeRuntime.enabled = true
    ipcInvoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      switch (command) {
        case 'recent_workspace':
          return '/tmp/workreport-history-routing'
        case 'open_workspace':
          return { path: '/tmp/workreport-history-routing', name: 'history-routing' }
        case 'list_entries':
        case 'list_previews':
        case 'list_entry_history':
          return []
        case 'load_holiday_data':
          return emptyHolidayState()
        case 'load_ai_settings':
          return { providers: [], defaultProviderId: null, generationPreference: '', summaryPrompt: '' }
        case 'load_appearance_settings':
          return { scale: 100 }
        case 'load_excel_settings':
          return { enabled: false, templatePath: '', prompt: '', outputDirectory: '' }
        case 'load_summary_settings':
          return { outputDirectory: '总结' }
        case 'load_database_settings':
          return { ...DEFAULT_DATABASE_SETTINGS }
        case 'load_ui_preferences':
          return {
            skin: 'default',
            layout: {
              type: 'split',
              direction: 'row',
              ratio: 0.5,
              first: { type: 'leaf', id: 'editor' },
              second: { type: 'leaf', id: 'summary' },
            },
          }
        case 'create_entry':
        case 'save_entry':
          return {
            date: args!.date,
            filename: `${args!.date}.md`,
            format: 'md',
            content: args!.content,
            revision: 'saved',
          }
        default:
          return null
      }
    })
    const wrapper = mount(App, { attachTo: document.body })
    await flushPromises()
    expect(wrapper.find('dialog.summary-dialog.is-inline[open]').exists()).toBe(true)
    const input = wrapper.get<HTMLTextAreaElement>('.markdown-editor')
    input.element.focus()
    await input.setValue('本次编辑')
    return { wrapper, input }
  }

  it('routes keyboard undo and redo while a summary module is open inline', async () => {
    const platform = vi.spyOn(navigator, 'platform', 'get').mockReturnValue('Win32')
    const { wrapper, input } = await mountHistoryApp()
    try {
      await input.trigger('keydown', { key: 'z', ctrlKey: true })
      expect(input.element.value).toBe('')
      await input.trigger('keydown', { key: 'z', ctrlKey: true, shiftKey: true })
      expect(input.element.value).toBe('本次编辑')
      await input.trigger('keydown', { key: 'z', ctrlKey: true })
      await input.trigger('keydown', { key: 'y', ctrlKey: true })
      expect(input.element.value).toBe('本次编辑')
    } finally {
      wrapper.unmount()
      platform.mockRestore()
    }
  })

  it('routes native macOS Edit menu actions while a summary module is open inline', async () => {
    const platform = vi.spyOn(navigator, 'platform', 'get').mockReturnValue('MacIntel')
    const { wrapper, input } = await mountHistoryApp()
    try {
      const historyAction = nativeRuntime.listeners.get('history-action')!
      expect(historyAction).toBeTypeOf('function')
      historyAction({ payload: 'undo' })
      await flushPromises()
      expect(input.element.value).toBe('')
      historyAction({ payload: 'redo' })
      await flushPromises()
      expect(input.element.value).toBe('本次编辑')
    } finally {
      wrapper.unmount()
      platform.mockRestore()
    }
  })

  it('still blocks journal history when a real modal is open', async () => {
    const platform = vi.spyOn(navigator, 'platform', 'get').mockReturnValue('Win32')
    const { wrapper, input } = await mountHistoryApp()
    try {
      const dialog = wrapper.get<HTMLDialogElement>('.layout-dialog')
      dialog.element.showModal()
      await input.trigger('keydown', { key: 'z', ctrlKey: true })
      expect(input.element.value).toBe('本次编辑')
      nativeRuntime.listeners.get('history-action')!({ payload: 'undo' })
      await flushPromises()
      expect(input.element.value).toBe('本次编辑')
      dialog.element.close()
      await input.trigger('keydown', { key: 'z', ctrlKey: true })
      expect(input.element.value).toBe('')
    } finally {
      wrapper.unmount()
      platform.mockRestore()
    }
  })
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
      '总结',
      '布局',
      '切换皮肤',
      '刷新',
      '设置',
    ])

    for (const label of ['日历', '数据概览', '布局', '切换皮肤', '刷新', '设置']) {
      await wrapper.get(`[aria-label="${label}"]`).trigger('click')
    }
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

  it('hides the copy icon while empty', () => {
    const wrapper = mountEditor('')

    expect(wrapper.find('.copy-icon').exists()).toBe(false)

    wrapper.unmount()
  })

  it('shows format, preview and copy icon buttons in the editor corner once there is content', () => {
    const wrapper = mountEditor('今天完成了接口联调')

    expect(wrapper.find('.editor-body .copy-icon').exists()).toBe(true)
    expect(wrapper.find('.copy-icon').attributes('aria-label')).toBe('复制日报')

    const tools = wrapper.find('.editor-body-tools')
    expect(tools.exists()).toBe(true)
    expect(tools.find('.format-toggle').text()).toBe('MD')
    expect(tools.find('.preview-toggle').exists()).toBe(true)
    expect(tools.find('.copy-icon').exists()).toBe(true)
    expect(wrapper.find('.editor-toolbar-actions').exists()).toBe(false)
    expect(wrapper.find('.ai-toggle').exists()).toBe(false)

    wrapper.unmount()
  })

  it('keeps plain-text mode without a preview toggle', () => {
    const wrapper = mountEditor('纯文本内容', 'txt')

    expect(wrapper.find('.preview-toggle').exists()).toBe(false)
    expect(wrapper.find('.format-toggle').text()).toBe('TXT')
    expect(wrapper.find('.copy-icon').exists()).toBe(true)

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
