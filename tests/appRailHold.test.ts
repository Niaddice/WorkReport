// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, type DOMWrapper, type VueWrapper } from '@vue/test-utils'
import AppRail from '../src/components/AppRail.vue'

let wrapper: VueWrapper
beforeEach(() => vi.useFakeTimers())
afterEach(() => {
  wrapper?.unmount()
  document.body.replaceChildren()
  vi.useRealTimers()
})
function setup(props = {}) {
  wrapper = mount(AppRail, { attachTo: document.body, props: { disabled: false, active: ['editor'], ...props } })
  return wrapper.get('[aria-label="编辑"]')
}
async function pointer(button: Pick<DOMWrapper<Element>, 'trigger'>, type: string, extra = {}) {
  await button.trigger(type, {
    pointerType: 'mouse',
    pointerId: 1,
    button: 0,
    buttons: 1,
    clientX: 90,
    clientY: 120,
    ...extra,
  })
}
const ring = () => document.querySelector<HTMLElement>('.rail-hold-progress')

describe('sidebar long press close', () => {
  it('waits two seconds, fills over two more seconds and consumes the trailing click', async () => {
    const button = setup()
    await pointer(button, 'pointerdown')
    await vi.advanceTimersByTimeAsync(1999)
    expect(ring()).toBeNull()
    await vi.advanceTimersByTimeAsync(1)
    expect(ring()?.getAttribute('aria-valuenow')).toBe('0')
    expect(ring()?.getAttribute('aria-label')).toContain('关闭编辑模块')
    expect(ring()?.style.left).toBe('90px')
    await vi.advanceTimersByTimeAsync(1000)
    expect(Number(ring()?.getAttribute('aria-valuenow'))).toBeGreaterThanOrEqual(49)
    expect(Number(ring()?.getAttribute('aria-valuenow'))).toBeLessThanOrEqual(51)
    expect(wrapper.emitted('remove')).toBeUndefined()
    await vi.advanceTimersByTimeAsync(1000)
    expect(wrapper.emitted('remove')).toEqual([['editor']])
    expect(ring()).toBeNull()
    await wrapper.setProps({ active: [] })
    await pointer(button, 'pointerup', { buttons: 0 })
    await button.trigger('click', { detail: 1 })
    expect(wrapper.emitted('module')).toBeUndefined()
    await vi.advanceTimersByTimeAsync(10_000)
    expect(wrapper.emitted('remove')).toHaveLength(1)
    // The next physical click can reopen the closed module.
    await pointer(button, 'pointerdown')
    await pointer(button, 'pointerup', { buttons: 0 })
    await button.trigger('click', { detail: 1 })
    expect(wrapper.emitted('module')).toEqual([['editor']])
  })

  it('keeps a short click and keyboard activation working', async () => {
    const button = setup()
    await pointer(button, 'pointerdown')
    await vi.advanceTimersByTimeAsync(500)
    await pointer(button, 'pointerup', { buttons: 0 })
    await button.trigger('click', { detail: 1 })
    await button.trigger('click', { detail: 0 })
    await vi.advanceTimersByTimeAsync(5000)
    expect(wrapper.emitted('module')).toEqual([['editor'], ['editor']])
    expect(wrapper.emitted('remove')).toBeUndefined()
    expect(ring()).toBeNull()
  })

  it('uses both configured durations and follows the mouse within the button', async () => {
    const button = setup({ holdDelayMs: 500, holdDurationMs: 1000 })
    await pointer(button, 'pointerdown')
    await vi.advanceTimersByTimeAsync(500)
    await pointer(button, 'pointermove', { clientX: 100, clientY: 125 })
    expect(ring()?.style.left).toBe('100px')
    expect(ring()?.style.top).toBe('125px')
    await vi.advanceTimersByTimeAsync(999)
    expect(wrapper.emitted('remove')).toBeUndefined()
    await vi.advanceTimersByTimeAsync(1)
    expect(wrapper.emitted('remove')).toEqual([['editor']])
  })

  it.each(['pointerup', 'pointerleave', 'pointercancel', 'blur', 'scroll', 'Escape'])(
    'cancels on %s without closing or activating the module',
    async (reason) => {
      const button = setup()
      await pointer(button, 'pointerdown')
      await vi.advanceTimersByTimeAsync(2500)
      expect(ring()).not.toBeNull()
      if (reason === 'Escape') window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
      else if (['blur', 'scroll'].includes(reason)) window.dispatchEvent(new Event(reason))
      else await pointer(button, reason, { buttons: 0 })
      await button.trigger('click', { detail: 1 })
      await vi.advanceTimersByTimeAsync(5000)
      expect(ring()).toBeNull()
      expect(wrapper.emitted('remove')).toBeUndefined()
      expect(wrapper.emitted('module')).toBeUndefined()
    },
  )

  it('cancels leaving the button even during the initial delay', async () => {
    const button = setup()
    await pointer(button, 'pointerdown')
    await vi.advanceTimersByTimeAsync(1000)
    await pointer(button, 'pointerleave')
    await vi.advanceTimersByTimeAsync(5000)
    expect(ring()).toBeNull()
    expect(wrapper.emitted('remove')).toBeUndefined()
  })

  it.each([{ disabled: true }, { settingsBusy: true }, { settingsActive: true }, { active: [] }])(
    'does not start when unavailable: %j',
    async (props) => {
      const button = setup(props)
      await pointer(button, 'pointerdown')
      await vi.advanceTimersByTimeAsync(5000)
      expect(ring()).toBeNull()
      expect(wrapper.emitted('remove')).toBeUndefined()
    },
  )

  it.each([{ button: 2, buttons: 2 }, { pointerType: 'touch' }, { pointerType: 'pen' }])(
    'ignores other input: %j',
    async (extra) => {
      const button = setup()
      await pointer(button, 'pointerdown', extra)
      await vi.advanceTimersByTimeAsync(5000)
      expect(ring()).toBeNull()
      expect(wrapper.emitted('remove')).toBeUndefined()
    },
  )

  it.each([{ disabled: true }, { settingsActive: true }, { active: [] }, { holdDelayMs: 500 }])(
    'cancels when the target or settings change: %j',
    async (props) => {
      const button = setup()
      await pointer(button, 'pointerdown')
      await vi.advanceTimersByTimeAsync(2500)
      await wrapper.setProps(props)
      await vi.advanceTimersByTimeAsync(5000)
      expect(ring()).toBeNull()
      expect(wrapper.emitted('remove')).toBeUndefined()
    },
  )

  it('cleans up an in-progress gesture when unmounted', async () => {
    const button = setup()
    await pointer(button, 'pointerdown')
    await vi.advanceTimersByTimeAsync(2500)
    wrapper.unmount()
    await vi.advanceTimersByTimeAsync(5000)
    expect(ring()).toBeNull()
    expect(wrapper.emitted('remove')).toBeUndefined()
  })
})
