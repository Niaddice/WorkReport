// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import TextContextMenu from '../src/components/TextContextMenu.vue'
import { installContextMenuGuard, registerContextEditor } from '../src/lib/contextMenus'
import { textMenuSession, type TextClipboard } from '../src/lib/textContextMenu'
import JournalEditor from '../src/components/JournalEditor.vue'

const cleanup: (() => void)[] = []
afterEach(() => {
  cleanup
    .splice(0)
    .reverse()
    .forEach((dispose) => dispose())
  document.getSelection()?.removeAllRanges()
  document.body.replaceChildren()
})

function clipboard(): TextClipboard & { read: ReturnType<typeof vi.fn>; write: ReturnType<typeof vi.fn> } {
  return { read: vi.fn(async () => '粘贴内容'), write: vi.fn(async () => {}) }
}
function field(value = '原有文本') {
  const element = document.createElement('textarea')
  element.value = value
  document.body.append(element)
  element.focus()
  element.setSelectionRange(0, 2)
  return element
}
function context(element: HTMLElement) {
  element.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, composed: true, button: 2 }))
}
function editable(element: HTMLTextAreaElement) {
  let scope = 'first'
  const adapter = {
    scope: () => scope,
    canUndo: () => true,
    canRedo: () => false,
    undo: vi.fn(),
    redo: vi.fn(),
    replace: vi.fn((text: string, start: number, end: number) => {
      element.setRangeText(text, start, end, 'end')
      element.dispatchEvent(new InputEvent('input', { bubbles: true }))
    }),
  }
  cleanup.push(registerContextEditor(element, adapter))
  return {
    adapter,
    changeScope: () => {
      scope = 'second'
    },
  }
}

describe('contextual text actions', () => {
  it('offers only the five editing actions and reads the clipboard only when paste is chosen', async () => {
    const element = field()
    const io = clipboard()
    const { adapter } = editable(element)
    const session = textMenuSession(element, io)!
    expect(session.items.map((item) => item.label)).toEqual(['复制', '剪切', '粘贴', '撤销', '重做'])
    expect(io.read).not.toHaveBeenCalled()
    expect(session.items.find((item) => item.id === 'redo')!.disabled).toBe(true)
    await session.items.find((item) => item.id === 'paste')!.run()
    expect(io.read).toHaveBeenCalledOnce()
    expect(adapter.replace).toHaveBeenCalledExactlyOnceWith('粘贴内容', 0, 2)
    expect(element.value).toBe('粘贴内容文本')
  })

  it('cuts the selected text only after writing it to the clipboard succeeds', async () => {
    const element = field()
    const io = clipboard()
    const { adapter } = editable(element)
    io.write.mockRejectedValueOnce(new Error('剪贴板不可用'))
    const session = textMenuSession(element, io)!
    await expect(session.items.find((item) => item.id === 'cut')!.run()).rejects.toThrow('剪贴板不可用')
    expect(element.value).toBe('原有文本')
    expect(adapter.replace).not.toHaveBeenCalled()
    io.write.mockResolvedValueOnce(undefined)
    await session.items.find((item) => item.id === 'cut')!.run()
    expect(io.write).toHaveBeenLastCalledWith('原有')
    expect(element.value).toBe('文本')
  })

  it.each(['content', 'caret', 'scope', 'removed'] as const)(
    'discards delayed paste after the %s changes',
    async (change) => {
      const element = field()
      const { adapter, changeScope } = editable(element)
      let resolve!: (text: string) => void
      const io = clipboard()
      io.read.mockReturnValueOnce(
        new Promise<string>((done) => {
          resolve = done
        }),
      )
      const session = textMenuSession(element, io)!
      const pending = session.items.find((item) => item.id === 'paste')!.run() as Promise<unknown>
      if (change === 'content') element.value = '用户继续输入'
      if (change === 'caret') element.setSelectionRange(3, 3)
      if (change === 'scope') changeScope()
      if (change === 'removed') element.remove()
      resolve('旧的剪贴板内容')
      await expect(pending).rejects.toThrow('内容或光标位置已变化')
      expect(adapter.replace).not.toHaveBeenCalled()
    },
  )

  it('disables cut and copy with no selection and delegates undo to the registered editor history', async () => {
    const element = field()
    element.setSelectionRange(1, 1)
    const { adapter } = editable(element)
    const session = textMenuSession(element, clipboard())!
    expect(session.items.find((item) => item.id === 'cut')!.disabled).toBe(true)
    expect(session.items.find((item) => item.id === 'copy')!.disabled).toBe(true)
    await session.items.find((item) => item.id === 'undo')!.run()
    expect(adapter.undo).toHaveBeenCalledOnce()
  })

  it('offers only copy for read-only text and suppresses empty space, buttons and disabled fields', async () => {
    const element = field()
    element.readOnly = true
    const io = clipboard()
    const session = textMenuSession(element, io)!
    expect(session.items.map((item) => item.id)).toEqual(['copy'])
    await session.items[0]!.run()
    expect(io.write).toHaveBeenCalledWith('原有')
    element.disabled = true
    expect(textMenuSession(element, io)).toBeNull()
    const blank = document.createElement('div')
    const button = document.createElement('button')
    button.textContent = '普通按钮'
    document.body.append(blank, button)
    expect(textMenuSession(blank, io)).toBeNull()
    expect(textMenuSession(button, io)).toBeNull()
  })

  it('copies a selected part of a read-only diff or its text block when nothing is selected', async () => {
    const block = document.createElement('pre')
    block.textContent = '+added line\n-removed line'
    document.body.append(block)
    const io = clipboard()
    const range = document.createRange()
    range.setStart(block.firstChild!, 1)
    range.setEnd(block.firstChild!, 6)
    document.getSelection()!.addRange(range)
    await textMenuSession(block, io)!.items[0]!.run()
    expect(io.write).toHaveBeenLastCalledWith('added')
    expect(textMenuSession(document.body, io)).toBeNull()
    document.getSelection()!.removeAllRanges()
    await textMenuSession(block, io)!.items[0]!.run()
    expect(io.write).toHaveBeenLastCalledWith(block.textContent)
  })
})

