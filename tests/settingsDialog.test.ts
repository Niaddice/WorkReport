// @vitest-environment happy-dom
import { describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import SettingsDialog from '../src/components/SettingsDialog.vue'
import { MAXIMIZE_DURATION, RESTORE_DURATION, MAXIMIZE_EASING, RESTORE_EASING } from '../src/lib/windowMotion'

function animatedWindow() {
  const frame = new DOMRect(8, 44, 1200, 640)
  let visualRect = frame
  const settingsButton = new DOMRect(1240, 660, 32, 32)
  const headerButton = new DOMRect(1160, 52, 28, 28)
  const wrapper = mount(SettingsDialog, {
    props: { visible: false, busy: false, buttonRect: () => settingsButton },
    global: { stubs: { transition: false } },
    attachTo: document.body,
  })
  const panel = wrapper.get<HTMLElement>('.settings-dialog').element
  const backdrop = wrapper.get<HTMLElement>('.settings-dialog-backdrop').element
  panel.getBoundingClientRect = () => visualRect
  wrapper.get<HTMLElement>('.settings-dialog-close').element.getBoundingClientRect = () => headerButton
  const animations: {
    target: HTMLElement
    frames: Keyframe[]
    options: KeyframeAnimationOptions
    finish: () => void
    cancel: ReturnType<typeof vi.fn>
  }[] = []
  for (const target of [panel, backdrop])
    target.animate = ((frames: Keyframe[], options: KeyframeAnimationOptions) => {
      let finish!: () => void
      let reject!: (cause: unknown) => void
      const finished = new Promise<void>((resolve, fail) => {
        finish = resolve
        reject = fail
      })
      const cancel = vi.fn(() => {
        if (target === panel) visualRect = frame
        reject(new DOMException('Cancelled', 'AbortError'))
      })
      animations.push({ target, frames, options, finish, cancel })
      return { finished, cancel } as unknown as Animation
    }) as typeof target.animate
  return { wrapper, panel, animations, setVisualRect: (rect: DOMRect) => (visualRect = rect) }
}

describe('maximized settings dialog', () => {
  it('focuses the settings window and restores the opener when it closes', async () => {
    const opener = document.createElement('button')
    document.body.append(opener)
    opener.focus()
    const wrapper = mount(SettingsDialog, {
      props: { visible: false, busy: false },
      attachTo: document.body,
      slots: { default: '<input aria-label="设置内容" />' },
    })
    try {
      await wrapper.setProps({ visible: true })
      await wrapper.vm.$nextTick()
      expect(document.activeElement).toBe(wrapper.get('.settings-dialog').element)
      expect(wrapper.get('.settings-dialog').isVisible()).toBe(true)
      expect(wrapper.get('.settings-dialog').attributes('aria-labelledby')).toBe(
        wrapper.get('.settings-dialog-heading h2').attributes('id'),
      )
      wrapper.get<HTMLInputElement>('input').element.focus()
      await wrapper.setProps({ visible: false })
      await wrapper.vm.$nextTick()
      expect(wrapper.get('.settings-dialog').isVisible()).toBe(false)
      expect(wrapper.get('.settings-dialog').attributes('inert')).toBeDefined()
      expect(document.activeElement).toBe(opener)
    } finally {
      wrapper.unmount()
      opener.remove()
    }
  })

  it('routes close actions while respecting busy operations and nested modal dialogs', async () => {
    const wrapper = mount(SettingsDialog, { props: { visible: true, busy: false }, attachTo: document.body })
    const nested = document.createElement('dialog')
    try {
      await wrapper.get('[aria-label="关闭设置"]').trigger('click')
      await wrapper.get('.settings-dialog-backdrop').trigger('click')
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
      expect(wrapper.emitted('close')).toEqual([['close'], ['settings'], ['settings']])

      await wrapper.setProps({ busy: true })
      expect(wrapper.get('[aria-label="关闭设置"]').attributes('disabled')).toBeDefined()
      await wrapper.get('.settings-dialog-backdrop').trigger('click')
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
      expect(wrapper.emitted('close')).toHaveLength(3)

      await wrapper.setProps({ busy: false })
      nested.setAttribute('open', '')
      document.body.append(nested)
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
      expect(wrapper.emitted('close')).toHaveLength(3)
      nested.remove()
      const handledEscape = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
      handledEscape.preventDefault()
      window.dispatchEvent(handledEscape)
      expect(wrapper.emitted('close')).toHaveLength(3)
    } finally {
      wrapper.unmount()
      nested.remove()
    }
  })

  it.each([
    { target: 'settings' as const, transform: 'translate(1232px, 616px) scale(0.02666666666666667, 0.05)' },
    { target: 'close' as const, transform: 'translate(1152px, 8px) scale(0.023333333333333334, 0.04375)' },
  ])('expands from settings and waits for the shrink toward $target before hiding', async ({ target, transform }) => {
    const { wrapper, panel, animations } = animatedWindow()
    try {
      await wrapper.setProps({ visible: true })
      expect(animations).toHaveLength(2)
      expect(animations[0]!.target).toBe(panel)
      expect(animations[0]!.frames[0]!.transform).toBe('translate(1232px, 616px) scale(0.02666666666666667, 0.05)')
      expect(animations[0]!.options).toMatchObject({ duration: MAXIMIZE_DURATION, easing: MAXIMIZE_EASING })
      animations.slice(0, 2).forEach((animation) => animation.finish())
      await flushPromises()

      await wrapper.setProps({ visible: false, closeTarget: target })
      expect(wrapper.get('.settings-dialog').isVisible()).toBe(true)
      expect(animations[2]!.frames[1]!.transform).toBe(transform)
      expect(animations[2]!.options).toMatchObject({ duration: RESTORE_DURATION, easing: RESTORE_EASING })
      animations.slice(2).forEach((animation) => animation.finish())
      await flushPromises()
      expect(wrapper.get('.settings-dialog').isVisible()).toBe(false)
    } finally {
      wrapper.unmount()
    }
  })

  it('continues a quickly reopened window from its current rectangle without a stale close hiding it', async () => {
    const { wrapper, animations, setVisualRect } = animatedWindow()
    try {
      await wrapper.setProps({ visible: true })
      animations.slice().forEach((animation) => animation.finish())
      await flushPromises()
      await wrapper.setProps({ visible: false })
      const closing = animations.slice(2)
      setVisualRect(new DOMRect(608, 364, 600, 320))
      await wrapper.setProps({ visible: true })
      await flushPromises()
      expect(closing.every((animation) => animation.cancel.mock.calls.length > 0)).toBe(true)
      expect(animations[4]!.frames[0]!.transform).toBe('translate(600px, 320px) scale(0.5, 0.5)')
      closing.forEach((animation) => animation.finish())
      await flushPromises()
      expect(wrapper.get('.settings-dialog').isVisible()).toBe(true)
      animations.slice(4).forEach((animation) => animation.finish())
      await flushPromises()
      expect(wrapper.get('.settings-dialog').isVisible()).toBe(true)
    } finally {
      wrapper.unmount()
    }
  })

  it('honors reduced motion without waiting for a window animation', async () => {
    const media = vi.spyOn(window, 'matchMedia').mockReturnValue({ matches: true } as MediaQueryList)
    const { wrapper, animations } = animatedWindow()
    try {
      await wrapper.setProps({ visible: true })
      expect(animations).toHaveLength(0)
      await wrapper.setProps({ visible: false })
      expect(wrapper.get('.settings-dialog').isVisible()).toBe(false)
    } finally {
      wrapper.unmount()
      media.mockRestore()
    }
  })
})
