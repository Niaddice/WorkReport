// @vitest-environment happy-dom
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import dayjs from 'dayjs'
import App from '../src/App.vue'
import JournalEditor from '../src/components/JournalEditor.vue'
import type { WorkspaceDocument } from '../src/lib/useWorkspaceDocument'
import { DEFAULT_DATABASE_SETTINGS } from '../src/lib/database'
import { emptyHolidayState } from '../src/lib/holidays'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke, isTauri: () => true }))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ onCloseRequested: async () => () => {}, destroy: async () => {} }),
}))
vi.mock('@tauri-apps/api/event', () => ({ listen: async () => () => {} }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: async () => null }))

beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function () {
    this.open = true
  }
  HTMLDialogElement.prototype.close = function () {
    this.open = false
  }
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver
  vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockImplementation(function (this: HTMLElement) {
    return this.classList.contains('module-board') ? 1200 : 0
  })
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockImplementation(function (this: HTMLElement) {
    return this.classList.contains('module-board') ? 800 : 0
  })
})
afterEach(() => invoke.mockReset())

const today = dayjs().format('YYYYMMDD')
const dailyPath = `${today.slice(0, 4)}/${today}.txt`
function file(path: string, change: Partial<WorkspaceDocument> = {}): WorkspaceDocument {
  return {
    path,
    filename: path,
    kind: 'text',
    content: `content of ${path}`,
    revision: 'original',
    encoding: 'utf-8',
    markdown: false,
    journalDate: null,
    sheets: [],
    message: '',
    ...change,
  }
}
async function setup(documents: WorkspaceDocument[]) {
  const daily = { date: today, filename: `${today}.txt`, format: 'txt', content: '原有日报', revision: 'daily' }
  invoke.mockImplementation(async (command, args) => {
    switch (command) {
      case 'recent_workspace':
        return '/workspace'
      case 'open_workspace':
        return { path: '/workspace', name: 'workspace' }
      case 'list_entries':
        return [{ ...daily, size: 12 }]
      case 'read_entry':
        return daily
      case 'list_previews':
      case 'list_entry_history':
        return []
      case 'list_workspace_files':
        return [
          { path: today.slice(0, 4), name: today.slice(0, 4), isDir: true, size: 0 },
          { path: dailyPath, name: `${today}.txt`, isDir: false, size: 12, journalDate: today },
          ...documents.map((document) => ({
            path: document.path,
            name: document.filename,
            isDir: false,
            size: 20,
            journalDate: null,
          })),
        ]
      case 'read_workspace_document':
        return documents.find((document) => document.path === args.path)
      case 'save_workspace_document':
        return file(args.path, { content: args.content, revision: 'saved' })
      case 'load_holiday_data':
        return emptyHolidayState()
      case 'load_ai_settings':
        return { providers: [], defaultProviderId: null, generationPreference: '', summaryPrompt: '' }
      case 'load_appearance_settings':
        return { scale: 100 }
      case 'load_excel_settings':
        return { enabled: false, templatePath: '', prompt: '' }
      case 'load_summary_settings':
        return { outputDirectory: 'summary' }
      case 'load_database_settings':
        return DEFAULT_DATABASE_SETTINGS
      case 'load_ui_preferences':
        return {
          skin: 'default',
          layout: {
            type: 'split',
            direction: 'row',
            ratio: 0.3,
            first: { type: 'leaf', id: 'directory' },
            second: { type: 'leaf', id: 'editor' },
          },
        }
      default:
        return null
    }
  })
  const wrapper = mount(App, { attachTo: document.body })
  await flushPromises()
  return wrapper
}

