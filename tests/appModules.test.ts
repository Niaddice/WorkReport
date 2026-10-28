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
import ModuleBoard from '../src/components/ModuleBoard.vue'
import SettingsDialog from '../src/components/SettingsDialog.vue'
import AiSummaryDialog from '../src/components/AiSummaryDialog.vue'
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
    case 'list_workspace_files':
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
      return { enabled: false, templatePath: '', prompt: '' }
    case 'list_excel_exports':
      return []
    case 'load_summary_settings':
      return { outputDirectory: 'summary' }
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
  const wrapper = mount(App, { attachTo: document.body, global: { stubs: { teleport: true } } })
  await flushPromises()
  return wrapper
}

describe('Git journal integration', () => {
  it('shows the conflicting repository names and keeps the draft when duplicate identity validation rejects a save', async () => {
    invoke.mockImplementation(async (command: string) => {
      if (command === 'save_workspace_settings')
        throw new Error('仓库“项目克隆”与“项目仓库”指向同一 Git 仓库，不能重复添加，请只保留一个')
      return commandResult(command)
    })
    const wrapper = await openDemoWorkspace()
    try {
      await wrapper.get('.app-rail [aria-label="设置"]').trigger('click')
      await flushPromises()
      await wrapper.get('[aria-controls="settings-panel-git"]').trigger('click')
      for (const [name, path] of [
        ['项目仓库', '/tmp/project'],
        ['项目克隆', '/tmp/clone'],
      ]) {
        await wrapper.get('.git-repositories-heading button').trigger('click')
        const fields = wrapper.findAll('.git-repository-form').at(-1)!.findAll('input')
        await fields[0]!.setValue(name)
        await fields[1]!.setValue(path)
      }
      await wrapper.get('.app-rail [aria-label="设置"]').trigger('click')
      await flushPromises()
      expect(wrapper.get('.settings-dialog').isVisible()).toBe(true)
      expect(wrapper.get('.settings-viewport').text()).toContain('仓库“项目克隆”与“项目仓库”指向同一 Git 仓库')
      expect(wrapper.findAll('.git-repository-form')).toHaveLength(2)
      expect(invoke.mock.calls.filter(([command]) => command === 'save_workspace_settings')).toHaveLength(1)
    } finally {
      wrapper.unmount()
    }
  })

  it('adds the module from the rail and preserves existing content when appending and undoing a report', async () => {
    const date = dayjs().format('YYYYMMDD')
    const entry = { date, filename: `${date}.txt`, format: 'txt', content: '已有的会议记录\n', revision: 'v1' }
    invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === 'load_git_settings')
        return {
          executable: 'git',
          includeMerges: false,
          repositories: [{ id: 'repo', name: '日报项目', path: '/tmp/project', author: '', branch: '' }],
        }
      if (command === 'list_entries') return [entry]
      if (command === 'read_entry') return entry
      if (command === 'save_entry') return { ...entry, content: args!.content, revision: 'v2' }
      if (command === 'read_git_daily_log')
        return {
          date,
          repositories: [
            {
              id: 'repo',
              name: '日报项目',
              error: null,
              truncated: false,
              commits: [
                {
                  hash: 'abc123456',
                  author: '作者',
                  email: 'me@example.test',
                  committedAt: dayjs().toISOString(),
                  message: '完成 Git 日报',
                },
              ],
            },
          ],
        }
      return commandResult(command)
    })
    const wrapper = await openDemoWorkspace()
    try {
      await wrapper.get('.app-rail [aria-label="Git 日报"]').trigger('click')
      await flushPromises()
      expect(savedIds()).toContain('git')
      await wrapper.get('.git-toolbar .git-fetch').trigger('click')
      await flushPromises()
      await wrapper.get('.git-report-actions .git-fetch').trigger('click')
      await flushPromises()
      const editor = wrapper.get<HTMLTextAreaElement>('.markdown-editor')
      expect(editor.element.value).toBe('已有的会议记录\n\n日报项目\n1、完成 Git 日报')
      expect(invoke).toHaveBeenCalledWith(
        'save_entry',
        expect.objectContaining({
          date,
          content: '已有的会议记录\n\n日报项目\n1、完成 Git 日报',
          expectedRevision: 'v1',
        }),
      )
      editor.element.focus()
      await editor.trigger('keydown', { key: 'z', ctrlKey: true })
      await flushPromises()
      expect(editor.element.value).toBe(entry.content)
      expect(wrapper.get('.git-report-actions .git-fetch').attributes('disabled')).toBeUndefined()
    } finally {
      wrapper.unmount()
    }
  })

  it('saves Git configuration in the unified settings transaction and applies it to the module', async () => {
    invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) =>
      command === 'save_workspace_settings' ? args!.settings : commandResult(command),
    )
    const wrapper = await openDemoWorkspace()
    try {
      await wrapper.get('.app-rail [aria-label="设置"]').trigger('click')
      await flushPromises()
      await wrapper.get('[aria-controls="settings-panel-git"]').trigger('click')
      await wrapper.get('.git-repositories-heading button').trigger('click')
      const fields = wrapper.findAll('.git-repository-form input')
      await fields[0]!.setValue('项目仓库')
      await fields[1]!.setValue('/tmp/project')
      await fields[2]!.setValue('me@example.test')
      await fields[3]!.setValue('main')
      expect(invoke.mock.calls.some(([command]) => command === 'save_workspace_settings')).toBe(false)
      await wrapper.get('.app-rail [aria-label="设置"]').trigger('click')
      await flushPromises()
      const saves = invoke.mock.calls.filter(([command]) => command === 'save_workspace_settings')
      expect(saves).toHaveLength(1)
      expect(saves[0]![1]).toMatchObject({
        workspacePath: DEMO_WORKSPACE,
        settings: {
          git: {
            executable: 'git',
            includeMerges: false,
            repositories: [{ name: '项目仓库', path: '/tmp/project', author: 'me@example.test', branch: 'main' }],
          },
        },
      })
      await wrapper.get('.app-rail [aria-label="Git 日报"]').trigger('click')
      await flushPromises()
      expect(wrapper.get('.git-module').text()).toContain('1 个仓库')
      expect(wrapper.get('.git-toolbar .git-fetch').attributes('disabled')).toBeUndefined()
    } finally {
      wrapper.unmount()
    }
  })
})

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
    expect(wrapper.getComponent(ModuleBoard).props('workspacePath')).toBe(DEMO_WORKSPACE)

    await wrapper.find('.workspace-switch-dialog md-filled-button').trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('open_workspace', { path: target, syncSettingsFrom: DEMO_WORKSPACE })
    expect(wrapper.getComponent(ModuleBoard).props('workspacePath')).toBe(target)
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
    expect(wrapper.getComponent(ModuleBoard).props('workspacePath')).toBe(target)
    wrapper.unmount()
  })

  it('keeps the original workspace when the confirmation is cancelled', async () => {
    const wrapper = await openDemoWorkspace()
    await chooseAnotherWorkspace(wrapper, target)
    await wrapper.findAll('.workspace-switch-dialog md-text-button')[0]!.trigger('click')
    await flushPromises()
    expect(invoke.mock.calls.filter(([command]) => command === 'open_workspace')).toHaveLength(1)
    expect(wrapper.getComponent(ModuleBoard).props('workspacePath')).toBe(DEMO_WORKSPACE)
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
    expect(wrapper.getComponent(ModuleBoard).props('workspacePath')).toBe(DEMO_WORKSPACE)
    expect(wrapper.find('.workspace-switch-dialog .dialog-error').text()).toContain('目标设置无法写入')
    expect((wrapper.find('.workspace-switch-dialog').element as HTMLDialogElement).open).toBe(true)
    await wrapper.findAll('.workspace-switch-dialog md-text-button')[1]!.trigger('click')
    await flushPromises()
    expect(wrapper.getComponent(ModuleBoard).props('workspacePath')).toBe(target)
    wrapper.unmount()
  })
})

