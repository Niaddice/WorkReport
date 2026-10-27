// @vitest-environment happy-dom
import { flushPromises, mount } from '@vue/test-utils'
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { ref } from 'vue'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))

import DirectoryModule from '../src/components/modules/DirectoryModule.vue'
import { MODULE_CONTEXT, type ModuleContext } from '../src/modules/context'
import type { FileNode } from '../src/lib/workspace'

beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function showModal(this: HTMLDialogElement) {
    this.setAttribute('open', '')
  }
  HTMLDialogElement.prototype.close = function close(this: HTMLDialogElement) {
    this.removeAttribute('open')
  }
})

function mountDirectory(files: FileNode[], selectedDate = '20261027') {
  const selectReport = vi.fn()
  const context = {
    journal: {
      workspace: ref({ path: '/tmp/workspace', name: 'workspace' }),
      entries: ref([]),
      selectedDate: ref(selectedDate),
      busy: ref(false),
    },
    disabled: ref(false),
    actions: { selectReport },
  }
  invoke.mockImplementation((command: string, args?: { path?: string }) => {
    if (command === 'list_workspace_files') return Promise.resolve(files)
    if (command === 'read_workspace_excel')
      return Promise.resolve({ filename: args?.path?.split('/').pop() ?? '', path: args?.path ?? '', sheets: [] })
    return Promise.resolve(undefined)
  })
  const wrapper = mount(DirectoryModule, {
    global: { provide: { [MODULE_CONTEXT as symbol]: context as unknown as ModuleContext } },
  })
  return { wrapper, selectReport }
}

const files: FileNode[] = [
  { path: '2026', name: '2026', isDir: true, size: 0 },
  { path: '2026/20261027.md', name: '20261027.md', isDir: false, size: 1024 },
  { path: '2026/20261020.md', name: '20261020.md', isDir: false, size: 512 },
  { path: '2025', name: '2025', isDir: true, size: 0 },
  { path: '2025/20250915.txt', name: '20250915.txt', isDir: false, size: 2048 },
  { path: '总结', name: '总结', isDir: true, size: 0 },
  { path: '总结/周报.md', name: '周报.md', isDir: false, size: 100 },
  { path: '周报.xlsx', name: '周报.xlsx', isDir: false, size: 300 },
  { path: 'notes.md', name: 'notes.md', isDir: false, size: 50 },
]

function fileButton(wrapper: Awaited<ReturnType<typeof mountReady>>['wrapper'], name: string) {
  return wrapper.findAll('.directory-item').find((button) => button.find('.directory-item-name').text() === name)!
}
function dirButton(wrapper: Awaited<ReturnType<typeof mountReady>>['wrapper'], name: string) {
  return wrapper.findAll('.directory-tree-node').find((button) => button.find('.directory-tree-label').text() === name)!
}

async function mountReady(files: FileNode[], selectedDate?: string) {
  const { wrapper, selectReport } = mountDirectory(files, selectedDate)
  await flushPromises()
  return { wrapper, selectReport }
}

beforeEach(() => {
  invoke.mockReset()
})

describe('directory module', () => {
  it('renders the workspace file tree and highlights the current report', async () => {
    const { wrapper } = await mountReady(files)

    expect(wrapper.findAll('.directory-tree-node')).toHaveLength(3)
    expect(wrapper.findAll('.directory-item')).toHaveLength(6)
    expect(wrapper.find('.directory-module-count').text()).toContain('6 个文件')

    expect(fileButton(wrapper, '20261027.md').classes()).toContain('is-current')
    expect(fileButton(wrapper, '20261027.md').find('.directory-item-check').exists()).toBe(true)
    expect(fileButton(wrapper, '20261020.md').classes()).not.toContain('is-current')

    wrapper.unmount()
  })

  it('lets report files switch dates and excel files open a preview', async () => {
    const { wrapper, selectReport } = await mountReady(files)

    const report = fileButton(wrapper, '20261020.md')
    expect((report.element as HTMLButtonElement).disabled).toBe(false)
    await report.trigger('click')
    expect(selectReport).toHaveBeenCalledExactlyOnceWith('20261020')

    const excel = fileButton(wrapper, '周报.xlsx')
    expect((excel.element as HTMLButtonElement).disabled).toBe(false)
    expect(excel.classes()).toContain('is-excel')

    // 非日报、非 Excel 文件不可点击（禁用）。
    expect((fileButton(wrapper, 'notes.md').element as HTMLButtonElement).disabled).toBe(true)
    expect((fileButton(wrapper, '周报.md').element as HTMLButtonElement).disabled).toBe(true)

    wrapper.unmount()
  })

  it('previews an excel file in the export dialog', async () => {
    const { wrapper } = await mountReady(files)

    await fileButton(wrapper, '周报.xlsx').trigger('click')
    await flushPromises()

    expect(invoke).toHaveBeenCalledWith('read_workspace_excel', {
      workspacePath: '/tmp/workspace',
      path: '周报.xlsx',
    })
    expect(wrapper.find('.excel-export-preview-dialog').exists()).toBe(true)
    expect(wrapper.find('.excel-export-preview-header strong').text()).toBe('周报.xlsx')

    wrapper.unmount()
  })

  it('ignores clicking the already selected report', async () => {
    const { wrapper, selectReport } = await mountReady(files)
    await fileButton(wrapper, '20261027.md').trigger('click')
    expect(selectReport).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  it('collapses and expands directory nodes', async () => {
    const { wrapper } = await mountReady(files)

    await dirButton(wrapper, '2026').trigger('click')
    expect(dirButton(wrapper, '2026').attributes('aria-expanded')).toBe('false')
    expect(wrapper.findAll('.directory-item')).toHaveLength(4)

    await dirButton(wrapper, '2026').trigger('click')
    expect(wrapper.findAll('.directory-item')).toHaveLength(6)

    wrapper.unmount()
  })

  it('shows an empty hint when the workspace has no files', async () => {
    const { wrapper } = await mountReady([])
    expect(wrapper.findAll('.directory-item')).toHaveLength(0)
    expect(wrapper.find('.directory-empty').text()).toContain('还没有文件')
    wrapper.unmount()
  })
})
