// @vitest-environment happy-dom
import { describe, expect, it, vi } from 'vitest'
import { markRaw } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'
import ModulePanel from '../src/components/ModulePanel.vue'

const icon = markRaw({ template: '<span class="test-icon" />' })

function mountPanel(overrides: Record<string, unknown> = {}, subtitle = '') {
  return mount(ModulePanel, {
    attachTo: document.body,
    slots: { subtitle },
    props: {
      id: 'overview',
      title: '数据概览',
      icon,
      canRemove: true,
      dragging: false,
      ...overrides,
    },
  })
}

describe('module panel', () => {
  it('leaves the previous module input when clicking another module without focusing a control', async () => {
    const editor = mount(ModulePanel, {
      attachTo: document.body,
      props: { id: 'editor', title: '日报编辑', icon, canRemove: true, dragging: false },
      slots: { default: '<textarea aria-label="测试日报" />' },
    })
    const calendar = mountPanel({ id: 'calendar', title: '日历' })
    try {
      const input = editor.get<HTMLTextAreaElement>('textarea').element
      input.focus()
      await editor.get('.module-panel-body').trigger('pointerdown')
      expect(document.activeElement).toBe(input)

      await calendar.get('.module-panel-body').trigger('pointerdown')
      expect(document.activeElement).not.toBe(input)
      expect(calendar.emitted('activate')).toBeDefined()

      input.focus()
      await calendar.get('.module-panel-header').trigger('click')
      expect(document.activeElement).not.toBe(input)
    } finally {
      editor.unmount()
      calendar.unmount()
    }
  })

  it('anchors help beside the icon, keeps internal clicks open and dismisses outside clicks', async () => {
    const wrapper = mountPanel({ help: ['说明内容 A', '说明内容 B'] })
    const button = wrapper.get<HTMLButtonElement>('.module-help-button')
    button.element.getBoundingClientRect = () => new DOMRect(600, 80, 24, 24)
    try {
      await flushPromises()
      await button.trigger('click')
      await flushPromises()
      const popup = document.querySelector<HTMLElement>('.module-help-popover')!
      expect(document.body.contains(popup)).toBe(true)
      expect(wrapper.element.contains(popup)).toBe(false)
      expect(popup.textContent).toContain('说明内容 A')
      expect(popup.style.left).toBe('264px')
      expect(popup.style.top).toBe('110px')
      expect(button.attributes('aria-expanded')).toBe('true')
      expect(button.attributes('aria-controls')).toBe(popup.id)
      popup.querySelector('li')!.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
      popup.querySelector('li')!.dispatchEvent(new MouseEvent('click', { bubbles: true }))
      await flushPromises()
      expect(button.attributes('aria-expanded')).toBe('true')

      document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
      await flushPromises()
      expect(document.querySelector('.module-help-popover')).toBeNull()
      expect(button.attributes('aria-expanded')).toBe('false')
      await button.trigger('click')
      await flushPromises()
      await button.trigger('click')
      await flushPromises()
      expect(button.attributes('aria-expanded')).toBe('false')
    } finally {
      wrapper.unmount()
    }
  })

  it('keeps only one help popover open when another module explanation is clicked', async () => {
    const first = mountPanel({ help: ['第一个模块说明'] })
    const second = mountPanel({ id: 'history', title: '历史记录', help: ['第二个模块说明'] })
    try {
      await flushPromises()
      await first.get('.module-help-button').trigger('click')
      await flushPromises()
      await second.get('.module-help-button').trigger('click')
      await flushPromises()
      expect(document.querySelectorAll('.module-help-popover')).toHaveLength(1)
      expect(document.querySelector('.module-help-popover')!.textContent).toContain('第二个模块说明')
      expect(first.get('.module-help-button').attributes('aria-expanded')).toBe('false')
      expect(second.get('.module-help-button').attributes('aria-expanded')).toBe('true')
    } finally {
      first.unmount()
      second.unmount()
    }
  })

  it('flips above the icon near the viewport edge and restores focus on Escape', async () => {
    const height = vi.spyOn(HTMLElement.prototype, 'scrollHeight', 'get').mockImplementation(function (
      this: HTMLElement,
    ) {
      return this.classList.contains('module-help-popover') ? 220 : 0
    })
    const wrapper = mountPanel({ help: ['较长的模块说明'] })
    const button = wrapper.get<HTMLButtonElement>('.module-help-button')
    button.element.getBoundingClientRect = () => new DOMRect(window.innerWidth - 32, window.innerHeight - 32, 24, 24)
    try {
      await flushPromises()
      await button.trigger('click')
      await flushPromises()
      const popup = document.querySelector<HTMLElement>('.module-help-popover')!
      expect(popup.dataset.placement).toBe('top')
      expect(parseFloat(popup.style.left)).toBe(window.innerWidth - 368)
      expect(parseFloat(popup.style.top) + 220).toBeLessThan(window.innerHeight - 32)
      popup.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
      await flushPromises()
      expect(document.querySelector('.module-help-popover')).toBeNull()
      expect(document.activeElement).toBe(button.element)
    } finally {
      wrapper.unmount()
      height.mockRestore()
    }
  })

  it('renders the module chrome without a drag button or position counter', () => {
    const wrapper = mountPanel()
    expect(wrapper.find('.module-panel-title').text()).toBe('数据概览')
    expect(wrapper.find('.module-panel-position').exists()).toBe(false)
    expect(wrapper.find('.module-drag-handle').exists()).toBe(false)
    expect(wrapper.find('.module-maximize-button').attributes('aria-label')).toContain('放大')
    wrapper.unmount()
  })

  it('hides the remove action when only one module remains', () => {
    const last = mountPanel({ id: 'calendar', title: '日历', canRemove: false })
    expect(last.find('.module-panel-remove').exists()).toBe(false)
    last.unmount()
  })

  it('emits maximize, remove and drag gestures', async () => {
    const wrapper = mountPanel()
    await wrapper.find('.module-maximize-button').trigger('click')
    expect(wrapper.emitted('toggle-maximize')).toHaveLength(1)

    await wrapper.find('.module-panel-remove').trigger('click')
    expect(wrapper.emitted('remove')).toHaveLength(1)

    await wrapper.find('.module-panel-header').trigger('pointerdown', { button: 0 })
    expect(wrapper.emitted('drag-start')).toHaveLength(1)
    wrapper.unmount()
  })

  it('keeps title and date text plus action buttons out of the drag area', async () => {
    const wrapper = mountPanel(
      { help: ['说明内容'] },
      '<span class="module-panel-subtitle"><span>星期三</span><span>10 月 28 日</span></span>',
    )
    try {
      for (const selector of [
        '.module-panel-title',
        '.module-panel-subtitle span',
        '.module-help-button',
        '.module-help-button svg',
        '.module-maximize-button',
        '.module-maximize-button svg',
        '.module-panel-remove',
        '.module-panel-remove svg',
      ]) {
        await wrapper.get(selector).trigger('pointerdown', { button: 0 })
      }
      expect(wrapper.emitted('drag-start')).toBeUndefined()
    } finally {
      wrapper.unmount()
    }
  })

  it('ignores non-left clicks and header drags while maximized', async () => {
    const wrapper = mountPanel()
    try {
      await wrapper.get('.module-panel-header').trigger('pointerdown', { button: 2 })
      await wrapper.setProps({ maximized: true })
      for (const selector of ['.module-panel-header', '.module-panel-icon']) {
        await wrapper.get(selector).trigger('pointerdown', { button: 0 })
      }
      expect(wrapper.emitted('drag-start')).toBeUndefined()
      await wrapper.setProps({ maximized: false })
      await wrapper.get('.module-panel-header').trigger('pointerdown', { button: 0 })
      expect(wrapper.emitted('drag-start')).toHaveLength(1)
    } finally {
      wrapper.unmount()
    }
  })

  it('emits maximize requests and reflects the maximized state', async () => {
    const normal = mountPanel()
    await normal.find('.module-maximize-button').trigger('click')
    expect(normal.emitted('toggle-maximize')).toHaveLength(1)
    normal.unmount()

    const maximized = mountPanel({ maximized: true })
    expect(maximized.classes()).toContain('is-maximized')
    expect(maximized.attributes('role')).toBe('dialog')
    expect(maximized.attributes('aria-modal')).toBe('true')
    expect(maximized.find('.module-panel-remove').exists()).toBe(false)
    expect(maximized.find('.module-drag-handle').exists()).toBe(false)
    expect(maximized.find('.module-maximize-button').attributes('aria-label')).toContain('还原')
    await maximized.find('.module-maximize-button').trigger('click')
    expect(maximized.emitted('toggle-maximize')).toHaveLength(1)
    maximized.unmount()
  })
})
