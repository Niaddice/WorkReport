// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { BUTTON_TOOLTIP_DELAY, installButtonTooltips } from '../src/lib/buttonTooltips'

const cleanups: (() => void)[] = []
beforeEach(() => vi.useFakeTimers())
afterEach(() => {
  cleanups
    .splice(0)
    .reverse()
    .forEach((cleanup) => cleanup())
  document.body.replaceChildren()
  vi.useRealTimers()
})

function setup() {
  const show = vi.fn()
  const hide = vi.fn()
  const cleanup = installButtonTooltips({ show, hide })
  cleanups.push(cleanup)
  return { show, hide, cleanup }
}

function button(title = '功能说明') {
  const element = document.createElement('button')
  element.title = title
  document.body.append(element)
  return element
}

function over(element: HTMLElement) {
  element.dispatchEvent(new PointerEvent('pointerover', { bubbles: true, composed: true, pointerType: 'mouse' }))
}

function out(element: HTMLElement, relatedTarget: EventTarget | null = null) {
  element.dispatchEvent(new PointerEvent('pointerout', { bubbles: true, composed: true, relatedTarget }))
}

describe('button tooltip controller', () => {
  it('shows only after the complete one-second delay and suppresses the native title', async () => {
    const { show, hide } = setup()
    const target = button()
    over(target)
    expect(BUTTON_TOOLTIP_DELAY).toBe(1000)
    expect(target.getAttribute('title')).toBe('')
    await vi.advanceTimersByTimeAsync(999)
    expect(show).not.toHaveBeenCalled()
    await vi.advanceTimersByTimeAsync(1)
    expect(show).toHaveBeenCalledExactlyOnceWith({ anchor: target, text: '功能说明', portal: document.body })
    out(target)
    expect(hide).toHaveBeenCalledOnce()
    expect(target.title).toBe('功能说明')
  })

  it('cancels a pending hint on exit and gives the next button a fresh delay', async () => {
    const { show } = setup()
    const first = button('第一项')
    const second = button('第二项')
    over(first)
    await vi.advanceTimersByTimeAsync(700)
    out(first, second)
    over(second)
    await vi.advanceTimersByTimeAsync(999)
    expect(show).not.toHaveBeenCalled()
    await vi.advanceTimersByTimeAsync(1)
    expect(show).toHaveBeenCalledExactlyOnceWith({ anchor: second, text: '第二项', portal: document.body })
    expect(first.title).toBe('第一项')
  })

  it('does not restart the delay while moving between descendants of one control', async () => {
    const { show } = setup()
    const target = button()
    const first = document.createElement('span')
    const second = document.createElement('span')
    target.append(first, second)
    over(first)
    await vi.advanceTimersByTimeAsync(600)
    out(first, second)
    over(second)
    await vi.advanceTimersByTimeAsync(400)
    expect(show).toHaveBeenCalledOnce()
    expect(show.mock.calls[0]![0].anchor).toBe(target)
  })

  it('prefers a Material host hint over its internal shadow button label', async () => {
    const { show } = setup()
    const host = document.createElement('md-icon-button')
    host.title = '日报已存在，右键关闭'
    const shadow = host.attachShadow({ mode: 'open' })
    const inner = document.createElement('button')
    inner.setAttribute('aria-label', '日报')
    const icon = document.createElement('span')
    inner.append(icon)
    shadow.append(inner)
    document.body.append(host)
    over(icon)
    await vi.advanceTimersByTimeAsync(1000)
    expect(show).toHaveBeenCalledExactlyOnceWith({ anchor: host, text: '日报已存在，右键关闭', portal: document.body })
    out(icon, inner)
    expect(host.title).toBe('')
    out(inner)
    expect(host.title).toBe('日报已存在，右键关闭')
  })

  it('tracks reactive native titles inside a shadow root', async () => {
    const { show } = setup()
    const host = document.createElement('md-icon-button')
    const shadow = host.attachShadow({ mode: 'open' })
    const inner = document.createElement('button')
    inner.title = '初始说明'
    shadow.append(inner)
    document.body.append(host)
    over(inner)
    await vi.advanceTimersByTimeAsync(1000)
    inner.title = '更新说明'
    await vi.advanceTimersByTimeAsync(0)
    expect(show.mock.calls.at(-1)![0].text).toBe('更新说明')
    expect(inner.title).toBe('')
    out(inner)
    expect(inner.title).toBe('更新说明')
  })

  it('resolves nested select labels and wrapper hints for disabled buttons', async () => {
    const { show } = setup()
    const select = button()
    select.removeAttribute('title')
    select.setAttribute('role', 'combobox')
    const label = document.createElement('span')
    label.title = '完整选项名称'
    select.append(label)
    over(select)
    await vi.advanceTimersByTimeAsync(1000)
    expect(show.mock.calls.at(-1)![0]).toEqual({ anchor: select, text: '完整选项名称', portal: document.body })
    out(select)
    const wrapper = document.createElement('span')
    wrapper.title = '请先配置 AI'
    const disabled = document.createElement('button')
    disabled.disabled = true
    wrapper.append(disabled)
    document.body.append(wrapper)
    over(wrapper)
    await vi.advanceTimersByTimeAsync(1000)
    expect(show.mock.calls.at(-1)![0]).toEqual({ anchor: wrapper, text: '请先配置 AI', portal: document.body })
  })

  it('retains the latest reactive title after showing or cancelling in the same turn', async () => {
    const { show } = setup()
    const target = button('原说明')
    over(target)
    await vi.advanceTimersByTimeAsync(1000)
    target.title = '更新后的说明'
    await vi.advanceTimersByTimeAsync(0)
    expect(show.mock.calls.at(-1)![0].text).toBe('更新后的说明')
    expect(target.title).toBe('')
    target.title = '最后一次说明'
    out(target)
    expect(target.title).toBe('最后一次说明')
  })

  it('preserves reactive title removal rather than restoring an obsolete value', async () => {
    setup()
    const target = button()
    target.setAttribute('aria-label', '备用说明')
    over(target)
    target.removeAttribute('title')
    out(target)
    expect(target.hasAttribute('title')).toBe(false)
  })

  it.each(['pointerdown', 'click', 'contextmenu', 'scroll'])(
    'dismisses on %s and keeps native titles suppressed until exit',
    async (type) => {
      const { show, hide } = setup()
      const target = button()
      over(target)
      await vi.advanceTimersByTimeAsync(1000)
      target.dispatchEvent(new Event(type, { bubbles: true, composed: true }))
      expect(hide).toHaveBeenCalledOnce()
      expect(target.title).toBe('')
      over(target)
      await vi.advanceTimersByTimeAsync(1500)
      expect(show).toHaveBeenCalledOnce()
      out(target)
      expect(target.title).toBe('功能说明')
    },
  )

  it('cancels Escape without consuming the menu key and supports delayed keyboard focus', async () => {
    const { show, hide } = setup()
    const target = button()
    target.focus()
    await vi.advanceTimersByTimeAsync(1000)
    expect(show).not.toHaveBeenCalled()
    target.blur()
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }))
    target.focus()
    await vi.advanceTimersByTimeAsync(999)
    expect(show).not.toHaveBeenCalled()
    await vi.advanceTimersByTimeAsync(1)
    expect(show).toHaveBeenCalledOnce()
    const escape = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
    target.dispatchEvent(escape)
    expect(escape.defaultPrevented).toBe(false)
    expect(hide).toHaveBeenCalledOnce()
    expect(target.title).toBe('')
    target.blur()
    expect(target.title).toBe('功能说明')
  })

  it('portals into the native modal dialog containing the control', async () => {
    const { show } = setup()
    const dialog = document.createElement('dialog')
    const target = button()
    dialog.append(target)
    document.body.append(dialog)
    dialog.setAttribute('open', '')
    over(target)
    await vi.advanceTimersByTimeAsync(1000)
    expect(show).toHaveBeenCalledExactlyOnceWith({ anchor: target, text: '功能说明', portal: dialog })
  })

  it('dismisses when its native dialog closes', async () => {
    const { hide } = setup()
    const dialog = document.createElement('dialog')
    const target = button()
    dialog.append(target)
    document.body.append(dialog)
    dialog.setAttribute('open', '')
    over(target)
    await vi.advanceTimersByTimeAsync(1000)
    dialog.removeAttribute('open')
    await vi.advanceTimersByTimeAsync(0)
    expect(hide).toHaveBeenCalledOnce()
    expect(target.title).toBe('功能说明')
  })

  it('does not cover an already-open popup or an existing rich tooltip', async () => {
    const { show } = setup()
    const target = button()
    target.setAttribute('aria-expanded', 'true')
    over(target)
    await vi.advanceTimersByTimeAsync(1000)
    expect(show).not.toHaveBeenCalled()
    out(target)
    target.removeAttribute('aria-expanded')
    const rich = document.createElement('div')
    rich.id = 'existing-tooltip'
    rich.setAttribute('role', 'tooltip')
    document.body.append(rich)
    target.setAttribute('aria-describedby', rich.id)
    over(target)
    await vi.advanceTimersByTimeAsync(1000)
    expect(show).not.toHaveBeenCalled()
    expect(target.title).toBe('')
    out(target)
    rich.className = 'button-tooltip'
    over(target)
    await vi.advanceTimersByTimeAsync(1000)
    expect(show).toHaveBeenCalledOnce()
  })

  it.each(['inert', 'hidden', 'data-tooltip-disabled'])('skips controls inside %s containers', async (attribute) => {
    const { show } = setup()
    const target = button()
    const wrapper = document.createElement('div')
    wrapper.setAttribute(attribute, '')
    wrapper.append(target)
    document.body.append(wrapper)
    over(target)
    await vi.advanceTimersByTimeAsync(1000)
    expect(show).not.toHaveBeenCalled()
    expect(target.title).toBe('功能说明')
  })

  it('leaves noninteractive Markdown image titles untouched', async () => {
    const { show } = setup()
    const editor = document.createElement('div')
    editor.contentEditable = 'true'
    const image = document.createElement('span')
    image.title = '![图](image.png)'
    editor.append(image)
    document.body.append(editor)
    over(image)
    await vi.advanceTimersByTimeAsync(1000)
    expect(show).not.toHaveBeenCalled()
    expect(image.title).toBe('![图](image.png)')
  })

  it('honors an opt-out on a nested source even when hovering its button padding', async () => {
    const { show } = setup()
    const target = button()
    target.removeAttribute('title')
    const label = document.createElement('span')
    label.title = '嵌套说明'
    label.setAttribute('data-tooltip-disabled', '')
    target.append(label)
    over(target)
    await vi.advanceTimersByTimeAsync(1000)
    expect(show).not.toHaveBeenCalled()
    expect(label.title).toBe('嵌套说明')
  })

  it('restores titles and removes timers/listeners when disposed', async () => {
    const { show, cleanup } = setup()
    const target = button()
    over(target)
    cleanup()
    expect(target.title).toBe('功能说明')
    await vi.advanceTimersByTimeAsync(1000)
    expect(show).not.toHaveBeenCalled()
    over(target)
    await vi.advanceTimersByTimeAsync(1000)
    expect(show).not.toHaveBeenCalled()
  })

  it('hides and restores a control that is removed while its hint is open', async () => {
    const { hide } = setup()
    const target = button()
    over(target)
    await vi.advanceTimersByTimeAsync(1000)
    target.remove()
    await vi.advanceTimersByTimeAsync(0)
    expect(hide).toHaveBeenCalledOnce()
    expect(target.title).toBe('功能说明')
  })
})
