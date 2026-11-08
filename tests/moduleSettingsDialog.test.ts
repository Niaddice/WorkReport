// @vitest-environment happy-dom
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'

const { invoke, pickDirectory } = vi.hoisted(() => ({ invoke: vi.fn(), pickDirectory: vi.fn() }))

vi.mock('@tauri-apps/api/core', () => ({ invoke, isTauri: () => true }))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ onCloseRequested: async () => () => {}, destroy: async () => {} }),
}))
vi.mock('@tauri-apps/api/event', () => ({ listen: async () => () => {} }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: pickDirectory }))

import App from '../src/App.vue'
import { cloneFileSettings, type FileChangesPlan } from '../src/lib/fileSettings'
import { DEFAULT_DATABASE_SETTINGS } from '../src/lib/database'
import type { WorkspaceSettings } from '../src/lib/settingsDraft'
import { MAXIMIZE_DURATION, RESTORE_DURATION, windowRectTransform } from '../src/lib/windowMotion'

const DEMO_WORKSPACE = '/tmp/workreport-module-settings-demo'

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

const emptyPlan: FileChangesPlan = { token: 'plan-token', directories: [], warnings: [], filenames: [] }

function commandResult(command: string): unknown {
  switch (command) {
    case 'recent_workspace':
      return DEMO_WORKSPACE
    case 'open_workspace':
      return { path: DEMO_WORKSPACE, name: 'workreport-module-settings-demo' }
    case 'list_entries':
    case 'list_previews':
    case 'list_workspace_files':
    case 'list_entry_history':
    case 'list_excel_exports':
      return []
    case 'database_backup_warning':
      return null
    case 'load_holiday_data':
      return { years: [], failures: [] }
    case 'read_entry_history':
      return ''
    case 'load_ai_settings':
      return { providers: [], defaultProviderId: null, generationPreference: '', summaryPrompt: '' }
    case 'load_appearance_settings':
      return { scale: 100 }
    case 'load_excel_settings':
      return { enabled: false, templatePath: '', prompt: '' }
    case 'load_summary_settings':
      return { outputDirectory: 'summary' }
    case 'load_git_settings':
      return { executable: 'git', repositories: [], includeMerges: false, commitPrompt: '' }
    case 'load_file_settings':
      return cloneFileSettings()
    case 'preview_file_settings':
      return emptyPlan
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
  invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    if (command === 'save_workspace_settings') return args!.settings
    return commandResult(command)
  })
})

async function openDemoWorkspace(animated = false): Promise<VueWrapper> {
  const wrapper = mount(App, {
    attachTo: document.body,
    global: { stubs: { teleport: true, ...(animated ? { transition: false } : {}) } },
  })
  await flushPromises()
  await flushPromises()
  return wrapper
}

function panel(wrapper: VueWrapper, id: string) {
  return wrapper.get(`.module-panel[data-module-id="${id}"]`)
}

async function openModuleSettings(wrapper: VueWrapper, id: string) {
  await panel(wrapper, id).get('.module-settings-button').trigger('click')
  await flushPromises()
  await flushPromises()
}

async function addModule(wrapper: VueWrapper, label: string) {
  await wrapper.get(`.app-rail [aria-label="${label}"]`).trigger('click')
  await flushPromises()
}

async function openSettingsPage(wrapper: VueWrapper) {
  await wrapper.get('.app-rail [aria-label="设置"]').trigger('click')
  await flushPromises()
  await flushPromises()
}

function saveCalls() {
  return invoke.mock.calls.filter(([command]) => command === 'save_workspace_settings')
}

function savedSettings(index = 0) {
  return (saveCalls()[index]![1] as { settings: WorkspaceSettings }).settings
}

