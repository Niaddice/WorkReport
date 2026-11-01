import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useWorkspaceDocument, type WorkspaceDocument } from '../src/lib/useWorkspaceDocument'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
const text = (path = 'main.py', content = 'original'): WorkspaceDocument => ({
  path,
  filename: path,
  content,
  revision: content,
  kind: 'text',
  encoding: 'utf-8',
  markdown: false,
  journalDate: null,
  sheets: [],
  message: '',
})
function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: Error) => void
  const promise = new Promise<T>((done, fail) => {
    resolve = done
    reject = fail
  })
  return { promise, resolve, reject }
}
beforeEach(() => {
  invoke.mockReset()
  vi.useFakeTimers()
})
afterEach(() => vi.useRealTimers())
describe('workspace document persistence', () => {
  it('autosaves text and flushes before opening another file', async () => {
    invoke.mockImplementation(async (command, args) =>
      command === 'read_workspace_document' ? text(args.path) : text(args.path, args.content),
    )
    const state = useWorkspaceDocument(() => '/workspace')
    await state.open('main.py')
    state.edit('edited', true)
    await vi.advanceTimersByTimeAsync(650)
    expect(invoke).toHaveBeenCalledWith('save_workspace_document', {
      workspacePath: '/workspace',
      path: 'main.py',
      content: 'edited',
      expectedRevision: 'original',
    })
    expect(state.dirty.value).toBe(false)
    state.edit('before switch', true)
    await state.open('next.java')
    const calls = invoke.mock.calls.map(([command]) => command)
    expect(calls.slice(-2)).toEqual(['save_workspace_document', 'read_workspace_document'])
    expect(state.path.value).toBe('next.java')
    state.dispose()
  })
  it('retains a conflicting draft and does not switch away after save failure', async () => {
    invoke.mockImplementation(async (command) => {
      if (command === 'save_workspace_document') throw new Error('文件已被其他程序修改')
      return text()
    })
    const state = useWorkspaceDocument(() => '/workspace')
    await state.open('main.py')
    state.edit('keep draft')
    expect(await state.open('other.sql')).toBeNull()
    expect(state.path.value).toBe('main.py')
    expect(state.draft.value).toBe('keep draft')
    expect(state.error.value).toContain('其他程序')
    state.dispose()
  })
  it('keeps editing during a save and persists the latest draft with the new revision', async () => {
    const pending = deferred<WorkspaceDocument>()
    let saves = 0
    invoke.mockImplementation(async (command, args) =>
      command === 'read_workspace_document' ? text() : ++saves === 1 ? pending.promise : text(args.path, args.content),
    )
    const state = useWorkspaceDocument(() => '/workspace')
    await state.open('main.py')
    state.edit('first')
    const saving = state.flush()
    state.edit('second')
    pending.resolve(text('main.py', 'first'))
    await saving
    expect(invoke).toHaveBeenLastCalledWith('save_workspace_document', {
      workspacePath: '/workspace',
      path: 'main.py',
      content: 'second',
      expectedRevision: 'first',
    })
    expect(state.dirty.value).toBe(false)
    state.dispose()
  })
  it('accepts canonical line endings from the backend without looping forever', async () => {
    invoke.mockImplementation(async (command) =>
      command === 'read_workspace_document' ? text('a.txt', 'old\r\n') : text('a.txt', 'new\r\n'),
    )
    const state = useWorkspaceDocument(() => '/workspace')
    await state.open('a.txt')
    state.edit('new\n')
    await state.flush()
    expect(state.dirty.value).toBe(false)
    expect(invoke.mock.calls.filter(([command]) => command === 'save_workspace_document')).toHaveLength(1)
    state.dispose()
  })
  it('the newest click wins, and stale reads cannot enter a different workspace', async () => {
    const a = deferred<WorkspaceDocument>(),
      b = deferred<WorkspaceDocument>()
    invoke.mockImplementation((_command, args) => (args.path === 'a.py' ? a.promise : b.promise))
    let workspace = '/workspace'
    const state = useWorkspaceDocument(() => workspace)
    const openingA = state.open('a.py')
    await Promise.resolve()
    const openingB = state.open('b.py')
    await Promise.resolve()
    b.resolve(text('b.py', 'b'))
    await openingB
    a.resolve(text('a.py', 'a'))
    await openingA
    expect(state.path.value).toBe('b.py')
    expect(state.draft.value).toBe('b')
    const c = deferred<WorkspaceDocument>()
    invoke.mockReturnValueOnce(c.promise)
    const opening = state.open('c.py')
    await Promise.resolve()
    workspace = '/another'
    state.clear()
    c.resolve(text('c.py', 'late'))
    await opening
    expect(state.path.value).toBe('')
    expect(state.document.value).toBeNull()
    state.dispose()
  })
  it('never saves tables or unsupported files, and preserves undo across an unchanged refresh', async () => {
    const state = useWorkspaceDocument(() => '/workspace')
    for (const kind of ['table', 'unsupported'] as const) {
      invoke.mockResolvedValueOnce({ ...text('table.csv'), kind })
      await state.open('table.csv', true)
      state.edit('should not edit')
      await state.flush()
      expect(state.draft.value).toBe('original')
    }
    expect(invoke.mock.calls.some(([command]) => command === 'save_workspace_document')).toBe(false)
    invoke.mockImplementation(async (command, args) =>
      text('main.py', command === 'save_workspace_document' ? args.content : 'original'),
    )
    await state.open('main.py')
    state.edit('edited', true)
    await state.flush()
    invoke.mockResolvedValueOnce(text('main.py', 'edited'))
    await state.reload()
    expect(state.canUndo.value).toBe(true)
    state.undo()
    expect(state.draft.value).toBe('original')
    state.remap('main.py', 'renamed.py')
    expect(state.path.value).toBe('renamed.py')
    state.remap('renamed.py', null)
    expect(state.path.value).toBe('')
    state.dispose()
  })
})
