// @vitest-environment happy-dom
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { ref } from 'vue'
import DirectoryModule from '../src/components/modules/DirectoryModule.vue'
import { MODULE_CONTEXT, type ModuleContext } from '../src/modules/context'
import { cloneFileSettings } from '../src/lib/fileSettings'
import type { FileNode } from '../src/lib/workspace'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function () {
    this.open = true
  }
  HTMLDialogElement.prototype.close = function () {
    this.open = false
  }
})
beforeEach(() => invoke.mockReset())
async function setup() {
  const files: FileNode[] = [
    { path: '资料', name: '资料', isDir: true, size: 0 },
    { path: '资料/notes.txt', name: 'notes.txt', isDir: false, size: 5, journalDate: null },
    { path: '2026', name: '2026', isDir: true, size: 0 },
    { path: '2026/项目.md', name: '项目.md', isDir: false, size: 10, journalDate: '20261101' },
    { path: 'work.sqlite', name: 'work.sqlite', isDir: false, size: 10, protected: true },
  ]
  const workspace = ref({ path: '/workspace', name: 'workspace' })
  const change = vi.fn().mockResolvedValue({ files: cloneFileSettings(), path: '资料/new.txt' })
  const flush = vi.fn().mockResolvedValue(true)
  const selectReport = vi.fn()
  const openWorkspaceFile = vi.fn().mockResolvedValue(undefined)
  const focusEditor = vi.fn()
  invoke.mockImplementation(async (command) => {
    if (command === 'list_workspace_files') return [...files]
    if (command === 'preview_workspace_file_change')
      return { token: 'snapshot', fileCount: 1, directoryCount: 0, journalCount: 1, settingsChanged: false }
    if (command === 'list_workspace_trash') return []
  })
  const context = {
    journal: { workspace, entries: ref([]), selectedDate: ref('20261101'), busy: ref(false), flush },
    disabled: ref(false),
    storageMode: ref('files'),
    actions: { changeWorkspaceFiles: change, selectReport, openWorkspaceFile },
    editor: ref({ focus: focusEditor }),
  }
  const wrapper = mount(DirectoryModule, {
    attachTo: document.body,
    global: { stubs: { teleport: true }, provide: { [MODULE_CONTEXT as symbol]: context as unknown as ModuleContext } },
  })
  await flushPromises()
  return { wrapper, files, workspace, change, flush, selectReport, openWorkspaceFile, focusEditor }
}
function menuItem(wrapper: ReturnType<typeof mount>, text: string) {
  return wrapper.findAll('[role="menuitem"]').find((item) => item.text() === text)!
}
describe('workspace directory context operations', () => {
  it('shows a menu on ordinary files and creates a file in that file’s parent', async () => {
    const { wrapper, change, files, openWorkspaceFile, focusEditor } = await setup()
    try {
      await wrapper.get('[data-file-path="资料/notes.txt"]').trigger('contextmenu', { clientX: 200, clientY: 100 })
      expect(wrapper.get('[role="menu"]').attributes('aria-label')).toBe('文件与目录操作')
      await menuItem(wrapper, '新建文件…').trigger('click')
      expect(wrapper.find('dialog').exists()).toBe(false)
      const input = wrapper.get<HTMLInputElement>('.directory-new-node-name')
      expect(document.activeElement).toBe(input.element)
      expect(input.element.selectionStart).toBe(0)
      expect(input.element.selectionEnd).toBe(input.element.value.length)
      expect(input.element.value).toBe('')
      expect(wrapper.get('.directory-new-node').attributes('aria-level')).toBe('2')
      expect(change).not.toHaveBeenCalled()
      change.mockImplementationOnce(async () => {
        files.push({ path: '资料/new.txt', name: 'new.txt', isDir: false, size: 0, journalDate: null })
        return { files: cloneFileSettings(), path: '资料/new.txt' }
      })
      await input.setValue('new.txt')
      await input.trigger('keydown', { key: 'Enter' })
      await flushPromises()
      expect(change).toHaveBeenCalledExactlyOnceWith(
        '/workspace',
        { action: 'createFile', path: '资料', name: 'new.txt' },
        undefined,
      )
      expect(wrapper.find('dialog').exists()).toBe(false)
      expect(wrapper.find('.directory-new-node').exists()).toBe(false)
      expect(openWorkspaceFile).toHaveBeenCalledExactlyOnceWith('资料/new.txt')
      expect(focusEditor).toHaveBeenCalledOnce()
    } finally {
      wrapper.unmount()
    }
  })
  it('supports root creation even in an empty tree and keeps failed edits reviewable', async () => {
    const { wrapper, files, change } = await setup()
    try {
      files.length = 0
      await wrapper.get('[aria-label="刷新目录"]').trigger('click')
      await flushPromises()
      await wrapper.get('.directory-module').trigger('contextmenu')
      await menuItem(wrapper, '新建文件夹…').trigger('click')
      expect(wrapper.find('[role="tree"]').exists()).toBe(true)
      expect(wrapper.get('.directory-new-node').attributes('aria-level')).toBe('1')
      expect(wrapper.find('[aria-label="文件夹名称"]').exists()).toBe(true)
      await wrapper.get('.directory-new-node-name').setValue('../outside')
      await wrapper.get('.directory-new-node-form').trigger('submit')
      expect(change).not.toHaveBeenCalled()
      expect(wrapper.get('.directory-new-node-error').text()).toContain('名称')
      await wrapper.get('.directory-new-node-name').setValue('资料')
      change.mockRejectedValueOnce(new Error('同名目录已存在'))
      await wrapper.get('.directory-new-node-name').trigger('keydown', { key: 'Enter' })
      await flushPromises()
      expect(wrapper.get('.directory-new-node-error').text()).toContain('同名目录已存在')
      expect(wrapper.get<HTMLInputElement>('.directory-new-node-name').element.value).toBe('资料')
      expect(document.activeElement).toBe(wrapper.get('.directory-new-node-name').element)
    } finally {
      wrapper.unmount()
    }
  })
  it('expands a collapsed folder and cancels the new node with Escape without creating anything', async () => {
    const { wrapper, change } = await setup()
    try {
      const folder = wrapper.get('[data-file-path="资料"]')
      await folder.trigger('click')
      expect(folder.attributes('aria-expanded')).toBe('false')
      await folder.trigger('contextmenu')
      await menuItem(wrapper, '新建文件夹…').trigger('click')
      expect(folder.attributes('aria-expanded')).toBe('true')
      const rows = wrapper.findAll('[role="treeitem"]')
      const parentIndex = rows.findIndex((row) => row.attributes('data-file-path') === '资料')
      expect(rows[parentIndex + 1]!.classes()).toContain('directory-new-node')
      expect(rows[parentIndex + 1]!.attributes('aria-level')).toBe('2')
      await wrapper.get('.directory-new-node-name').setValue('暂不创建')
      await wrapper.get('.directory-new-node-name').trigger('keydown', { key: 'Escape' })
      await flushPromises()
      expect(wrapper.find('.directory-new-node').exists()).toBe(false)
      expect(change).not.toHaveBeenCalled()
      expect(document.activeElement).toBe(folder.element)
    } finally {
      wrapper.unmount()
    }
  })
  it('has separate toolbar actions and a single empty input without inline action buttons', async () => {
    const { wrapper, change } = await setup()
    try {
      for (const label of ['新建文件', '新建文件夹']) {
        await wrapper.get(`button[aria-label="${label}"]`).trigger('click')
        expect(wrapper.findAll('.directory-new-node')).toHaveLength(1)
        expect(wrapper.get('.directory-new-node').attributes('aria-level')).toBe('1')
        expect(wrapper.find('dialog').exists()).toBe(false)
        expect(wrapper.get<HTMLInputElement>('.directory-new-node-name').element.value).toBe('')
        expect(wrapper.find('[aria-label="确认新建"]').exists()).toBe(false)
        expect(wrapper.find('[aria-label="取消新建"]').exists()).toBe(false)
      }
      await wrapper.get('.directory-new-node-name').trigger('keydown', { key: 'Escape' })
      expect(wrapper.find('.directory-new-node').exists()).toBe(false)
      expect(change).not.toHaveBeenCalled()
    } finally {
      wrapper.unmount()
    }
  })
  it('ignores IME confirmation and prevents duplicate creation while the request is pending', async () => {
    const { wrapper, change, files } = await setup()
    try {
      await wrapper.get('[data-file-path="资料"]').trigger('contextmenu')
      await menuItem(wrapper, '新建文件夹…').trigger('click')
      const input = wrapper.get<HTMLInputElement>('.directory-new-node-name')
      await input.setValue('子目录')
      await input.trigger('keydown', { key: 'Enter', isComposing: true })
      await input.trigger('keydown', { key: 'Enter', keyCode: 229 })
      expect(change).not.toHaveBeenCalled()
      let resolve!: (result: { files: ReturnType<typeof cloneFileSettings>; path: string }) => void
      change.mockImplementationOnce(
        () =>
          new Promise((done) => {
            resolve = done
          }),
      )
      await input.trigger('keydown', { key: 'Enter' })
      await wrapper.get('.directory-new-node-form').trigger('submit')
      expect(change).toHaveBeenCalledExactlyOnceWith(
        '/workspace',
        { action: 'createDirectory', path: '资料', name: '子目录' },
        undefined,
      )
      expect(input.element.disabled).toBe(true)
      expect(wrapper.find('[aria-label="正在创建"]').exists()).toBe(true)
      files.push({ path: '资料/子目录', name: '子目录', isDir: true, size: 0 })
      resolve({ files: cloneFileSettings(), path: '资料/子目录' })
      await flushPromises()
      expect(wrapper.find('.directory-new-node').exists()).toBe(false)
      expect(wrapper.find('[data-file-path="资料/子目录"]').exists()).toBe(true)
    } finally {
      wrapper.unmount()
    }
  })
  it('clears inline creation on workspace changes and ignores a late result', async () => {
    const { wrapper, change, workspace } = await setup()
    try {
      await wrapper.get('button[aria-label="新建文件"]').trigger('click')
      await wrapper.get('.directory-new-node-name').setValue('late.txt')
      let resolve!: (result: { files: ReturnType<typeof cloneFileSettings>; path: string }) => void
      change.mockImplementationOnce(
        () =>
          new Promise((done) => {
            resolve = done
          }),
      )
      await wrapper.get('.directory-new-node-form').trigger('submit')
      workspace.value = { path: '/another', name: 'another' }
      await flushPromises()
      expect(wrapper.find('.directory-new-node').exists()).toBe(false)
      resolve({ files: cloneFileSettings(), path: 'late.txt' })
      await flushPromises()
      expect(wrapper.find('.directory-new-node').exists()).toBe(false)
      expect(wrapper.find('[data-file-path="late.txt"]').exists()).toBe(false)
      expect(change).toHaveBeenCalledTimes(1)
    } finally {
      wrapper.unmount()
    }
  })
  it('renames the existing node inline with F2 and preserves the journal identity hint', async () => {
    const { wrapper, change } = await setup()
    try {
      await wrapper.get('[data-file-path="2026/项目.md"]').trigger('keydown', { key: 'F2' })
      expect(wrapper.find('dialog').exists()).toBe(false)
      expect(wrapper.get('.directory-rename-node').text()).toContain('关联原日报日期')
      const input = wrapper.get<HTMLInputElement>('.directory-new-node-name')
      expect(input.element.value).toBe('项目.md')
      expect(input.element.selectionStart).toBe(0)
      expect(input.element.selectionEnd).toBe(2)
      await input.setValue('重命名.md')
      await input.trigger('keydown', { key: 'Enter' })
      await flushPromises()
      expect(change).toHaveBeenCalledWith(
        '/workspace',
        { action: 'rename', path: '2026/项目.md', name: '重命名.md' },
        undefined,
      )
    } finally {
      wrapper.unmount()
    }
  })
  it('selects the filename stem, cycles F2 selection, and cancels rename without writing', async () => {
    const { wrapper, change } = await setup()
    try {
      await wrapper.get('[data-file-path="资料/notes.txt"]').trigger('keydown', { key: 'F2' })
      const input = wrapper.get<HTMLInputElement>('.directory-new-node-name')
      expect([input.element.selectionStart, input.element.selectionEnd]).toEqual([0, 5])
      await input.trigger('keydown', { key: 'F2' })
      expect([input.element.selectionStart, input.element.selectionEnd]).toEqual([0, 9])
      await input.trigger('keydown', { key: 'F2' })
      expect([input.element.selectionStart, input.element.selectionEnd]).toEqual([6, 9])
      await input.setValue('cancelled.txt')
      await input.trigger('keydown', { key: 'Escape' })
      await flushPromises()
      expect(change).not.toHaveBeenCalled()
      expect(wrapper.find('.directory-rename-node').exists()).toBe(false)
      expect(document.activeElement).toBe(wrapper.get('[data-file-path="资料/notes.txt"]').element)
      await wrapper.get('[data-file-path="资料/notes.txt"]').trigger('keydown', { key: 'F2' })
      await wrapper.get('.directory-new-node-name').trigger('keydown', { key: 'Enter' })
      expect(change).not.toHaveBeenCalled()
      expect(wrapper.find('.directory-rename-node').exists()).toBe(false)
    } finally {
      wrapper.unmount()
    }
  })
  it('keeps invalid journal rename errors inline and retries a failed rename with the draft intact', async () => {
    const { wrapper, change } = await setup()
    try {
      await wrapper.get('[data-file-path="2026/项目.md"]').trigger('keydown', { key: 'F2' })
      const input = wrapper.get<HTMLInputElement>('.directory-new-node-name')
      await input.setValue('项目.py')
      await input.trigger('keydown', { key: 'Enter' })
      expect(change).not.toHaveBeenCalled()
      expect(wrapper.get('.directory-new-node-error').text()).toContain('扩展名')
      await input.setValue('新的日报.md')
      change.mockRejectedValueOnce(new Error('外部文件已变化'))
      await input.trigger('keydown', { key: 'Enter' })
      await flushPromises()
      expect(input.element.value).toBe('新的日报.md')
      expect(wrapper.get('.directory-new-node-error').text()).toContain('外部文件已变化')
      await input.trigger('keydown', { key: 'Enter' })
      await flushPromises()
      expect(change).toHaveBeenCalledTimes(2)
      expect(wrapper.find('.directory-rename-node').exists()).toBe(false)
      expect(wrapper.find('dialog').exists()).toBe(false)
    } finally {
      wrapper.unmount()
    }
  })
  it('preserves folder collapse state and uses the renamed folder for the next toolbar action', async () => {
    const { wrapper, files, change } = await setup()
    try {
      await wrapper.get('[data-file-path="资料"]').trigger('click')
      await wrapper.get('[data-file-path="资料"]').trigger('keydown', { key: 'F2' })
      const input = wrapper.get<HTMLInputElement>('.directory-new-node-name')
      expect([input.element.selectionStart, input.element.selectionEnd]).toEqual([0, 2])
      await input.setValue('归档')
      change.mockImplementationOnce(async () => {
        files.splice(
          0,
          files.length,
          ...files.map((file) =>
            file.path === '资料' || file.path.startsWith('资料/')
              ? { ...file, path: '归档' + file.path.slice(2), name: file.path === '资料' ? '归档' : file.name }
              : file,
          ),
        )
        return { files: cloneFileSettings(), path: '归档' }
      })
      await wrapper.get('button[aria-label="新建文件"]').trigger('click')
      await flushPromises()
      expect(change).toHaveBeenCalledWith('/workspace', { action: 'rename', path: '资料', name: '归档' }, undefined)
      expect(wrapper.get('[data-file-path="归档"]').attributes('aria-expanded')).toBe('true')
      expect(wrapper.get('.directory-new-node').attributes('aria-level')).toBe('2')
      await wrapper.get('.directory-new-node-name').setValue('after.txt')
      await wrapper.get('.directory-new-node-name').trigger('keydown', { key: 'Enter' })
      await flushPromises()
      expect(change).toHaveBeenLastCalledWith(
        '/workspace',
        { action: 'createFile', path: '归档', name: 'after.txt' },
        undefined,
      )
    } finally {
      wrapper.unmount()
    }
  })
  it('uses the selected folder or selected file’s parent as the toolbar creation location', async () => {
    const { wrapper, change } = await setup()
    try {
      await wrapper.get('[data-file-path="资料"]').trigger('click')
      await wrapper.get('button[aria-label="新建文件"]').trigger('click')
      expect(wrapper.get('[data-file-path="资料"]').attributes('aria-expanded')).toBe('true')
      expect(wrapper.get('.directory-new-node').attributes('aria-level')).toBe('2')
      await wrapper.get('.directory-new-node-name').trigger('keydown', { key: 'Escape' })
      await wrapper.get('[data-file-path="资料/notes.txt"]').trigger('click')
      await wrapper.get('button[aria-label="新建文件夹"]').trigger('click')
      await wrapper.get('.directory-new-node-name').setValue('同级目录')
      await wrapper.get('.directory-new-node-name').trigger('keydown', { key: 'Enter' })
      await flushPromises()
      expect(change).toHaveBeenCalledExactlyOnceWith(
        '/workspace',
        { action: 'createDirectory', path: '资料', name: '同级目录' },
        undefined,
      )
    } finally {
      wrapper.unmount()
    }
  })
  it('validates duplicate names while typing and keeps Enter on invalid input in the tree', async () => {
    const { wrapper, change } = await setup()
    try {
      await wrapper.get('[data-file-path="资料/notes.txt"]').trigger('contextmenu')
      await menuItem(wrapper, '新建文件…').trigger('click')
      const input = wrapper.get<HTMLInputElement>('.directory-new-node-name')
      await input.setValue('notes.txt')
      expect(input.attributes('aria-invalid')).toBe('true')
      expect(wrapper.get('.directory-new-node-error').text()).toContain('同名')
      await input.trigger('keydown', { key: 'Enter' })
      expect(change).not.toHaveBeenCalled()
      expect(wrapper.find('.directory-new-node').exists()).toBe(true)
      await input.setValue('valid.py')
      expect(wrapper.find('.directory-new-node-error').exists()).toBe(false)
    } finally {
      wrapper.unmount()
    }
  })
  it('accepts a valid name when focus moves to another control', async () => {
    const { wrapper, change, files, openWorkspaceFile } = await setup()
    try {
      await wrapper.get('button[aria-label="新建文件"]').trigger('click')
      await wrapper.get('.directory-new-node-name').setValue('blur.py')
      change.mockImplementationOnce(async () => {
        files.push({ path: 'blur.py', name: 'blur.py', isDir: false, size: 0 })
        return { files: cloneFileSettings(), path: 'blur.py' }
      })
      wrapper.get<HTMLButtonElement>('[aria-label="刷新目录"]').element.focus()
      await vi.waitFor(() =>
        expect(change).toHaveBeenCalledExactlyOnceWith(
          '/workspace',
          { action: 'createFile', path: '', name: 'blur.py' },
          undefined,
        ),
      )
      await flushPromises()
      expect(openWorkspaceFile).toHaveBeenCalledWith('blur.py')
      expect(wrapper.find('.directory-new-node').exists()).toBe(false)
    } finally {
      wrapper.unmount()
    }
  })
  it.each(['', '../bad'])(
    'cancels a %j name on blur, and blank tree clicks reset the creation location',
    async (name) => {
      const { wrapper, change } = await setup()
      try {
        await wrapper.get('[data-file-path="资料"]').trigger('click')
        await wrapper.get('button[aria-label="新建文件"]').trigger('click')
        await wrapper.get('.directory-new-node-name').setValue(name)
        await wrapper.get('.directory-module').trigger('pointerdown')
        await vi.waitFor(() => expect(wrapper.find('.directory-new-node').exists()).toBe(false))
        expect(change).not.toHaveBeenCalled()
        await wrapper.get('button[aria-label="新建文件夹"]').trigger('click')
        expect(wrapper.get('.directory-new-node').attributes('aria-level')).toBe('1')
      } finally {
        wrapper.unmount()
      }
    },
  )
  it('keeps input on window focus loss when DOM focus stays in the input', async () => {
    const { wrapper, change } = await setup()
    try {
      await wrapper.get('button[aria-label="新建文件"]').trigger('click')
      const input = wrapper.get<HTMLInputElement>('.directory-new-node-name')
      await input.setValue('still-editing.txt')
      await input.trigger('blur')
      await new Promise((resolve) => setTimeout(resolve, 10))
      expect(document.activeElement).toBe(input.element)
      expect(wrapper.find('.directory-new-node').exists()).toBe(true)
      expect(change).not.toHaveBeenCalled()
    } finally {
      wrapper.unmount()
    }
  })
  it('navigates selection with arrows without opening files and uses that selection for creation', async () => {
    const { wrapper, openWorkspaceFile } = await setup()
    try {
      const folder = wrapper.get<HTMLButtonElement>('[data-file-path="资料"]')
      folder.element.focus()
      await folder.trigger('keydown', { key: 'ArrowRight' })
      await flushPromises()
      const child = wrapper.get('[data-file-path="资料/notes.txt"]')
      expect(document.activeElement).toBe(child.element)
      expect(child.attributes('aria-selected')).toBe('true')
      expect(openWorkspaceFile).not.toHaveBeenCalled()
      await child.trigger('keydown', { key: 'ArrowLeft' })
      await flushPromises()
      expect(document.activeElement).toBe(folder.element)
      await wrapper.get('button[aria-label="新建文件夹"]').trigger('click')
      expect(wrapper.get('.directory-new-node').attributes('aria-level')).toBe('2')
    } finally {
      wrapper.unmount()
    }
  })
  it('flushes before delete preview and only deletes after confirmation', async () => {
    const { wrapper, change, flush } = await setup()
    try {
      await wrapper.get('[data-file-path="2026/项目.md"]').trigger('keydown', { key: 'Delete' })
      await flushPromises()
      expect(flush).toHaveBeenCalledOnce()
      expect(invoke).toHaveBeenCalledWith('preview_workspace_file_change', {
        workspacePath: '/workspace',
        request: { action: 'delete', path: '2026/项目.md' },
      })
      expect(change).not.toHaveBeenCalled()
      expect(wrapper.find('dialog').exists()).toBe(false)
      expect(wrapper.get('.directory-actions-panel').text()).toContain('其中 1 篇日报')
      await wrapper.get('.directory-delete-confirm').trigger('click')
      await flushPromises()
      expect(change).toHaveBeenCalledExactlyOnceWith(
        '/workspace',
        { action: 'delete', path: '2026/项目.md' },
        'snapshot',
      )
    } finally {
      wrapper.unmount()
    }
  })
  it('does not mutate when deletion is cancelled or a late preview belongs to another workspace', async () => {
    const { wrapper, change, workspace } = await setup()
    try {
      let resolve!: (value: unknown) => void
      invoke.mockImplementation((command) =>
        command === 'preview_workspace_file_change'
          ? new Promise((done) => {
              resolve = done
            })
          : Promise.resolve([]),
      )
      await wrapper.get('[data-file-path="2026/项目.md"]').trigger('keydown', { key: 'Delete' })
      await flushPromises()
      await wrapper.get('[aria-label="关闭文件操作"]').trigger('click')
      workspace.value = { path: '/another', name: 'another' }
      await flushPromises()
      resolve({ token: 'old', fileCount: 1, directoryCount: 0, journalCount: 1 })
      await flushPromises()
      expect(change).not.toHaveBeenCalled()
      expect(wrapper.find('.directory-actions-panel').exists()).toBe(false)
      expect(wrapper.find('dialog').exists()).toBe(false)
    } finally {
      wrapper.unmount()
    }
  })
  it('disables mutation of active database files and supports keyboard context menus', async () => {
    const { wrapper, change } = await setup()
    try {
      await wrapper.get('[data-file-path="work.sqlite"]').trigger('keydown', { key: 'F10', shiftKey: true })
      expect(menuItem(wrapper, '删除（数据库正在使用）').attributes('disabled')).toBeDefined()
      expect(menuItem(wrapper, '重命名（数据库正在使用）').attributes('disabled')).toBeDefined()
      expect(change).not.toHaveBeenCalled()
    } finally {
      wrapper.unmount()
    }
  })
  it('restores trash entries and refreshes the trash list', async () => {
    const { wrapper, change } = await setup()
    try {
      const entry = {
        id: '2bf8d40e-45ad-4b9a-a051-4cf423979e92',
        name: '项目.md',
        path: '2026/项目.md',
        isDir: false,
        deletedAt: 1700000000,
        fileCount: 1,
        directoryCount: 0,
      }
      invoke.mockImplementation(async (command) => (command === 'list_workspace_trash' ? [entry] : []))
      await wrapper.get('[aria-label="工作空间回收站"]').trigger('click')
      await flushPromises()
      expect(wrapper.find('dialog').exists()).toBe(false)
      expect(wrapper.get('.directory-actions-panel').attributes('role')).toBe('region')
      await wrapper.get('[aria-label="恢复 项目.md"]').trigger('click')
      await flushPromises()
      expect(change).toHaveBeenCalledWith('/workspace', { action: 'restore', path: '', trashId: entry.id }, undefined)
      expect(invoke.mock.calls.filter(([command]) => command === 'list_workspace_trash')).toHaveLength(2)
    } finally {
      wrapper.unmount()
    }
  })
  it('keeps recovery errors in the embedded trash panel and closes it with Escape', async () => {
    const { wrapper, change } = await setup()
    try {
      const original = invoke.getMockImplementation()!
      invoke.mockImplementation(async (command) =>
        command === 'list_workspace_trash'
          ? [
              {
                id: 'trash-id',
                path: '资料/notes.txt',
                name: 'notes.txt',
                isDir: false,
                deletedAt: 1700000000,
                fileCount: 1,
                directoryCount: 0,
              },
            ]
          : original(command),
      )
      await wrapper.get('[aria-label="工作空间回收站"]').trigger('click')
      await flushPromises()
      expect(wrapper.get('[aria-label="工作空间回收站"]').attributes('aria-expanded')).toBe('true')
      change.mockRejectedValueOnce(new Error('原位置已有同名文件，未覆盖'))
      await wrapper.get('[aria-label="恢复 notes.txt"]').trigger('click')
      await flushPromises()
      expect(wrapper.get('.directory-action-error').text()).toContain('未覆盖')
      expect(wrapper.find('[aria-label="恢复 notes.txt"]').exists()).toBe(true)
      await wrapper.get('.directory-actions-panel').trigger('keydown', { key: 'Escape' })
      await flushPromises()
      expect(wrapper.find('.directory-actions-panel').exists()).toBe(false)
      expect(wrapper.get('[aria-label="工作空间回收站"]').attributes('aria-expanded')).toBe('false')
      expect(wrapper.find('dialog').exists()).toBe(false)
    } finally {
      wrapper.unmount()
    }
  })
})