function motionScene(wrapper: VueWrapper) {
  const gear = panel(wrapper, 'editor').get<HTMLButtonElement>('.module-settings-button').element
  const dialog = wrapper.get<HTMLElement>('.module-settings-dialog').element
  const backdrop = wrapper.get<HTMLElement>('.module-settings-backdrop').element
  const close = wrapper.get<HTMLButtonElement>('.module-settings-close').element
  const gearRect = new DOMRect(1040, 80, 28, 28)
  const panelRect = new DOMRect(100, 100, 800, 480)
  const closeRect = new DOMRect(850, 120, 28, 28)
  gear.getBoundingClientRect = () => gearRect
  dialog.getBoundingClientRect = () => panelRect
  close.getBoundingClientRect = () => closeRect
  const animations: { element: HTMLElement; frames: Keyframe[]; duration: number; finish: () => void }[] = []
  for (const element of [dialog, backdrop]) {
    element.animate = ((frames: Keyframe[], options: KeyframeAnimationOptions) => {
      let finish!: () => void
      let reject!: (cause: unknown) => void
      const finished = new Promise<void>((resolve, fail) => {
        finish = resolve
        reject = fail
      })
      animations.push({ element, frames, duration: Number(options.duration), finish })
      return { finished, cancel: () => reject(new DOMException('Cancelled', 'AbortError')) } as unknown as Animation
    }) as typeof element.animate
  }
  return {
    dialog,
    backdrop,
    animations,
    gearTransform: windowRectTransform(gearRect, panelRect),
    closeTransform: windowRectTransform(closeRect, panelRect),
    async finish() {
      animations.forEach((animation) => animation.finish())
      await flushPromises()
    },
  }
}

describe('module settings motion and dismissal', () => {
  it.each(['top', 'footer', 'escape', 'save'] as const)(
    'expands from the module gear and uses the correct destination for %s',
    async (target) => {
      const wrapper = await openDemoWorkspace(true)
      try {
        const scene = motionScene(wrapper)
        await openModuleSettings(wrapper, 'editor')
        expect(scene.animations[0]).toMatchObject({
          element: scene.dialog,
          duration: MAXIMIZE_DURATION,
          frames: [{ transform: scene.gearTransform }, { opacity: 1 }],
        })
        expect(scene.animations[1]).toMatchObject({
          element: scene.backdrop,
          duration: MAXIMIZE_DURATION,
          frames: [{ opacity: 0 }, { opacity: 1 }],
        })
        await scene.finish()

        await wrapper.get('.module-settings-backdrop').trigger('click')
        expect(wrapper.get('.module-settings-dialog').isVisible()).toBe(true)
        expect(wrapper.find('.module-settings-confirm').exists()).toBe(false)
        if (target === 'escape') {
          window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
        } else if (target === 'save') {
          await wrapper.get('#polish-prompt').setValue('测试保存')
          await wrapper.get('.module-settings-save').trigger('click')
        } else {
          await wrapper
            .get(target === 'top' ? '.module-settings-close' : '.module-settings-actions .text-action')
            .trigger('click')
        }
        await flushPromises()
        expect(scene.animations[2]).toMatchObject({
          element: scene.dialog,
          duration: RESTORE_DURATION,
          frames: [{ opacity: 1 }, { transform: target === 'top' ? scene.closeTransform : scene.gearTransform }],
        })
        expect(scene.animations[3]).toMatchObject({
          element: scene.backdrop,
          frames: [{ opacity: 1 }, { opacity: 0 }],
        })
        expect(wrapper.get('.module-settings-dialog').isVisible()).toBe(true)
        expect(wrapper.get('.module-settings-heading h2').text()).toBe('编辑设置')
        expect(wrapper.find('#polish-prompt').exists()).toBe(true)
        await scene.finish()
        expect(wrapper.get('.module-settings-dialog').isVisible()).toBe(false)
        expect(wrapper.find('#polish-prompt').exists()).toBe(false)
      } finally {
        wrapper.unmount()
      }
    },
  )

  it.each(['top', 'footer'] as const)('retains the %s destination through discard confirmation', async (target) => {
    const wrapper = await openDemoWorkspace(true)
    try {
      const scene = motionScene(wrapper)
      await openModuleSettings(wrapper, 'editor')
      await scene.finish()
      await wrapper.get('#polish-prompt').setValue('临时修改')
      await wrapper.get('.module-settings-backdrop').trigger('click')
      expect(wrapper.find('.module-settings-confirm').exists()).toBe(false)
      await wrapper
        .get(target === 'top' ? '.module-settings-close' : '.module-settings-actions .text-action')
        .trigger('click')
      await wrapper.get('.module-settings-backdrop').trigger('click')
      expect(wrapper.get('.module-settings-confirm').text()).toContain('关闭后将丢失这些修改')
      await wrapper.get('.module-settings-confirm .settings-save-button').trigger('click')
      await flushPromises()
      expect(scene.animations[2]!.frames[1]!.transform).toBe(
        target === 'top' ? scene.closeTransform : scene.gearTransform,
      )
      expect(wrapper.get<HTMLTextAreaElement>('#polish-prompt').element.value).toBe('临时修改')
      await scene.finish()
      await openModuleSettings(wrapper, 'editor')
      await scene.finish()
      expect(wrapper.get<HTMLTextAreaElement>('#polish-prompt').element.value).toBe('')
      expect(saveCalls()).toHaveLength(0)
    } finally {
      wrapper.unmount()
    }
  })
})

