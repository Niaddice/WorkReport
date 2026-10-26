// @vitest-environment happy-dom
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import dayjs from 'dayjs'

const { invoke, pickDirectory } = vi.hoisted(() => ({ invoke: vi.fn(), pickDirectory: vi.fn() }))

vi.mock('@tauri-apps/api/core', () => ({ invoke, isTauri: () => true }))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ onCloseRequested: async () => () => {}, destroy: async () => {} }),
}))
vi.mock('@tauri-apps/api/event', () => ({ listen: async () => () => {} }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: pickDirectory }))

import App from '../src/App.vue'
import { flattenLayout } from '../src/modules/layout'
import { DEFAULT_DATABASE_SETTINGS } from '../src/lib/database'
import type { WorkspaceSettings } from '../src/lib/settingsDraft'

const DEMO_WORKSPACE = '/tmp/workreport-module-demo'

const leaf = (id: string) => ({ type: 'leaf', id })
const initialLayout = {
  layout: {
    type: 'split',
    direction: 'row',
    ratio: 0.5,
    first: leaf('calendar'),
    second: leaf('editor'),
  },
  skin: 'default',
}

function commandResult(command: string): unknown {
  switch (command) {
    case 'recent_workspace':
      return DEMO_WORKSPACE
    case 'open_workspace':
      return { path: DEMO_WORKSPACE, name: 'workreport-module-demo' }
    case 'list_entries':
    case 'list_previews':
      return []
    case 'database_backup_warning':
      return null
    case 'load_holiday_data':
      return { years: [], failures: [] }
    case 'list_entry_history':
      return []
    case 'read_entry_history':
      return ''
    case 'load_ai_settings':
      return { providers: [], defaultProviderId: null, generationPreference: '', summaryPrompt: '' }
    case 'load_appearance_settings':
      return { scale: 100 }
    case 'load_excel_settings':
      return { enabled: false, templatePath: '', prompt: '', outputDirectory: '' }
    case 'list_excel_exports':
      return []
    case 'load_summary_settings':
      return { outputDirectory: '总结' }
    case 'load_database_settings':
      return { ...DEFAULT_DATABASE_SETTINGS }
    case 'load_ui_preferences':
      return initialLayout
    default:
      return undefined
  }
}

beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function showModal(this: HTMLDialogElement) {
    this.setAttribute('open', '')
  }
  HTMLDialogElement.prototype.close = function close(this: HTMLDialogElement) {
    this.removeAttribute('open')
  }
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

beforeEach(() => {
  invoke.mockReset()
  pickDirectory.mockReset()
  pickDirectory.mockResolvedValue(null)
  invoke.mockImplementation(async (command: string) => commandResult(command))
})

async function openDemoWorkspace() {
  const wrapper = mount(App, { attachTo: document.body })
  await flushPromises()
  return wrapper
}

function savedPreferences() {
  const calls = invoke.mock.calls.filter(([command]) => command === 'save_ui_preferences')
  expect(calls.length).toBeGreaterThan(0)
  // 每次布局保存都带工作空间路径，后端据此拒绝迟到的旧空间写入。
  expect((calls.at(-1)![1] as { workspacePath: string }).workspacePath).toBe(DEMO_WORKSPACE)
  return calls.at(-1)![1]!.preferences as {
    layout: Parameters<typeof flattenLayout>[0]
    skin: string
  }
}

function savedIds() {
  return flattenLayout(savedPreferences().layout).map((item) => item.id)
}

async function chooseAnotherWorkspace(wrapper: Awaited<ReturnType<typeof openDemoWorkspace>>, path: string | null) {
  await wrapper.find('.app-rail [aria-label="设置"]').trigger('click')
  await flushPromises()
  pickDirectory.mockResolvedValueOnce(path)
  await wrapper.find('#settings-panel-workspace .text-action').trigger('click')
  await flushPromises()
}

