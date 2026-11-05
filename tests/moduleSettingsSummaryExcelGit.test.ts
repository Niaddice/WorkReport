// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { computed, nextTick, ref, type Component } from 'vue'
import SummaryModuleSettings from '../src/components/moduleSettings/SummaryModuleSettings.vue'
import ExcelModuleSettings from '../src/components/moduleSettings/ExcelModuleSettings.vue'
import GitModuleSettings from '../src/components/moduleSettings/GitModuleSettings.vue'
import { MODULE_SETTINGS_CONTEXT, type ModuleSettingsContext } from '../src/modules/settingsContext'
import { cloneWorkspaceSettings, type WorkspaceSettings } from '../src/lib/settingsDraft'
import { cloneGitSettings } from '../src/lib/useGit'
import { cloneDatabaseSettings, DEFAULT_DATABASE_SETTINGS } from '../src/lib/database'
import { emptyHolidayState } from '../src/lib/holidays'

// 组件本身不直接调用 Tauri，这里统一 stub，避免桌面 API 在浏览器环境报错。
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: () => false }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: async () => null }))

const wrappers: { unmount(): void }[] = []
afterEach(() => {
  wrappers.splice(0).forEach((wrapper) => wrapper.unmount())
})

/** 完整的工作空间设置；模块组件只读取其中的字段，保持结构与真实保存一致。 */
function bundle(overrides: Partial<WorkspaceSettings> = {}): WorkspaceSettings {
  const base: WorkspaceSettings = {
    appearance: { scale: 100, rail: 'right' },
    git: cloneGitSettings(),
    database: cloneDatabaseSettings(DEFAULT_DATABASE_SETTINGS),
    ai: { providers: [], defaultProviderId: null, generationPreference: '', summaryPrompt: '' },
    excel: { enabled: false, templatePath: '', prompt: '' },
    summary: { outputDirectory: 'summary' },
  }
  return Object.assign(base, overrides)
}

function createContext(overrides: Partial<WorkspaceSettings> = {}) {
  const draft = ref(cloneWorkspaceSettings(bundle(overrides)))
  const openGlobalSettings = vi.fn()
  const syncHolidays = vi.fn()
  const context: ModuleSettingsContext = {
    draft,
    workspace: computed(() => ({ path: '/tmp/workreport-workspace', name: '示例工作空间' })),
    busy: computed(() => false),
    aiReady: computed(() => true),
    holidays: {
      state: computed(() => emptyHolidayState()),
      currentYear: computed(() => 2026),
      loading: computed(() => false),
      syncing: computed(() => false),
      error: computed(() => ''),
    },
    openGlobalSettings,
    syncHolidays,
  }
  return { context, draft, openGlobalSettings, syncHolidays }
}

function mountSettings(component: Component, context: ModuleSettingsContext, attachToBody = false): VueWrapper {
  const wrapper = mount(component, {
    attachTo: attachToBody ? document.body : undefined,
    global: {
      plugins: [
        {
          install(app) {
            app.provide(MODULE_SETTINGS_CONTEXT, context)
          },
        },
      ],
    },
  })
  wrappers.push(wrapper)
  return wrapper
}

type Exposed = { validate: () => boolean; focusInvalid: () => void }

function exposed(wrapper: VueWrapper): Exposed {
  return wrapper.vm as unknown as Exposed
}

/** 默认 AI 已配置时的 provider，用于展示与 Excel 前置条件。 */
function configuredProvider() {
  return {
    id: 'provider-1',
    name: '本机 Claude',
    kind: 'claude' as const,
    executable: 'claude',
    model: 'claude-sonnet',
    baseUrl: '',
    apiKey: '',
  }
}

