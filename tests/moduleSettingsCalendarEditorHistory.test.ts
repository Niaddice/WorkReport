// @vitest-environment happy-dom
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { computed, nextTick, ref, type App, type Component } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'
import CalendarModuleSettings from '../src/components/moduleSettings/CalendarModuleSettings.vue'
import EditorModuleSettings from '../src/components/moduleSettings/EditorModuleSettings.vue'
import HistoryModuleSettings from '../src/components/moduleSettings/HistoryModuleSettings.vue'
import {
  cloneDatabaseSettings,
  DEFAULT_DATABASE_SETTINGS,
  MAX_HISTORY_LIMIT_BYTES,
  MIN_HISTORY_LIMIT_BYTES,
} from '../src/lib/database'
import { cloneFileSettings } from '../src/lib/fileSettings'
import { emptyHolidayState, type HolidayState } from '../src/lib/holidays'
import {
  cloneEditorSettings,
  cloneWorkspaceSettings,
  DEFAULT_EDITOR_SETTINGS,
  type WorkspaceSettings,
} from '../src/lib/settingsDraft'
import { cloneGitSettings } from '../src/lib/useGit'
import { MODULE_SETTINGS_CONTEXT, type ModuleSettingsContext } from '../src/modules/settingsContext'

const GB = 1024 * 1024 * 1024
const wrappers: { unmount(): void }[] = []

beforeAll(() => {
  // AppSelect（放假年份选择）用 ResizeObserver 跟随触发按钮定位，happy-dom 未实现。
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver
})

afterEach(() => {
  wrappers.splice(0).forEach((wrapper) => wrapper.unmount())
})

function bundle(): WorkspaceSettings {
  return {
    appearance: { scale: 100, rail: 'right' },
    ai: { providers: [], defaultProviderId: null, generationPreference: '', summaryPrompt: '' },
    excel: { enabled: false, templatePath: '', prompt: '' },
    summary: { outputDirectory: 'summary' },
    editor: cloneEditorSettings(DEFAULT_EDITOR_SETTINGS),
    database: cloneDatabaseSettings(DEFAULT_DATABASE_SETTINGS),
    git: cloneGitSettings(),
    files: cloneFileSettings(),
  }
}

function syncedHolidays(): HolidayState {
  return {
    years: [
      {
        year: 2026,
        days: [
          { date: '2026-01-01', name: '元旦', isOffDay: true },
          { date: '2026-02-14', name: '春节调休上班', isOffDay: false },
        ],
        syncedAt: '2026-01-02T08:00:00.000Z',
        sourceUrl: 'https://raw.githubusercontent.com/NateScarlet/holiday-cn/master/2026.json',
      },
    ],
    failures: [],
  }
}

function createContext(holidays: HolidayState = emptyHolidayState()) {
  const draft = ref<WorkspaceSettings>(cloneWorkspaceSettings(bundle()))
  const holidayState = ref<HolidayState>(holidays)
  const syncHolidays = vi.fn()
  const openGlobalSettings = vi.fn()
  const context: ModuleSettingsContext = {
    draft,
    workspace: computed(() => ({ path: '/tmp/ws', name: 'ws' })),
    busy: computed(() => false),
    aiReady: computed(() => false),
    holidays: {
      state: computed(() => holidayState.value),
      currentYear: computed(() => 2026),
      loading: computed(() => false),
      syncing: computed(() => false),
      error: computed(() => ''),
    },
    openGlobalSettings,
    syncHolidays,
  }
  return { context, draft, holidayState, syncHolidays, openGlobalSettings }
}

function mountWithContext<T extends Component>(component: T, context: ModuleSettingsContext) {
  const wrapper = mount(component, {
    global: {
      plugins: [
        {
          install(app: App) {
            app.provide(MODULE_SETTINGS_CONTEXT, context)
          },
        },
      ],
    },
  })
  wrappers.push(wrapper)
  return wrapper
}

describe('日历模块设置', () => {
  it('同步按钮按当前年份调用同步动作，并展示该年度的同步状态', async () => {
    const { context, syncHolidays } = createContext(syncedHolidays())
    const wrapper = mountWithContext(CalendarModuleSettings, context)
    await flushPromises()
    expect(wrapper.text()).toContain('2026 年 · 已同步到日历')
    expect(wrapper.text()).toContain('1 天放假 · 1 天调休上班')
    const syncButton = wrapper.findAll('button').find((button) => button.text().includes('同步放假信息'))
    expect(syncButton).toBeDefined()
    await syncButton!.trigger('click')
    expect(syncHolidays).toHaveBeenCalledTimes(1)
    expect(syncHolidays).toHaveBeenCalledWith(2026)
    expect(wrapper.vm.validate()).toBe(true)
  })
})

