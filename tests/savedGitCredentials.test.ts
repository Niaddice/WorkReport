// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke, isTauri: () => true }))
import SavedGitCredentials from '../src/components/SavedGitCredentials.vue'

beforeEach(() => invoke.mockReset())
describe('saved Git credentials management', () => {
  it('loads metadata only on request and removes one server without requesting its password', async () => {
    invoke.mockImplementation(async (command: string) =>
      command === 'list_git_credentials'
        ? [
            { url: 'https://one.example.test', username: 'one-user' },
            { url: 'https://two.example.test', username: 'two-user' },
          ]
        : undefined,
    )
    const wrapper = mount(SavedGitCredentials, { props: { disabled: false } })
    try {
      expect(invoke).not.toHaveBeenCalled()
      await wrapper.get('button').trigger('click')
      await flushPromises()
      expect(wrapper.text()).toContain('one-user')
      await wrapper.get('[aria-label="清除 https://one.example.test 的 Git 凭据"]').trigger('click')
      await flushPromises()
      expect(invoke).toHaveBeenCalledWith('forget_git_credential', { origin: 'https://one.example.test' })
      expect(wrapper.text()).not.toContain('one-user')
      expect(wrapper.text()).toContain('two-user')
    } finally {
      wrapper.unmount()
    }
  })
  it('allows clearing a damaged file after explicit confirmation', async () => {
    invoke.mockRejectedValueOnce('凭据文件损坏').mockResolvedValue(undefined)
    const wrapper = mount(SavedGitCredentials, { props: { disabled: false } })
    try {
      await wrapper.get('button').trigger('click')
      await flushPromises()
      expect(wrapper.get('[role="alert"]').text()).toContain('文件损坏')
      await wrapper
        .findAll('button')
        .find((button) => button.text() === '清除全部已保存凭据')!
        .trigger('click')
      expect(invoke).toHaveBeenCalledTimes(1)
      await wrapper
        .findAll('button')
        .find((button) => button.text() === '确认清除全部')!
        .trigger('click')
      await flushPromises()
      expect(invoke).toHaveBeenCalledWith('forget_git_credential', { origin: null })
      expect(wrapper.text()).toContain('尚未保存 Git 凭据')
    } finally {
      wrapper.unmount()
    }
  })
})
