// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest'
import { DOMWrapper, flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import PreviousJournalButton from '../src/components/PreviousJournalButton.vue'
import type { Document } from '../src/lib/workspace'

const previous: Document = {
  date: '20261023',
  filename: '20261023.md',
  format: 'md',
  content: '# 周五工作\n\n完成接口联调\n',
  revision: 'r1',
}
const wrappers: VueWrapper[] = []
afterEach(() => {
  wrappers.splice(0).forEach((wrapper) => wrapper.unmount())
  document.body.innerHTML = ''
})
function setup(load = vi.fn<() => Promise<Document | null>>().mockResolvedValue(previous)) {
  const wrapper = mount(PreviousJournalButton, {
    attachTo: document.body,
    props: { disabled: false, hasContent: false, sourceKey: '/workspace/20261026/md', load },
  })
  wrappers.push(wrapper)
  return { wrapper, load, trigger: wrapper.get('button') }
}
function dialog() {
  return new DOMWrapper(document.querySelector<HTMLElement>('.previous-journal-popover')!)
}
function button(label: string) {
  return dialog()
    .findAll('button')
    .find((value) => value.text() === label)!
}

describe('bring in a previous journal', () => {
  it('loads only when clicked and requires confirmation before emitting the complete content', async () => {
    const { wrapper, trigger, load } = setup()
    expect(load).not.toHaveBeenCalled()
    await trigger.trigger('click')
    await flushPromises()
    expect(dialog().text()).toContain('2026-10-23')
    expect(dialog().get('pre').text()).toContain('完成接口联调')
    expect(dialog().text()).toContain('确认后填入当前日报并自动保存')
    expect(document.activeElement?.textContent).toBe('取消')
    expect(wrapper.emitted('confirm')).toBeUndefined()
    await button('确认带入').trigger('click')
    expect(wrapper.emitted('confirm')).toEqual([[previous.content]])
    expect(trigger.attributes('aria-expanded')).toBe('false')
  })

  it('warns before replacing existing content and allows canceling without any edit', async () => {
    const { wrapper, trigger } = setup()
    await wrapper.setProps({ hasContent: true })
    await trigger.trigger('click')
    await flushPromises()
    expect(dialog().text()).toContain('将替换当前内容，可通过撤销恢复')
    await button('取消').trigger('click')
    expect(wrapper.emitted('confirm')).toBeUndefined()
    expect(document.activeElement).toBe(trigger.element)
    expect(trigger.attributes('aria-expanded')).toBe('false')
  })

  it('shows an empty state without a confirmation action when no earlier content exists', async () => {
    const { wrapper, trigger } = setup(vi.fn().mockResolvedValue(null))
    await trigger.trigger('click')
    await flushPromises()
    expect(dialog().text()).toContain('未找到当前日期之前有内容的日报')
    expect(button('确认带入')).toBeUndefined()
    await button('关闭').trigger('click')
    expect(wrapper.emitted('confirm')).toBeUndefined()
  })

  it('keeps lookup errors local and supports retrying', async () => {
    const { wrapper, trigger, load } = setup(vi.fn().mockRejectedValueOnce('读取失败').mockResolvedValue(previous))
    await trigger.trigger('click')
    await flushPromises()
    expect(dialog().get('[role="alert"]').text()).toContain('读取上次日报失败')
    expect(button('确认带入')).toBeUndefined()
    await button('重试').trigger('click')
    await flushPromises()
    expect(load).toHaveBeenCalledTimes(2)
    expect(dialog().text()).toContain('2026-10-23')
    expect(wrapper.emitted('confirm')).toBeUndefined()
  })

  it.each(['sourceKey', 'disabled'] as const)('ignores a late lookup after %s changes', async (property) => {
    let resolve!: (value: Document) => void
    const { wrapper, trigger } = setup(
      vi.fn(
        () =>
          new Promise<Document>((done) => {
            resolve = done
          }),
      ),
    )
    await trigger.trigger('click')
    await flushPromises()
    expect(dialog().attributes('aria-busy')).toBe('true')
    expect(button('确认带入')).toBeUndefined()
    await wrapper.setProps(property === 'sourceKey' ? { sourceKey: '/other/20261027/md' } : { disabled: true })
    resolve(previous)
    await flushPromises()
    expect(document.querySelector('.previous-journal-popover')).toBeNull()
    expect(wrapper.emitted('confirm')).toBeUndefined()
  })

  it('dismisses with Escape or an outside click, and does not load while disabled', async () => {
    const { wrapper, trigger, load } = setup()
    await trigger.trigger('click')
    await flushPromises()
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await flushPromises()
    expect(document.querySelector('.previous-journal-popover')).toBeNull()
    expect(document.activeElement).toBe(trigger.element)
    await trigger.trigger('click')
    await flushPromises()
    document.body.dispatchEvent(new Event('pointerdown', { bubbles: true }))
    await flushPromises()
    expect(document.querySelector('.previous-journal-popover')).toBeNull()
    await wrapper.setProps({ disabled: true })
    await trigger.trigger('click')
    expect(load).toHaveBeenCalledTimes(2)
  })
})