describe('modular home integration', () => {
  it('keeps summary prompt edits temporary and cancels a confirmation while AI settings are still loading', async () => {
    const date = dayjs().format('YYYYMMDD')
    const aiSettings = {
      providers: [
        {
          id: 'ai',
          name: '默认 AI',
          kind: 'http',
          model: 'example',
          executable: '',
          baseUrl: 'https://example.test',
          apiKey: '',
        },
      ],
      defaultProviderId: 'ai',
      generationPreference: '',
      summaryPrompt: '设置中的总结规则',
    }
    let delayLoad = false
    let finishLoad!: (value: typeof aiSettings) => void
    invoke.mockImplementation(async (command: string) => {
      if (command === 'load_ai_settings')
        return delayLoad
          ? new Promise((resolve) => {
              finishLoad = resolve
            })
          : aiSettings
      if (command === 'list_entries') return [{ date, filename: `${date}.md`, format: 'md' }]
      if (command === 'read_entry')
        return { date, filename: `${date}.md`, format: 'md', content: '完成项目任务', revision: 'v1' }
      if (command === 'summarize_entries') return '## 项目成果\n\n完成任务'
      return commandResult(command)
    })
    const wrapper = await openDemoWorkspace()
    try {
      await wrapper.get('.app-rail [aria-label="总结"]').trigger('click')
      await flushPromises()
      await wrapper.get('.summary-run-button').trigger('click')
      expect(wrapper.get<HTMLTextAreaElement>('#summary-session-prompt').element.value).toBe(aiSettings.summaryPrompt)
      await wrapper.get('#summary-session-prompt').setValue('仅本次按项目总结')
      delayLoad = true
      await wrapper.get('.summary-start-generation').trigger('click')
      await flushPromises()
      await wrapper.get('#summary-generation-dialog [aria-label="关闭 AI 总结"]').trigger('click')
      finishLoad(aiSettings)
      await flushPromises()
      expect(invoke.mock.calls.some(([command]) => command === 'summarize_entries')).toBe(false)
      delayLoad = false
      await wrapper.get('.summary-run-button').trigger('click')
      expect(wrapper.get<HTMLTextAreaElement>('#summary-session-prompt').element.value).toBe(aiSettings.summaryPrompt)
      await wrapper.get('#summary-session-prompt').setValue('本次突出交付成果')
      await wrapper.get('.summary-start-generation').trigger('click')
      await flushPromises()
      expect(invoke).toHaveBeenCalledWith('summarize_entries', {
        content: expect.stringContaining('完成项目任务'),
        requestId: expect.any(String),
        prompt: '本次突出交付成果',
      })
      expect(wrapper.get('.summary-generation-result').text()).toContain('完成任务')
      expect(invoke.mock.calls.some(([command]) => command === 'save_workspace_settings')).toBe(false)
    } finally {
      wrapper.unmount()
    }
  })

  it('sends the temporary Excel prompt through generation without saving it to workspace settings', async () => {
    const configured = { enabled: true, templatePath: '/templates/report.xlsx', prompt: '设置中的原始规则' }
    invoke.mockImplementation(async (command: string) => {
      if (command === 'load_excel_settings') return configured
      if (command === 'load_ai_settings')
        return {
          providers: [
            {
              id: 'ai',
              name: '默认 AI',
              kind: 'http',
              model: 'example',
              executable: '',
              baseUrl: 'https://example.test',
              apiKey: '',
            },
          ],
          defaultProviderId: 'ai',
          generationPreference: '',
          summaryPrompt: '',
        }
      if (command === 'prepare_excel')
        return {
          id: 'temporary-preview',
          templateName: 'report.xlsx',
          outputDirectory: `${DEMO_WORKSPACE}/excel`,
          start: dayjs().format('YYYYMMDD'),
          end: dayjs().format('YYYYMMDD'),
          cells: [],
          styles: [],
          sheets: [],
        }
      return commandResult(command)
    })
    const wrapper = await openDemoWorkspace()
    try {
      await wrapper.get('.app-rail [aria-label="Excel报告"]').trigger('click')
      await flushPromises()
      await wrapper.get('.excel-generate-button').trigger('click')
      expect(invoke.mock.calls.some(([command]) => command === 'prepare_excel')).toBe(false)
      await wrapper.get('#excel-session-prompt').setValue('本次只提炼项目成果')
      await wrapper.get('.excel-start-generation').trigger('click')
      await flushPromises()
      expect(invoke).toHaveBeenCalledWith('prepare_excel', {
        workspacePath: DEMO_WORKSPACE,
        dates: [dayjs().format('YYYYMMDD')],
        requestId: expect.any(String),
        prompt: '本次只提炼项目成果',
      })
      expect(invoke.mock.calls.some(([command]) => command === 'save_workspace_settings')).toBe(false)
      await wrapper.get('.excel-preview-dialog [aria-label="关闭 Excel报告"]').trigger('click')
      await flushPromises()
      await wrapper.get('.excel-generate-button').trigger('click')
      expect(wrapper.get<HTMLTextAreaElement>('#excel-session-prompt').element.value).toBe(configured.prompt)
    } finally {
      wrapper.unmount()
    }
  })

  it('exports a summary with the complete selected range, including empty boundary days', async () => {
    const start = dayjs().startOf('month').date(10)
    const middle = start.add(1, 'day')
    const end = start.add(2, 'day')
    const date = middle.format('YYYYMMDD')
    const entry = { date, filename: `${date}.md`, format: 'md' }
    invoke.mockImplementation(async (command: string) => {
      if (command === 'list_entries') return [entry]
      if (command === 'read_entry') return { ...entry, content: '完成范围内的任务', revision: 'v1' }
      if (command === 'export_summary')
        return { filename: '总结_export.md', path: `${DEMO_WORKSPACE}/summary/总结_export.md` }
      return commandResult(command)
    })
    const wrapper = await openDemoWorkspace()
    try {
      await wrapper.get('.app-rail [aria-label="总结"]').trigger('click')
      await flushPromises()
      const day = (value: ReturnType<typeof dayjs>) =>
        wrapper.get(`.calendar-panel .day-cell[aria-label^="${value.format('YYYY年M月D日 ')}"]`)
      await day(start).trigger('pointerdown', { button: 0 })
      await day(end).trigger('pointerenter')
      window.dispatchEvent(new Event('pointerup'))
      await flushPromises()
      expect(wrapper.get('.summary-module .summary-result').text()).toContain('完成范围内的任务')
      expect(wrapper.get('.summary-module .summary-export-icon').attributes('disabled')).toBeUndefined()
      wrapper.getComponent(AiSummaryDialog).vm.$emit('export', 'md')
      await flushPromises()
      expect(invoke).toHaveBeenCalledWith('export_summary', {
        workspacePath: DEMO_WORKSPACE,
        dates: [start, middle, end].map((value) => value.format('YYYYMMDD')),
        content: expect.stringContaining('完成范围内的任务'),
        format: 'md',
      })
    } finally {
      wrapper.unmount()
    }
  })

  it.each(['single', 'range', 'same-day'] as const)(
    'reloads Excel reports from disk after a %s calendar selection without manual refresh',
    async (selection) => {
      const target = selection === 'same-day' ? dayjs() : dayjs().startOf('month').date(10)
      const end = target.add(2, 'day')
      const filename = `总结_${target.format('YYYYMMDD')}-${end.format('YYYYMMDD')}_calendar.xlsx`
      let diskReports: { filename: string; path: string; modified: number; size: number }[] = []
      invoke.mockImplementation(async (command: string) =>
        command === 'list_excel_exports' ? [...diskReports] : commandResult(command),
      )
      const wrapper = await openDemoWorkspace()
      try {
        await wrapper.get('.app-rail [aria-label="Excel报告"]').trigger('click')
        await flushPromises()
        expect(wrapper.findAll('.excel-export-name')).toHaveLength(0)
        const reads = invoke.mock.calls.filter(([command]) => command === 'list_excel_exports').length
        diskReports = [{ filename, path: `${DEMO_WORKSPACE}/excel/${filename}`, modified: 0, size: 100 }]
        const day = (date: ReturnType<typeof dayjs>) =>
          wrapper.get(`.calendar-panel .day-cell[aria-label^="${date.format('YYYY年M月D日 ')}"]`)
        if (selection === 'range') {
          await day(target).trigger('pointerdown', { button: 0 })
          await day(end).trigger('pointerenter')
          window.dispatchEvent(new Event('pointerup'))
        } else await day(target).trigger('click')
        await flushPromises()
        expect(wrapper.findAll('.excel-export-name').map((item) => item.text())).toEqual([filename])
        expect(invoke.mock.calls.filter(([command]) => command === 'list_excel_exports').length).toBeGreaterThan(reads)
      } finally {
        wrapper.unmount()
      }
    },
  )

  it.each(['calendar', 'overview'] as const)(
    'keeps the last clicked module %s active after a delayed calendar selection',
    async (lastClicked) => {
      const today = dayjs()
      const target = today.startOf('month').date(today.date() === 5 ? 6 : 5)
      const makeDocument = (date: ReturnType<typeof dayjs>) => ({
        date: date.format('YYYYMMDD'),
        filename: `${date.format('YYYYMMDD')}.md`,
        format: 'md',
        content: `日报 ${date.format('YYYYMMDD')}`,
        revision: date.format('YYYYMMDD'),
      })
      const currentDocument = makeDocument(today)
      const targetDocument = makeDocument(target)
      let completeRead!: (document: typeof targetDocument) => void
      const nextRead = new Promise<typeof targetDocument>((resolve) => {
        completeRead = resolve
      })
      invoke.mockImplementation(async (command: string, args?: { date?: string }) => {
        if (command === 'list_entries') return [currentDocument, targetDocument]
        if (command === 'read_entry') return args?.date === targetDocument.date ? nextRead : currentDocument
        return commandResult(command)
      })
      const wrapper = await openDemoWorkspace()
      const activeIds = () =>
        wrapper.findAll('.module-panel.is-active').map((panel) => panel.attributes('data-module-id'))
      try {
        expect(activeIds()).toEqual(['editor'])
        if (lastClicked === 'overview') {
          await wrapper.get('.app-rail [aria-label="数据概览"]').trigger('click')
          await flushPromises()
        }
        wrapper.get<HTMLTextAreaElement>('.markdown-editor').element.focus()
        await wrapper.vm.$nextTick()
        expect(activeIds()).toEqual(['editor'])
        const day = wrapper.get<HTMLButtonElement>(
          `.calendar-panel .day-cell[aria-label^="${target.format('YYYY年M月D日 ')}"]`,
        )
        await day.trigger('pointerdown', { button: 0 })
        expect(activeIds()).toEqual(['calendar'])
        day.element.focus()
        await wrapper.vm.$nextTick()
        expect(activeIds()).toEqual(['calendar'])
        window.dispatchEvent(new Event('pointerup'))
        await day.trigger('click')
        await flushPromises()
        expect(invoke).toHaveBeenCalledWith('read_entry', { date: targetDocument.date, format: 'md' })
        expect(wrapper.get('.markdown-editor').attributes('disabled')).toBeDefined()
        expect(activeIds()).toEqual(['calendar'])

        if (lastClicked === 'overview') {
          const overview = wrapper.get('[data-module-id="overview"]')
          await overview.get('.module-panel-header').trigger('pointerdown', { button: 0 })
          overview.get<HTMLButtonElement>('.module-maximize-button').element.focus()
          await wrapper.vm.$nextTick()
          expect(activeIds()).toEqual(['overview'])
        }

        completeRead(targetDocument)
        await flushPromises()
        const editor = wrapper.get<HTMLTextAreaElement>('.markdown-editor').element
        expect(editor.value).toBe(targetDocument.content)
        expect(document.activeElement).not.toBe(editor)
        expect(activeIds()).toEqual([lastClicked])

        await wrapper.get('.markdown-editor').trigger('pointerdown', { button: 0 })
        editor.focus()
        await wrapper.vm.$nextTick()
        expect(document.activeElement).toBe(editor)
        expect(activeIds()).toEqual(['editor'])
      } finally {
        completeRead(targetDocument)
        wrapper.unmount()
      }
    },
  )

  it.each(['current', 'missing'] as const)('keeps calendar navigation active for a %s report', async (kind) => {
    const today = dayjs()
    const target = kind === 'current' ? today : today.startOf('month').date(today.date() === 5 ? 6 : 5)
    const wrapper = await openDemoWorkspace()
    try {
      const editor = wrapper.get<HTMLTextAreaElement>('.markdown-editor').element
      editor.focus()
      await wrapper.vm.$nextTick()
      const day = wrapper.get<HTMLButtonElement>(
        `.calendar-panel .day-cell[aria-label^="${target.format('YYYY年M月D日 ')}"]`,
      )
      day.element.focus()
      await day.trigger('click')
      await flushPromises()

      expect(wrapper.get('.module-panel.is-active').attributes('data-module-id')).toBe('calendar')
      expect(document.activeElement).not.toBe(editor)
      expect(invoke.mock.calls.some(([command]) => command === 'create_entry')).toBe(false)
    } finally {
      wrapper.unmount()
    }
  })

  it('docks the preview against the editor and chooses the other side after the layout changes', async () => {
    const date = dayjs().format('YYYYMMDD')
    const document = { date, filename: `${date}.md`, format: 'md', content: '测试正文', revision: '1' }
    invoke.mockImplementation(async (command: string) => {
      if (command === 'list_entries') return [document]
      if (command === 'read_entry') return document
      return commandResult(command)
    })
    let editorRect = new DOMRect(500, 100, window.innerWidth - 508, 600)
    const original = HTMLElement.prototype.getBoundingClientRect
    const rect = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (
      this: HTMLElement,
    ) {
      return this.dataset.moduleId === 'editor' ? editorRect : original.call(this)
    })
    const wrapper = await openDemoWorkspace()
    try {
      const toggle = wrapper.get('.editor-body-tools [aria-label="日报预览"]')
      await toggle.trigger('click')
      await flushPromises()
      let preview = wrapper.get<HTMLElement>('.journal-preview-window').element
      expect(parseFloat(preview.style.left)).toBe(8)
      expect(parseFloat(preview.style.width)).toBe(492)
      expect(parseFloat(preview.style.top)).toBe(editorRect.y)
      expect(parseFloat(preview.style.left) + parseFloat(preview.style.width)).toBe(editorRect.x)
      await toggle.trigger('click')
      editorRect = new DOMRect(8, 100, 400, 600)
      await toggle.trigger('click')
      await flushPromises()
      preview = wrapper.get<HTMLElement>('.journal-preview-window').element
      expect(parseFloat(preview.style.left)).toBe(editorRect.right)
      expect(parseFloat(preview.style.width)).toBe(520)
    } finally {
      wrapper.unmount()
      rect.mockRestore()
    }
  })

  it('toggles a floating journal preview from the editor and renders unsaved changes immediately', async () => {
    const date = dayjs().format('YYYYMMDD')
    const document = { date, filename: `${date}.md`, format: 'md', content: '# 初始标题', revision: '1' }
    invoke.mockImplementation(async (command: string) => {
      if (command === 'list_entries') return [document]
      if (command === 'read_entry') return document
      return commandResult(command)
    })
    const wrapper = await openDemoWorkspace()
    expect(wrapper.find('.app-rail [aria-label="Markdown 预览"]').exists()).toBe(false)
    const trigger = wrapper.get('.editor-body-tools [aria-label="日报预览"]')
    expect(trigger.attributes('disabled')).toBeUndefined()
    await trigger.trigger('click')
    await flushPromises()
    const preview = wrapper.get('.markdown-preview-module')
    expect(preview.get('.journal-prose').text()).toContain('初始标题')
    expect(wrapper.find('.markdown-editor').isVisible()).toBe(true)
    expect(wrapper.findAll('.module-panel')).toHaveLength(2)
    expect(invoke.mock.calls.some(([command]) => command === 'save_ui_preferences')).toBe(false)
    expect(wrapper.get('.journal-preview-window').attributes('aria-modal')).toBe('false')

    await wrapper
      .get('.markdown-editor')
      .setValue('# 未保存的标题\n\n**实时草稿**\n\n| 项目 | 状态 |\n| --- | --- |\n| 预览 | 更新 |')
    expect(preview.get('.journal-prose').text()).toContain('未保存的标题')
    expect(preview.get('.journal-prose strong').text()).toBe('实时草稿')
    expect(preview.get('.journal-prose table').text()).toContain('更新')
    expect(invoke.mock.calls.some(([command]) => command === 'save_entry')).toBe(false)

    const input = wrapper.get<HTMLTextAreaElement>('.markdown-editor')
    const previewViewport = preview.get<HTMLElement>('.markdown-preview-module-content').element
    Object.defineProperties(input.element, {
      clientWidth: { value: 300 },
      clientHeight: { value: 200 },
      scrollHeight: { value: 1000 },
    })
    Object.defineProperties(previewViewport, { clientHeight: { value: 200 }, scrollHeight: { value: 1600 } })
    input.element.scrollTop = 400
    await input.trigger('scroll')
    expect(previewViewport.scrollTop).toBe(700)
    input.element.scrollTop = 800
    await input.trigger('scroll')
    expect(previewViewport.scrollTop).toBe(1400)

    await trigger.trigger('click')
    await flushPromises()
    expect(wrapper.find('.journal-preview-window').exists()).toBe(false)
    await trigger.trigger('click')
    await flushPromises()
    expect(wrapper.findAll('.markdown-preview-module')).toHaveLength(1)
    await wrapper.get('[aria-label="关闭日报预览"]').trigger('click')
    await flushPromises()
    expect(wrapper.find('.journal-preview-window').exists()).toBe(false)
    expect(trigger.attributes('aria-pressed')).toBe('false')
    wrapper.unmount()
  })

  it('enables preview for an empty Markdown draft and disables it again after switching to TXT', async () => {
    const wrapper = await openDemoWorkspace()
    const trigger = wrapper.get('.editor-body-tools [aria-label="日报预览"]')
    expect(trigger.attributes('disabled')).toBeDefined()
    await trigger.trigger('click')
    expect(wrapper.find('.markdown-preview-module').exists()).toBe(false)

    await wrapper.get('.format-toggle').trigger('click')
    await flushPromises()
    expect(wrapper.get('.format-toggle').text()).toBe('MD')
    expect(trigger.attributes('disabled')).toBeUndefined()
    await trigger.trigger('click')
    await flushPromises()
    expect(wrapper.get('.markdown-preview-module-empty').text()).toContain('编辑内容后')

    await wrapper.get('.format-toggle').trigger('click')
    await flushPromises()
    expect(wrapper.get('.format-toggle').text()).toBe('TXT')
    expect(trigger.attributes('disabled')).toBeDefined()
    expect(wrapper.find('.journal-preview-window').exists()).toBe(false)
    expect(invoke.mock.calls.some(([command]) => command === 'create_entry')).toBe(false)
    wrapper.unmount()
  })

  it('follows the editor when switching dates and displays the selected range in the floating window', async () => {
    const current = dayjs()
    const first = current.startOf('month').date(current.date() === 5 ? 6 : 5)
    const last = first.add(1, 'day')
    const documents = [
      { date: current.format('YYYYMMDD'), content: '# 当前日期' },
      { date: first.format('YYYYMMDD'), content: '# 另一天的内容' },
    ].map((document) => ({ ...document, filename: `${document.date}.md`, format: 'md', revision: '1' }))
    invoke.mockImplementation(async (command: string, args?: { date: string }) => {
      if (command === 'list_entries') return documents
      if (command === 'read_entry') return documents.find((document) => document.date === args?.date)
      return commandResult(command)
    })
    const wrapper = await openDemoWorkspace()
    const trigger = wrapper.get('.editor-body-tools [aria-label="日报预览"]')
    await trigger.trigger('click')
    await flushPromises()
    expect(wrapper.get('.markdown-preview-module-content').text()).toContain('当前日期')

    const cells = wrapper.findAll('.calendar-panel .day-cell')
    const start = cells.find((cell) => cell.attributes('aria-label')?.startsWith(first.format('YYYY年M月D日 ')))!
    const end = cells.find((cell) => cell.attributes('aria-label')?.startsWith(last.format('YYYY年M月D日 ')))!
    await start.trigger('click')
    await flushPromises()
    expect(wrapper.get('.markdown-preview-module-content').text()).toContain('另一天的内容')
    expect(wrapper.get('.markdown-preview-module-heading').text()).toContain(`${first.format('YYYYMMDD')}.md`)

    await start.trigger('pointerdown', { button: 0 })
    await end.trigger('pointerenter')
    window.dispatchEvent(new Event('pointerup'))
    await flushPromises()
    expect(wrapper.get('.range-overview [aria-label="日报预览"]').attributes('disabled')).toBeUndefined()
    expect(wrapper.get('.markdown-preview-module-content').text()).toContain('另一天的内容')
    expect(wrapper.get('.markdown-preview-module-heading').text()).toContain('2 天 · 1 篇日报')
    const rangeBody = wrapper.get<HTMLElement>('.range-overview-body')
    const previewViewport = wrapper.get<HTMLElement>('.markdown-preview-module-content').element
    Object.defineProperties(rangeBody.element, { clientHeight: { value: 200 }, scrollHeight: { value: 1000 } })
    Object.defineProperties(previewViewport, { clientHeight: { value: 200 }, scrollHeight: { value: 1600 } })
    rangeBody.element.scrollTop = 400
    await rangeBody.trigger('scroll')
    expect(previewViewport.scrollTop).toBe(700)

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()
    expect(wrapper.get('.editor-body-tools [aria-label="日报预览"]').attributes('disabled')).toBeUndefined()
    expect(wrapper.get('.markdown-preview-module-content').text()).toContain('另一天的内容')
    wrapper.unmount()
  })

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
    expect(wrapper.find('.workspace-gate .gate-error').text()).toContain('工作空间目录已不存在')
    expect(wrapper.find('.recent-workspace').attributes('disabled')).toBeUndefined()
    wrapper.unmount()
  })

  it('keeps the workspace failure visible and reports the same error again after retrying', async () => {
    let attempts = 0
    let rejectOpen!: (error: Error) => void
    invoke.mockImplementation(async (command: string) => {
      if (command !== 'open_workspace') return commandResult(command)
      if (++attempts === 1) throw new Error('工作空间目录已不存在')
      return new Promise<never>((_, reject) => {
        rejectOpen = reject
      })
    })
    const wrapper = await openDemoWorkspace()
    await wrapper.find('.app-message-close').trigger('click')
    await flushPromises()
    expect(wrapper.find('.app-message').exists()).toBe(false)
    expect(wrapper.find('.workspace-gate .gate-error').text()).toContain('工作空间目录已不存在')

    await wrapper.find('.recent-workspace').trigger('click')
    await flushPromises()
    expect(wrapper.find('.recent-workspace').text()).toContain('正在打开工作空间')
    expect(wrapper.find('.workspace-gate .gate-error').exists()).toBe(false)
    expect(wrapper.find('.recent-workspace').attributes('disabled')).toBeDefined()
    expect(wrapper.find('.workspace-gate md-filled-button').attributes('disabled')).toBeDefined()
    await wrapper.find('.recent-workspace').trigger('click')
    expect(attempts).toBe(2)

    rejectOpen(new Error('工作空间目录已不存在'))
    await flushPromises()
    expect(wrapper.find('.workspace-gate .gate-error').text()).toContain('工作空间目录已不存在')
    expect(wrapper.find('.app-message').text()).toContain('工作空间目录已不存在')
    expect(wrapper.find('.recent-workspace').attributes('disabled')).toBeUndefined()
    wrapper.unmount()
  })

  it.each(['retry', 'choose'])('recovers from an unavailable recent workspace by %s', async (action) => {
    let attempts = 0
    invoke.mockImplementation(async (command: string, args?: { path: string }) => {
      if (command !== 'open_workspace') return commandResult(command)
      if (++attempts === 1) throw new Error('工作空间目录已不存在')
      return { path: args!.path, name: args!.path.split('/').at(-1) }
    })
    const wrapper = await openDemoWorkspace()
    const target = action === 'retry' ? DEMO_WORKSPACE : '/tmp/workreport-recovered'
    if (action === 'choose') pickDirectory.mockResolvedValueOnce(target)
    await wrapper.find(action === 'retry' ? '.recent-workspace' : '.workspace-gate md-filled-button').trigger('click')
    await flushPromises()

    expect(invoke.mock.calls.filter(([command]) => command === 'open_workspace')).toEqual([
      ['open_workspace', { path: DEMO_WORKSPACE }],
      ['open_workspace', { path: target }],
    ])
    expect(wrapper.getComponent(ModuleBoard).props('workspacePath')).toBe(target)
    expect(wrapper.find('.module-board').exists()).toBe(true)
    expect(wrapper.find('.workspace-gate').exists()).toBe(false)
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
    for (const label of ['历史记录', '总结', 'Excel报告']) {
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

  it('adds the directory module and switches dates from the file tree', async () => {
    const today = dayjs()
    const yesterday = today.subtract(1, 'day')
    const twoDaysAgo = today.subtract(2, 'day')
    const year = today.format('YYYY')
    const report = (date: ReturnType<typeof dayjs>) => ({
      date: date.format('YYYYMMDD'),
      filename: `${date.format('YYYYMMDD')}.md`,
      format: 'md' as const,
    })
    const entries = [report(today), report(yesterday), report(twoDaysAgo)]
    const files = [
      { path: year, name: year, isDir: true, size: 0 },
      ...entries.map((entry) => ({ path: `${year}/${entry.filename}`, name: entry.filename, isDir: false, size: 128 })),
    ]
    invoke.mockImplementation(async (command: string, args?: { date?: string }) => {
      if (command === 'list_entries') return entries
      if (command === 'list_workspace_files') return files
      if (command === 'read_entry') {
        const entry = entries.find((item) => item.date === args?.date)
        return entry ? { ...entry, content: '', revision: entry.date } : undefined
      }
      return commandResult(command)
    })
    const wrapper = await openDemoWorkspace()
    await wrapper.find('.app-rail [aria-label="工作空间目录"]').trigger('click')
    await flushPromises()

    expect(wrapper.find('.directory-module').exists()).toBe(true)
    expect(savedIds()).toContain('directory')

    const items = wrapper.findAll('.directory-item')
    expect(items).toHaveLength(3)
    const currentName = `${today.format('YYYYMMDD')}.md`
    expect(items.find((item) => item.find('.directory-item-name').text() === currentName)!.classes()).toContain(
      'is-current',
    )

    const yesterdayName = `${yesterday.format('YYYYMMDD')}.md`
    await items.find((item) => item.find('.directory-item-name').text() === yesterdayName)!.trigger('click')
    await flushPromises()

    expect(wrapper.find('.directory-item.is-current').find('.directory-item-name').text()).toBe(yesterdayName)
    expect(wrapper.get('.module-panel.is-active').attributes('data-module-id')).toBe('directory')
    expect(document.activeElement).not.toBe(wrapper.get('.markdown-editor').element)
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
      expect(range.find('.range-heading').text()).toBe(`${first.format('M 月 D 日')} – ${last.format('D 日')}`)
      expect(range.findAll('button').map((button) => button.attributes('aria-label'))).toEqual(['预览', '日报预览'])
      const preview = range.find('.editor-body-tools .preview-toggle')
      expect(preview.text()).toBe('')
      await preview.trigger('click')
      expect(preview.attributes('aria-pressed')).toBe('true')
      expect(preview.attributes('aria-label')).toBe('查看原文')
      expect(range.findAll('.journal-prose')).toHaveLength(2)
      await preview.trigger('click')
      expect(preview.attributes('aria-pressed')).toBe('false')
      expect(range.findAll('.range-text')).toHaveLength(2)
      expect(range.find('.editor-text-actions').exists()).toBe(false)
      const summary = wrapper.find('.summary-module')
      if (mode === 'ai') {
        const runButton = summary.find('.summary-run-button')
        expect((runButton.element as HTMLButtonElement).disabled).toBe(false)
        await runButton.trigger('click')
        await wrapper.get('#summary-generation-dialog .summary-start-generation').trigger('click')
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
    expect(wrapper.get('.settings-dialog').isVisible()).toBe(true)
    expect(wrapper.get('.settings-dialog').attributes('role')).toBe('dialog')
    expect(wrapper.get('.home').isVisible()).toBe(true)
    expect(wrapper.get('.home').attributes('inert')).toBeDefined()
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
    expect(wrapper.get('.settings-dialog').isVisible()).toBe(false)
    expect(invoke.mock.calls.some(([command]) => command === 'save_workspace_settings')).toBe(false)
    expect((wrapper.find('.settings-leave-dialog').element as HTMLDialogElement).open).toBe(false)
    wrapper.unmount()
  })

  it('confirms unsaved settings when closing the maximized window or pressing Escape', async () => {
    const wrapper = await openDemoWorkspace()
    try {
      await wrapper.get('.app-rail [aria-label="设置"]').trigger('click')
      await flushPromises()
      await wrapper.get('[aria-controls="settings-panel-appearance"]').trigger('click')
      await wrapper.get('#settings-panel-appearance [aria-label="放大字体"]').trigger('click')
      await wrapper.get('[aria-label="关闭设置"]').trigger('click')
      await flushPromises()
      expect(wrapper.getComponent(SettingsDialog).props('closeTarget')).toBe('close')
      expect(wrapper.get<HTMLDialogElement>('.settings-leave-dialog').element.open).toBe(true)
      expect(wrapper.get('.settings-dialog').isVisible()).toBe(true)

      await wrapper.get('.settings-leave-dialog md-text-button').trigger('click')
      await flushPromises()
      expect(wrapper.get<HTMLDialogElement>('.settings-leave-dialog').element.open).toBe(false)
      expect(wrapper.get('.appearance-scale-value').text()).toBe('110%')
      expect(wrapper.getComponent(SettingsDialog).props('closeTarget')).toBe('settings')
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
      await flushPromises()
      expect(wrapper.get<HTMLDialogElement>('.settings-leave-dialog').element.open).toBe(true)
      await wrapper.findAll('.settings-leave-dialog md-text-button')[1]!.trigger('click')
      await flushPromises()
      expect(wrapper.get('.settings-dialog').isVisible()).toBe(false)
      expect(wrapper.get('.home').attributes('inert')).toBeUndefined()
      expect(invoke.mock.calls.some(([command]) => command === 'save_workspace_settings')).toBe(false)
    } finally {
      wrapper.unmount()
    }
  })

  it.each([false, true])(
    'keeps the window close button as the shrink target after confirming save=%s',
    async (save) => {
      invoke.mockImplementation(async (command: string, args?: { settings: WorkspaceSettings }) =>
        command === 'save_workspace_settings' ? args!.settings : commandResult(command),
      )
      const wrapper = await openDemoWorkspace()
      try {
        await wrapper.get('.app-rail [aria-label="设置"]').trigger('click')
        await flushPromises()
        await wrapper.get('[aria-controls="settings-panel-appearance"]').trigger('click')
        await wrapper.get('#settings-panel-appearance [aria-label="放大字体"]').trigger('click')
        await wrapper.get('[aria-label="关闭设置"]').trigger('click')
        await flushPromises()
        if (save) await wrapper.get('.settings-leave-dialog md-filled-button').trigger('click')
        else await wrapper.findAll('.settings-leave-dialog md-text-button')[1]!.trigger('click')
        await flushPromises()
        expect(wrapper.get('.settings-dialog').isVisible()).toBe(false)
        expect(wrapper.getComponent(SettingsDialog).props('closeTarget')).toBe('close')
      } finally {
        wrapper.unmount()
      }
    },
  )

  it('keeps the calendar selection when Escape closes the settings window', async () => {
    const wrapper = await openDemoWorkspace()
    try {
      const first = dayjs().startOf('month').date(5)
      const last = first.add(1, 'day')
      await wrapper
        .get(`.day-cell[aria-label^="${first.format('YYYY年M月D日 ')}"]`)
        .trigger('pointerdown', { button: 0 })
      await wrapper.get(`.day-cell[aria-label^="${last.format('YYYY年M月D日 ')}"]`).trigger('pointerenter')
      window.dispatchEvent(new Event('pointerup'))
      await flushPromises()
      expect(wrapper.find('.range-overview').exists()).toBe(true)

      await wrapper.get('.app-rail [aria-label="设置"]').trigger('click')
      await flushPromises()
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
      await flushPromises()
      expect(wrapper.get('.settings-dialog').isVisible()).toBe(false)
      expect(wrapper.find('.range-overview').exists()).toBe(true)
    } finally {
      wrapper.unmount()
    }
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
      settings: expect.objectContaining({ appearance: { scale: 110, rail: 'right' } }),
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
    expect(wrapper.get('.module-panel.is-active').attributes('data-module-id')).toBe('editor')

    // 1. 点击右侧“数据概览”功能，模块追加到分栏树最右侧并持久化。
    await wrapper.find('.app-rail [aria-label="数据概览"]').trigger('click')
    await flushPromises()
    expect(wrapper.findAll('.module-panel')).toHaveLength(3)
    expect(savedIds().at(-1)).toBe('overview')
    expect(wrapper.get('.module-panel.is-active').attributes('data-module-id')).toBe('overview')

    // 2. 模块可以从面板关闭。
    const overviewPanel = wrapper.findAll('.module-panel').find((panel) => panel.text().includes('数据概览'))!
    expect(overviewPanel.find('.overview-module').findAll('button, md-text-button')).toHaveLength(0)
    await overviewPanel.find('.module-panel-remove').trigger('click')
    await flushPromises()
    expect(wrapper.findAll('.module-panel')).toHaveLength(2)
    expect(savedIds()).not.toContain('overview')
    expect(wrapper.get('.module-panel.is-active').attributes('data-module-id')).toBe('editor')

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

describe('rail side preference', () => {
  it('keeps the rail on the right by default', async () => {
    const wrapper = await openDemoWorkspace()
    expect(wrapper.find('.app-shell').classes()).not.toContain('rail-left')
    expect(wrapper.find('.app-rail').exists()).toBe(true)
    wrapper.unmount()
  })

  it('moves the rail to the left when the saved appearance says so', async () => {
    invoke.mockImplementation(async (command: string) =>
      command === 'load_appearance_settings' ? { scale: 100, rail: 'left' } : commandResult(command),
    )
    const wrapper = await openDemoWorkspace()
    expect(wrapper.find('.app-shell').classes()).toContain('rail-left')
    expect(wrapper.find('.app-rail').exists()).toBe(true)
    wrapper.unmount()
  })
})
