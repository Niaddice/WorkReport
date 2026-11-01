// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, type VueWrapper } from '@vue/test-utils'
import { nextTick } from 'vue'
import ButtonTooltip from '../src/components/ButtonTooltip.vue'

let wrapper: VueWrapper | undefined
beforeEach(() => vi.useFakeTimers())
afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
  document.body.replaceChildren()
  delete document.documentElement.dataset.skin
  vi.restoreAllMocks()
  vi.useRealTimers()
})
function button(parent: HTMLElement = document.body) {
  const control = document.createElement('button')
  control.title = '切换皮肤'
  control.setAttribute('aria-describedby', 'existing-description')
  parent.append(control)
  return control
}
async function hover(control: HTMLElement) {
  control.dispatchEvent(new PointerEvent('pointerover', { bubbles: true, composed: true, pointerType: 'mouse' }))
  await vi.advanceTimersByTimeAsync(1000)
  await nextTick()
  await nextTick()
}

describe('themed button tooltip host', () => {
  it('renders a text-only tooltip and restores existing descriptions after dismissal', async () => {
    const control = button()
    control.title = '<script>literal</script>'
    wrapper = mount(ButtonTooltip, { attachTo: document.body })
    await hover(control)
    const tip = document.querySelector<HTMLElement>('.button-tooltip')!
    expect(tip?.getAttribute('role')).toBe('tooltip')
    expect(tip.textContent).toContain('<script>literal</script>')
    expect(tip.querySelector('script')).toBeNull()
    expect(control.getAttribute('aria-describedby')).toBe(`existing-description ${tip.id}`)
    control.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, composed: true }))
    await nextTick()
    expect(document.querySelector('.button-tooltip')).toBeNull()
    expect(control.getAttribute('aria-describedby')).toBe('existing-description')
    expect(control.title).toBe('')
    control.dispatchEvent(
      new PointerEvent('pointerout', { bubbles: true, composed: true, relatedTarget: document.body }),
    )
    expect(control.title).toBe('<script>literal</script>')
  })

  it('renders inside an open modal dialog and keeps the same hint across theme changes', async () => {
    const dialog = document.createElement('dialog')
    dialog.setAttribute('open', '')
    document.body.append(dialog)
    const control = button(dialog)
    wrapper = mount(ButtonTooltip, { attachTo: document.body })
    await hover(control)
    expect(dialog.querySelector('.button-tooltip')?.textContent).toContain('切换皮肤')
    document.documentElement.dataset.skin = 'graphite'
    await nextTick()
    expect(dialog.querySelector('.button-tooltip')?.textContent).toContain('切换皮肤')
    wrapper.unmount()
    wrapper = undefined
    expect(control.getAttribute('aria-describedby')).toBe('existing-description')
    expect(control.title).toBe('切换皮肤')
  })
})
