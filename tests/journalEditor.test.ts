// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { DOMWrapper, flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import JournalEditor from '../src/components/JournalEditor.vue'
import RichMarkdownEditor from '../src/components/RichMarkdownEditor.vue'

// Material Web reflects its disabled property; keep the same boolean semantics
// while these DOM tests avoid loading the component's shadow DOM.
if (!customElements.get('md-icon-button')) {
  customElements.define(
    'md-icon-button',
    class extends HTMLElement {
      get disabled() {
        return this.hasAttribute('disabled')
      }
      set disabled(value: boolean) {
        this.toggleAttribute('disabled', value)
        this.tabIndex = value ? -1 : 0
      }
    },
  )
}

const wrappers: VueWrapper[] = []
const resizeCallbacks = new Map<Element, () => void>()
let widthSpy: ReturnType<typeof vi.spyOn>
beforeEach(() => {
  resizeCallbacks.clear()
  widthSpy = vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockImplementation(function (this: HTMLElement) {
    return this.classList.contains('format-actions') ? 200 : 0
  })
  vi.stubGlobal(
    'ResizeObserver',
    class {
      private elements = new Set<Element>()
      constructor(private callback: () => void) {}
      observe(element: Element) {
        this.elements.add(element)
        resizeCallbacks.set(element, this.callback)
      }
      disconnect() {
        for (const element of this.elements) resizeCallbacks.delete(element)
        this.elements.clear()
      }
    },
  )
})
afterEach(() => {
  wrappers.splice(0).forEach((wrapper) => wrapper.unmount())
  widthSpy.mockRestore()
  vi.unstubAllGlobals()
  document.body.innerHTML = ''
})

async function mountEditor(content = '今天完成接口联调', format: 'md' | 'txt' = 'md') {
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
  await flushPromises()
  return wrapper
}

async function resizeToolbar(wrapper: VueWrapper, width: number, gap = 0) {
  const element = wrapper.get<HTMLElement>('.format-actions').element
  Object.defineProperty(element, 'clientWidth', { configurable: true, value: width })
  element.style.columnGap = `${gap}px`
  resizeCallbacks.get(element)!()
  await flushPromises()
}

describe('editor focus after a temporary refresh', () => {
  it.each(['md', 'txt'] as const)(
    'restores %s focus and selection after the refresh disables the editor',
    async (format) => {
      const wrapper = await mountEditor('今天完成接口联调', format)
      const input = wrapper.get<HTMLElement>('.markdown-editor').element
      if (format === 'md') await selectEditorText(wrapper, 2, 6)
      else {
        input.focus()
        ;(input as HTMLTextAreaElement).setSelectionRange(2, 6, 'backward')
      }
      expect(document.activeElement === input).toBe(true)
      await wrapper.setProps({ disabled: true })
      // WebKit blurs disabled inputs; reproduce that native behavior explicitly.
      input.blur()
      input.scrollTop = 120
      await wrapper.setProps({ disabled: false })
      await flushPromises()
      await new Promise((resolve) => requestAnimationFrame(resolve))
      expect(document.activeElement === input).toBe(true)
      expect(input.scrollTop).toBe(120)
      if (format === 'md')
        expect(wrapper.getComponent(RichMarkdownEditor).vm.getSelection()).toEqual({ from: 3, to: 7 })
      else {
        expect((input as HTMLTextAreaElement).selectionStart).toBe(2)
        expect((input as HTMLTextAreaElement).selectionEnd).toBe(6)
        expect((input as HTMLTextAreaElement).selectionDirection).toBe('backward')
      }
    },
  )

  it('keeps focus on a control chosen while the refresh is in progress', async () => {
    const wrapper = await mountEditor()
    await selectEditorText(wrapper, 2, 6)
    await wrapper.setProps({ disabled: true })
    const other = document.createElement('input')
    document.body.append(other)
    other.focus()
    await wrapper.setProps({ disabled: false })
    await flushPromises()
    expect(document.activeElement).toBe(other)
  })

  it('does not focus an editor that was not active before the refresh', async () => {
    const wrapper = await mountEditor()
    await wrapper.setProps({ disabled: true })
    await wrapper.setProps({ disabled: false })
    await flushPromises()
    expect(document.activeElement).toBe(document.body)
  })

  it('does not restore focus after switching document scope', async () => {
    const wrapper = await mountEditor()
    await selectEditorText(wrapper, 2, 6)
    await wrapper.setProps({ disabled: true })
    wrapper.get<HTMLElement>('.markdown-editor').element.blur()
    await wrapper.setProps({ scrollSourceKey: 'another-workspace', disabled: false })
    await flushPromises()
    expect(document.activeElement).toBe(document.body)
  })
})

