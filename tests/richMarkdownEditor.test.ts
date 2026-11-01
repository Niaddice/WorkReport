// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from 'vitest'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import RichMarkdownEditor from '../src/components/RichMarkdownEditor.vue'
import type {
  HeadingLevel,
  MarkdownTableAction,
  MarkdownToolbarAction,
  MarkdownToolbarState,
} from '../src/lib/markdownToolbar'

interface RichEditorApi {
  replaceSelection(text: string): void
  selectAll(): void
  apply(action: MarkdownToolbarAction): void
  getToolbarState(): MarkdownToolbarState
  setHeading(level: HeadingLevel | 0): void
  insertTable(rows: number, columns: number): void
  tableAction(action: MarkdownTableAction): void
  getMarkdown(): string
  getSelectedMarkdown(): string
  getSelection(): { from: number; to: number }
  transactionVersion(): number
  getEditableElement(): HTMLElement
}
const wrappers: VueWrapper[] = []
async function makeEditor(content = '初始正文') {
  const wrapper = mount(RichMarkdownEditor, {
    attachTo: document.body,
    props: { content, disabled: false, sourceKey: '20261031.md' },
  })
  wrappers.push(wrapper)
  await flushPromises()
  return { wrapper, api: wrapper.vm as unknown as RichEditorApi }
}
afterEach(() => {
  wrappers.splice(0).forEach((wrapper) => wrapper.unmount())
  document.body.innerHTML = ''
})

