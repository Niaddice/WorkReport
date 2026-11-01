// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { h, Teleport } from 'vue'
import { installContextMenuGuard } from '../src/lib/contextMenus'

const cleanup: (() => void)[] = []

afterEach(() => {
  cleanup
    .splice(0)
    .reverse()
    .forEach((dispose) => dispose())
  document.body.replaceChildren()
})

function contextMenu(target: Element) {
  const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true, composed: true, button: 2 })
  return { event, allowed: target.dispatchEvent(event) }
}

describe('app-wide context menus', () => {
  it('blocks native menus while allowing contextual application handlers', () => {
    const target = document.createElement('button')
    document.body.append(target)
    const customMenu = vi.fn()
    const documentMenu = vi.fn()
    target.addEventListener('contextmenu', customMenu)
    document.addEventListener('contextmenu', documentMenu, { capture: true })
    cleanup.push(() => document.removeEventListener('contextmenu', documentMenu, { capture: true }))
    cleanup.push(installContextMenuGuard())

    const { event, allowed } = contextMenu(target)

    expect(allowed).toBe(false)
    expect(event.defaultPrevented).toBe(true)
    expect(customMenu).toHaveBeenCalledOnce()
    expect(documentMenu).toHaveBeenCalledOnce()
  })

  it('also filters native menus inside teleported dialogs without stopping editing handlers', () => {
    const customMenu = vi.fn()
    const wrapper = mount(
      { render: () => h(Teleport, { to: 'body' }, h('dialog', [h('textarea', { onContextmenu: customMenu })])) },
      { attachTo: document.body },
    )
    cleanup.push(() => wrapper.unmount())
    cleanup.push(installContextMenuGuard())
    const textarea = document.querySelector('dialog textarea')!

    expect(wrapper.element.contains(textarea)).toBe(false)
    expect(contextMenu(textarea).allowed).toBe(false)
    expect(customMenu).toHaveBeenCalledOnce()
  })

  it('removes the native menu filter during cleanup', () => {
    const target = document.createElement('textarea')
    document.body.append(target)
    const customMenu = vi.fn()
    target.addEventListener('contextmenu', customMenu)
    const removeGuard = installContextMenuGuard()
    cleanup.push(removeGuard)

    expect(contextMenu(target).allowed).toBe(false)
    removeGuard()

    expect(contextMenu(target).allowed).toBe(true)
    expect(customMenu).toHaveBeenCalledTimes(2)
  })

  it('keeps ordinary clicks, clipboard events, and copy shortcuts available', () => {
    const target = document.createElement('textarea')
    document.body.append(target)
    const click = vi.fn()
    const keydown = vi.fn()
    const copy = vi.fn()
    target.addEventListener('click', click)
    target.addEventListener('keydown', keydown)
    target.addEventListener('copy', copy)
    cleanup.push(installContextMenuGuard())
    const events = [
      new MouseEvent('click', { bubbles: true, cancelable: true }),
      new KeyboardEvent('keydown', { key: 'c', ctrlKey: true, bubbles: true, cancelable: true }),
      new KeyboardEvent('keydown', { key: 'c', metaKey: true, bubbles: true, cancelable: true }),
      new Event('copy', { bubbles: true, cancelable: true }),
    ]

    for (const event of events) {
      expect(target.dispatchEvent(event)).toBe(true)
      expect(event.defaultPrevented).toBe(false)
    }
    expect(click).toHaveBeenCalledOnce()
    expect(keydown).toHaveBeenCalledTimes(2)
    expect(copy).toHaveBeenCalledOnce()
  })
})