function menu() {
  return document.querySelector<HTMLElement>('.markdown-tools-menu[aria-label="更多 Markdown 工具"]')
}
function menuButton(label: string) {
  return Array.from(menu()!.querySelectorAll<HTMLButtonElement>('button[role^="menuitem"]')).find((item) =>
    item.textContent?.includes(label),
  )!
}
function popover(role: 'menu' | 'dialog', label: string, context?: string) {
  const element = document.querySelector<HTMLElement>(`[role="${role}"][aria-label="${label}"]`)
  expect(element, context).not.toBeNull()
  return new DOMWrapper(element!)
}
function popupButton(wrapper: DOMWrapper<HTMLElement>, label: string) {
  return wrapper
    .findAll('button')
    .find((button) => button.attributes('aria-label') === label || button.text().trim() === label)!
}
async function selectCell(wrapper: VueWrapper) {
  const editor = wrapper.get<HTMLElement>('.rich-markdown-document').element
  editor.focus()
  const cell = editor.querySelector('td p')!
  const range = document.createRange()
  if (cell.firstChild) range.setStart(cell.firstChild, 0)
  else range.setStart(cell, 0)
  range.collapse(true)
  document.getSelection()!.removeAllRanges()
  document.getSelection()!.addRange(range)
  document.dispatchEvent(new Event('selectionchange'))
  await flushPromises()
}
async function selectEditorText(wrapper: VueWrapper, start: number, end: number) {
  const input = wrapper.get<HTMLElement>('.rich-markdown-document').element
  input.focus()
  const text = input.querySelector('p')!.firstChild!
  const range = document.createRange()
  range.setStart(text, start)
  range.setEnd(text, end)
  document.getSelection()!.removeAllRanges()
  document.getSelection()!.addRange(range)
  document.dispatchEvent(new Event('selectionchange'))
  await flushPromises()
  expect(wrapper.getComponent(RichMarkdownEditor).vm.getSelection()).toEqual({ from: start + 1, to: end + 1 })
  return input
}

describe('journal scroll synchronization', () => {
  it('synchronizes the rich editor and preview in both directions and remeasures after resizing', async () => {
    let resize!: () => void
    vi.stubGlobal(
      'ResizeObserver',
      class {
        constructor(private callback: () => void) {}
        observe(element: HTMLElement) {
          if (element.classList.contains('markdown-preview')) resize = this.callback
        }
        disconnect() {}
      },
    )
    const wrapper = await mountEditor('# 标题\n\n第一段\n\n第二段\n\n第三段')
    try {
      await wrapper.setProps({ scrollSourceKey: 'report' })
      const input = wrapper.get<HTMLElement>('.rich-markdown-scroll')
      const viewport = wrapper.get<HTMLElement>('.markdown-preview')
      Object.defineProperties(input.element, {
        clientWidth: { value: 300 },
        clientHeight: { value: 200 },
        scrollHeight: { value: 1000 },
      })
      Object.defineProperties(viewport.element, {
        clientHeight: { value: 200 },
        scrollHeight: { configurable: true, value: 1200 },
      })
      input.element.scrollTop = 80
      await input.trigger('scroll')
      await wrapper.setProps({ preview: true })
      expect(viewport.element.scrollTop).toBe(100)
      await viewport.trigger('scroll')
      await input.trigger('scroll')
      expect(input.element.scrollTop).toBe(80)
      Object.defineProperty(viewport.element, 'scrollHeight', { value: 1400 })
      resize()
      expect(viewport.element.scrollTop).toBe(120)

      viewport.element.scrollTop = 750
      await viewport.trigger('scroll')
      expect(input.element.scrollTop).toBe(500)
      await input.trigger('scroll')
      expect(viewport.element.scrollTop).toBe(750)
      await wrapper.setProps({ scrollSourceKey: 'another-report' })
      expect(wrapper.get<HTMLElement>('.rich-markdown-scroll').element.scrollTop).toBe(0)
      expect(viewport.element.scrollTop).toBe(0)
    } finally {
      wrapper.unmount()
      wrappers.splice(wrappers.indexOf(wrapper), 1)
      vi.unstubAllGlobals()
    }
  })
})

