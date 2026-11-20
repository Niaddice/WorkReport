// @vitest-environment happy-dom
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import HistoryDiffDialog from '../src/components/HistoryDiffDialog.vue'
import HistoryModule from '../src/components/modules/HistoryModule.vue'
import { MODULE_CONTEXT, type ModuleContext } from '../src/modules/context'
import type { HistoryVersion } from '../src/lib/history'
import { clearMessages, messages } from '../src/lib/messages'

const { list, read } = vi.hoisted(() => ({
  list: vi.fn(),
  read: vi.fn(),
}))

vi.mock('../src/lib/history', () => ({
  historyApi: { list, read },
}))

const wrappers: VueWrapper[] = []

beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function showModal(this: HTMLDialogElement) {
    this.setAttribute('open', '')
  }
  HTMLDialogElement.prototype.close = function close(this: HTMLDialogElement) {
    this.removeAttribute('open')
  }
})

afterEach(() => {
  wrappers.splice(0).forEach((wrapper) => wrapper.unmount())
  clearMessages()
  vi.restoreAllMocks()
  document.body.innerHTML = ''
})

describe('HistoryDiffDialog component', () => {
  const baseVersion: HistoryVersion = {
    id: '1731040000000-hash1.md',
    date: '20261108',
    savedAt: 1731040000000,
    format: 'md',
    bytes: 120,
  }

  const otherVersion: HistoryVersion = {
    id: '1731030000000-hash0.md',
    date: '20261108',
    savedAt: 1731030000000,
    format: 'md',
    bytes: 80,
  }

  it('renders diff dialog, loads contents and displays stats and split view', async () => {
    const readContent = vi.fn(async (ver: HistoryVersion) => {
      if (ver.id === baseVersion.id) {
        return '# 今日任务\n- [ ] 任务一\n- [ ] 旧任务二\n- 遗留问题'
      }
      return '# 今日任务\n- 初始项'
    })

    const currentDraft = '# 今日任务\n- [ ] 任务一\n- [x] 新任务二完成\n- 遗留问题\n- 明日计划'

    const wrapper = mount(HistoryDiffDialog, {
      props: {
        baseVersion,
        availableVersions: [baseVersion, otherVersion],
        currentDraftContent: currentDraft,
        readContent,
      },
      attachTo: document.body,
    })
    wrappers.push(wrapper)

    // Initially closed
    expect(wrapper.find('dialog').attributes('open')).toBeUndefined()

    wrapper.vm.open()
    await flushPromises()

    expect(wrapper.find('dialog').attributes('open')).toBeDefined()
    expect(wrapper.text()).toContain('历史版本差异对比')
    expect(wrapper.text()).toContain('2026年11月08日')

    // Read content called for base
    expect(readContent).toHaveBeenCalledWith(baseVersion)

    // Stats
    expect(wrapper.find('.stats-add').text()).toContain('+')
    expect(wrapper.find('.stats-remove').text()).toContain('−')

    // Split view headers
    expect(wrapper.find('.history-diff-split-view').exists()).toBe(true)
    expect(wrapper.text()).toContain('基准历史版本 (旧)')
    expect(wrapper.text()).toContain('当前编辑区草稿 (新)')

    // Toggle view mode to unified
    const unifiedBtn = wrapper.findAll('.mode-btn')[1]!
    await unifiedBtn.trigger('click')
    await flushPromises()

    expect(wrapper.find('.history-diff-unified-view').exists()).toBe(true)

    // Switch comparison target to other historical version
    const select = wrapper.find('.target-select')
    await select.setValue(otherVersion.id)
    await flushPromises()

    expect(readContent).toHaveBeenCalledWith(otherVersion)
  })

  it('shows identical message when contents match', async () => {
    const identicalText = '# 相同内容\n- 任务'
    const readContent = vi.fn(async () => identicalText)

    const wrapper = mount(HistoryDiffDialog, {
      props: {
        baseVersion,
        availableVersions: [baseVersion],
        currentDraftContent: identicalText,
        readContent,
      },
      attachTo: document.body,
    })
    wrappers.push(wrapper)

    wrapper.vm.open()
    await flushPromises()

    expect(wrapper.text()).toContain('内容完全一致')
    expect(wrapper.find('.history-diff-placeholder.identical').exists()).toBe(true)
    expect(wrapper.find('.restore-btn').attributes('disabled')).toBeDefined()
  })

  it('handles restore confirmation flow and emits restore event', async () => {
    const readContent = vi.fn(async () => '# 历史待恢复内容')
    const wrapper = mount(HistoryDiffDialog, {
      props: {
        baseVersion,
        availableVersions: [baseVersion],
        currentDraftContent: '# 当前草稿内容',
        readContent,
      },
      attachTo: document.body,
    })
    wrappers.push(wrapper)

    wrapper.vm.open()
    await flushPromises()

    const restoreBtn = wrapper.find('.restore-btn')
    expect(restoreBtn.exists()).toBe(true)

    // Clicking restore button reveals confirmation prompt
    await restoreBtn.trigger('click')
    expect(wrapper.find('.restore-confirm-popover').exists()).toBe(true)
    expect(wrapper.text()).toContain('覆盖并恢复到此版本？')

    // Confirm
    const confirmBtn = wrapper.find('.btn.danger')
    await confirmBtn.trigger('click')

    expect(wrapper.emitted('restore')).toBeTruthy()
    expect(wrapper.emitted('restore')![0]).toEqual([baseVersion, '# 历史待恢复内容'])
  })
})

describe('HistoryModule diff integration', () => {
  it('opens diff dialog when clicking diff button on row and restores to editor', async () => {
    const v1: HistoryVersion = {
      id: 'v1',
      date: '20261025',
      savedAt: 1792886400000,
      format: 'md',
      bytes: 200,
    }
    list.mockResolvedValue([v1])
    read.mockResolvedValue('恢复的历史版本文本')

    const mockEdit = vi.fn()
    const mockFlush = vi.fn(async () => true)
    const context = {
      journal: {
        selectedDate: ref('20261025'),
        workspace: ref({ path: '/tmp/history-test', name: 'history-test' }),
        busy: ref(false),
        entries: ref([]),
        document: ref({ date: '20261025', format: 'md', content: '现存内容', filename: '20261025.md' }),
        draft: ref('当前正在编辑的未保存草稿'),
        edit: mockEdit,
        flush: mockFlush,
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
    await flushPromises()

    const diffButton = wrapper.find('.history-diff-button')
    expect(diffButton.exists()).toBe(true)
    expect(diffButton.attributes('title')).toContain('差异')

    await diffButton.trigger('click')
    await flushPromises()

    // Dialog should be open
    const dialog = wrapper.findComponent(HistoryDiffDialog)
    expect(dialog.exists()).toBe(true)
    expect(dialog.vm.isOpen()).toBe(true)

    // Trigger restore from dialog
    dialog.vm.$emit('restore', v1, '恢复的历史版本文本')
    await flushPromises()

    expect(mockEdit).toHaveBeenCalledWith('恢复的历史版本文本', true)
    expect(mockFlush).toHaveBeenCalled()
    expect(messages.value.at(-1)?.text).toContain('已成功恢复')
  })
})
