// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { DOMWrapper, flushPromises, mount, type VueWrapper } from '@vue/test-utils'

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
      busy: ref(false),
      entries: ref([]),
    },
    disabled: ref(false),
    actions: { selectReport: vi.fn(async () => {}) },
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

function historyMenu() {
  return new DOMWrapper(document.body.querySelector<HTMLElement>('.history-context-menu')!)
}
function menuItem(label: string) {
  return historyMenu()
    .findAll<HTMLButtonElement>('[role="menuitem"]')
    .find((item) => item.text() === label)!
}

describe('history version context menu', () => {
  it('copies the exact right-clicked version and clears the hover preview while the menu is open', async () => {
    const { wrapper } = mountHistory()
    await flushPromises()
    const items = wrapper.findAll('.history-module-item')
    await items[1]!.trigger('mouseenter', { clientX: 120, clientY: 180 })
    await flushPromises()
    expect(document.body.querySelector('.history-tooltip')).not.toBeNull()

    await items[1]!.trigger('contextmenu', { clientX: 120, clientY: 180 })
    await flushPromises()
    expect(historyMenu().attributes('aria-label')).toBe('历史版本操作')
    expect(
      historyMenu()
        .findAll('[role="menuitem"]')
        .map((item) => item.text()),
    ).toEqual(['预览版本内容', '查看差异对比…', '恢复至此版本…', '复制版本内容', '查看当日日报', '刷新历史版本'])
    expect(document.body.querySelector('.history-tooltip')).toBeNull()
    expect(wrapper.findAll('.history-module-row')[1]!.classes()).toContain('is-context-target')

    await menuItem('复制版本内容').trigger('click')
    await flushPromises()
    expect(copy).toHaveBeenCalledExactlyOnceWith('旧版原文\n第二行')
    expect(read).toHaveBeenCalledExactlyOnceWith('20261025', 'v1')
    expect(wrapper.findAll('.history-copy-button')[1]!.classes()).toContain('is-copied')
    expect(document.body.querySelector('.history-context-menu')).toBeNull()
    expect(document.body.querySelector('.history-tooltip')).toBeNull()
  })

  it('keeps the chosen menu preview when closing the menu moves the pointer over another version', async () => {
    const { wrapper } = mountHistory()
    await flushPromises()
    await wrapper.findAll('.history-module-item')[1]!.trigger('contextmenu')
    await flushPromises()
    await menuItem('预览版本内容').trigger('click')
    await flushPromises()
    expect(document.body.querySelector('.history-tooltip')?.textContent).toContain('旧版原文\n第二行')
    expect(read).toHaveBeenCalledExactlyOnceWith('20261025', 'v1')
    expect(copy).not.toHaveBeenCalled()
    expect(document.body.querySelector('.history-context-menu')).toBeNull()

    await wrapper.findAll('.history-module-item')[1]!.trigger('mouseleave')
    await wrapper.get('.history-module-item').trigger('mouseenter', { clientX: 200, clientY: 230 })
    await flushPromises()
    expect(document.body.querySelector('.history-tooltip')?.textContent).toContain('旧版原文\n第二行')
    expect(read).toHaveBeenCalledExactlyOnceWith('20261025', 'v1')
    expect(document.body.querySelector('[aria-label="关闭历史版本预览"]')).not.toBeNull()
  })

  it('updates the history list after a successful journal save changes its index', async () => {
    const { wrapper, context } = mountHistory()
    await flushPromises()
    expect(wrapper.findAll('.history-module-item')).toHaveLength(2)
    const oldVersions = await list.mock.results[0]!.value
    const updatedVersions = [
      { id: 'v3', date: '20261025', savedAt: 1792886460000, format: 'md', bytes: 300 },
      ...oldVersions,
    ]
    let finishList!: (versions: typeof updatedVersions) => void
    list.mockReturnValueOnce(
      new Promise((resolve) => {
        finishList = resolve
      }),
    )
    context.journal.entries.value = [{ date: '20261025', filename: '20261025.md', format: 'md' }]
    await flushPromises()
    expect(wrapper.findAll('.history-module-item')).toHaveLength(2)
    expect(wrapper.attributes('aria-busy')).toBe('true')
    finishList(updatedVersions)
    await flushPromises()
    expect(wrapper.findAll('.history-module-item')).toHaveLength(3)
    expect(list).toHaveBeenCalledTimes(2)
  })

  it.each(['close', 'Escape', 'outside'])(
    'dismisses a menu preview via %s while allowing its content to scroll',
    async (action) => {
      const { wrapper } = mountHistory()
      await flushPromises()
      await wrapper.get('.history-module-item').trigger('contextmenu')
      await flushPromises()
      await menuItem('预览版本内容').trigger('click')
      await flushPromises()
      const preview = new DOMWrapper(document.body.querySelector<HTMLElement>('.history-tooltip')!)
      expect(preview.attributes('role')).toBe('dialog')
      await preview.get('.history-tooltip-content').trigger('scroll')
      expect(document.body.querySelector('.history-tooltip')).not.toBeNull()
      if (action === 'close') await preview.get('[aria-label="关闭历史版本预览"]').trigger('click')
      else if (action === 'Escape') window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
      else document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
      await flushPromises()
      expect(document.body.querySelector('.history-tooltip')).toBeNull()
    },
  )

  it('opens with Shift+F10, supports keyboard navigation and restores focus without reopening the tooltip', async () => {
    const { wrapper } = mountHistory()
    await flushPromises()
    const item = wrapper.get<HTMLButtonElement>('.history-module-item')
    item.element.focus()
    await item.trigger('keydown', { key: 'F10', shiftKey: true })
    await flushPromises()
    expect(document.activeElement?.textContent).toBe('预览版本内容')
    await historyMenu().trigger('keydown', { key: 'ArrowDown' })
    expect(document.activeElement?.textContent).toBe('查看差异对比…')
    await historyMenu().trigger('keydown', { key: 'Escape' })
    await flushPromises()
    expect(document.body.querySelector('.history-context-menu')).toBeNull()
    expect(document.activeElement).toBe(item.element)
    expect(document.body.querySelector('.history-tooltip')).toBeNull()
  })

  it('routes a version in a selected date range to its own report date', async () => {
    list.mockResolvedValue([
      { id: 'v2', date: '20261025', savedAt: 1792886400000, format: 'md', bytes: 200 },
      { id: 'v1', date: '20261024', savedAt: 1792800000000, format: 'txt', bytes: 100 },
    ])
    const { wrapper, context } = mountHistory()
    context.rangeDates.value = ['20261024', '20261025']
    await flushPromises()
    await wrapper.findAll('.history-module-item')[1]!.trigger('contextmenu')
    await flushPromises()
    await menuItem('查看当日日报').trigger('click')
    await flushPromises()
    expect(context.actions.selectReport).toHaveBeenCalledExactlyOnceWith('20261024')
    expect(copy).not.toHaveBeenCalled()
  })

  it('offers refresh on empty space and invalidates cached version content', async () => {
    const { wrapper } = mountHistory()
    await flushPromises()
    await wrapper.get('.history-copy-button').trigger('click')
    await flushPromises()
    await wrapper.get('.history-module').trigger('contextmenu', { clientX: 50, clientY: 60 })
    await flushPromises()
    expect(
      historyMenu()
        .findAll('[role="menuitem"]')
        .map((item) => item.text()),
    ).toEqual(['刷新历史版本'])
    await menuItem('刷新历史版本').trigger('click')
    await flushPromises()
    expect(list).toHaveBeenCalledTimes(2)
    read.mockResolvedValueOnce('刷新后的版本原文')
    await wrapper.get('.history-copy-button').trigger('click')
    await flushPromises()
    expect(read).toHaveBeenCalledTimes(2)
    expect(copy).toHaveBeenLastCalledWith('刷新后的版本原文')
  })

  it.each(['outside', 'scroll', 'workspace', 'date', 'busy'])('dismisses the menu on %s', async (action) => {
    const { wrapper, context } = mountHistory()
    await flushPromises()
    await wrapper.get('.history-module-item').trigger('contextmenu')
    await flushPromises()
    expect(document.body.querySelector('.history-context-menu')).not.toBeNull()
    if (action === 'outside') document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
    else if (action === 'scroll') await wrapper.get('.history-module-scroll').trigger('scroll')
    else if (action === 'workspace') context.journal.workspace.value = { path: '/tmp/other-history', name: 'other' }
    else if (action === 'date') context.journal.selectedDate.value = '20261024'
    else context.journal.busy.value = true
    await flushPromises()
    expect(document.body.querySelector('.history-context-menu')).toBeNull()
  })

  it('does not execute an old workspace menu even before its close watcher has run', async () => {
    const { wrapper, context } = mountHistory()
    await flushPromises()
    await wrapper.get('.history-module-item').trigger('contextmenu')
    await flushPromises()
    const button = menuItem('复制版本内容')
    context.journal.workspace.value = { path: '/tmp/other-history', name: 'other' }
    button.element.click()
    await flushPromises()
    expect(copy).not.toHaveBeenCalled()
    expect(read).not.toHaveBeenCalled()
  })
})