describe('journal Markdown tools', () => {
  it.each(['md', 'txt'] as const)('adds confirmed previous content as one undoable edit in %s mode', async (format) => {
    const wrapper = await mountEditor('当前草稿', format)
    const previous = {
      date: '20261023',
      filename: '20261023.md',
      format: 'md' as const,
      content: '# 上次日报\n\n原始内容\n',
      revision: 'r1',
    }
    await wrapper.setProps({ loadPreviousReport: vi.fn().mockResolvedValue(previous) })
    await wrapper.get('.editor-body-tools .previous-journal-trigger').trigger('click')
    await flushPromises()
    expect(wrapper.emitted('edit')).toBeUndefined()
    await popupButton(popover('dialog', '带入上次日报'), '确认带入').trigger('click')
    await flushPromises()
    expect(wrapper.emitted('edit')).toEqual([[previous.content, true]])
    expect(wrapper.props('document')?.format).toBe(format)
    expect(wrapper.emitted('format')).toBeUndefined()
    expect(document.activeElement).toBe(wrapper.get(format === 'md' ? '.rich-markdown-document' : 'textarea').element)
    await wrapper.setProps({ filePath: 'notes/readme.md' })
    expect(wrapper.find('.previous-journal-trigger').exists()).toBe(false)
  })

  it('uses layout button widths while module animations scale their painted rectangles', async () => {
    const wrapper = await mountEditor()
    const button = wrapper.get<HTMLElement>('md-icon-button').element
    Object.defineProperty(button, 'offsetWidth', { configurable: true, value: 26 })
    const rect = vi.spyOn(button, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, 0, 52, 52))
    try {
      await resizeToolbar(wrapper, 502, 2)
      expect(wrapper.findAll('[data-markdown-action]')).toHaveLength(18)
      expect(wrapper.find('.markdown-tools-trigger').exists()).toBe(false)
      await resizeToolbar(wrapper, 250, 2)
      expect(wrapper.findAll('[data-markdown-action]')).toHaveLength(8)
      rect.mockReturnValue(new DOMRect(0, 0, 13, 13))
      await resizeToolbar(wrapper, 249, 2)
      expect(wrapper.findAll('[data-markdown-action]')).toHaveLength(7)
    } finally {
      rect.mockRestore()
    }
  })

  it('shows every tool when it fits, hides only the right suffix and restores tools as the toolbar grows', async () => {
    const wrapper = await mountEditor()
    const labels = () => wrapper.findAll('[data-markdown-action]').map((button) => button.attributes('aria-label'))
    const all = [
      '撤销',
      '重做',
      '标题与正文',
      '加粗',
      '插入列表',
      '插入待办',
      '斜体',
      '删除线',
      '有序列表',
      '引用',
      '行内代码',
      '代码块',
      '链接',
      '分隔线',
      '表格',
      '增加缩进',
      '减少缩进',
      '清除格式',
    ]
    await resizeToolbar(wrapper, 502, 2)
    expect(labels()).toEqual(all)
    expect(wrapper.find('.markdown-tools-trigger').exists()).toBe(false)
    await resizeToolbar(wrapper, 501, 2)
    expect(labels()).toEqual(all.slice(0, 16))
    await wrapper.get('.markdown-tools-trigger').trigger('click')
    await flushPromises()
    expect([...menu()!.querySelectorAll('button')].map((button) => button.textContent?.trim())).toEqual(all.slice(16))
    await resizeToolbar(wrapper, 166, 2)
    expect(menu()).toBeNull()
    expect(labels()).toEqual(all.slice(0, 5))
    await resizeToolbar(wrapper, 110, 2)
    expect(labels()).toEqual(all.slice(0, 3))
    await wrapper.get('.markdown-tools-trigger').trigger('click')
    await flushPromises()
    expect([...menu()!.querySelectorAll('button')].map((button) => button.textContent?.trim())).toEqual(all.slice(3))
    await resizeToolbar(wrapper, 25)
    expect(labels()).toEqual([])
    expect(wrapper.find('.markdown-tools-trigger').exists()).toBe(true)
    await resizeToolbar(wrapper, 468)
    expect(labels()).toEqual(all)
    expect(wrapper.find('.markdown-tools-trigger').exists()).toBe(false)
  })

  it('keeps keyboard focus usable when a focused tool or the More trigger is removed by resizing', async () => {
    const wrapper = await mountEditor()
    await selectEditorText(wrapper, 0, 8)
    await wrapper.get('[aria-label="加粗"]').trigger('click')
    await flushPromises()
    await resizeToolbar(wrapper, 500)
    const lastTool = wrapper.get<HTMLElement>('[data-markdown-action="clearFormat"]').element
    lastTool.tabIndex = 0
    lastTool.focus()
    await resizeToolbar(wrapper, 130)
    expect(document.activeElement).toBe(wrapper.get('.markdown-tools-trigger').element)
    await resizeToolbar(wrapper, 500)
    expect(document.activeElement).toBe(wrapper.get('[data-markdown-action="clearFormat"]').element)
    await resizeToolbar(wrapper, 130)
    await wrapper.get('.markdown-tools-trigger').trigger('click')
    await flushPromises()
    window.dispatchEvent(new Event('resize'))
    await flushPromises()
    expect(menu()).toBeNull()
    expect(document.activeElement).toBe(wrapper.get('.markdown-tools-trigger').element)
  })

  it('applies a format as one independent history operation and restores the selection', async () => {
    const wrapper = await mountEditor()
    const input = await selectEditorText(wrapper, 4, 8)
    await wrapper.get('[aria-label="加粗"]').trigger('click')
    await flushPromises()
    expect(wrapper.emitted('edit')).toEqual([['今天完成**接口联调**', true]])
    expect(input.querySelector('strong')?.textContent).toBe('接口联调')
    expect(document.getSelection()?.toString()).toBe('接口联调')
    expect(document.activeElement).toBe(input)
  })

  it('keeps ordinary typing in the normal grouped history', async () => {
    const wrapper = await mountEditor('')
    const input = wrapper.get<HTMLElement>('.rich-markdown-document').element
    input.focus()
    input.querySelector('p')!.textContent = '正常输入'
    input.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: '正常输入' }))
    await flushPromises()
    expect(wrapper.emitted('edit')).toEqual([['正常输入', false]])
  })

  it('formats the remembered selection while keeping both panes visible', async () => {
    const wrapper = await mountEditor()
    const input = await selectEditorText(wrapper, 4, 8)
    await wrapper.get('.journal-split-toggle').trigger('click')
    expect(wrapper.props('preview')).toBe(true)
    await wrapper.get('[aria-label="加粗"]').trigger('click')
    await flushPromises()
    expect(wrapper.props('preview')).toBe(true)
    expect(wrapper.get('.rich-markdown-document').isVisible()).toBe(true)
    expect(wrapper.get('.markdown-preview').isVisible()).toBe(true)
    expect(wrapper.props('content')).toBe('今天完成**接口联调**')
    expect(input.querySelector('strong')?.textContent).toBe('接口联调')
    expect(document.getSelection()?.toString()).toBe('接口联调')
  })

  it('teleports the extra tools outside the clipping panel and inserts one selected action', async () => {
    const wrapper = await mountEditor('')
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
      '增加缩进',
      '减少缩进',
      '清除格式',
    ])
    menuButton('表格').click()
    await flushPromises()
    expect(menu()).toBeNull()
    const tableDialog = popover('dialog', '表格')
    await tableDialog.get('[aria-label="行数"]').setValue('3')
    await tableDialog.get('[aria-label="列数"]').setValue('4')
    await tableDialog.get('form').trigger('submit')
    await flushPromises()
    expect(wrapper.emitted('edit')).toHaveLength(1)
    expect(wrapper.emitted('edit')![0]![1]).toBe(true)
    expect(wrapper.findAll('.rich-markdown-document table tr')).toHaveLength(3)
    expect(wrapper.findAll('.rich-markdown-document table th')).toHaveLength(4)
    expect(wrapper.props('content')).toContain('|')
    expect(menu()).toBeNull()
  })

  it('supports arrow navigation, boundaries and Escape back to the trigger', async () => {
    const wrapper = await mountEditor()
    const trigger = wrapper.get<HTMLButtonElement>('.markdown-tools-trigger')
    await trigger.trigger('keydown', { key: 'ArrowDown' })
    await flushPromises()
    expect(document.activeElement).toBe(menuButton('斜体'))
    menu()!.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }))
    expect(document.activeElement).toBe(menuButton('删除线'))
    menu()!.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true }))
    expect(document.activeElement).toBe([...menu()!.querySelectorAll('button:not(:disabled)')].at(-1))
    menu()!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true }))
    expect(document.activeElement).toBe(menuButton('斜体'))
    menu()!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await flushPromises()
    expect(menu()).toBeNull()
    expect(document.activeElement).toBe(trigger.element)
    expect(trigger.attributes('aria-expanded')).toBe('false')
  })

  it('closes when focus or pointer leaves, the viewport resizes, or an ancestor scrolls', async () => {
    const wrapper = await mountEditor()
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
    const wrapper = await mountEditor()
    await wrapper.get('.markdown-tools-trigger').trigger('click')
    await flushPromises()
    menu()!.dispatchEvent(new Event('scroll', { bubbles: true }))
    await flushPromises()
    expect(menu()).not.toBeNull()
  })

  it('disables tools during a busy operation and closes an already open menu', async () => {
    const wrapper = await mountEditor()
    await wrapper.get('.markdown-tools-trigger').trigger('click')
    await wrapper.setProps({ disabled: true })
    expect(menu()).toBeNull()
    await wrapper.get('[aria-label="加粗"]').trigger('click')
    await wrapper.get('.markdown-tools-trigger').trigger('click')
    expect(wrapper.emitted('edit')).toBeUndefined()
    expect(menu()).toBeNull()
  })

  it('hides Markdown tools for plain text and removes a menu when the format changes', async () => {
    const wrapper = await mountEditor()
    await wrapper.get('.markdown-tools-trigger').trigger('click')
    await wrapper.setProps({
      document: { date: '20261026', filename: '20261026.txt', format: 'txt', content: '', revision: '2' },
    })
    expect(wrapper.find('.format-actions').exists()).toBe(false)
    expect(wrapper.find('.markdown-tools-trigger').exists()).toBe(false)
    expect(menu()).toBeNull()
    expect(wrapper.text()).toContain('纯文本编辑')
  })

  it('reflects active formatting and clears the selected formatting as one history operation', async () => {
    const wrapper = await mountEditor()
    await resizeToolbar(wrapper, 500)
    await selectEditorText(wrapper, 4, 8)
    const bold = wrapper.get('[aria-label="加粗"]')
    expect(bold.attributes('aria-pressed')).toBe('false')
    expect(wrapper.get('[aria-label="清除格式"]').attributes('disabled')).toBeDefined()
    await bold.trigger('click')
    await flushPromises()
    expect(bold.attributes('aria-pressed')).toBe('true')
    expect(bold.classes()).toContain('is-active')
    expect(wrapper.get('[aria-label="清除格式"]').attributes('disabled')).toBeUndefined()
    await wrapper.get('[aria-label="清除格式"]').trigger('click')
    await flushPromises()
    expect(wrapper.props('content')).toBe('今天完成接口联调')
    expect(bold.attributes('aria-pressed')).toBe('false')
    expect(wrapper.find('.rich-markdown-document strong').exists()).toBe(false)
    expect(wrapper.emitted('edit')).toEqual([
      ['今天完成**接口联调**', true],
      ['今天完成接口联调', true],
    ])
  })

  it('uses the journal history flags for toolbar undo and redo', async () => {
    const wrapper = await mountEditor()
    const undo = wrapper.get('[aria-label="撤销"]')
    const redo = wrapper.get('[aria-label="重做"]')
    expect(undo.attributes('disabled')).toBeDefined()
    expect(redo.attributes('disabled')).toBeDefined()
    await undo.trigger('click')
    expect(wrapper.emitted('undo')).toBeUndefined()
    await wrapper.setProps({ canUndo: true, canRedo: true })
    expect(undo.attributes('disabled')).toBeUndefined()
    expect(redo.attributes('disabled')).toBeUndefined()
    await undo.trigger('click')
    await redo.trigger('click')
    expect(wrapper.emitted('undo')).toHaveLength(1)
    expect(wrapper.emitted('redo')).toHaveLength(1)
    expect(wrapper.emitted('edit')).toBeUndefined()
  })

  it('sets each heading level explicitly and returns to paragraph without losing the text selection', async () => {
    const wrapper = await mountEditor()
    await selectEditorText(wrapper, 0, 8)
    for (let level = 1; level <= 6; level++) {
      await wrapper.get('[aria-label="标题与正文"]').trigger('click')
      await flushPromises()
      const headings = popover('menu', '标题与正文', `heading menu for H${level}`)
      expect(headings.findAll('[role="menuitemradio"]')).toHaveLength(7)
      await headings.get(`[aria-label="H${level}"]`).trigger('click')
      await flushPromises()
      expect(wrapper.get(`.rich-markdown-document h${level}`).text()).toBe('今天完成接口联调')
      expect(wrapper.getComponent(RichMarkdownEditor).vm.getSelection()).toEqual({ from: 1, to: 9 })
      expect(wrapper.props('content')).toBe(`${'#'.repeat(level)} 今天完成接口联调`)
      expect(wrapper.get('[aria-label="标题与正文"]').classes()).toContain('is-active')
    }
    await wrapper.get('[aria-label="标题与正文"]').trigger('click')
    await flushPromises()
    await popover('menu', '标题与正文').get('[aria-label="正文"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('.rich-markdown-document p').text()).toBe('今天完成接口联调')
    expect(wrapper.props('content')).toBe('今天完成接口联调')
    expect(wrapper.emitted('edit')).toHaveLength(7)
  })

  it('opens the heading menu from overflow and preserves selection for formatting actions', async () => {
    const wrapper = await mountEditor()
    await selectEditorText(wrapper, 4, 8)
    await resizeToolbar(wrapper, 25)
    await wrapper.get('.markdown-tools-trigger').trigger('click')
    await flushPromises()
    menuButton('标题与正文').click()
    await flushPromises()
    expect(menu()).toBeNull()
    const headingMenu = popover('menu', '标题与正文')
    await headingMenu.get('[aria-label="H3"]').trigger('click')
    await flushPromises()
    expect(wrapper.props('content')).toBe('### 今天完成接口联调')
    expect(wrapper.getComponent(RichMarkdownEditor).vm.getSelection()).toEqual({ from: 5, to: 9 })
    await wrapper.get('.markdown-tools-trigger').trigger('click')
    await flushPromises()
    menuButton('加粗').click()
    await flushPromises()
    expect(wrapper.props('content')).toBe('### 今天完成**接口联调**')
    expect(wrapper.emitted('edit')).toEqual([
      ['### 今天完成接口联调', true],
      ['### 今天完成**接口联调**', true],
    ])
  })

  it('supports heading keyboard navigation and Escape back to the toolbar', async () => {
    const wrapper = await mountEditor()
    const trigger = wrapper.get<HTMLElement>('[aria-label="标题与正文"]')
    await trigger.trigger('keydown', { key: 'ArrowDown' })
    await flushPromises()
    const choices = popover('menu', '标题与正文')
    expect(document.activeElement).toBe(choices.get('[aria-label="正文"]').element)
    await choices.trigger('keydown', { key: 'ArrowDown' })
    expect(document.activeElement).toBe(choices.get('[aria-label="H1"]').element)
    await choices.trigger('keydown', { key: 'End' })
    expect(document.activeElement).toBe(choices.get('[aria-label="H6"]').element)
    await choices.trigger('keydown', { key: 'Home' })
    expect(document.activeElement).toBe(choices.get('[aria-label="正文"]').element)
    await choices.trigger('keydown', { key: 'Escape' })
    await flushPromises()
    expect(document.querySelector('[role="menu"][aria-label="标题与正文"]')).toBeNull()
    expect(document.activeElement).toBe(trigger.element)
    expect(wrapper.emitted('edit')).toBeUndefined()
  })

  it('edits table rows and columns, aligns a complete column and keeps incompatible block tools disabled', async () => {
    const wrapper = await mountEditor('| A | B |\n| --- | --- |\n| C | D |')
    await resizeToolbar(wrapper, 500)
    await selectCell(wrapper)
    for (const action of ['heading', 'bulletList', 'taskList', 'orderedList', 'quote', 'codeBlock']) {
      expect(wrapper.get(`[data-markdown-action="${action}"]`).attributes('disabled')).toBeDefined()
    }
    expect(wrapper.get('[data-markdown-action="bold"]').attributes('disabled')).toBeUndefined()
    const run = async (label: string) => {
      const edits = wrapper.emitted('edit')?.length ?? 0
      await wrapper.get('[aria-label="表格"]').trigger('click')
      await flushPromises()
      const dialog = popover('dialog', '表格')
      await popupButton(dialog, label).trigger('click')
      await flushPromises()
      expect(wrapper.emitted('edit')).toHaveLength(edits + 1)
      expect(wrapper.emitted('edit')?.at(-1)?.[1]).toBe(true)
      expect(document.querySelector('[role="dialog"][aria-label="表格"]')).toBeNull()
    }
    await run('下方插入行')
    expect(wrapper.findAll('.rich-markdown-document table tr')).toHaveLength(3)
    await run('右侧插入列')
    expect(wrapper.findAll('.rich-markdown-document table th')).toHaveLength(3)
    expect(wrapper.findAll('.rich-markdown-document table td')).toHaveLength(6)
    await run('列居中')
    expect(wrapper.props('content')).toMatch(/\|\s*:-+:\s*\|/)
    await run('删除当前列')
    expect(wrapper.findAll('.rich-markdown-document table th')).toHaveLength(2)
    await run('删除当前行')
    expect(wrapper.findAll('.rich-markdown-document table tr')).toHaveLength(2)
    await run('删除表格')
    expect(wrapper.find('.rich-markdown-document table').exists()).toBe(false)
  })

  it('rejects invalid table dimensions and closes action popovers during a busy operation', async () => {
    const wrapper = await mountEditor('')
    await resizeToolbar(wrapper, 500)
    await wrapper.get('[aria-label="表格"]').trigger('click')
    await flushPromises()
    const dialog = popover('dialog', '表格')
    await dialog.get('[aria-label="行数"]').setValue('2.5')
    await dialog.get('form').trigger('submit')
    expect(dialog.get('[role="alert"]').text()).toContain('整数')
    expect(wrapper.emitted('edit')).toBeUndefined()
    await wrapper.setProps({ disabled: true })
    expect(document.querySelector('[role="dialog"][aria-label="表格"]')).toBeNull()
    await wrapper.get('[aria-label="表格"]').trigger('click')
    expect(document.querySelector('[role="dialog"][aria-label="表格"]')).toBeNull()
  })

  it('enables list indentation for a movable item and preserves it through outdent', async () => {
    const wrapper = await mountEditor('- 第一项\n- 第二项')
    await resizeToolbar(wrapper, 500)
    const editor = wrapper.get<HTMLElement>('.rich-markdown-document').element
    editor.focus()
    const item = editor.querySelectorAll('li p')[1]!
    const range = document.createRange()
    range.setStart(item.firstChild!, 0)
    range.collapse(true)
    document.getSelection()!.removeAllRanges()
    document.getSelection()!.addRange(range)
    document.dispatchEvent(new Event('selectionchange'))
    await flushPromises()
    const indent = wrapper.get('[aria-label="增加缩进"]')
    expect(indent.attributes('disabled')).toBeUndefined()
    await indent.trigger('click')
    await flushPromises()
    expect(wrapper.get('.rich-markdown-document li li').text()).toBe('第二项')
    const outdent = wrapper.get('[aria-label="减少缩进"]')
    expect(outdent.attributes('disabled')).toBeUndefined()
    await outdent.trigger('click')
    await flushPromises()
    expect(wrapper.findAll('.rich-markdown-document > ul > li')).toHaveLength(2)
    expect(wrapper.find('.rich-markdown-document li li').exists()).toBe(false)
    expect(wrapper.emitted('edit')).toHaveLength(2)
    expect(wrapper.emitted('edit')?.every((entry) => entry[1] === true)).toBe(true)
  })

  it('closes all toolbar popovers when the document scope changes with the same date', async () => {
    const wrapper = await mountEditor()
    await resizeToolbar(wrapper, 500)
    for (const [label, role] of [
      ['标题与正文', 'menu'],
      ['表格', 'dialog'],
    ] as const) {
      await wrapper.get(`[aria-label="${label}"]`).trigger('click')
      await flushPromises()
      expect(document.querySelector(`[role="${role}"][aria-label="${label}"]`)).not.toBeNull()
      await wrapper.setProps({ scrollSourceKey: `workspace-${label}` })
      await flushPromises()
      expect(document.querySelector(`[role="${role}"][aria-label="${label}"]`)).toBeNull()
    }
    await resizeToolbar(wrapper, 200)
    await wrapper.get('.markdown-tools-trigger').trigger('click')
    await flushPromises()
    expect(menu()).not.toBeNull()
    await wrapper.setProps({ scrollSourceKey: 'workspace-more' })
    await flushPromises()
    expect(menu()).toBeNull()
  })

  it('opens AI polish menu, displays polish options, and applies polished content', async () => {
    const { aiApi } = await import('../src/lib/ai')
    const polishSpy = vi.spyOn(aiApi, 'polish').mockResolvedValue('润色后的优化文本')
    const wrapper = await mountEditor('今天完成接口联调', 'txt')
    const polishBtn = wrapper.find('.ai-polish-button')
    expect(polishBtn.exists()).toBe(true)
    await polishBtn.trigger('click')
    await flushPromises()

    const polishMenu = document.querySelector('.editor-polish-menu')
    expect(polishMenu).not.toBeNull()
    const options = Array.from(polishMenu?.querySelectorAll('.polish-menu-item') ?? [])
    expect(options.length).toBe(4)
    expect(options[0]?.textContent).toContain('智能润色')
    expect(options[1]?.textContent).toContain('精炼简短')

    // Click "智能润色"
    ;(options[0] as HTMLButtonElement).click()
    await flushPromises()

    expect(polishSpy).toHaveBeenCalledWith('今天完成接口联调', expect.any(String), 'txt', '')
    expect(wrapper.emitted('edit')).toEqual([['润色后的优化文本', true]])
  })
})
