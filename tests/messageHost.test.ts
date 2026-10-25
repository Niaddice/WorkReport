// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import MessageHost from '../src/components/MessageHost.vue'
import { clearMessages, messages, pushMessage } from '../src/lib/messages'

beforeEach(() => {
  clearMessages()
})
afterEach(() => {
  clearMessages()
})

async function mountHost() {
  const wrapper = mount(MessageHost, { attachTo: document.body })
  await nextTick()
  return wrapper
}

describe('message host', () => {
  it('renders tones, titles and actions above the app', async () => {
    const wrapper = await mountHost()
    pushMessage({ tone: 'success', title: '完成', text: '设置已保存' })
    pushMessage({ tone: 'error', text: '保存失败' })
    await nextTick()

    const messages = wrapper.findAll('.app-message')
    expect(messages).toHaveLength(2)
    expect(messages[0]!.classes()).toContain('is-success')
    expect(messages[0]!.text()).toContain('完成')
    expect(messages[1]!.attributes('role')).toBe('alert')
    wrapper.unmount()
  })

  it('runs actions and dismisses them unless keepOpen is set', async () => {
    const wrapper = await mountHost()
    const run = vi.fn()
    pushMessage({
      tone: 'warning',
      text: '数据库备份待重试',
      duration: 0,
      actions: [{ label: '重试备份', run }],
    })
    await nextTick()
    await wrapper.find('.app-message-action').trigger('click')
    expect(run).toHaveBeenCalledOnce()
    expect(messages.value).toHaveLength(0)

    pushMessage({
      tone: 'info',
      text: '持续消息',
      duration: 0,
      actions: [{ label: '保留', run, keepOpen: true }],
    })
    await nextTick()
    await wrapper.find('.app-message-action').trigger('click')
    expect(messages.value).toHaveLength(1)
    wrapper.unmount()
  })

  it('dismisses through the close button', async () => {
    const wrapper = await mountHost()
    pushMessage({ tone: 'info', text: '可关闭' })
    await nextTick()
    await wrapper.find('.app-message-close').trigger('click')
    expect(messages.value).toHaveLength(0)
    wrapper.unmount()
  })
})