describe('module settings entry in module title bars', () => {
  it('keeps only global categories in the settings page', async () => {
    const wrapper = await openDemoWorkspace()
    try {
      await openSettingsPage(wrapper)
      const labels = wrapper.findAll('.settings-nav-item strong').map((item) => item.text())
      expect(labels).toEqual(['工作空间', '外观与字体', '数据存储', '网络代理', '应用更新', '模型配置'])
      for (const removed of ['目录设置', '放假信息', 'Git 配置', '生成提示词', '总结文件', 'Excel报告']) {
        expect(labels).not.toContain(removed)
      }
      expect(wrapper.find('[aria-controls="settings-panel-directories"]').exists()).toBe(false)
      expect(wrapper.find('[aria-controls="settings-panel-holidays"]').exists()).toBe(false)
      expect(wrapper.find('[aria-controls="settings-panel-git"]').exists()).toBe(false)
      expect(wrapper.find('[aria-controls="settings-panel-prompts"]').exists()).toBe(false)
      expect(wrapper.find('[aria-controls="settings-panel-summary"]').exists()).toBe(false)
      expect(wrapper.find('[aria-controls="settings-panel-excel"]').exists()).toBe(false)
    } finally {
      wrapper.unmount()
    }
  })

  it('shows a settings button only for modules with their own settings', async () => {
    const wrapper = await openDemoWorkspace()
    try {
      expect(panel(wrapper, 'calendar').find('.module-settings-button').exists()).toBe(true)
      expect(panel(wrapper, 'editor').find('.module-settings-button').exists()).toBe(true)
      expect(panel(wrapper, 'calendar').get('.module-settings-button').attributes('aria-label')).toBe(
        '打开「日历」设置',
      )

      await addModule(wrapper, '数据概览')
      expect(panel(wrapper, 'overview').find('.module-settings-button').exists()).toBe(false)
      await addModule(wrapper, '工作空间目录')
      expect(panel(wrapper, 'directory').find('.module-settings-button').exists()).toBe(false)
    } finally {
      wrapper.unmount()
    }
  })

  it('opens the calendar settings with holiday sync only', async () => {
    const wrapper = await openDemoWorkspace()
    try {
      await openModuleSettings(wrapper, 'calendar')
      const dialog = wrapper.get('.module-settings-dialog')
      expect(dialog.text()).toContain('日历设置')
      expect(dialog.text()).toContain('放假与调休')
      expect(dialog.find('.holiday-sync-button').exists()).toBe(true)
      const save = dialog.get('.module-settings-save')
      expect(save.text()).toBe('完成')
      await save.trigger('click')
      await flushPromises()
      expect(wrapper.get('.module-settings-dialog').isVisible()).toBe(false)
      expect(saveCalls()).toHaveLength(0)
    } finally {
      wrapper.unmount()
    }
  })

  it('saves editor settings from the title bar entry in one transaction', async () => {
    const wrapper = await openDemoWorkspace()
    try {
      await openModuleSettings(wrapper, 'editor')
      const dialog = wrapper.get('.module-settings-dialog')
      expect(dialog.text()).toContain('编辑设置')
      expect(dialog.text()).toContain('新日报默认格式')
      expect(dialog.find('#journal-filename-template').exists()).toBe(true)

      await dialog.get('#journal-filename-template').setValue('工作_{date}')
      await dialog.get('#polish-prompt').setValue('突出进展')
      const save = dialog.get('.module-settings-save')
      expect(save.attributes('disabled')).toBeUndefined()
      await save.trigger('click')
      await flushPromises()
      await flushPromises()

      expect(saveCalls()).toHaveLength(1)
      expect(savedSettings()).toMatchObject({
        files: { journal: { filenameTemplate: '工作_{date}' } },
        ai: { generationPreference: '突出进展' },
      })
      expect(wrapper.get('.module-settings-dialog').isVisible()).toBe(false)
    } finally {
      wrapper.unmount()
    }
  })

  it('asks before discarding unsaved module settings', async () => {
    const wrapper = await openDemoWorkspace()
    try {
      await openModuleSettings(wrapper, 'editor')
      await wrapper.get('#polish-prompt').setValue('临时修改')
      await wrapper.get('.module-settings-close').trigger('click')
      await flushPromises()
      expect(wrapper.get('.module-settings-confirm').text()).toContain('关闭后将丢失这些修改')
      await wrapper
        .findAll('.module-settings-confirm button')
        .find((item) => item.text() === '继续编辑')!
        .trigger('click')
      await flushPromises()
      expect(wrapper.find('.module-settings-confirm').exists()).toBe(false)
      expect(wrapper.get<HTMLTextAreaElement>('#polish-prompt').element.value).toBe('临时修改')

      await wrapper.get('.module-settings-close').trigger('click')
      await flushPromises()
      await wrapper
        .findAll('.module-settings-confirm button')
        .find((item) => item.text() === '放弃修改')!
        .trigger('click')
      await flushPromises()
      expect(wrapper.get('.module-settings-dialog').isVisible()).toBe(false)
      expect(saveCalls()).toHaveLength(0)

      await openModuleSettings(wrapper, 'editor')
      expect(wrapper.get<HTMLTextAreaElement>('#polish-prompt').element.value).toBe('')
    } finally {
      wrapper.unmount()
    }
  })

  it('saves the history storage limit from the history module settings', async () => {
    const wrapper = await openDemoWorkspace()
    try {
      await addModule(wrapper, '历史记录')
      await openModuleSettings(wrapper, 'history')
      const dialog = wrapper.get('.module-settings-dialog')
      expect(dialog.text()).toContain('历史记录设置')
      await dialog.get('#history-limit').setValue('3')
      await dialog.get('.module-settings-save').trigger('click')
      await flushPromises()
      expect(saveCalls()).toHaveLength(1)
      expect(savedSettings().database.historyLimitBytes).toBe(3 * 1024 * 1024 * 1024)
      expect(wrapper.get('.module-settings-dialog').isVisible()).toBe(false)
    } finally {
      wrapper.unmount()
    }
  })

  it('saves summary directory rules and jumps to the global AI settings', async () => {
    const wrapper = await openDemoWorkspace()
    try {
      await addModule(wrapper, '总结')
      await openModuleSettings(wrapper, 'summary')
      expect(wrapper.get('.module-settings-dialog').text()).toContain('总结设置')

      await wrapper.get('#summary-prompt').setValue('按项目归类')
      await wrapper.get('#summary-save-directory').setValue('归档/总结')
      await wrapper.get('.module-settings-save').trigger('click')
      await flushPromises()
      await flushPromises()
      expect(saveCalls()).toHaveLength(1)
      expect(savedSettings()).toMatchObject({
        ai: { summaryPrompt: '按项目归类' },
        files: { summary: { directory: '归档/总结' } },
      })

      await openModuleSettings(wrapper, 'summary')
      await wrapper
        .findAll('.module-settings-dialog button')
        .find((item) => item.text() === '前往模型配置')!
        .trigger('click')
      await flushPromises()
      await flushPromises()
      expect(wrapper.get('.module-settings-dialog').isVisible()).toBe(false)
      expect(wrapper.get('.settings-dialog').isVisible()).toBe(true)
      expect(wrapper.get('.settings-nav-item[aria-current="page"]').text()).toContain('模型配置')
      expect(wrapper.get('#settings-panel-ai').isVisible()).toBe(true)
    } finally {
      wrapper.unmount()
    }
  })

  it('validates git repositories before saving and applies the module settings', async () => {
    const wrapper = await openDemoWorkspace()
    try {
      await addModule(wrapper, 'Git')
      await openModuleSettings(wrapper, 'git')
      const dialog = wrapper.get('.module-settings-dialog')
      expect(dialog.text()).toContain('Git 设置')

      await dialog.get('.git-repositories-heading button').trigger('click')
      await dialog.get('.module-settings-save').trigger('click')
      await flushPromises()
      expect(saveCalls()).toHaveLength(0)
      expect(dialog.text()).toContain('请检查标出的设置项后重试')

      const fields = dialog.findAll('.git-repository-form input')
      await fields[0]!.setValue('项目仓库')
      await fields[1]!.setValue('/tmp/project')
      await dialog.get('.module-settings-save').trigger('click')
      await flushPromises()
      await flushPromises()
      expect(saveCalls()).toHaveLength(1)
      expect(savedSettings().git.repositories).toMatchObject([{ name: '项目仓库', path: '/tmp/project' }])
      expect(wrapper.get('.module-settings-dialog').isVisible()).toBe(false)
    } finally {
      wrapper.unmount()
    }
  })

  it('saves Excel directory rules from the Excel module settings', async () => {
    const wrapper = await openDemoWorkspace()
    try {
      await addModule(wrapper, 'Excel报告')
      await openModuleSettings(wrapper, 'excel')
      const dialog = wrapper.get('.module-settings-dialog')
      expect(dialog.text()).toContain('Excel 设置')
      expect(dialog.text()).toContain('启用 Excel 输出')
      expect(dialog.find('#excel-save-directory').exists()).toBe(true)
      await dialog.get('#excel-save-directory').setValue('表格')
      await dialog.get('.module-settings-save').trigger('click')
      await flushPromises()
      await flushPromises()
      expect(saveCalls()).toHaveLength(1)
      expect(savedSettings().files!.excel.directory).toBe('表格')
    } finally {
      wrapper.unmount()
    }
  })

  it("maintains each module's own settings options in the left navigation sidebar", async () => {
    const wrapper = await openDemoWorkspace()
    try {
      // 1. 打开编辑设置：左侧导航展示编辑模块自身的设置选项
      await openModuleSettings(wrapper, 'editor')
      const dialog = wrapper.get('.module-settings-dialog')

      expect(dialog.get('.module-settings-sidebar-title').text()).toBe('编辑设置')
      const editorNavItems = dialog.findAll('.module-settings-nav-item')
      expect(editorNavItems.length).toBe(4)
      expect(editorNavItems[0]!.text()).toContain('默认格式')
      expect(editorNavItems[1]!.text()).toContain('保存位置')
      expect(editorNavItems[2]!.text()).toContain('每日骨架')
      expect(editorNavItems[3]!.text()).toContain('日报润色')
      expect(editorNavItems[0]!.classes()).toContain('is-active')

      // 点击“每日骨架”导航项
      await editorNavItems[2]!.trigger('click')
      await flushPromises()
      expect(editorNavItems[2]!.classes()).toContain('is-active')

      // 修改润色提示词，对应的选项项显示未保存标记
      await dialog.get('#polish-prompt').setValue('测试润色提示词修改')
      await flushPromises()
      expect(editorNavItems[3]!.find('.module-settings-dirty-dot').exists()).toBe(true)

      // 保存设置
      await dialog.get('.module-settings-save').trigger('click')
      await flushPromises()
      await flushPromises()
      expect(savedSettings().ai.generationPreference).toBe('测试润色提示词修改')
      expect(wrapper.get('.module-settings-dialog').isVisible()).toBe(false)

      // 2. 打开 Git 设置：左侧导航展示 Git 模块自身的设置选项
      await addModule(wrapper, 'Git')
      await openModuleSettings(wrapper, 'git')
      expect(dialog.get('.module-settings-sidebar-title').text()).toBe('Git 设置')
      const gitNavItems = dialog.findAll('.module-settings-nav-item')
      expect(gitNavItems.length).toBe(4)
      expect(gitNavItems[0]!.text()).toContain('本地仓库')
      expect(gitNavItems[1]!.text()).toContain('提交说明')
      expect(gitNavItems[2]!.text()).toContain('认证凭据')
      expect(gitNavItems[3]!.text()).toContain('高级设置')
      await dialog.get('.module-settings-close').trigger('click')
      await flushPromises()

      // 3. 打开 Excel 设置：左侧导航展示 Excel 模块自身的设置选项
      await addModule(wrapper, 'Excel报告')
      await openModuleSettings(wrapper, 'excel')
      expect(dialog.get('.module-settings-sidebar-title').text()).toBe('Excel 设置')
      const excelNavItems = dialog.findAll('.module-settings-nav-item')
      expect(excelNavItems.length).toBe(2)
      expect(excelNavItems[0]!.text()).toContain('Excel 输出')
      expect(excelNavItems[1]!.text()).toContain('保存位置')
    } finally {
      wrapper.unmount()
    }
  })
})
