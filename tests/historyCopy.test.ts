// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'

const { list, read, copy, isTauri } = vi.hoisted(() => ({
  list: vi.fn(),
  read: vi.fn(),
  copy: vi.fn(),
  isTauri: vi.fn(),
}))
vi.mock('../src/lib/history', () => ({ historyApi: { list, read } }))
vi.mock('../src/lib/workspace', () => ({ api: { copy } }))
vi.mock('@tauri-apps/api/core', () => ({ isTauri }))

import HistoryModule from '../src/components/modules/HistoryModule.vue'
import { MODULE_CONTEXT, type ModuleContext } from '../src/modules/context'
import { clearMessages, messages } from '../src/lib/messages'

const wrappers: VueWrapper[] = []

function mountHistory() {
  const context = {
    journal: {
      selectedDate: ref('20261025'),
      workspace: ref({ path: '/tmp/history-copy', name: 'history-copy' }),
    },
    rangeDates: ref<string[]>([]),
    rangeLoading: ref(false),
  } as unknown as ModuleContext
  const wrapper = mount(HistoryModule, {
    attachTo: document.body,
    global: { provide: { [MODULE_CONTEXT as symbol]: context } },
  })
  wrappers.push(wrapper)
  return { wrapper, context }
}

beforeEach(() => {
  list.mockReset().mockResolvedValue([
    { id: 'v2', date: '20261025', savedAt: 1792886400000, format: 'md', bytes: 200 },
    { id: 'v1', date: '20261025', savedAt: 1792886300000, format: 'txt', bytes: 100 },
  ])
  read.mockReset().mockImplementation(async (_date, id) => (id === 'v1' ? '旧版原文\n第二行' : '# 新版原文'))
  copy.mockReset().mockResolvedValue(undefined)
  isTauri.mockReset().mockReturnValue(true)
  clearMessages()
})

afterEach(() => {
  wrappers.splice(0).forEach((wrapper) => wrapper.unmount())
  clearMessages()
  vi.restoreAllMocks()
  document.body.innerHTML = ''
})

describe('history version copy', () => {
  it('copies the chosen historical version verbatim and confirms success on that row', async () => {
    const { wrapper } = mountHistory()
    await flushPromises()
    const buttons = wrapper.findAll('.history-copy-button')
    expect(buttons).toHaveLength(2)
    expect(buttons[1]!.attributes('aria-label')).toContain('TXT 版本')

    await buttons[1]!.trigger('click')
    await flushPromises()

    expect(read).toHaveBeenCalledExactlyOnceWith('20261025', 'v1')
    expect(copy).toHaveBeenCalledExactlyOnceWith('旧版原文\n第二行')
    expect(buttons[1]!.classes()).toContain('is-copied')
    expect(buttons[0]!.classes()).not.toContain('is-copied')
    expect(buttons[1]!.attributes('title')).toBe('已复制')
    expect(messages.value.at(-1)?.text).toBe('历史版本已复制到剪贴板')
  })

  it('shares a pending preview read with copying and prevents repeated submissions', async () => {
    let resolveRead!: (content: string) => void
    read.mockReturnValue(new Promise<string>((resolve) => (resolveRead = resolve)))
    const { wrapper } = mountHistory()
    await flushPromises()
    await wrapper.get('.history-module-item').trigger('mouseenter', { clientX: 120, clientY: 180 })
    await flushPromises()

    const button = wrapper.get('.history-copy-button')
    await button.trigger('click')
    await button.trigger('click')
    expect(button.attributes('aria-busy')).toBe('true')
    expect(copy).not.toHaveBeenCalled()
    resolveRead('预览中的历史原文')
    await flushPromises()

    expect(read).toHaveBeenCalledTimes(1)
    expect(copy).toHaveBeenCalledExactlyOnceWith('预览中的历史原文')
    expect(document.body.querySelector('.history-tooltip')).toBeNull()
    expect(button.attributes('disabled')).toBeUndefined()
  })

  it('uses the browser clipboard when outside Tauri and supports empty historical content', async () => {
    isTauri.mockReturnValue(false)
    read.mockResolvedValue('')
    const writeText = vi.spyOn(navigator.clipboard, 'writeText').mockResolvedValue()
    const { wrapper } = mountHistory()
    await flushPromises()
    await wrapper.get('.history-copy-button').trigger('click')
    await flushPromises()

    expect(writeText).toHaveBeenCalledExactlyOnceWith('')
    expect(copy).not.toHaveBeenCalled()
    expect(wrapper.get('.history-copy-button').classes()).toContain('is-copied')
  })

  it('reports read failures and allows retry without copying the wrong content', async () => {
    read.mockRejectedValueOnce(new Error('历史版本读取失败'))
    const { wrapper } = mountHistory()
    await flushPromises()
    const button = wrapper.get('.history-copy-button')
    await button.trigger('click')
    await flushPromises()

    expect(copy).not.toHaveBeenCalled()
    expect(messages.value.at(-1)?.tone).toBe('error')
    expect(messages.value.at(-1)?.text).toContain('历史版本读取失败')
    expect(button.classes()).not.toContain('is-copied')
    expect(button.attributes('disabled')).toBeUndefined()

    await button.trigger('click')
    await flushPromises()
    expect(copy).toHaveBeenCalledExactlyOnceWith('# 新版原文')
  })

  it('reports clipboard failure without showing a success state', async () => {
    copy.mockRejectedValueOnce(new Error('剪贴板不可用'))
    const { wrapper } = mountHistory()
    await flushPromises()
    await wrapper.get('.history-copy-button').trigger('click')
    await flushPromises()

    expect(messages.value.at(-1)?.tone).toBe('error')
    expect(messages.value.at(-1)?.text).toContain('剪贴板不可用')
    expect(wrapper.get('.history-copy-button').classes()).not.toContain('is-copied')
    expect(wrapper.get('.history-copy-button').attributes('disabled')).toBeUndefined()
  })

  it('discards pending copies and cached reads when switching workspaces', async () => {
    let resolveOldRead!: (content: string) => void
    read.mockReturnValueOnce(new Promise<string>((resolve) => (resolveOldRead = resolve)))
    const { wrapper, context } = mountHistory()
    await flushPromises()
    await wrapper.get('.history-copy-button').trigger('click')

    context.journal.workspace.value = { path: '/tmp/other-history', name: 'other-history' }
    await flushPromises()
    resolveOldRead('旧工作空间的内容')
    await flushPromises()
    expect(copy).not.toHaveBeenCalled()

    read.mockResolvedValueOnce('新工作空间的内容')
    await wrapper.get('.history-copy-button').trigger('click')
    await flushPromises()
    expect(read).toHaveBeenCalledTimes(2)
    expect(copy).toHaveBeenCalledExactlyOnceWith('新工作空间的内容')
  })

  it('does not start a stale hover read after its workspace has changed', async () => {
    const { wrapper, context } = mountHistory()
    await flushPromises()
    wrapper.get('.history-module-item').element.dispatchEvent(new MouseEvent('mouseenter'))
    context.journal.workspace.value = { path: '/tmp/other-history', name: 'other-history' }
    await flushPromises()

    expect(read).not.toHaveBeenCalled()
    expect(document.body.querySelector('.history-tooltip')).toBeNull()
    await wrapper.get('.history-copy-button').trigger('click')
    await flushPromises()
    expect(read).toHaveBeenCalledExactlyOnceWith('20261025', 'v2')
    expect(copy).toHaveBeenCalledExactlyOnceWith('# 新版原文')
  })
})
