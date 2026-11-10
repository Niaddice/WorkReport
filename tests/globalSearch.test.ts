// @vitest-environment happy-dom
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))

vi.mock('@tauri-apps/api/core', () => ({
  invoke,
  isTauri: () => true,
}))

import GlobalSearchDialog from '../src/components/GlobalSearchDialog.vue'
import { searchShortcut } from '../src/lib/shortcuts'
import type { Entry, SearchResultItem } from '../src/lib/workspace'

beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function showModal(this: HTMLDialogElement) {
    this.setAttribute('open', '')
  }
  HTMLDialogElement.prototype.close = function close(this: HTMLDialogElement) {
    this.removeAttribute('open')
  }
})

beforeEach(() => {
  invoke.mockReset()
})

const demoEntries: Entry[] = [
  { date: '20261028', filename: '20261028.md', format: 'md' },
  { date: '20261030', filename: '20261030.md', format: 'md' },
]

describe('global full-text search', () => {
  it('recognizes Cmd+K on macOS and Ctrl+K on other platforms', () => {
    expect(searchShortcut({ key: 'k', metaKey: true, ctrlKey: false, altKey: false, shiftKey: false }, true)).toBe(true)
    expect(searchShortcut({ key: 'k', metaKey: false, ctrlKey: true, altKey: false, shiftKey: false }, false)).toBe(
      true,
    )
    expect(searchShortcut({ key: 'k', metaKey: true, ctrlKey: false, altKey: false, shiftKey: true }, true)).toBe(false)
  })

  it('shows recent journal entries when query is empty and searches journals and files with keyboard navigation', async () => {
    vi.useFakeTimers()
    const mockResults: SearchResultItem[] = [
      {
        kind: 'journal',
        date: '20261030',
        path: '2026/20261030.md',
        title: '2026年10月30日',
        format: 'md',
        snippet: '完成了全局全文检索弹窗与后端索引',
        matchCount: 2,
      },
      {
        kind: 'file',
        date: null,
        path: 'notes/架构说明.md',
        title: '架构说明.md',
        format: 'md',
        snippet: '支持快捷键唤起全文检索',
        matchCount: 1,
      },
    ]
    invoke.mockResolvedValue(mockResults)

    const wrapper = mount(GlobalSearchDialog, {
      props: {
        workspacePath: '/tmp/demo-workspace',
        entries: demoEntries,
        isMac: true,
      },
      attachTo: document.body,
    })

    try {
      wrapper.vm.open()
      await flushPromises()

      // Empty query shows recent journals sorted newest first
      const initialItems = wrapper.findAll('.search-result-item')
      expect(initialItems).toHaveLength(2)
      expect(initialItems[0]!.text()).toContain('2026年10月30日')

      // Type a search query
      const input = wrapper.get('input.search-input')
      await input.setValue('全文检索')
      vi.advanceTimersByTime(150)
      await flushPromises()

      expect(invoke).toHaveBeenCalledWith('search_workspace', {
        workspacePath: '/tmp/demo-workspace',
        query: '全文检索',
        limit: 50,
      })

      const items = wrapper.findAll('.search-result-item')
      expect(items).toHaveLength(2)
      expect(wrapper.findAll('mark.search-highlight').length).toBeGreaterThan(0)

      // Filter by file only
      const tabs = wrapper.findAll('[role="tab"]')
      await tabs[2]!.trigger('click')
      expect(wrapper.findAll('.search-result-item')).toHaveLength(1)
      expect(wrapper.get('.search-result-item').text()).toContain('架构说明.md')

      // Reset to all and navigate with ArrowDown + Enter
      await tabs[0]!.trigger('click')
      const shell = wrapper.get('.search-dialog-shell')
      await shell.trigger('keydown', { key: 'ArrowDown' })
      await shell.trigger('keydown', { key: 'Enter' })

      expect(wrapper.emitted('selectFile')).toEqual([['notes/架构说明.md']])

      // Navigate up to first item (journal) and press Enter
      await shell.trigger('keydown', { key: 'ArrowUp' })
      await shell.trigger('keydown', { key: 'Enter' })

      expect(wrapper.emitted('selectJournal')).toEqual([['20261030']])

      // Search for settings (e.g. '网络代理')
      invoke.mockResolvedValue([])
      await input.setValue('代理')
      vi.advanceTimersByTime(150)
      await flushPromises()

      const settingItems = wrapper.findAll('.search-result-item')
      expect(settingItems.length).toBeGreaterThan(0)
      expect(settingItems[0]!.text()).toContain('网络代理')
      expect(settingItems[0]!.text()).toContain('设置')

      await settingItems[0]!.trigger('click')
      expect(wrapper.emitted('selectSetting')).toEqual([
        [
          {
            type: 'global',
            section: 'proxy',
          },
        ],
      ])

      // Switch to setting tab when empty
      await input.setValue('')
      vi.advanceTimersByTime(150)
      await flushPromises()

      const settingTab = wrapper.findAll('[role="tab"]')[3]!
      await settingTab.trigger('click')
      const allSettingsInTab = wrapper.findAll('.search-result-item')
      expect(allSettingsInTab.length).toBeGreaterThan(5)
    } finally {
      vi.useRealTimers()
      wrapper.unmount()
    }
  })
})
