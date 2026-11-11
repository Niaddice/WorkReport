// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import ShortcutsDialog from '../src/components/ShortcutsDialog.vue'

describe('ShortcutsDialog', () => {
  let wrappers: ReturnType<typeof mount>[] = []

  beforeEach(() => {
    wrappers = []
  })

  afterEach(() => {
    wrappers.forEach((w) => w.unmount())
    wrappers = []
    document.body.innerHTML = ''
    vi.restoreAllMocks()
  })

  function mountDialog(visible = true) {
    const wrapper = mount(ShortcutsDialog, {
      attachTo: document.body,
      props: { visible },
    })
    wrappers.push(wrapper)
    return wrapper
  }

  it('renders shortcuts dialog and lists key combinations', async () => {
    const wrapper = mountDialog(true)
    await flushPromises()

    const dialog = document.querySelector('#shortcuts-dialog')
    expect(dialog).not.toBeNull()
    expect(document.querySelector('.shortcuts-title')?.textContent).toContain('快捷键速查与高效指南')

    const cards = document.querySelectorAll('.shortcut-card')
    expect(cards.length).toBeGreaterThan(5)

    const closeBtn = document.querySelector<HTMLButtonElement>('.shortcuts-close-btn')
    closeBtn?.click()
    expect(wrapper.emitted('close')).toHaveLength(1)
  })

  it('filters shortcuts when category tab is selected', async () => {
    mountDialog(true)
    await flushPromises()

    const tabs = Array.from(document.querySelectorAll<HTMLButtonElement>('.category-tab-btn'))
    const editorTab = tabs.find((t) => t.textContent?.includes('日报编辑'))
    expect(editorTab).not.toBeUndefined()
    editorTab?.click()
    await flushPromises()

    const sectionTitle = document.querySelector('.group-title')?.textContent
    expect(sectionTitle).toBe('日报编辑')

    const cardTitles = Array.from(document.querySelectorAll('.shortcut-name')).map((el) => el.textContent)
    expect(cardTitles).toContain('立即保存草稿')
    expect(cardTitles).toContain('撤销上一步')
    expect(cardTitles).not.toContain('多仓库一键 Fetch')
  })

  it('filters shortcuts when user types in search input', async () => {
    mountDialog(true)
    await flushPromises()

    const input = document.querySelector<HTMLInputElement>('.shortcuts-search-input')
    expect(input).not.toBeNull()
    if (input) {
      input.value = 'Fetch'
      input.dispatchEvent(new Event('input'))
    }
    await flushPromises()

    const cardTitles = Array.from(document.querySelectorAll('.shortcut-name')).map((el) => el.textContent)
    expect(cardTitles).toContain('多仓库一键 Fetch')
    expect(cardTitles).not.toContain('立即保存草稿')
  })

  it('emits close on Escape keydown', async () => {
    const wrapper = mountDialog(true)
    await flushPromises()

    const dialog = document.querySelector<HTMLDialogElement>('#shortcuts-dialog')
    dialog?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    expect(wrapper.emitted('close')).toHaveLength(1)
  })
})