describe('rich Markdown editor integration', () => {
  it('routes editor shortcuts and native history input events to journal history', async () => {
    const { wrapper, api } = await makeEditor('正文')
    const input = api.getEditableElement()
    const undo = new KeyboardEvent('keydown', { key: 'z', metaKey: true, bubbles: true, cancelable: true })
    input.dispatchEvent(undo)
    expect(undo.defaultPrevented).toBe(true)
    expect(wrapper.emitted('undo')).toHaveLength(1)
    input.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'z', metaKey: true, shiftKey: true, bubbles: true, cancelable: true }),
    )
    expect(wrapper.emitted('redo')).toHaveLength(1)
    const nativeUndo = new InputEvent('beforeinput', { inputType: 'historyUndo', bubbles: true, cancelable: true })
    input.dispatchEvent(nativeUndo)
    expect(nativeUndo.defaultPrevented).toBe(true)
    expect(wrapper.emitted('undo')).toHaveLength(2)
    input.dispatchEvent(new InputEvent('beforeinput', { inputType: 'historyRedo', bubbles: true, cancelable: true }))
    expect(wrapper.emitted('redo')).toHaveLength(2)
    expect(wrapper.emitted('edit')).toBeUndefined()
    input.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'z', metaKey: true, isComposing: true, bubbles: true, cancelable: true }),
    )
    expect(wrapper.emitted('undo')).toHaveLength(2)
  })

  it('publishes stored-mark state at a caret without emitting a document edit', async () => {
    const { wrapper, api } = await makeEditor('正文')
    expect(api.getToolbarState().active.bold).toBe(false)
    api.apply('bold')
    expect(api.getToolbarState().active.bold).toBe(true)
    expect((wrapper.emitted('toolbar-state')!.at(-1)![0] as MarkdownToolbarState).active.bold).toBe(true)
    expect(wrapper.emitted('edit')).toBeUndefined()
    api.apply('bold')
    expect(api.getToolbarState().active.bold).toBe(false)
    await wrapper.setProps({ disabled: true })
    expect(Object.values(api.getToolbarState().enabled).every((value) => !value)).toBe(true)
  })

  it('sets each heading level explicitly and returns to a paragraph', async () => {
    const { wrapper, api } = await makeEditor('正文')
    for (const level of [1, 2, 3, 4, 5, 6] as HeadingLevel[]) {
      api.setHeading(level)
      expect(api.getToolbarState().headingLevel).toBe(level)
      expect(api.getEditableElement().querySelector(`h${level}`)?.textContent).toBe('正文')
    }
    api.setHeading(6)
    expect(api.getEditableElement().querySelector('h6')?.textContent).toBe('正文')
    expect(wrapper.emitted('edit')).toHaveLength(6)
    api.setHeading(0)
    expect(api.getToolbarState().headingLevel).toBe(0)
    expect(api.getEditableElement().querySelector('p')?.textContent).toBe('正文')
    expect(wrapper.emitted('edit')!.every((entry) => entry[1] === true)).toBe(true)
  })

  it('disables incompatible block tools inside cells while keeping inline tools available', async () => {
    const { wrapper, api } = await makeEditor('| A | B |\n| --- | --- |\n| C | D |')
    const state = api.getToolbarState()
    expect(state.inTable).toBe(true)
    expect(state.enabled.bold).toBe(true)
    for (const action of ['heading', 'bulletList', 'orderedList', 'taskList', 'quote', 'codeBlock'] as const) {
      expect(state.enabled[action]).toBe(false)
      api.apply(action)
    }
    api.setHeading(1)
    expect(wrapper.emitted('edit')).toBeUndefined()
    expect(api.getEditableElement().querySelectorAll('table')).toHaveLength(1)
    expect(api.getEditableElement().querySelectorAll('tr')).toHaveLength(2)
  })

  it('inserts the requested table and treats each table action as one journal history operation', async () => {
    const { wrapper, api } = await makeEditor('')
    api.insertTable(3, 4)
    expect(api.getEditableElement().querySelectorAll('tr')).toHaveLength(3)
    expect(api.getEditableElement().querySelectorAll('th')).toHaveLength(4)
    expect(wrapper.emitted('edit')).toHaveLength(1)
    api.tableAction('addRowAfter')
    expect(api.getEditableElement().querySelectorAll('tr')).toHaveLength(4)
    expect(wrapper.emitted('edit')).toHaveLength(2)
    api.tableAction('alignCenter')
    expect(wrapper.emitted('edit')).toHaveLength(3)
    expect(api.getToolbarState().tableAlignment).toBe('center')
    expect(wrapper.emitted('edit')!.every((entry) => entry[1] === true)).toBe(true)
  })

  it('edits and removes an existing link at the caret without duplicating its text', async () => {
    const { wrapper, api } = await makeEditor('[参考资料](https://old.example.com)')
    const input = api.getEditableElement()
    input.focus()
    const text = input.querySelector('a')!.firstChild!
    const range = document.createRange()
    range.setStart(text, 2)
    range.collapse(true)
    document.getSelection()!.removeAllRanges()
    document.getSelection()!.addRange(range)
    document.dispatchEvent(new Event('selectionchange'))
    await flushPromises()
    api.apply('link')
    await flushPromises()
    expect(wrapper.find('[aria-label="链接文字"]').exists()).toBe(false)
    await wrapper.get('[aria-label="链接地址"]').setValue('https://new.example.com/a)b')
    await wrapper.get('form').trigger('submit')
    expect(input.querySelectorAll('a')).toHaveLength(1)
    expect(input.textContent).toBe('参考资料')
    expect(input.querySelector('a')!.getAttribute('href')).toBe('https://new.example.com/a)b')
    expect(api.getMarkdown()).toBe('[参考资料](https://new.example.com/a\\)b)')
    expect(wrapper.emitted('edit')).toHaveLength(1)
    api.apply('link')
    await flushPromises()
    await wrapper.get('.rich-markdown-unlink').trigger('click')
    expect(input.querySelector('a')).toBeNull()
    expect(api.getMarkdown()).toBe('参考资料')
    expect(wrapper.emitted('edit')).toHaveLength(2)
  })

  it('opens links with the keyboard and discards the form on external document changes', async () => {
    const { wrapper, api } = await makeEditor('正文')
    api
      .getEditableElement()
      .dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true, cancelable: true }))
    await flushPromises()
    expect(wrapper.find('form[aria-label="编辑链接"]').exists()).toBe(true)
    await wrapper.setProps({ sourceKey: 'other', content: '下一篇正文' })
    expect(wrapper.find('form').exists()).toBe(false)
    expect(api.getMarkdown()).toBe('下一篇正文')
    expect(wrapper.emitted('edit')).toBeUndefined()
  })

  it('does not emit a normalized edit on mount or external content updates', async () => {
    const { wrapper, api } = await makeEditor('# 标题\n\n正文\n')
    expect(wrapper.emitted('edit')).toBeUndefined()
    await wrapper.setProps({ content: '## 外部更新\n\n后续正文' })
    expect(api.getEditableElement().querySelector('h2')?.textContent).toBe('外部更新')
    expect(wrapper.emitted('edit')).toBeUndefined()
  })

  it('keeps the live document and selection for own v-model echoes and same-document saves', async () => {
    const { wrapper, api } = await makeEditor('正文')
    api.selectAll()
    api.replaceSelection('## 更新标题\n\n| 项目 | 状态 |\n| --- | --- |\n| 接口 | 完成 |')
    const emitted = wrapper.emitted('edit')!
    expect(emitted).toHaveLength(1)
    expect(emitted[0]?.[1]).toBe(true)
    const content = emitted[0]?.[0] as string
    const version = api.transactionVersion()
    const selection = api.getSelection()
    await wrapper.setProps({ content })
    expect(api.transactionVersion()).toBe(version)
    expect(api.getSelection()).toEqual(selection)
    await wrapper.setProps({ content })
    expect(api.transactionVersion()).toBe(version)
    expect(api.getEditableElement().querySelector('table')).not.toBeNull()
  })

  it('resets selection when switching source keys and clamps it for shorter external content', async () => {
    const { wrapper, api } = await makeEditor('一段比较长的原始日报正文')
    api.selectAll()
    api.replaceSelection('新正文，光标位于结尾')
    await wrapper.setProps({ content: '短' })
    expect(api.getSelection().from).toBeLessThanOrEqual(2)
    expect(wrapper.emitted('edit')).toHaveLength(1)
    await wrapper.setProps({ sourceKey: '20261101.md', content: '下一天正文' })
    expect(api.getSelection()).toEqual({ from: 1, to: 1 })
    expect(wrapper.emitted('edit')).toHaveLength(1)
  })

  it('parses pasted Markdown as real tables and tasks in one forced history operation', async () => {
    const { wrapper, api } = await makeEditor('')
    const event = new Event('paste', { bubbles: true, cancelable: true })
    Object.defineProperty(event, 'clipboardData', {
      value: {
        getData: (format: string) =>
          format === 'text/plain' ? '| A | B |\n| --- | --- |\n| C | D |\n\n- [ ] 待办' : '',
      },
    })
    api.getEditableElement().dispatchEvent(event)
    await flushPromises()
    expect(event.defaultPrevented).toBe(true)
    expect(api.getEditableElement().querySelector('table')).not.toBeNull()
    expect(api.getEditableElement().querySelector('input[type="checkbox"]')).not.toBeNull()
    expect(wrapper.emitted('edit')).toHaveLength(1)
    expect(wrapper.emitted('edit')?.[0]?.[1]).toBe(true)
  })

  it('applies toolbar tables and headings while preserving document history ownership', async () => {
    const { wrapper, api } = await makeEditor('标题正文')
    api.apply('heading')
    expect(api.getEditableElement().querySelector('h2')?.textContent).toBe('标题正文')
    api.selectAll()
    api.replaceSelection('')
    api.apply('table')
    expect(api.getEditableElement().querySelectorAll('table th')).toHaveLength(2)
    expect(api.getMarkdown()).toMatch(/\| 项目\s+\| 内容\s+\|/)
    expect(wrapper.emitted('edit')?.every((entry) => entry[1] === true)).toBe(true)
  })

  it('copies actual Markdown so native copy and paste retain tables and formatting', async () => {
    const { api } = await makeEditor('**重要**\n\n| A | B |\n| --- | --- |\n| C | D |')
    api.selectAll()
    expect(api.getSelectedMarkdown()).toContain('**重要**')
    const data = new Map<string, string>()
    const clipboard = {
      clearData: () => data.clear(),
      setData: (format: string, value: string) => data.set(format, value),
      getData: (format: string) => data.get(format) ?? '',
    }
    const copy = new Event('copy', { bubbles: true, cancelable: true })
    Object.defineProperty(copy, 'clipboardData', { value: clipboard })
    api.getEditableElement().dispatchEvent(copy)
    expect(data.get('text/plain')).toContain('**重要**')
    expect(data.get('text/plain')).toMatch(/\| A\s+\| B\s+\|/)
    api.replaceSelection('')
    const paste = new Event('paste', { bubbles: true, cancelable: true })
    Object.defineProperty(paste, 'clipboardData', { value: clipboard })
    api.getEditableElement().dispatchEvent(paste)
    expect(api.getEditableElement().querySelector('strong')?.textContent).toBe('重要')
    expect(api.getEditableElement().querySelectorAll('table')).toHaveLength(1)
  })

  it('advances table cells on Enter and keeps composition events untouched', async () => {
    const { wrapper, api } = await makeEditor('| A | B |\n| --- | --- |\n| C | D |')
    const input = api.getEditableElement()
    const first = api.getSelection()
    input.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', keyCode: 229, isComposing: true, bubbles: true, cancelable: true }),
    )
    expect(api.getSelection()).toEqual(first)
    for (let index = 0; index < 4; index++)
      input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', keyCode: 13, bubbles: true, cancelable: true }))
    expect(input.querySelectorAll('table')).toHaveLength(1)
    expect(input.querySelectorAll('table tr')).toHaveLength(3)
    expect(wrapper.emitted('edit')).toHaveLength(1)
    expect(wrapper.emitted('edit')?.[0]?.[1]).toBe(true)
  })

  it('prevents disabled commands and keeps focus metadata out of the transaction version', async () => {
    const { wrapper, api } = await makeEditor()
    const version = api.transactionVersion()
    api.getEditableElement().focus()
    expect(api.transactionVersion()).toBe(version)
    await wrapper.setProps({ disabled: true })
    api.selectAll()
    api.replaceSelection('禁止修改')
    api.apply('table')
    expect(wrapper.emitted('edit')).toBeUndefined()
    expect(api.getEditableElement().getAttribute('contenteditable')).toBe('false')
  })
})
