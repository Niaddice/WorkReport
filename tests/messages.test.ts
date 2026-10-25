// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { clearMessages, dismissMessage, messages, pushMessage } from '../src/lib/messages'

beforeEach(() => {
  clearMessages()
})
afterEach(() => {
  clearMessages()
  vi.useRealTimers()
})

describe('top message store', () => {
  it('pushes messages with tone defaults and caps the stack', () => {
    pushMessage({ text: '已复制' })
    pushMessage({ tone: 'success', text: '设置已保存' })
    pushMessage({ tone: 'warning', text: '数据库备份待重试', duration: 0 })
    pushMessage({ tone: 'error', text: '保存失败' })
    pushMessage({ text: '第五条' })
    expect(messages.value).toHaveLength(4)
    expect(messages.value[0]!.text).toBe('设置已保存')
    expect(messages.value.at(-1)).toMatchObject({ tone: 'info', text: '第五条', duration: 3200 })
  })

  it('refreshes an identical message instead of stacking duplicates', () => {
    const first = pushMessage({ tone: 'error', text: '保存失败' })
    const second = pushMessage({ tone: 'error', text: '保存失败' })
    expect(first).toBe(second)
    expect(messages.value).toHaveLength(1)
  })

  it('dismisses messages and clears pending timers', () => {
    vi.useFakeTimers()
    const id = pushMessage({ text: '提示' })
    expect(messages.value).toHaveLength(1)
    dismissMessage(id)
    expect(messages.value).toHaveLength(0)
    vi.advanceTimersByTime(10_000)
    expect(messages.value).toHaveLength(0)
  })

  it('auto-dismisses transient messages but keeps zero-duration ones', () => {
    vi.useFakeTimers()
    pushMessage({ text: '自动关闭' })
    pushMessage({ tone: 'warning', text: '持续提示', duration: 0 })
    vi.advanceTimersByTime(4000)
    expect(messages.value.map((message) => message.text)).toEqual(['持续提示'])
  })
})
