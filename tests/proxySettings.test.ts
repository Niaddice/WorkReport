// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { defineComponent } from 'vue'
import ProxySettings from '../src/components/ProxySettings.vue'
import SettingsView from '../src/components/SettingsView.vue'
import { cloneProxySettings, validateProxySettings } from '../src/lib/proxy'
import { cloneDatabaseSettings, DEFAULT_DATABASE_SETTINGS } from '../src/lib/database'
import { cloneGitSettings } from '../src/lib/useGit'
import type { WorkspaceSettings } from '../src/lib/settingsDraft'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke, isTauri: () => true }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: async () => null }))
const select = defineComponent({
  props: { modelValue: String, options: Array, id: String, disabled: Boolean },
  emits: ['update:modelValue'],
  template:
    '<select :id="id" :value="modelValue" :disabled="disabled" @change="$emit(\'update:modelValue\', $event.target.value)"><option v-for="item in options" :value="item.value">{{item.label}}</option></select>',
})
const wrappers: { unmount(): void }[] = []
beforeEach(() => {
  invoke.mockReset()
  invoke.mockImplementation(async (command, args) => {
    if (command === 'load_proxy_settings') return cloneProxySettings()
    if (command === 'save_proxy_settings') return args.settings
    if (command === 'test_proxy_settings') return '连接测试成功'
  })
})
afterEach(() => wrappers.splice(0).forEach((wrapper) => wrapper.unmount()))
function create() {
  const wrapper = mount(ProxySettings, { props: { busy: false }, global: { stubs: { AppSelect: select } } })
  wrappers.push(wrapper)
  return wrapper
}
async function manual(wrapper: ReturnType<typeof create>) {
  await wrapper.get('#proxy-mode').setValue('manual')
  await wrapper.get('#proxy-url').setValue('http://127.0.0.1:7890')
}

it('loads saved settings and commits only through explicit save, retaining errors and edits for retry', async () => {
  invoke.mockResolvedValueOnce({ mode: 'manual', url: 'socks5h://localhost:1080', noProxy: 'localhost' })
  const wrapper = create()
  await flushPromises()
  expect(wrapper.get<HTMLInputElement>('#proxy-url').element.value).toBe('socks5h://localhost:1080')
  expect(wrapper.vm.unchanged).toBe(true)
  await wrapper.get('#proxy-url').setValue('http://127.0.0.1:7890')
  expect(wrapper.vm.unchanged).toBe(false)
  expect(invoke.mock.calls.filter(([cmd]) => cmd === 'save_proxy_settings')).toHaveLength(0)
  invoke.mockRejectedValueOnce('磁盘已满')
  expect(await wrapper.vm.save()).toBe(false)
  expect(wrapper.text()).toContain('磁盘已满')
  expect(wrapper.vm.unchanged).toBe(false)
  expect(await wrapper.vm.save()).toBe(true)
  expect(wrapper.vm.unchanged).toBe(true)
  await wrapper.get('#proxy-mode').setValue('direct')
  wrapper.vm.reset()
  await flushPromises()
  expect(wrapper.get<HTMLSelectElement>('#proxy-mode').element.value).toBe('manual')
  expect(wrapper.vm.unchanged).toBe(true)
})

it('tests the draft without saving and discards a late result after edits', async () => {
  const wrapper = create()
  await flushPromises()
  await manual(wrapper)
  let finish!: (message: string) => void
  invoke.mockImplementationOnce(
    () =>
      new Promise<string>((resolve) => {
        finish = resolve
      }),
  )
  const testButton = () => wrapper.findAll('button').find((button) => button.text().includes('测试'))!
  await testButton().trigger('click')
  expect(invoke).toHaveBeenLastCalledWith('test_proxy_settings', {
    settings: { mode: 'manual', url: 'http://127.0.0.1:7890', noProxy: 'localhost,127.0.0.1,::1' },
  })
  await wrapper.get('#proxy-url').setValue('http://127.0.0.1:7891')
  finish('过期的测试结果')
  await flushPromises()
  expect(wrapper.text()).not.toContain('过期的测试结果')
  expect(wrapper.vm.unchanged).toBe(false)
  expect(invoke.mock.calls.filter(([cmd]) => cmd === 'save_proxy_settings')).toHaveLength(0)
  await testButton().trigger('click')
  await flushPromises()
  expect(wrapper.text()).toContain('连接测试成功')
})

