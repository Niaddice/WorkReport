// @vitest-environment happy-dom
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import PolishDiffDialog from '../src/components/PolishDiffDialog.vue'

describe('PolishDiffDialog', () => {
  let wrappers: ReturnType<typeof mount>[] = []

  beforeEach(() => {
    wrappers = []
  })

  afterEach(() => {
    wrappers.forEach((w) => w.unmount())
    wrappers = []
    document.body.innerHTML = ''
    vi.restoreAllMocks()
  })

  function mountDialog(props: {
    visible: boolean
    originalText: string
    polishedText: string
    isSelection: boolean
    optionLabel?: string
  }) {
    const wrapper = mount(PolishDiffDialog, {
      attachTo: document.body,
      props,
    })
    wrappers.push(wrapper)
    return wrapper
  }

  it('renders diff comparison and character stats correctly', async () => {
    const wrapper = mountDialog({
      visible: true,
      originalText: '今天完成了接口联调\n修复了2个已知缺陷',
      polishedText: '今天顺利完成系统接口联调，\n并修复了2个线上缺陷。',
      isSelection: false,
      optionLabel: '商务正式',
    })
    await flushPromises()

    const dialog = document.querySelector('#polish-diff-dialog')
    expect(dialog).not.toBeNull()
    expect(document.querySelector('.polish-badge.scope')?.textContent).toBe('全文润色')
    expect(document.querySelector('.polish-badge.option')?.textContent).toBe('商务正式')

    // Character stats check
    const charStat = document.querySelector('.char-stat')?.textContent
    expect(charStat).toContain('原 19 字 → 现 25 字')
    const charDelta = document.querySelector('.char-delta')?.textContent
    expect(charDelta).toContain('增加 6 字')

    // Emits accept when accept button is clicked
    const acceptBtn = document.querySelector<HTMLButtonElement>('.polish-diff-accept')
    acceptBtn?.click()
    expect(wrapper.emitted('accept')).toHaveLength(1)
  })

  it('displays identical state message when texts are the same', async () => {
    mountDialog({
      visible: true,
      originalText: '完全一致的文本',
      polishedText: '完全一致的文本',
      isSelection: true,
      optionLabel: '精炼简短',
    })
    await flushPromises()

    expect(document.querySelector('.polish-badge.scope')?.textContent).toBe('选区润色')
    expect(document.querySelector('.polish-diff-identical')).not.toBeNull()
    expect(document.querySelector('.char-delta')?.textContent).toContain('字数未变')
  })

  it('supports keyboard navigation: Enter to accept, Escape to discard', async () => {
    const wrapper = mountDialog({
      visible: true,
      originalText: '原始文本',
      polishedText: '润色后的更长文本',
      isSelection: false,
    })
    await flushPromises()

    const dialog = document.querySelector<HTMLDialogElement>('#polish-diff-dialog')
    expect(dialog).not.toBeNull()

    // Dispatch Escape keydown
    dialog?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    expect(wrapper.emitted('discard')).toHaveLength(1)

    // Dispatch Enter keydown
    dialog?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    expect(wrapper.emitted('accept')).toHaveLength(1)
  })

  it('copies polished text to clipboard on copy button click', async () => {
    const writeTextSpy = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, 'clipboard', {
      value: {
        writeText: writeTextSpy,
      },
      writable: true,
      configurable: true,
    })

    mountDialog({
      visible: true,
      originalText: '原文',
      polishedText: '已复制的润色结果',
      isSelection: false,
    })
    await flushPromises()

    const copyBtn = document.querySelector<HTMLButtonElement>('.btn-copy')
    expect(copyBtn).not.toBeNull()
    copyBtn?.click()
    await flushPromises()

    expect(writeTextSpy).toHaveBeenCalledWith('已复制的润色结果')
    expect(copyBtn?.textContent).toContain('已复制结果')
  })
})
