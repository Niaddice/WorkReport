// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import JournalPreviewWindow from '../src/components/JournalPreviewWindow.vue'

function pointer(type: string, x: number, y: number) {
  return new PointerEvent(type, { bubbles: true, button: 0, pointerId: 1, clientX: x, clientY: y })
}
function geometry(element: HTMLElement) {
  return {
    x: parseFloat(element.style.left),
    y: parseFloat(element.style.top),
    width: parseFloat(element.style.width),
    height: parseFloat(element.style.height),
  }
}

describe('floating journal preview window', () => {
  it('moves, resizes and maximizes without changing the stored normal size', async () => {
    const wrapper = mount(JournalPreviewWindow, {
      props: { visible: true },
      global: { stubs: { teleport: true } },
      slots: { default: '<p>预览正文</p>' },
    })
    try {
      const panel = wrapper.get<HTMLElement>('.journal-preview-window')
      const initial = geometry(panel.element)
      wrapper.get('.journal-preview-window-header').element.dispatchEvent(pointer('pointerdown', 400, 100))
      window.dispatchEvent(pointer('pointermove', 300, 120))
      window.dispatchEvent(pointer('pointerup', 300, 120))
      await wrapper.vm.$nextTick()
      const moved = geometry(wrapper.get<HTMLElement>('.journal-preview-window').element)
      expect(moved.x).toBe(initial.x - 100)
      expect(moved.y).toBe(initial.y + 20)
      wrapper.get('.journal-preview-window-resize').element.dispatchEvent(pointer('pointerdown', 600, 500))
      window.dispatchEvent(pointer('pointermove', 680, 540))
      window.dispatchEvent(pointer('pointerup', 680, 540))
      await wrapper.vm.$nextTick()
      const resized = geometry(wrapper.get<HTMLElement>('.journal-preview-window').element)
      expect(resized.width).toBe(initial.width + 80)
      expect(resized.height).toBe(initial.height + 40)
      await wrapper.get('[aria-label="最大化日报预览"]').trigger('click')
      expect(wrapper.get('.journal-preview-window').classes()).toContain('is-maximized')
      expect(geometry(wrapper.get<HTMLElement>('.journal-preview-window').element).width).toBe(window.innerWidth - 16)
      expect(wrapper.find('.journal-preview-window-resize').exists()).toBe(false)
      await wrapper.get('[aria-label="还原日报预览"]').trigger('click')
      expect(geometry(wrapper.get<HTMLElement>('.journal-preview-window').element)).toEqual(resized)
      await wrapper.get('[aria-label="最大化日报预览"]').trigger('click')
      await wrapper.setProps({ visible: false })
      expect(wrapper.find('.journal-preview-window').exists()).toBe(false)
      await wrapper.setProps({ visible: true })
      expect(wrapper.get('.journal-preview-window').classes()).not.toContain('is-maximized')
      expect(geometry(wrapper.get<HTMLElement>('.journal-preview-window').element)).toEqual(resized)
    } finally {
      wrapper.unmount()
    }
    expect(document.body.classList.contains('is-preview-window-moving')).toBe(false)
    expect(document.body.classList.contains('is-preview-window-resizing')).toBe(false)
  })

  it('supports keyboard resizing, escape and closing, and stays inside the viewport', async () => {
    const wrapper = mount(JournalPreviewWindow, {
      props: { visible: true, topInset: 36 },
      global: { stubs: { teleport: true } },
    })
    try {
      const panel = wrapper.get<HTMLElement>('.journal-preview-window')
      const before = geometry(panel.element)
      await wrapper.get('.journal-preview-window-resize').trigger('keydown', { key: 'ArrowRight' })
      expect(geometry(wrapper.get<HTMLElement>('.journal-preview-window').element).width).toBe(before.width + 10)
      wrapper.get('.journal-preview-window-header').element.dispatchEvent(pointer('pointerdown', 0, 0))
      window.dispatchEvent(pointer('pointermove', -10000, -10000))
      window.dispatchEvent(pointer('pointerup', -10000, -10000))
      await wrapper.vm.$nextTick()
      expect(geometry(wrapper.get<HTMLElement>('.journal-preview-window').element).x).toBe(8)
      expect(geometry(wrapper.get<HTMLElement>('.journal-preview-window').element).y).toBe(44)
      await wrapper.get('.journal-preview-window-header').trigger('dblclick')
      expect(wrapper.get('.journal-preview-window').classes()).toContain('is-maximized')
      await wrapper.get('.journal-preview-window').trigger('keydown', { key: 'Escape' })
      expect(wrapper.get('.journal-preview-window').classes()).not.toContain('is-maximized')
      await wrapper.get('.journal-preview-window').trigger('keydown', { key: 'Escape' })
      expect(wrapper.emitted('close')).toHaveLength(1)
      await wrapper.get('[aria-label="关闭日报预览"]').trigger('click')
      expect(wrapper.emitted('close')).toHaveLength(2)
    } finally {
      wrapper.unmount()
    }
  })
})