it('blocks invalid manual proxies while allowing a switch back to direct', async () => {
  const wrapper = create()
  await flushPromises()
  await manual(wrapper)
  await wrapper.get('#proxy-url').setValue('http://user:secret@proxy.test:80')
  expect(await wrapper.vm.save()).toBe(false)
  expect(wrapper.text()).toContain('请勿包含账号密码')
  await wrapper.get('#proxy-mode').setValue('direct')
  expect(await wrapper.vm.save()).toBe(true)
})

it('can repair an unreadable configuration by explicitly restoring defaults', async () => {
  invoke.mockRejectedValueOnce('配置无效')
  const wrapper = create()
  await flushPromises()
  expect(wrapper.vm.unchanged).toBe(true)
  expect(wrapper.text()).toContain('配置无效')
  await wrapper
    .findAll('button')
    .find((button) => button.text() === '恢复默认')!
    .trigger('click')
  expect(wrapper.vm.unchanged).toBe(false)
  expect(await wrapper.vm.save()).toBe(true)
  expect(invoke).toHaveBeenLastCalledWith('save_proxy_settings', { settings: cloneProxySettings() })
})

function bundle(): WorkspaceSettings {
  return {
    appearance: { scale: 100, rail: 'right' },
    git: cloneGitSettings(),
    database: cloneDatabaseSettings(DEFAULT_DATABASE_SETTINGS),
    ai: { providers: [], defaultProviderId: null, generationPreference: '', summaryPrompt: '' },
    excel: { enabled: false, templatePath: '', prompt: '' },
    summary: { outputDirectory: 'summary' },
  }
}
it('includes proxy edits in the settings save/discard flow and saves without a workspace', async () => {
  const persist = vi.fn(async () => true)
  const wrapper = mount(SettingsView, {
    props: {
      workspace: null,
      picking: false,
      bundle: bundle(),
      loading: false,
      saving: false,
      databaseBusy: false,
      beforeDatabaseTransfer: async () => true,
      error: '',
      save: persist,
      holidays: { years: [], failures: [] },
      holidayLoading: false,
      holidaySyncing: false,
      holidayError: '',
      currentYear: 2026,
    },
    global: {
      stubs: {
        AppSelect: select,
        DirectorySettings: true,
        DatabaseSettings: true,
        ExcelSettings: true,
        GitSettings: true,
        SummaryOutputSettings: true,
        HolidaySettings: true,
      },
    },
  })
  wrappers.push(wrapper)
  await flushPromises()
  await wrapper.get('[aria-controls="settings-panel-proxy"]').trigger('click')
  await wrapper.get('#proxy-mode').setValue('direct')
  expect(wrapper.vm.unchanged).toBe(false)
  const button = wrapper.get('.settings-savebar button')
  expect(button.attributes('disabled')).toBeUndefined()
  await button.trigger('click')
  await flushPromises()
  expect(invoke).toHaveBeenLastCalledWith('save_proxy_settings', {
    settings: { ...cloneProxySettings(), mode: 'direct' },
  })
  expect(persist).not.toHaveBeenCalled()
  expect(wrapper.vm.unchanged).toBe(true)
  await wrapper.get('#proxy-mode').setValue('system')
  wrapper.vm.reset()
  await flushPromises()
  expect(wrapper.get<HTMLSelectElement>('#proxy-mode').element.value).toBe('direct')
  expect(wrapper.vm.unchanged).toBe(true)
})

describe('proxy input validation', () => {
  it.each(['http://localhost:7890', 'https://proxy.test:8443', 'socks5://127.0.0.1:1080', 'socks5h://[::1]:1080'])(
    'supports %s',
    (url) => {
      expect(validateProxySettings({ mode: 'manual', url, noProxy: '' })).toBe('')
    },
  )
  it.each([
    'localhost:7890',
    'ftp://proxy.test',
    'socks5://localhost',
    'http://proxy.test/path',
    'http://proxy.test:0',
  ])('rejects %s', (url) => {
    expect(validateProxySettings({ mode: 'manual', url, noProxy: '' })).not.toBe('')
  })
})
