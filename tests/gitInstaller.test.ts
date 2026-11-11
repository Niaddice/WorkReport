// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke, isTauri: () => true }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: async () => null }))

import GitInstaller from '../src/components/GitInstaller.vue'
import GitSettings from '../src/components/GitSettings.vue'
import { cloneGitSettings } from '../src/lib/useGit'

const missing = { installed: false, version: '', path: '', platform: 'windows', method: 'winget' }
beforeEach(() => invoke.mockReset())

describe('Git installer', () => {
  it('offers installation when Git is missing and reports the installed path', async () => {
    invoke.mockImplementation(async (command: string) => {
      if (command === 'detect_git') return missing
      return {
        status: 'installed',
        executable: 'C:\\Program Files\\Git\\cmd\\git.exe',
        version: 'git version 2.50.0',
        message: 'Git 安装完成。',
      }
    })
    const wrapper = mount(GitInstaller, { props: { executable: 'git', disabled: false } })
    await wrapper.vm.detect()
    await flushPromises()
    expect(wrapper.text()).toContain('未检测到可用的 Git')
    expect(wrapper.text()).toContain('winget')
    await wrapper.get('.settings-save-button').trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('install_git')
    expect(wrapper.emitted('installed')).toEqual([['C:\\Program Files\\Git\\cmd\\git.exe']])
    expect(wrapper.text()).toContain('Git 安装完成')
  })

  it('shows macOS system installer progress without changing the executable', async () => {
    invoke.mockImplementation(async (command: string) =>
      command === 'detect_git'
        ? { ...missing, platform: 'macos', method: 'xcode' }
        : { status: 'pending', executable: '', version: '', message: '已打开系统的安装窗口' },
    )
    const wrapper = mount(GitInstaller, { props: { executable: 'git', disabled: false } })
    await wrapper.vm.detect()
    await flushPromises()
    await wrapper.get('.settings-save-button').trigger('click')
    await flushPromises()
    expect(wrapper.emitted('installed')).toBeUndefined()
    expect(wrapper.text()).toContain('已打开系统的安装窗口')
  })

  it('reports installation failures and blocks unsupported systems', async () => {
    invoke.mockImplementation(async (command: string) => {
      if (command === 'detect_git') return missing
      throw 'Git 安装失败：网络错误'
    })
    const wrapper = mount(GitInstaller, { props: { executable: 'git', disabled: false } })
    await wrapper.vm.detect()
    await flushPromises()
    await wrapper.get('.settings-save-button').trigger('click')
    await flushPromises()
    expect(wrapper.get('[role="alert"]').text()).toContain('网络错误')

    invoke.mockResolvedValue({ ...missing, platform: 'other', method: 'unavailable' })
    await wrapper.findAll('button')[1].trigger('click')
    await flushPromises()
    expect(wrapper.get<HTMLButtonElement>('.settings-save-button').element.disabled).toBe(true)
  })

  it('fills the Git program path of the settings draft after installation', async () => {
    invoke.mockImplementation(async (command: string) =>
      command === 'detect_git'
        ? { ...missing, platform: 'macos', method: 'homebrew' }
        : { status: 'installed', executable: '/opt/homebrew/bin/git', version: 'git version 2.50.0', message: 'ok' },
    )
    const wrapper = mount(GitSettings, {
      props: { modelValue: cloneGitSettings(), busy: false, workspacePath: '/tmp/workspace' },
    })
    wrapper.get<HTMLDetailsElement>('.git-advanced').element.open = true
    await wrapper.get('.git-advanced').trigger('toggle')
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('detect_git', { executable: 'git' })
    await wrapper.get('.git-installer .settings-save-button').trigger('click')
    await flushPromises()
    expect(wrapper.emitted('update:modelValue')!.at(-1)![0]).toMatchObject({ executable: '/opt/homebrew/bin/git' })
  })
})