describe('SummaryModuleSettings', () => {
  it('把框选总结提示词写回草稿，超长时校验失败并给出提示', async () => {
    const { context, draft } = createContext()
    const wrapper = mountSettings(SummaryModuleSettings, context)

    await wrapper.get('#summary-prompt').setValue('按项目归纳成果')
    expect(draft.value.ai.summaryPrompt).toBe('按项目归纳成果')
    expect(wrapper.get('.preference-count').text()).toBe('7 / 2000')

    draft.value.ai.summaryPrompt = 'x'.repeat(2001)
    await nextTick()
    expect(wrapper.text()).toContain('不能超过 2000 字')
    expect(exposed(wrapper).validate()).toBe(false)
  })

  it('未配置默认 AI 时展示提示并打开模型配置', async () => {
    const { context, openGlobalSettings } = createContext()
    const wrapper = mountSettings(SummaryModuleSettings, context)

    expect(wrapper.text()).toContain('尚未配置默认 AI')
    const button = wrapper.findAll('button').find((item) => item.text() === '前往模型配置')
    expect(button).toBeTruthy()
    await button!.trigger('click')
    expect(openGlobalSettings).toHaveBeenCalledWith('ai')
  })

  it('已配置默认 AI 时展示名称，不再显示模型配置按钮', () => {
    const provider = configuredProvider()
    const { context } = createContext({
      ai: { providers: [provider], defaultProviderId: provider.id, generationPreference: '', summaryPrompt: '' },
    })
    const wrapper = mountSettings(SummaryModuleSettings, context)

    expect(wrapper.text()).toContain('使用默认 AI：本机 Claude')
    expect(wrapper.findAll('button').some((item) => item.text() === '前往模型配置')).toBe(false)
  })

  it('提示词超长时 focusInvalid 聚焦提示词输入框', async () => {
    const { context, draft } = createContext()
    const wrapper = mountSettings(SummaryModuleSettings, context, true)
    draft.value.ai.summaryPrompt = 'x'.repeat(2001)
    await nextTick()

    exposed(wrapper).focusInvalid()
    expect(document.activeElement).toBe(wrapper.get('#summary-prompt').element)
  })
})

describe('ExcelModuleSettings', () => {
  const excelBundle = { enabled: true, templatePath: '/tmp/workreport-template.xlsx', prompt: '旧提示词' }

  it('把 Excel 输出提示词写回草稿', async () => {
    const { context, draft } = createContext({ excel: excelBundle })
    const wrapper = mountSettings(ExcelModuleSettings, context)

    await wrapper.get('#excel-output-prompt').setValue('按项目汇总工时，填入 A2 单元格')
    expect(draft.value.excel.prompt).toBe('按项目汇总工时，填入 A2 单元格')
  })

  it('把 Excel 保存目录写回 files 草稿，合法时校验通过', async () => {
    const { context, draft } = createContext({ excel: excelBundle })
    const wrapper = mountSettings(ExcelModuleSettings, context)

    await wrapper.get('#excel-save-directory').setValue('reports/excel')
    expect(draft.value.files?.excel.directory).toBe('reports/excel')
    expect(exposed(wrapper).validate()).toBe(true)
  })

  it('Excel 输出不合法时校验失败，focusInvalid 聚焦模板选择', async () => {
    const { context } = createContext({ excel: { ...excelBundle, templatePath: '' } })
    const wrapper = mountSettings(ExcelModuleSettings, context, true)

    expect(exposed(wrapper).validate()).toBe(false)
    exposed(wrapper).focusInvalid()
    expect(document.activeElement).toBe(wrapper.get('#excel-template').element)
  })
})

describe('GitModuleSettings', () => {
  it('添加仓库后写回草稿，名称为空时校验失败', async () => {
    const { context, draft } = createContext()
    const wrapper = mountSettings(GitModuleSettings, context, true)

    const addButton = wrapper.findAll('button').find((item) => item.text().includes('添加仓库'))
    expect(addButton).toBeTruthy()
    await addButton!.trigger('click')
    await flushPromises()

    expect(draft.value.git.repositories).toHaveLength(1)
    expect(exposed(wrapper).validate()).toBe(false)
    exposed(wrapper).focusInvalid()
    expect(document.activeElement).toBe(wrapper.get('[data-repository-id] input').element)
  })

  it('填写仓库名称和目录后校验通过', async () => {
    const { context, draft } = createContext({
      git: cloneGitSettings({
        executable: 'git',
        repositories: [{ id: 'repo-1', name: '', path: '', author: '', branch: '' }],
        includeMerges: false,
      }),
    })
    const wrapper = mountSettings(GitModuleSettings, context)
    const repository = wrapper.get('[data-repository-id]')
    const [nameInput, pathInput] = repository.findAll('input')

    await nameInput!.setValue('客户管理平台')
    await pathInput!.setValue('/tmp/repos/customer')
    expect(draft.value.git.repositories[0]).toMatchObject({ name: '客户管理平台', path: '/tmp/repos/customer' })
    expect(exposed(wrapper).validate()).toBe(true)
  })
})
