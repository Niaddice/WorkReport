// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from 'vitest'
import { mount, type VueWrapper } from '@vue/test-utils'
import SettingHint from '../src/components/SettingHint.vue'

const wrappers: VueWrapper[] = []

afterEach(() => {
  wrappers.splice(0).forEach((wrapper) => wrapper.unmount())
  document.body.innerHTML = ''
})

describe('SettingHint', () => {
  it('renders a hint trigger button with data-tooltip, title, and aria-label', () => {
    const hintText = '仅调整字体大小，保存后应用到整个 App。'
    const wrapper = mount(SettingHint, {
      props: { text: hintText },
      attachTo: document.body,
    })
    wrappers.push(wrapper)

    const btn = wrapper.find('button.setting-hint-trigger')
    expect(btn.exists()).toBe(true)
    expect(btn.attributes('data-tooltip')).toBe(hintText)
    expect(btn.attributes('title')).toBe(hintText)
    expect(btn.attributes('aria-label')).toBe(`提示：${hintText}`)
    expect(btn.attributes('type')).toBe('button')
    expect(btn.find('svg').exists()).toBe(true)
  })

  it('updates attributes reactively when text prop changes', async () => {
    const wrapper = mount(SettingHint, {
      props: { text: '旧的提示文字' },
      attachTo: document.body,
    })
    wrappers.push(wrapper)

    const btn = wrapper.get('button.setting-hint-trigger')
    expect(btn.attributes('data-tooltip')).toBe('旧的提示文字')

    await wrapper.setProps({ text: '新的提示文字' })
    expect(btn.attributes('data-tooltip')).toBe('新的提示文字')
    expect(btn.attributes('title')).toBe('新的提示文字')
    expect(btn.attributes('aria-label')).toBe('提示：新的提示文字')
  })
})
