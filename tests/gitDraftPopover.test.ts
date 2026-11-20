// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from 'vitest'
import { DOMWrapper, flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import GitDraftPopover from '../src/components/GitDraftPopover.vue'

const wrappers: VueWrapper[] = []

afterEach(() => {
  wrappers.splice(0).forEach((wrapper) => wrapper.unmount())
  document.body.innerHTML = ''
})

function setup(props: Partial<InstanceType<typeof GitDraftPopover>['$props']> = {}) {
  const anchor = document.createElement('button')
  document.body.appendChild(anchor)

  const wrapper = mount(GitDraftPopover, {
    attachTo: document.body,
    props: {
      anchor,
      opened: true,
      date: '20261108',
      hasContent: false,
      generating: false,
      ...props,
    },
  })
  wrappers.push(wrapper)
  return { wrapper, anchor }
}

function popover() {
  const el = document.querySelector<HTMLElement>('.git-draft-popover')
  if (!el) throw new Error('Popover element not found in DOM')
  return new DOMWrapper(el)
}

describe('GitDraftPopover', () => {
  it('renders popover with date and description when opened', async () => {
    setup({ date: '20261108' })
    await flushPromises()

    const p = popover()
    expect(p.text()).toContain('从 Git 提交生成日报')
    expect(p.text()).toContain('2026年11月08日')
    expect(p.text()).toContain('今日提交记录')
    expect(p.find('.mode-select-wrap').exists()).toBe(false)
  })

  it('hides popover when opened is false', async () => {
    setup({ opened: false })
    await flushPromises()
    expect(document.querySelector('.git-draft-popover')).toBeNull()
  })

  it('emits confirm with default replace mode and close when hasContent is false', async () => {
    const { wrapper } = setup({ hasContent: false })
    await flushPromises()

    const confirmBtn = popover().find('.popover-btn-primary')
    await confirmBtn.trigger('click')

    expect(wrapper.emitted('confirm')).toEqual([[{ mode: 'replace', customPrompt: undefined }]])
    expect(wrapper.emitted('close')).toHaveLength(1)
  })

  it('shows mode selector defaulting to append when hasContent is true', async () => {
    const { wrapper } = setup({ hasContent: true })
    await flushPromises()

    const p = popover()
    expect(p.find('.mode-select-wrap').exists()).toBe(true)
    expect(p.text()).toContain('追加到文末')
    expect(p.text()).toContain('完全替换当前内容')

    // Confirm with default append
    const confirmBtn = p.find('.popover-btn-primary')
    await confirmBtn.trigger('click')

    expect(wrapper.emitted('confirm')).toEqual([[{ mode: 'append', customPrompt: undefined }]])
    expect(wrapper.emitted('close')).toHaveLength(1)
  })

  it('allows switching to replace mode when hasContent is true', async () => {
    const { wrapper } = setup({ hasContent: true })
    await flushPromises()

    const p = popover()
    const replaceRadio = p.find<HTMLInputElement>('input[value="replace"]')
    await replaceRadio.setValue(true)

    const confirmBtn = p.find('.popover-btn-primary')
    await confirmBtn.trigger('click')

    expect(wrapper.emitted('confirm')).toEqual([[{ mode: 'replace', customPrompt: undefined }]])
  })

  it('allows toggling custom prompt and includes prompt in confirm', async () => {
    const { wrapper } = setup()
    await flushPromises()

    const p = popover()
    expect(p.find('.custom-prompt-input').exists()).toBe(false)

    // Expand prompt
    const toggleBtn = p.find('.prompt-toggle-btn')
    await toggleBtn.trigger('click')
    expect(p.find('.custom-prompt-input').exists()).toBe(true)

    // Input prompt
    const textarea = p.find('.custom-prompt-input')
    await textarea.setValue('  突出性能优化与关键缺陷修复  ')

    // Confirm
    const confirmBtn = p.find('.popover-btn-primary')
    await confirmBtn.trigger('click')

    expect(wrapper.emitted('confirm')).toEqual([[{ mode: 'replace', customPrompt: '突出性能优化与关键缺陷修复' }]])
  })

  it('supports Cmd/Ctrl+Enter shortcut to confirm', async () => {
    const { wrapper } = setup()
    await flushPromises()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', metaKey: true }))
    expect(wrapper.emitted('confirm')).toEqual([[{ mode: 'replace', customPrompt: undefined }]])
  })

  it('emits close on Escape key or cancel buttons', async () => {
    const { wrapper } = setup()
    await flushPromises()

    // Header close button
    const closeBtn = popover().find('.popover-close-btn')
    await closeBtn.trigger('click')
    expect(wrapper.emitted('close')).toHaveLength(1)

    // Footer cancel button
    const cancelBtn = popover().find('.popover-btn-secondary')
    await cancelBtn.trigger('click')
    expect(wrapper.emitted('close')).toHaveLength(2)

    // Escape key
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    expect(wrapper.emitted('close')).toHaveLength(3)
  })

  it('displays generating status and disables primary button during generation', async () => {
    const { wrapper } = setup({ generating: true })
    await flushPromises()

    const p = popover()
    expect(p.text()).toContain('AI 正在生成…')
    const confirmBtn = p.find<HTMLButtonElement>('.popover-btn-primary')
    expect(confirmBtn.element.disabled).toBe(true)

    // Clicking primary button while generating does nothing
    await confirmBtn.trigger('click')
    expect(wrapper.emitted('confirm')).toBeUndefined()

    // Cancel link emits cancelGeneration
    const cancelLink = p.find('.cancel-link')
    await cancelLink.trigger('click')
    expect(wrapper.emitted('cancelGeneration')).toHaveLength(1)
  })

  it('automatically closes when generating state finishes while opened', async () => {
    const { wrapper } = setup({ generating: true })
    await flushPromises()

    await wrapper.setProps({ generating: false })
    await flushPromises()

    expect(wrapper.emitted('close')).toHaveLength(1)
  })
})
