// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import JournalEditor from '../src/components/JournalEditor.vue'

const wrappers: VueWrapper[] = []
afterEach(() => {
  wrappers.splice(0).forEach((wrapper) => wrapper.unmount())
  document.body.innerHTML = ''
})

function mountEditor(content = '今天完成接口联调', format: 'md' | 'txt' = 'md') {
  const wrapper = mount(JournalEditor, {
    attachTo: document.body,
    props: {
      date: '20261026',
      document: { date: '20261026', filename: `20261026.${format}`, format, content, revision: '1' },
      content,
      isNew: false,
      dirty: false,
      saving: false,
      disabled: false,
      error: '',
      savedAt: null,
      canUndo: false,
      canRedo: false,
      canRestore: false,
      preview: false,
      onEdit: (value: string) => {
        void wrapper.setProps({ content: value })
      },
      'onUpdate:preview': (value: boolean) => {
        void wrapper.setProps({ preview: value })
      },
    },
  })
  wrappers.push(wrapper)
  return wrapper
}

function menu() {
  return document.querySelector<HTMLElement>('.markdown-tools-menu')
}
function menuButton(label: string) {
  return Array.from(menu()!.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')).find((item) =>
    item.textContent?.includes(label),
  )!
}

describe('journal Markdown tools', () => {
  it('applies a format as one independent history operation and restores the selection', async () => {
    const wrapper = mountEditor()
    const input = wrapper.get<HTMLTextAreaElement>('textarea').element
    input.setSelectionRange(4, 8)
    await wrapper.get('[aria-label="加粗"]').trigger('click')
    await flushPromises()
    expect(wrapper.emitted('edit')).toEqual([['今天完成**接口联调**', true]])
    expect(input.value.slice(input.selectionStart, input.selectionEnd)).toBe('接口联调')
    expect(document.activeElement).toBe(input)
  })

  it('keeps ordinary typing in the normal grouped history', async () => {
    const wrapper = mountEditor('')
    await wrapper.get('textarea').setValue('正常输入')
    expect(wrapper.emitted('edit')).toEqual([['正常输入']])
  })

  it('formats the remembered selection when returning from preview', async () => {
    const wrapper = mountEditor()
    const input = wrapper.get<HTMLTextAreaElement>('textarea').element
    input.setSelectionRange(4, 8)
    await wrapper.get('textarea').trigger('select')
    await wrapper.get('.preview-toggle').trigger('click')
    expect(wrapper.props('preview')).toBe(true)
    await wrapper.get('[aria-label="加粗"]').trigger('click')
    await flushPromises()
    expect(wrapper.props('preview')).toBe(false)
    expect(wrapper.props('content')).toBe('今天完成**接口联调**')
    expect(input.value.slice(input.selectionStart, input.selectionEnd)).toBe('接口联调')
  })

  it('teleports the extra tools outside the clipping panel and inserts one selected action', async () => {
    const wrapper = mountEditor('')
    await wrapper.get('.markdown-tools-trigger').trigger('click')
    await flushPromises()
    expect(menu()).not.toBeNull()
    expect(wrapper.element.contains(menu())).toBe(false)
    expect(menu()!.style.position).toBe('fixed')
    expect(Array.from(menu()!.querySelectorAll('button')).map((item) => item.textContent)).toEqual([
      '斜体',
      '删除线',
      '有序列表',
      '引用',
      '行内代码',
      '代码块',
      '链接',
      '分隔线',
      '表格',
    ])
    menuButton('表格').click()
    await flushPromises()
    expect(wrapper.emitted('edit')).toHaveLength(1)
    expect(wrapper.emitted('edit')![0]![1]).toBe(true)
    expect(wrapper.props('content')).toContain('| 项目 | 内容 |')
    expect(menu()).toBeNull()
  })

  it('supports arrow navigation, boundaries and Escape back to the trigger', async () => {
    const wrapper = mountEditor()
    const trigger = wrapper.get<HTMLButtonElement>('.markdown-tools-trigger')
    await trigger.trigger('keydown', { key: 'ArrowDown' })
    await flushPromises()
    expect(document.activeElement).toBe(menuButton('斜体'))
    menu()!.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
    expect(document.activeElement).toBe(menuButton('删除线'))
    menu()!.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true }))
    expect(document.activeElement).toBe(menuButton('表格'))
    menu()!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true }))
    expect(document.activeElement).toBe(menuButton('斜体'))
    menu()!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await flushPromises()
    expect(menu()).toBeNull()
    expect(document.activeElement).toBe(trigger.element)
    expect(trigger.attributes('aria-expanded')).toBe('false')
  })

  it('closes when focus or pointer leaves, the viewport resizes, or an ancestor scrolls', async () => {
    const wrapper = mountEditor()
    for (const event of ['pointerdown', 'focusin', 'resize', 'scroll']) {
      await wrapper.get('.markdown-tools-trigger').trigger('click')
      await flushPromises()
      expect(menu()).not.toBeNull()
      if (event === 'resize') window.dispatchEvent(new Event(event))
      else document.body.dispatchEvent(new Event(event, { bubbles: true }))
      await flushPromises()
      expect(menu()).toBeNull()
    }
  })

  it('does not close while scrolling the menu itself', async () => {
    const wrapper = mountEditor()
    await wrapper.get('.markdown-tools-trigger').trigger('click')
    await flushPromises()
    menu()!.dispatchEvent(new Event('scroll', { bubbles: true }))
    await flushPromises()
    expect(menu()).not.toBeNull()
  })

  it('disables tools during a busy operation and closes an already open menu', async () => {
    const wrapper = mountEditor()
    await wrapper.get('.markdown-tools-trigger').trigger('click')
    await wrapper.setProps({ disabled: true })
    expect(menu()).toBeNull()
    await wrapper.get('[aria-label="加粗"]').trigger('click')
    await wrapper.get('.markdown-tools-trigger').trigger('click')
    expect(wrapper.emitted('edit')).toBeUndefined()
    expect(menu()).toBeNull()
  })

  it('hides Markdown tools for plain text and removes a menu when the format changes', async () => {
    const wrapper = mountEditor()
    await wrapper.get('.markdown-tools-trigger').trigger('click')
    await wrapper.setProps({
      document: { date: '20261026', filename: '20261026.txt', format: 'txt', content: '', revision: '2' },
    })
    expect(wrapper.find('.format-actions').exists()).toBe(false)
    expect(wrapper.find('.markdown-tools-trigger').exists()).toBe(false)
    expect(menu()).toBeNull()
    expect(wrapper.text()).toContain('纯文本编辑')
  })
})