describe('directory files inside the journal module', () => {
  it('edits source files, saves before switching, and returns to the same daily entry', async () => {
    const wrapper = await setup([file('main.py'), file('app.yml')])
    try {
      await wrapper.get('[data-file-path="main.py"]').trigger('click')
      await flushPromises()
      const editor = wrapper.getComponent(JournalEditor)
      expect(editor.props('filePath')).toBe('main.py')
      expect(wrapper.find('.format-toggle').exists()).toBe(false)
      expect(wrapper.find('[aria-label="复制文件内容"]').exists()).toBe(true)
      await wrapper.get('textarea[aria-label="文件文本内容"]').setValue('print("编辑后")\n')
      await wrapper.get('[data-file-path="app.yml"]').trigger('click')
      await flushPromises()
      expect(invoke).toHaveBeenCalledWith('save_workspace_document', {
        workspacePath: '/workspace',
        path: 'main.py',
        content: 'print("编辑后")\n',
        expectedRevision: 'original',
      })
      expect(wrapper.getComponent(JournalEditor).props('filePath')).toBe('app.yml')
      await wrapper.get(`[data-file-path="${dailyPath}"]`).trigger('click')
      await flushPromises()
      expect(wrapper.find('.workspace-document-view').exists()).toBe(false)
      expect(wrapper.get('textarea[aria-label="日报文本内容"]').element).toHaveProperty('value', '原有日报')
      expect(invoke.mock.calls.some(([command]) => ['create_entry', 'save_entry'].includes(command))).toBe(false)
    } finally {
      wrapper.unmount()
    }
  })
  it('uses the markdown editor for ordinary md files and flushes before returning to the journal', async () => {
    const wrapper = await setup([file('notes.md', { markdown: true, content: '## 标题\n\n内容' })])
    try {
      await wrapper.get('[data-file-path="notes.md"]').trigger('click')
      await flushPromises()
      expect(wrapper.get('.workspace-document-view .markdown-editor h2').text()).toBe('标题')
      expect(wrapper.find('.format-toggle').exists()).toBe(false)
      wrapper.getComponent(JournalEditor).vm.$emit('edit', '## 新标题\n')
      await wrapper.get('[aria-label="返回日报"]').trigger('click')
      await flushPromises()
      expect(invoke).toHaveBeenCalledWith(
        'save_workspace_document',
        expect.objectContaining({ path: 'notes.md', content: '## 新标题\n' }),
      )
      expect(wrapper.find('.workspace-document-view').exists()).toBe(false)
    } finally {
      wrapper.unmount()
    }
  })
  it('renders tables read-only and shows unsupported Office formats in the same module', async () => {
    const table = file('report.csv', {
      kind: 'table',
      content: '',
      sheets: [
        {
          name: 'report',
          state: 'visible',
          truncated: false,
          regions: [{ startRow: 1, startColumn: 1, rowCount: 2, columnCount: 2, rows: [], columns: [], cells: [] }],
        },
      ],
    })
    const wrapper = await setup([
      table,
      file('report.docx', { kind: 'unsupported', message: '暂不支持此文件格式' }),
      file('slides.pptx', { kind: 'unsupported', message: '暂不支持此文件格式' }),
    ])
    try {
      await wrapper.get('[data-file-path="report.csv"]').trigger('click')
      await flushPromises()
      expect(wrapper.get('.workspace-document-mode').text()).toBe('只读预览')
      expect(wrapper.get('.workspace-document-view table').attributes('aria-readonly')).toBe('true')
      expect(wrapper.find('.workspace-document-view .excel-sheet-legend').exists()).toBe(false)
      expect(wrapper.find('.workspace-document-view textarea').exists()).toBe(false)
      expect(wrapper.find('.workspace-document-view [contenteditable="true"]').exists()).toBe(false)
      for (const path of ['report.docx', 'slides.pptx']) {
        await wrapper.get(`[data-file-path="${path}"]`).trigger('click')
        await flushPromises()
        expect(wrapper.get('.workspace-document-state').text()).toContain('暂不支持此文件格式')
        expect(wrapper.find('.workspace-document-view textarea').exists()).toBe(false)
      }
      expect(invoke.mock.calls.some(([command]) => command === 'save_workspace_document')).toBe(false)
      expect(invoke.mock.calls.some(([command]) => command === 'open_workspace_item')).toBe(false)
    } finally {
      wrapper.unmount()
    }
  })
})
