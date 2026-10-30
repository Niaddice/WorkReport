import { isTauri } from '@tauri-apps/api/core'
import { api } from './workspace'
import { contextEditor, type ContextMenuItem } from './contextMenus'

export interface TextMenuSession {
  anchor: HTMLElement
  items: ContextMenuItem[]
  restore: () => void
}

export interface TextClipboard {
  read: () => Promise<string>
  write: (text: string) => Promise<unknown>
}

const clipboard: TextClipboard = {
  read: () => (isTauri() ? api.clipboardText() : navigator.clipboard.readText()),
  write: (text) => (isTauri() ? api.copy(text) : navigator.clipboard.writeText(text)),
}

function unavailable(element: HTMLElement): boolean {
  return !element.isConnected || !!element.closest('[inert], [aria-disabled="true"]') || element.matches(':disabled')
}

function selectedRange(target: HTMLElement, includeCaret = false): Range | null {
  const selection = target.ownerDocument.getSelection()
  if (!selection?.rangeCount || (selection.isCollapsed && !includeCaret)) return null
  const range = selection.getRangeAt(0)
  return range.intersectsNode(target) ? range.cloneRange() : null
}

/** Build only text actions for the exact field or text block under the pointer. */
export function textMenuSession(target: HTMLElement, io: TextClipboard = clipboard): TextMenuSession | null {
  if (unavailable(target) || target.closest('[role="menu"], button, select, [role="button"], [role="combobox"]'))
    return null
  if (
    target.matches(
      'html, body, #app, .app-shell, .app-main, .app-content, .module-board, .module-panel, .module-panel-body',
    )
  )
    return null
  const field = target.closest<HTMLElement>(
    'input, textarea, [contenteditable="true"], [contenteditable="plaintext-only"]',
  )
  const control = field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement ? field : null
  if (control && !['text', 'search', 'url', 'tel', 'email', 'password', 'number', 'textarea'].includes(control.type))
    return null
  if (field && unavailable(field)) return null

  if (field) {
    const doc = field.ownerDocument
    field.focus({ preventScroll: true })
    const value = () => (control ? control.value : (field.textContent ?? ''))
    const original = value()
    const start = control?.selectionStart ?? null
    const end = control?.selectionEnd ?? null
    const direction = control?.selectionDirection ?? 'none'
    const range = control ? null : selectedRange(field, true)
    const selected = control && start !== null && end !== null ? original.slice(start, end) : (range?.toString() ?? '')
    const readOnly = control?.readOnly ?? false
    const editor = contextEditor(field)
    const scope = editor?.scope()
    const valid = () =>
      !unavailable(field) &&
      value() === original &&
      (!editor || (contextEditor(field) === editor && editor.scope() === scope))
    const restore = () => {
      if (!valid()) return
      field.focus({ preventScroll: true })
      if (control && start !== null && end !== null) control.setSelectionRange(start, end, direction)
      else if (range) {
        doc.getSelection()?.removeAllRanges()
        doc.getSelection()?.addRange(range)
      }
    }
    const assertCurrent = () => {
      const selection = doc.getSelection()
      const currentRange = !control && range && selection?.rangeCount ? selection.getRangeAt(0) : null
      if (
        !valid() ||
        (control?.readOnly ?? false) ||
        doc.activeElement !== field ||
        (control && start !== null && (control.selectionStart !== start || control.selectionEnd !== end)) ||
        (range &&
          (!currentRange ||
            currentRange.startContainer !== range.startContainer ||
            currentRange.startOffset !== range.startOffset ||
            currentRange.endContainer !== range.endContainer ||
            currentRange.endOffset !== range.endOffset))
      ) {
        throw new Error('内容或光标位置已变化，请重新打开右键菜单。')
      }
    }
    const native = (command: string, text?: string) => {
      const before = value()
      let emitted = false
      const onInput = () => {
        emitted = true
      }
      field.addEventListener('input', onInput)
      let success: boolean
      try {
        success = doc.execCommand?.(command, false, text) ?? false
      } finally {
        field.removeEventListener('input', onInput)
      }
      if (!success && before === value()) throw new Error('当前输入框暂不支持此操作，请使用键盘快捷键。')
      if (!emitted && before !== value())
        field.dispatchEvent(new InputEvent('input', { bubbles: true, composed: true }))
    }
    const replace = (text: string) => {
      assertCurrent()
      if (editor && start !== null && end !== null) editor.replace(text, start, end)
      else native('insertText', text)
    }
    const enabled = (action: 'undo' | 'redo') => {
      if (editor) return action === 'undo' ? editor.canUndo() : editor.canRedo()
      try {
        return doc.queryCommandEnabled?.(action) ?? false
      } catch {
        return false
      }
    }
    const copyText = readOnly ? selected || original : selected
    const items: ContextMenuItem[] = [{ id: 'copy', label: '复制', disabled: !copyText, run: () => io.write(copyText) }]
    if (!readOnly) {
      items.push(
        {
          id: 'cut',
          label: '剪切',
          disabled: !selected,
          run: async () => {
            restore()
            assertCurrent()
            await io.write(selected)
            replace('')
          },
        },
        {
          id: 'paste',
          label: '粘贴',
          run: async () => {
            restore()
            assertCurrent()
            const text = await io.read()
            if (text) replace(text)
          },
        },
        {
          id: 'undo',
          label: '撤销',
          disabled: !enabled('undo'),
          run: () => {
            restore()
            assertCurrent()
            return editor ? editor.undo() : native('undo')
          },
        },
        {
          id: 'redo',
          label: '重做',
          disabled: !enabled('redo'),
          run: () => {
            restore()
            assertCurrent()
            return editor ? editor.redo() : native('redo')
          },
        },
      )
    }
    return { anchor: field, items, restore }
  }

  const range = selectedRange(target)
  const block = target.closest<HTMLElement>(
    'pre, code, p, [data-context-copy], .journal-prose, .range-text, .summary-result',
  )
  const text = range?.toString() || block?.textContent || ''
  if (!text.trim()) return null
  return {
    anchor: target,
    items: [{ id: 'copy', label: '复制', run: () => io.write(text) }],
    restore: () => {
      if (range && target.isConnected) {
        target.ownerDocument.getSelection()?.removeAllRanges()
        target.ownerDocument.getSelection()?.addRange(range)
      }
    },
  }
}
