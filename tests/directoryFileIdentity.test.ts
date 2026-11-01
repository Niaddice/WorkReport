// @vitest-environment happy-dom
import { flushPromises, mount } from '@vue/test-utils'
import { expect, it, vi } from 'vitest'
import { ref } from 'vue'
import DirectoryModule from '../src/components/modules/DirectoryModule.vue'
import { MODULE_CONTEXT, type ModuleContext } from '../src/modules/context'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))

it('opens renamed journal files by backend date and respects explicit non-journal identity', async () => {
  invoke.mockResolvedValue([
    { path: '工作日报', name: '工作日报', isDir: true, size: 0, journalDate: null },
    { path: '工作日报/2026', name: '2026', isDir: true, size: 0, journalDate: null },
    {
      path: '工作日报/2026/完成_2026年10月30日.md',
      name: '完成_2026年10月30日.md',
      isDir: false,
      size: 10,
      journalDate: '20261030',
    },
    { path: '2026', name: '2026', isDir: true, size: 0, journalDate: null },
    { path: '2026/20261029.md', name: '20261029.md', isDir: false, size: 10, journalDate: null },
  ])
  const selectReport = vi.fn()
  const context = {
    journal: {
      workspace: ref({ path: '/workspace', name: 'workspace' }),
      entries: ref([]),
      selectedDate: ref('20261031'),
      busy: ref(false),
    },
    disabled: ref(false),
    actions: { selectReport },
  }
  const wrapper = mount(DirectoryModule, {
    global: { provide: { [MODULE_CONTEXT as symbol]: context as unknown as ModuleContext } },
  })
  await flushPromises()
  const buttons = wrapper.findAll('.directory-item')
  const custom = buttons.find((button) => button.text().includes('完成_2026年10月30日.md'))!
  const ordinary = buttons.find((button) => button.text().includes('20261029.md'))!
  expect((custom.element as HTMLButtonElement).disabled).toBe(false)
  await custom.trigger('click')
  expect(selectReport).toHaveBeenCalledExactlyOnceWith('20261030')
  expect((ordinary.element as HTMLButtonElement).disabled).toBe(false)
  await ordinary.trigger('click')
  expect(selectReport).toHaveBeenCalledTimes(1)
  wrapper.unmount()
})
