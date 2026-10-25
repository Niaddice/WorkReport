// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest'
import { markRaw } from 'vue'
import { mount } from '@vue/test-utils'
import ModulePanel from '../src/components/ModulePanel.vue'

const icon = markRaw({ template: '<span class="test-icon" />' })

function mountPanel(overrides: Record<string, unknown> = {}) {
  return mount(ModulePanel, {
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
  it('renders the module chrome and drag handle without a position counter', () => {
    const wrapper = mountPanel()
    expect(wrapper.find('.module-panel-title').text()).toBe('数据概览')
    expect(wrapper.find('.module-panel-position').exists()).toBe(false)
    expect(wrapper.find('.module-drag-handle').attributes('aria-label')).toBe('拖动「数据概览」模块到其他模块上分栏')
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

    await wrapper.find('.module-drag-handle').trigger('pointerdown', { button: 0 })
    expect(wrapper.emitted('drag-start')).toHaveLength(1)
    wrapper.unmount()
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
    expect(maximized.find('.module-drag-handle').attributes('disabled')).toBeDefined()
    expect(maximized.find('.module-maximize-button').attributes('aria-label')).toContain('还原')
    await maximized.find('.module-maximize-button').trigger('click')
    expect(maximized.emitted('toggle-maximize')).toHaveLength(1)
    maximized.unmount()
  })
})