describe('text context menu integration', () => {
  it('renders the edit whitelist in a teleported dialog and lets Git handlers own their menu', async () => {
    const controller = mount(TextContextMenu, { props: { scope: 'first' } })
    cleanup.push(() => controller.unmount(), installContextMenuGuard())
    const dialog = document.createElement('dialog')
    dialog.setAttribute('open', '')
    const element = field()
    dialog.append(element)
    document.body.append(dialog)
    context(element)
    await flushPromises()
    const menu = dialog.querySelector('[role="menu"]')!
    expect(menu.getAttribute('aria-label')).toBe('文本操作')
    expect([...menu.querySelectorAll('button')].map((item) => item.textContent!.trim())).toEqual([
      '复制',
      '剪切',
      '粘贴',
      '撤销',
      '重做',
    ])
    await controller.setProps({ scope: 'another' })
    expect(document.querySelector('.text-context-menu')).toBeNull()
    const gitRow = document.createElement('button')
    const gitMenu = vi.fn((event: Event) => {
      event.preventDefault()
      event.stopPropagation()
    })
    gitRow.addEventListener('contextmenu', gitMenu)
    document.body.append(gitRow)
    context(gitRow)
    await flushPromises()
    expect(gitMenu).toHaveBeenCalledOnce()
    expect(document.querySelector('.text-context-menu')).toBeNull()
  })

  it('integrates cut, undo and redo with journal history and creates an independent edit', async () => {
    const wrapper = mount(JournalEditor, {
      attachTo: document.body,
      props: {
        date: '20261029',
        document: { date: '20261029', filename: '20261029.txt', format: 'txt', content: '原有文本', revision: 'v1' },
        isNew: false,
        content: '原有文本',
        dirty: false,
        saving: false,
        disabled: false,
        error: '',
        savedAt: null,
        canRestore: false,
        canUndo: true,
        canRedo: true,
        preview: false,
      },
    })
    cleanup.push(() => wrapper.unmount())
    await flushPromises()
    const element = wrapper.get<HTMLTextAreaElement>('textarea').element
    element.focus()
    element.setSelectionRange(0, 2)
    const session = textMenuSession(element, clipboard())!
    await session.items.find((item) => item.id === 'cut')!.run()
    expect(wrapper.emitted('edit')).toEqual([['文本', true]])
    await session.items.find((item) => item.id === 'undo')!.run()
    await session.items.find((item) => item.id === 'redo')!.run()
    expect(wrapper.emitted('undo')).toHaveLength(1)
    expect(wrapper.emitted('redo')).toHaveLength(1)
  })
})