function mockWorkspaceSwitch() {
  invoke.mockImplementation(async (command: string, args?: { path: string; syncSettingsFrom?: string }) =>
    command === 'open_workspace' ? { path: args!.path, name: args!.path.split('/').at(-1) } : commandResult(command),
  )
}

describe('workspace settings sync confirmation', () => {
  const target = '/tmp/workreport-next'

  it('asks before switching and passes the source only after sync is confirmed', async () => {
    mockWorkspaceSwitch()
    const wrapper = await openDemoWorkspace()
    await chooseAnotherWorkspace(wrapper, target)
    expect((wrapper.find('.workspace-switch-dialog').element as HTMLDialogElement).open).toBe(true)
    expect(wrapper.find('.workspace-switch-path').text()).toBe(target)
    expect(invoke.mock.calls.filter(([command]) => command === 'open_workspace')).toHaveLength(1)
    expect(wrapper.find('.workspace-path-text').text()).toBe(DEMO_WORKSPACE)

    await wrapper.find('.workspace-switch-dialog md-filled-button').trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('open_workspace', { path: target, syncSettingsFrom: DEMO_WORKSPACE })
    expect(wrapper.find('.workspace-path-text').text()).toBe(target)
    expect((wrapper.find('.workspace-switch-dialog').element as HTMLDialogElement).open).toBe(false)
    wrapper.unmount()
  })

  it('switches without importing settings when only switching is selected', async () => {
    mockWorkspaceSwitch()
    const wrapper = await openDemoWorkspace()
    await chooseAnotherWorkspace(wrapper, target)
    await wrapper.findAll('.workspace-switch-dialog md-text-button')[1]!.trigger('click')
    await flushPromises()
    expect(invoke.mock.calls.filter(([command]) => command === 'open_workspace').at(-1)).toEqual([
      'open_workspace',
      { path: target },
    ])
    expect(wrapper.find('.workspace-path-text').text()).toBe(target)
    wrapper.unmount()
  })

  it('keeps the original workspace when the confirmation is cancelled', async () => {
    const wrapper = await openDemoWorkspace()
    await chooseAnotherWorkspace(wrapper, target)
    await wrapper.findAll('.workspace-switch-dialog md-text-button')[0]!.trigger('click')
    await flushPromises()
    expect(invoke.mock.calls.filter(([command]) => command === 'open_workspace')).toHaveLength(1)
    expect(wrapper.find('.workspace-path-text').text()).toBe(DEMO_WORKSPACE)
    expect((wrapper.find('.workspace-switch-dialog').element as HTMLDialogElement).open).toBe(false)
    wrapper.unmount()
  })

  it.each([null, DEMO_WORKSPACE])('does not prompt or reopen after choosing %s', async (path) => {
    const wrapper = await openDemoWorkspace()
    await chooseAnotherWorkspace(wrapper, path)
    expect(invoke.mock.calls.filter(([command]) => command === 'open_workspace')).toHaveLength(1)
    expect((wrapper.find('.workspace-switch-dialog').element as HTMLDialogElement).open).toBe(false)
    wrapper.unmount()
  })

  it('keeps a failed sync reviewable and allows switching without syncing instead', async () => {
    invoke.mockImplementation(async (command: string, args?: { path: string; syncSettingsFrom?: string }) => {
      if (command !== 'open_workspace') return commandResult(command)
      if (args?.syncSettingsFrom) throw new Error('目标设置无法写入')
      return { path: args!.path, name: args!.path.split('/').at(-1) }
    })
    const wrapper = await openDemoWorkspace()
    await chooseAnotherWorkspace(wrapper, target)
    await wrapper.find('.workspace-switch-dialog md-filled-button').trigger('click')
    await flushPromises()
    expect(wrapper.find('.workspace-path-text').text()).toBe(DEMO_WORKSPACE)
    expect(wrapper.find('.workspace-switch-dialog .dialog-error').text()).toContain('目标设置无法写入')
    expect((wrapper.find('.workspace-switch-dialog').element as HTMLDialogElement).open).toBe(true)
    await wrapper.findAll('.workspace-switch-dialog md-text-button')[1]!.trigger('click')
    await flushPromises()
    expect(wrapper.find('.workspace-path-text').text()).toBe(target)
    wrapper.unmount()
  })
})

