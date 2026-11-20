// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { defineComponent, h } from 'vue'
import GithubAuthentication from '../src/components/GithubAuthentication.vue'
import { useGithubLogin, type GithubLoginProgress } from '../src/lib/useGithubLogin'
import { isGithubHttpsOrigin } from '../src/lib/gitAuthentication'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke, isTauri: () => true }))
const waiting: GithubLoginProgress = {
  requestId: 'login-1',
  phase: 'waiting',
  code: 'ABCD-1234',
  username: null,
  message: '请在浏览器登录 GitHub',
}
const authorized: GithubLoginProgress = {
  ...waiting,
  phase: 'authorized',
  code: null,
  username: 'test-user',
  message: 'GitHub 登录成功，授权凭据已加密保存。',
}
const mounted: { unmount(): void }[] = []
beforeEach(() => {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
  invoke.mockReset()
  invoke.mockImplementation(async (command) => {
    if (command === 'start_github_login') return waiting
    if (command === 'read_github_login') return authorized
  })
})
afterEach(async () => {
  mounted.splice(0).forEach((wrapper) => wrapper.unmount())
  await flushPromises()
  vi.useRealTimers()
})
function create() {
  const wrapper = mount(GithubAuthentication, { props: { disabled: false } })
  mounted.push(wrapper)
  return wrapper
}
function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

it('shows a public code, opens the browser on request and authorizes only after the saved native result', async () => {
  const wrapper = create()
  await wrapper.get('.settings-save-button').trigger('click')
  await flushPromises()
  expect(wrapper.text()).toContain('ABCD-1234')
  expect(wrapper.emitted('authorized')).toBeUndefined()
  await wrapper
    .findAll('button')
    .find((button) => button.text() === '复制授权码')!
    .trigger('click')
  expect(invoke).toHaveBeenLastCalledWith('copy_content', { content: 'ABCD-1234' })
  await wrapper
    .findAll('button')
    .find((button) => button.text() === '打开授权页面')!
    .trigger('click')
  expect(invoke).toHaveBeenLastCalledWith('open_github_login_browser')
  await vi.advanceTimersByTimeAsync(1000)
  await flushPromises()
  expect(wrapper.emitted('authorized')).toEqual([[]])
  expect(wrapper.text()).toContain('test-user')
  expect(wrapper.text()).not.toContain('ABCD-1234')
  expect(invoke.mock.calls.filter(([name]) => name === 'read_github_login')).toHaveLength(1)
  await vi.advanceTimersByTimeAsync(5000)
  expect(wrapper.emitted('authorized')).toHaveLength(1)
  expect(invoke.mock.calls.filter(([name]) => name === 'read_github_login')).toHaveLength(1)
})

it('cancels a session returned after its component is unmounted', async () => {
  const pending = deferred<GithubLoginProgress>()
  invoke.mockImplementationOnce(() => pending.promise)
  const wrapper = create()
  await wrapper.get('.settings-save-button').trigger('click')
  wrapper.unmount()
  mounted.splice(mounted.indexOf(wrapper), 1)
  pending.resolve(waiting)
  await flushPromises()
  expect(invoke).toHaveBeenLastCalledWith('cancel_github_login', { requestId: 'login-1' })
  expect(wrapper.emitted('authorized')).toBeUndefined()
  await vi.advanceTimersByTimeAsync(2000)
  expect(invoke.mock.calls.filter(([name]) => name === 'read_github_login')).toHaveLength(0)
})

it('ignores an authorized response arriving after cancellation or a scope change', async () => {
  const pending = deferred<GithubLoginProgress>()
  invoke.mockImplementation(async (command) =>
    command === 'start_github_login' ? waiting : command === 'read_github_login' ? pending.promise : undefined,
  )
  const wrapper = create()
  await wrapper.get('.settings-save-button').trigger('click')
  await flushPromises()
  await vi.advanceTimersByTimeAsync(1000)
  await wrapper.setProps({ disabled: true })
  await flushPromises()
  pending.resolve(authorized)
  await flushPromises()
  expect(invoke.mock.calls.some(([name]) => name === 'cancel_github_login')).toBe(true)
  expect(wrapper.emitted('authorized')).toBeUndefined()
  expect(wrapper.text()).not.toContain('test-user')
})

it('does not authorize when the browser flow fails or persistence is rejected', async () => {
  invoke.mockImplementation(async (command) =>
    command === 'start_github_login'
      ? waiting
      : { ...waiting, phase: 'failed', code: null, message: '应用凭据保存失败' },
  )
  const wrapper = create()
  await wrapper.get('.settings-save-button').trigger('click')
  await flushPromises()
  await vi.advanceTimersByTimeAsync(1000)
  await flushPromises()
  expect(wrapper.emitted('authorized')).toBeUndefined()
  expect(wrapper.get('[role="alert"]').text()).toContain('应用凭据保存失败')
  expect(wrapper.get('.settings-save-button').attributes('disabled')).toBeUndefined()
})

it('reports a missing CLI and permits retry without opening any browser page', async () => {
  invoke.mockRejectedValueOnce('未检测到 GitHub CLI（gh）')
  const wrapper = create()
  await wrapper.get('.settings-save-button').trigger('click')
  await flushPromises()
  expect(wrapper.text()).toContain('未检测到 GitHub CLI')
  expect(wrapper.get('.settings-save-button').attributes('disabled')).toBeUndefined()
  expect(wrapper.emitted('authorized')).toBeUndefined()
})

it('cancels the native session when polling fails instead of leaving an orphan login', async () => {
  invoke.mockImplementation(async (command) => {
    if (command === 'start_github_login') return waiting
    if (command === 'read_github_login') throw new Error('IPC unavailable')
  })
  const wrapper = create()
  await wrapper.get('.settings-save-button').trigger('click')
  await flushPromises()
  await vi.advanceTimersByTimeAsync(1000)
  await flushPromises()
  expect(wrapper.text()).toContain('读取 GitHub 登录状态失败')
  expect(invoke).toHaveBeenLastCalledWith('cancel_github_login', { requestId: 'login-1' })
  expect(wrapper.emitted('authorized')).toBeUndefined()
})

it('waits for a pending start before cancelling and permits a clean next login', async () => {
  const pending = deferred<GithubLoginProgress>()
  let login!: ReturnType<typeof useGithubLogin>
  const Host = defineComponent({
    setup() {
      login = useGithubLogin(() => false, vi.fn())
      return () => h('div')
    },
  })
  const wrapper = mount(Host)
  mounted.push(wrapper)
  invoke.mockImplementationOnce(() => pending.promise)
  const start = login.start()
  const cancel = login.cancel()
  expect(login.busy.value).toBe(true)
  expect(login.cancelling.value).toBe(true)
  pending.resolve(waiting)
  await Promise.all([start, cancel])
  expect(login.busy.value).toBe(false)
  expect(invoke).toHaveBeenLastCalledWith('cancel_github_login', { requestId: 'login-1' })
  await login.start()
  expect(login.progress.value?.requestId).toBe('login-1')
})

describe('browser login host boundaries', () => {
  it.each(['https://github.com', 'https://github.com/', 'https://GITHUB.com:443'])('allows %s', (url) =>
    expect(isGithubHttpsOrigin(url)).toBe(true),
  )
  it.each([
    'http://github.com',
    'https://github.com.evil.test',
    'https://example.test',
    'https://github.com:8443',
    'https://user@github.com',
    'https://github.com/team/repo',
    'https://github.com?host=evil',
    'ssh://github.com',
    'invalid',
  ])('rejects %s', (url) => expect(isGithubHttpsOrigin(url)).toBe(false))
})