describe('编辑模块设置', () => {
  it('修改日报文件名规则后写回设置草稿', async () => {
    const { context, draft } = createContext()
    const wrapper = mountWithContext(EditorModuleSettings, context)
    await flushPromises()
    await wrapper.get('#journal-filename-template').setValue('日报-{date}')
    expect(draft.value.files?.journal.filenameTemplate).toBe('日报-{date}')
  })

  it('清空文件名后校验失败并展示错误', async () => {
    const { context } = createContext()
    const wrapper = mountWithContext(EditorModuleSettings, context)
    await flushPromises()
    await wrapper.get('#journal-filename-template').setValue('')
    expect(wrapper.vm.validate()).toBe(false)
    await nextTick()
    expect(wrapper.text()).toContain('请填写文件名规则。')
  })

  it('切换默认格式与编辑润色提示词都写回设置草稿', async () => {
    const { context, draft } = createContext()
    const wrapper = mountWithContext(EditorModuleSettings, context)
    await flushPromises()
    await wrapper.get('input[name="default-entry-format"][value="md"]').setValue(true)
    expect(draft.value.database.defaultFormat).toBe('md')
    await wrapper.get('#polish-prompt').setValue('简洁客观，突出下一步计划。')
    expect(draft.value.ai.generationPreference).toBe('简洁客观，突出下一步计划。')
  })

  it('提示词超过 2000 字时校验失败并展示错误', async () => {
    const { context, draft } = createContext()
    const wrapper = mountWithContext(EditorModuleSettings, context)
    await flushPromises()
    draft.value.ai.generationPreference = '字'.repeat(2001)
    await nextTick()
    expect(wrapper.text()).toContain('日报润色提示词不能超过 2000 字。')
    expect(wrapper.vm.validate()).toBe(false)
  })

  it('未配置默认 AI 时提供前往模型配置的入口', async () => {
    const { context, openGlobalSettings } = createContext()
    const wrapper = mountWithContext(EditorModuleSettings, context)
    await flushPromises()
    expect(wrapper.text()).toContain('尚未配置默认 AI')
    const configureButton = wrapper.findAll('button').find((button) => button.text() === '前往模型配置')
    expect(configureButton).toBeDefined()
    await configureButton!.trigger('click')
    expect(openGlobalSettings).toHaveBeenCalledWith('ai')
  })

  it('已选择默认 AI 时展示其名称', async () => {
    const { context, draft } = createContext()
    draft.value.ai.providers = [
      {
        id: 'main',
        name: 'DeepSeek',
        kind: 'http',
        executable: '',
        model: 'deepseek-chat',
        baseUrl: 'https://example.test/v1',
        apiKey: '',
      },
    ]
    draft.value.ai.defaultProviderId = 'main'
    const wrapper = mountWithContext(EditorModuleSettings, context)
    await flushPromises()
    expect(wrapper.text()).toContain('使用默认 AI：DeepSeek')
  })
})

describe('历史记录模块设置', () => {
  it('滑块输入后把历史上限收敛到合法范围并写回草稿', async () => {
    const { context, draft } = createContext()
    const wrapper = mountWithContext(HistoryModuleSettings, context)
    await flushPromises()
    expect(wrapper.get('output').text()).toBe('1 GB')
    const slider = wrapper.get<HTMLInputElement>('#history-limit')
    slider.element.value = '4'
    await slider.trigger('input')
    expect(draft.value.database.historyLimitBytes).toBe(4 * GB)
    expect(wrapper.get('output').text()).toBe('4 GB')
    slider.element.value = '20'
    await slider.trigger('input')
    expect(draft.value.database.historyLimitBytes).toBe(MAX_HISTORY_LIMIT_BYTES)
    slider.element.value = '0'
    await slider.trigger('input')
    expect(draft.value.database.historyLimitBytes).toBe(MIN_HISTORY_LIMIT_BYTES)
    expect(wrapper.vm.validate()).toBe(true)
  })
})