describe('modular home integration', () => {
  it.each(['missing', 'blank'])('silently disables summary actions for %s journal content', async (kind) => {
    const date = dayjs().format('YYYYMMDD')
    invoke.mockImplementation(async (command: string) => {
      if (kind === 'blank' && command === 'list_entries') return [{ date, filename: `${date}.md`, format: 'md' }]
      if (command === 'read_entry')
        return { date, filename: `${date}.md`, format: 'md', content: ' \n\t ', revision: 'blank' }
      return commandResult(command)
    })
    const wrapper = await openDemoWorkspace()
    await wrapper.find('.app-rail [aria-label="总结"]').trigger('click')
    await flushPromises()

    const summary = wrapper.find('.summary-module')
    expect(summary.find('.summary-result').text()).toBe('')
    expect(summary.find('.summary-error').exists()).toBe(false)
    expect(wrapper.find('.app-message').exists()).toBe(false)
    const runButton = summary.find('.summary-run-button')
    expect((runButton.element as HTMLButtonElement).disabled).toBe(true)
    await runButton.trigger('click')
    await flushPromises()
    expect(summary.find('.summary-confirm-menu').exists()).toBe(false)
    expect(summary.find('.summary-result').text()).toBe('')
    expect(wrapper.find('.app-message').exists()).toBe(false)
    expect(invoke.mock.calls.some(([command]) => command === 'summarize_entries')).toBe(false)
    wrapper.unmount()
  })

  it('automatically restores the saved workspace without flashing the welcome screen', async () => {
    let resolveRecent!: (path: string) => void
    const recent = new Promise<string>((resolve) => {
      resolveRecent = resolve
    })
    invoke.mockImplementation(async (command: string) =>
      command === 'recent_workspace' ? recent : commandResult(command),
    )
    const wrapper = mount(App)
    expect(wrapper.find('.workspace-gate').exists()).toBe(false)
    expect(wrapper.find('.workspace-loading').exists()).toBe(true)
    expect(invoke.mock.calls.some(([command]) => command === 'open_workspace')).toBe(false)

    resolveRecent(DEMO_WORKSPACE)
    await flushPromises()
    expect(invoke.mock.calls.filter(([command]) => command === 'open_workspace')).toEqual([
      ['open_workspace', { path: DEMO_WORKSPACE }],
    ])
    expect(wrapper.find('.module-board').exists()).toBe(true)
    expect(wrapper.find('.workspace-gate').exists()).toBe(false)
    expect(wrapper.find('.workspace-loading').exists()).toBe(false)
    wrapper.unmount()
  })

  it('shows the workspace picker on the first launch', async () => {
    invoke.mockImplementation(async (command: string) =>
      command === 'recent_workspace' ? null : commandResult(command),
    )
    const wrapper = mount(App)
    await flushPromises()
    expect(wrapper.find('.workspace-gate').exists()).toBe(true)
    expect(wrapper.find('.workspace-loading').exists()).toBe(false)
    expect(invoke.mock.calls.some(([command]) => command === 'open_workspace')).toBe(false)
    wrapper.unmount()
  })

  it('returns to the workspace picker when the saved directory cannot be opened', async () => {
    invoke.mockImplementation(async (command: string) => {
      if (command === 'open_workspace') throw new Error('工作空间目录已不存在')
      return commandResult(command)
    })
    const wrapper = mount(App)
    await flushPromises()
    expect(wrapper.find('.workspace-gate').exists()).toBe(true)
    expect(wrapper.find('.workspace-loading').exists()).toBe(false)
    expect(wrapper.find('.app-message').text()).toContain('工作空间目录已不存在')
    expect(wrapper.find('.recent-workspace').attributes('disabled')).toBeUndefined()
    wrapper.unmount()
  })

  it('does not open a late workspace after the app has unmounted', async () => {
    let resolveRecent!: (path: string) => void
    const recent = new Promise<string>((resolve) => {
      resolveRecent = resolve
    })
    invoke.mockImplementation(async (command: string) =>
      command === 'recent_workspace' ? recent : commandResult(command),
    )
    const wrapper = mount(App)
    await flushPromises()
    wrapper.unmount()
    resolveRecent(DEMO_WORKSPACE)
    await flushPromises()
    expect(invoke.mock.calls.some(([command]) => command === 'open_workspace')).toBe(false)
  })

  it('opens a workspace with a non-default persisted module layout', async () => {
    const persisted = {
      // Rust 返回的新载荷同时含 layout 与旧 columns/modules 回填字段，layout 必须优先。
      columns: 2,
      modules: [{ id: 'calendar' }, { id: 'editor' }],
      layout: {
        type: 'split',
        direction: 'column',
        ratio: 0.5,
        first: leaf('editor'),
        second: leaf('calendar'),
      },
      skin: 'default',
    }
    invoke.mockImplementation(async (command: string) =>
      command === 'load_ui_preferences' ? persisted : commandResult(command),
    )
    const wrapper = await openDemoWorkspace()
    expect(wrapper.find('.module-board').exists()).toBe(true)
    expect(wrapper.findAll('.module-panel')).toHaveLength(2)
    // 恢复的是保存的单列顺序，而不是默认的 calendar/editor。
    const titles = wrapper.findAll('.module-panel-title').map((title) => title.text())
    expect(titles).toEqual(['日报编辑', '日历'])
    expect(wrapper.find('.module-gutter.is-column').exists()).toBe(true)
    expect(wrapper.find('.app-rail [aria-label="布局"]').exists()).toBe(true)
    wrapper.unmount()
  })

  it('adds history, AI summary and Excel modules from the right rail', async () => {
    const wrapper = await openDemoWorkspace()
    for (const label of ['历史记录', '总结', 'Excel 周报']) {
      await wrapper.find(`.app-rail [aria-label="${label}"]`).trigger('click')
      await flushPromises()
    }
    expect(wrapper.findAll('.module-panel')).toHaveLength(5)
    expect(savedIds()).toEqual(['calendar', 'editor', 'history', 'summary', 'excel'])
    expect(wrapper.find('.history-module').exists()).toBe(true)
    expect(wrapper.find('.summary-module').exists()).toBe(true)
    expect(wrapper.find('.excel-module').exists()).toBe(true)
    expect(wrapper.find('.app-rail [aria-label="总结"]').attributes('aria-pressed')).toBe('true')
    wrapper.unmount()
  })

  it('groups history for the dates selected by dragging in the calendar', async () => {
    const first = dayjs().startOf('month').date(5)
    const middle = first.add(1, 'day')
    const last = first.add(2, 'day')
    const versions = [
      { id: 'last-new', date: last.format('YYYYMMDD'), savedAt: Date.now(), format: 'md', bytes: 300 },
      { id: 'last-old', date: last.format('YYYYMMDD'), savedAt: Date.now() - 1000, format: 'txt', bytes: 200 },
      { id: 'first', date: first.format('YYYYMMDD'), savedAt: Date.now() - 2000, format: 'md', bytes: 100 },
      { id: 'outside', date: last.add(1, 'day').format('YYYYMMDD'), savedAt: Date.now(), format: 'md', bytes: 400 },
    ]
    invoke.mockImplementation(async (command: string, args?: { date?: string }) => {
      if (command === 'list_entry_history')
        return args?.date ? versions.filter((version) => version.date === args.date) : versions
      return commandResult(command)
    })
    const wrapper = await openDemoWorkspace()
    await wrapper.find('.app-rail [aria-label="历史记录"]').trigger('click')
    await flushPromises()

    const cells = wrapper.findAll('.calendar-panel .day-cell')
    const start = cells.find((cell) => cell.attributes('aria-label')?.startsWith(first.format('YYYY年M月D日 ')))!
    const end = cells.find((cell) => cell.attributes('aria-label')?.startsWith(last.format('YYYY年M月D日 ')))!
    await start.trigger('pointerdown', { button: 0 })
    await end.trigger('pointerenter')
    window.dispatchEvent(new Event('pointerup'))
    await flushPromises()

    const groups = wrapper.findAll('.history-module-group')
    expect(groups).toHaveLength(3)
    expect(groups.map((group) => group.find('.history-module-group-title > span').text())).toEqual(
      [last, middle, first].map((date) => date.format('YYYY 年 M 月 D 日 dddd')),
    )
    expect(groups[0]!.findAll('.history-module-item')).toHaveLength(2)
    expect(groups[1]!.find('.history-module-group-empty').exists()).toBe(true)
    expect(groups[2]!.findAll('.history-module-item')).toHaveLength(1)
    expect(wrapper.findAll('.history-module-item')).toHaveLength(3)

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()
    expect(wrapper.find('.history-module-group-title').exists()).toBe(false)
    expect(invoke.mock.calls.filter(([command]) => command === 'list_entry_history').at(-1)).toEqual([
      'list_entry_history',
      { date: dayjs().format('YYYYMMDD') },
    ])
    wrapper.unmount()
  })

  it.each(['direct', 'ai'])(
    'summarizes a partially populated date range through the %s module action',
    async (mode) => {
      const first = dayjs().startOf('month').date(5)
      const last = first.add(2, 'day')
      const outside = last.add(1, 'day')
      const documents = [
        { date: first.format('YYYYMMDD'), content: '完成选中任务 A' },
        { date: last.format('YYYYMMDD'), content: '完成选中任务 B' },
        { date: outside.format('YYYYMMDD'), content: '未选中内容，不应参与总结' },
      ].map((document) => ({ ...document, filename: `${document.date}.md`, format: 'md', revision: document.date }))
      invoke.mockImplementation(async (command: string, args?: { date?: string }) => {
        if (command === 'list_entries')
          return documents.map(({ date, filename, format }) => ({ date, filename, format }))
        if (command === 'read_entry') return documents.find((document) => document.date === args?.date)
        if (command === 'summarize_entries') return '# 框选 AI 总结结果'
        if (command === 'load_ai_settings' && mode === 'ai')
          return {
            providers: [
              {
                id: 'p1',
                name: '测试 AI',
                kind: 'http',
                executable: '',
                model: 'test',
                baseUrl: 'https://example.com/v1',
                apiKey: '',
              },
            ],
            defaultProviderId: 'p1',
            generationPreference: '',
            summaryPrompt: '',
          }
        return commandResult(command)
      })
      const wrapper = await openDemoWorkspace()
      await wrapper.find('.app-rail [aria-label="总结"]').trigger('click')
      await flushPromises()

      const cells = wrapper.findAll('.calendar-panel .day-cell')
      const start = cells.find((cell) => cell.attributes('aria-label')?.startsWith(first.format('YYYY年M月D日 ')))!
      const end = cells.find((cell) => cell.attributes('aria-label')?.startsWith(last.format('YYYY年M月D日 ')))!
      await start.trigger('pointerdown', { button: 0 })
      await end.trigger('pointerenter')
      window.dispatchEvent(new Event('pointerup'))
      await flushPromises()

      const range = wrapper.find('.range-overview')
      expect(range.findAll('button').map((button) => button.text())).toEqual(['预览'])
      expect(range.find('.editor-text-actions').exists()).toBe(false)
      const summary = wrapper.find('.summary-module')
      if (mode === 'ai') {
        const runButton = summary.find('.summary-run-button')
        expect((runButton.element as HTMLButtonElement).disabled).toBe(false)
        await runButton.trigger('click')
        await summary.find('.summary-confirm-primary').trigger('click')
      }
      await flushPromises()

      const requests = invoke.mock.calls.filter(([command]) => command === 'summarize_entries')
      if (mode === 'ai') {
        expect(requests).toHaveLength(1)
        expect(requests[0]![1].content).toContain('完成选中任务 A')
        expect(requests[0]![1].content).toContain('完成选中任务 B')
        expect(requests[0]![1].content).not.toContain('未选中内容')
        expect(summary.find('.summary-result').text()).toContain('框选 AI 总结结果')
      } else {
        expect(requests).toHaveLength(0)
        expect(summary.find('.summary-result').text()).toContain('完成选中任务 A')
        expect(summary.find('.summary-result').text()).toContain('完成选中任务 B')
        expect(summary.find('.summary-result').text()).not.toContain('未选中内容')
      }
      wrapper.unmount()
    },
  )

  it('keeps settings at the bottom of the rail and returns home on a second click without changes', async () => {
    const wrapper = await openDemoWorkspace()
    await wrapper.find('.app-rail [aria-label="设置"]').trigger('click')
    await flushPromises()

    expect(wrapper.find('.settings-viewport').exists()).toBe(true)
    expect(wrapper.find('.app-rail').exists()).toBe(true)
    const buttons = wrapper.findAll('.app-rail .rail-button')
    const last = buttons[buttons.length - 1]!
    expect(last.attributes('aria-label')).toBe('设置')
    expect(last.classes()).toContain('is-active')
    expect(last.attributes('title')).toBe('保存设置并返回首页')

    await last.trigger('click')
    await flushPromises()
    expect(last.classes()).not.toContain('is-active')
    expect(wrapper.find('.home').attributes('inert')).toBeUndefined()
    expect(wrapper.find('.settings-viewport').attributes('inert')).toBeDefined()
    expect(invoke.mock.calls.some(([command]) => command === 'save_workspace_settings')).toBe(false)
    expect((wrapper.find('.settings-leave-dialog').element as HTMLDialogElement).open).toBe(false)
    wrapper.unmount()
  })

  it('saves the settings draft once and waits for success before returning home from the rail', async () => {
    let finishSave!: () => void
    invoke.mockImplementation(async (command: string, args?: { settings: WorkspaceSettings }) => {
      if (command === 'save_workspace_settings') {
        return new Promise<WorkspaceSettings>((resolve) => {
          finishSave = () => resolve(args!.settings)
        })
      }
      return commandResult(command)
    })
    const wrapper = await openDemoWorkspace()
    const settings = wrapper.find('.app-rail [aria-label="设置"]')
    await settings.trigger('click')
    await flushPromises()
    await wrapper.find('[aria-controls="settings-panel-appearance"]').trigger('click')
    await wrapper.find('#settings-panel-appearance [aria-label="放大字体"]').trigger('click')

    await settings.trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('save_workspace_settings', {
      workspacePath: DEMO_WORKSPACE,
      settings: expect.objectContaining({ appearance: { scale: 110 } }),
    })
    expect(wrapper.find('.settings-viewport').attributes('inert')).toBeUndefined()
    expect((settings.element as HTMLButtonElement).disabled).toBe(true)
    expect((wrapper.find('.settings-leave-dialog').element as HTMLDialogElement).open).toBe(false)
    await settings.trigger('click')
    expect(invoke.mock.calls.filter(([command]) => command === 'save_workspace_settings')).toHaveLength(1)

    finishSave()
    await flushPromises()
    expect(wrapper.find('.home').attributes('inert')).toBeUndefined()
    expect(wrapper.find('.settings-viewport').attributes('inert')).toBeDefined()
    expect(settings.classes()).not.toContain('is-active')
    expect((settings.element as HTMLButtonElement).disabled).toBe(false)
    wrapper.unmount()
  })

  it('preserves the settings draft after a failed rail save and allows retrying', async () => {
    let failSave = true
    invoke.mockImplementation(async (command: string, args?: { settings: WorkspaceSettings }) => {
      if (command === 'save_workspace_settings') {
        if (failSave) throw new Error('写入失败')
        return args!.settings
      }
      return commandResult(command)
    })
    const wrapper = await openDemoWorkspace()
    const settings = wrapper.find('.app-rail [aria-label="设置"]')
    await settings.trigger('click')
    await flushPromises()
    await wrapper.find('[aria-controls="settings-panel-appearance"]').trigger('click')
    await wrapper.find('#settings-panel-appearance [aria-label="放大字体"]').trigger('click')

    await settings.trigger('click')
    await flushPromises()
    expect(wrapper.find('.settings-viewport').attributes('inert')).toBeUndefined()
    expect(wrapper.find('.appearance-scale-value').text()).toBe('110%')
    expect(wrapper.find('.settings-save-error').text()).toContain('保存设置失败')
    expect(settings.classes()).toContain('is-active')
    expect((settings.element as HTMLButtonElement).disabled).toBe(false)

    failSave = false
    await settings.trigger('click')
    await flushPromises()
    expect(invoke.mock.calls.filter(([command]) => command === 'save_workspace_settings')).toHaveLength(2)
    expect(wrapper.find('.home').attributes('inert')).toBeUndefined()
    expect(settings.classes()).not.toContain('is-active')
    wrapper.unmount()
  })

  it('allows closing every module except the last one', async () => {
    const wrapper = await openDemoWorkspace()
    const calendarPanel = wrapper.findAll('.module-panel').find((panel) => panel.text().includes('日历'))!
    await calendarPanel.find('.module-panel-remove').trigger('click')
    await flushPromises()

    expect(wrapper.findAll('.module-panel')).toHaveLength(1)
    expect(savedIds()).toEqual(['editor'])
    // 只剩一个模块时关闭按钮消失，不允许把首页清空。
    expect(wrapper.find('.module-panel-remove').exists()).toBe(false)
    wrapper.unmount()
  })

  it('adds and removes modules and resets the split layout from the right rail', async () => {
    const wrapper = await openDemoWorkspace()

    // 1. 点击右侧“数据概览”功能，模块追加到分栏树最右侧并持久化。
    await wrapper.find('.app-rail [aria-label="数据概览"]').trigger('click')
    await flushPromises()
    expect(wrapper.findAll('.module-panel')).toHaveLength(3)
    expect(savedIds().at(-1)).toBe('overview')

    // 2. 模块可以从面板关闭。
    const overviewPanel = wrapper.findAll('.module-panel').find((panel) => panel.text().includes('数据概览'))!
    expect(overviewPanel.find('.overview-module').findAll('button, md-text-button')).toHaveLength(0)
    await overviewPanel.find('.module-panel-remove').trigger('click')
    await flushPromises()
    expect(wrapper.findAll('.module-panel')).toHaveLength(2)
    expect(savedIds()).not.toContain('overview')

    // 3. 布局菜单可以恢复默认分栏。
    await wrapper.find('.app-rail [aria-label="布局"]').trigger('click')
    await flushPromises()
    expect(wrapper.find('.layout-dialog').exists()).toBe(true)
    await wrapper.find('.layout-dialog-actions .text-action').trigger('click')
    await flushPromises()
    expect(savedIds()).toEqual(['calendar', 'editor'])
    await wrapper.find('.layout-dialog .layout-dialog-actions .text-action:last-child').trigger('click')
    await flushPromises()

    // 4. 放大为浮层后可用 Esc 或遮罩还原，不影响布局数据。
    await wrapper.find('.module-maximize-button').trigger('click')
    expect(wrapper.find('.module-maximize-backdrop').exists()).toBe(true)
    expect(wrapper.find('.module-panel.is-maximized').exists()).toBe(true)
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await flushPromises()
    expect(wrapper.find('.module-maximize-backdrop').exists()).toBe(false)
    expect(wrapper.find('.module-panel.is-maximized').exists()).toBe(false)
    wrapper.unmount()
  })
})
