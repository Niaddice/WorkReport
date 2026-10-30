// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { DOMWrapper, flushPromises, mount, type VueWrapper } from '@vue/test-utils'

const { invoke, desktop } = vi.hoisted(() => ({ invoke: vi.fn(), desktop: { value: true } }))
vi.mock('@tauri-apps/api/core', () => ({ invoke, isTauri: () => desktop.value }))

import { useGitWorkbench, type GitHistoryCommit, type GitRepositoryState } from '../src/lib/useGitWorkbench'
import GitWorkbench from '../src/components/GitWorkbench.vue'
import type { GitRepository } from '../src/lib/useGit'

const repositories: GitRepository[] = [
  { id: 'one', name: '项目一', path: '/tmp/project-one', author: '', branch: '' },
  { id: 'two', name: '项目二', path: '/tmp/project-two', author: '', branch: '' },
]

function state(overrides: Partial<GitRepositoryState> = {}): GitRepositoryState {
  return {
    root: '/tmp/project-one',
    branch: 'main',
    detached: false,
    upstream: 'origin/main',
    ahead: 1,
    behind: 2,
    files: [
      { path: 'partially staged.txt', originalPath: null, indexStatus: 'M', worktreeStatus: 'M', conflicted: false },
      { path: 'new file.txt', originalPath: null, indexStatus: '?', worktreeStatus: '?', conflicted: false },
    ],
    branches: [
      { name: 'main', remote: false, current: true, upstream: 'origin/main' },
      { name: 'feature', remote: false, current: false, upstream: null },
      { name: 'origin/main', remote: true, current: false, upstream: null },
    ],
    stashes: [{ reference: 'stash@{0}', message: 'WIP on main: 保留编辑' }],
    remotes: ['origin'],
    ...overrides,
  }
}

function commit(hash: string): GitHistoryCommit {
  return {
    hash,
    parents: [],
    author: '开发者',
    email: 'developer@example.test',
    committedAt: '2026-10-29T12:00:00+08:00',
    message: `提交 ${hash}`,
    refs: 'HEAD -> main',
  }
}

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((done, fail) => {
    resolve = done
    reject = fail
  })
  return { promise, resolve, reject }
}

function mockBackend(repository = state()) {
  invoke.mockImplementation(async (command: string) => {
    if (command === 'read_git_repository') return repository
    if (command === 'read_git_history') return { commits: [commit('abc1234')], hasMore: false }
    if (command === 'read_git_diff') return { text: '+changed line', truncated: false }
    if (command === 'git_repository_action') return '操作完成'
    if (command === 'generate_git_commit_message') return 'feat: 完成暂存区改动'
    return undefined
  })
}

function actionCalls() {
  return invoke.mock.calls.filter(([command]) => command === 'git_repository_action')
}

function button(wrapper: VueWrapper, text: string) {
  const result = wrapper.findAll('button').find((item) => item.text() === text)
  if (!result) throw new Error(`找不到按钮：${text}`)
  return result
}

async function mountReady(repository = state()) {
  mockBackend(repository)
  const wrapper = mount(GitWorkbench, {
    attachTo: document.body,
    props: { workspacePath: '/tmp/workspace', repositories, disabled: false },
  })
  await flushPromises()
  return wrapper
}

beforeEach(() => {
  invoke.mockReset()
  desktop.value = true
})

describe('AI commit messages', () => {
  it('explains missing models and staging requirements, and opens model configuration', async () => {
    const wrapper = await mountReady()
    try {
      await wrapper.setProps({ aiProviderName: null })
      expect(wrapper.get('[aria-label="AI 生成提交说明"]').attributes('disabled')).toBeDefined()
      await wrapper.get('.configure-ai').trigger('click')
      expect(wrapper.emitted('configureAi')).toHaveLength(1)
      await wrapper.setProps({ aiProviderName: '团队模型' })
      expect(wrapper.get('[aria-label="AI 生成提交说明"]').attributes('title')).toContain('团队模型')
      expect(wrapper.get('[aria-label="AI 生成提交说明"]').attributes('title')).toContain('1 个已暂存文件')
    } finally {
      wrapper.unmount()
    }
  })

  it('keeps the commit form outside the scrollable list and supports keyboard resizing and compact back navigation', async () => {
    const wrapper = await mountReady()
    try {
      expect(wrapper.get('.change-pane').find('.commit-form').exists()).toBe(false)
      const gutter = wrapper.get('[role="separator"]')
      await gutter.trigger('keydown', { key: 'ArrowRight' })
      expect(gutter.attributes('aria-valuenow')).toBe('34')
      await gutter.trigger('keydown', { key: 'End' })
      expect(gutter.attributes('aria-valuenow')).toBe('60')
      await gutter.trigger('keydown', { key: 'Home' })
      expect(gutter.attributes('aria-valuenow')).toBe('24')
      await wrapper.get('[aria-label="查看暂存差异 partially staged.txt"]').trigger('click')
      await flushPromises()
      expect(wrapper.get('.changes-view').classes()).toContain('show-diff')
      expect(wrapper.get('.diff-stats').attributes('aria-label')).toBe('新增 1 行，删除 0 行')
      await wrapper.get('.compact-back').trigger('click')
      expect(wrapper.get('.changes-view').classes()).not.toContain('show-diff')
    } finally {
      wrapper.unmount()
    }
  })
  it('generates into the editable message without committing and sends only the configured repository scope', async () => {
    const wrapper = await mountReady()
    try {
      await wrapper.get('[aria-label="AI 生成提交说明"]').trigger('click')
      await flushPromises()
      expect(wrapper.get<HTMLTextAreaElement>('[aria-label="提交说明"]').element.value).toBe('feat: 完成暂存区改动')
      expect(actionCalls()).toHaveLength(0)
      expect(invoke).toHaveBeenCalledWith('generate_git_commit_message', {
        workspacePath: '/tmp/workspace',
        repositoryId: 'one',
        requestId: expect.any(String),
      })
      expect(button(wrapper, '提交').attributes('disabled')).toBeUndefined()
    } finally {
      wrapper.unmount()
    }
  })

  it('preserves manual edits made while generation is pending', async () => {
    const wrapper = await mountReady()
    const pending = deferred<string>()
    invoke.mockImplementation(async (command) =>
      command === 'generate_git_commit_message' ? pending.promise : state(),
    )
    try {
      await wrapper.get('[aria-label="AI 生成提交说明"]').trigger('click')
      expect(wrapper.get('[aria-label="提交说明"]').attributes('disabled')).toBeUndefined()
      expect(button(wrapper, '提交').attributes('disabled')).toBeDefined()
      await wrapper.get('[aria-label="提交说明"]').setValue('手动修改后的提交说明')
      pending.resolve('AI 返回的旧说明')
      await flushPromises()
      expect(wrapper.get<HTMLTextAreaElement>('[aria-label="提交说明"]').element.value).toBe('手动修改后的提交说明')
      expect(wrapper.text()).toContain('已保留你的修改')
    } finally {
      wrapper.unmount()
    }
  })

  it('cancels its own request and ignores a late response', async () => {
    const wrapper = await mountReady()
    const pending = deferred<string>()
    invoke.mockImplementation(async (command) =>
      command === 'generate_git_commit_message' ? pending.promise : undefined,
    )
    try {
      await wrapper.get('[aria-label="提交说明"]').setValue('保留原说明')
      await wrapper.get('[aria-label="AI 生成提交说明"]').trigger('click')
      const requestId = invoke.mock.calls.find(([command]) => command === 'generate_git_commit_message')![1].requestId
      await wrapper.get('[aria-label="取消生成提交说明"]').trigger('click')
      expect(invoke).toHaveBeenCalledWith('cancel_git_commit_message', { requestId })
      pending.resolve('已取消请求的旧说明')
      await flushPromises()
      expect(wrapper.get<HTMLTextAreaElement>('[aria-label="提交说明"]').element.value).toBe('保留原说明')
      expect(wrapper.find('[aria-label="AI 生成提交说明"]').exists()).toBe(true)
    } finally {
      wrapper.unmount()
    }
  })

  it('keeps the draft on an AI error and clears a stale request after workspace switching', async () => {
    const wrapper = await mountReady()
    try {
      await wrapper.get('[aria-label="提交说明"]').setValue('原有说明')
      invoke.mockRejectedValueOnce(new Error('请先在设置中选择默认 AI'))
      await wrapper.get('[aria-label="AI 生成提交说明"]').trigger('click')
      await flushPromises()
      expect(wrapper.text()).toContain('请先在设置中选择默认 AI')
      expect(wrapper.get<HTMLTextAreaElement>('[aria-label="提交说明"]').element.value).toBe('原有说明')
      const pending = deferred<string>()
      invoke.mockImplementation(async (command) =>
        command === 'generate_git_commit_message' ? pending.promise : state(),
      )
      await wrapper.get('[aria-label="AI 生成提交说明"]').trigger('click')
      await wrapper.setProps({ workspacePath: '/tmp/next-workspace' })
      await flushPromises()
      pending.resolve('上一个工作空间的说明')
      await flushPromises()
      expect(wrapper.get<HTMLTextAreaElement>('[aria-label="提交说明"]').element.value).toBe('')
      expect(actionCalls()).toHaveLength(0)
    } finally {
      wrapper.unmount()
    }
  })

  it.each([
    ['empty index', state({ files: [] })],
    [
      'conflicts',
      state({
        files: [{ path: 'conflict.txt', originalPath: null, indexStatus: 'U', worktreeStatus: 'U', conflicted: true }],
      }),
    ],
  ])('disables AI generation with %s', async (_reason, repository) => {
    const wrapper = await mountReady(repository)
    try {
      expect(wrapper.get('[aria-label="AI 生成提交说明"]').attributes('disabled')).toBeDefined()
      await wrapper.get('[aria-label="AI 生成提交说明"]').trigger('click')
      expect(invoke.mock.calls.filter(([command]) => command === 'generate_git_commit_message')).toHaveLength(0)
    } finally {
      wrapper.unmount()
    }
  })
})

function menuItem(label: string) {
  const element = [...document.querySelectorAll<HTMLButtonElement>('.git-context-menu [role="menuitem"]')].find(
    (item) => item.textContent?.trim() === label,
  )
  if (!element) throw new Error(`找不到菜单项：${label}`)
  return new DOMWrapper(element)
}

describe('history and action guidance', () => {
  it('preserves readable history and the failure reason when a repository action is rejected', async () => {
    invoke.mockImplementation(async (command) => {
      if (command === 'read_git_history') return { commits: [commit('previous')], hasMore: false }
      if (command === 'git_repository_action') throw new Error('分支切换被拒绝')
      if (command === 'read_git_repository') return state()
    })
    const git = useGitWorkbench(() => ({ workspacePath: '/tmp/workspace', repositoryId: 'one' }))
    await git.loadHistory({ query: '', reference: '', skip: 0 })
    expect(await git.runAction({ operation: 'switchBranch', reference: 'feature' })).toBe(false)
    expect(git.history.value?.commits[0]?.hash).toBe('previous')
    expect(git.error.value).toContain('分支切换被拒绝')
  })
  it('opens history diffs in the compact preview and returns to the selected commit', async () => {
    const wrapper = await mountReady()
    try {
      await button(wrapper, '提交历史').trigger('click')
      await flushPromises()
      await wrapper.get('.history-entry').trigger('click')
      await flushPromises()
      expect(wrapper.get('.history-split').classes()).toContain('show-diff')
      expect(wrapper.get('.diff-stats').attributes('aria-label')).toBe('新增 1 行，删除 0 行')
      await wrapper.get('[aria-label="返回提交列表"]').trigger('click')
      expect(wrapper.get('.history-split').classes()).not.toContain('show-diff')
      expect(document.activeElement).toBe(wrapper.get('.history-entry.selected').element)
      await wrapper.get('[aria-label="调整提交列表与差异宽度"]').trigger('keydown', { key: 'ArrowRight' })
      expect(wrapper.get('[aria-label="调整提交列表与差异宽度"]').attributes('aria-valuenow')).toBe('34')
    } finally {
      wrapper.unmount()
    }
  })

  it('clears an empty search and reloads the current branch without changing Git data', async () => {
    const wrapper = await mountReady()
    invoke.mockImplementation(async (command, args) => {
      if (command === 'read_git_repository') return state()
      if (command === 'read_git_history') return { commits: args.query ? [] : [commit('found')], hasMore: false }
    })
    try {
      await button(wrapper, '提交历史').trigger('click')
      await flushPromises()
      await wrapper.get('[aria-label="搜索提交说明"]').setValue('不存在的内容')
      await wrapper.get('.history-filters').trigger('submit')
      await flushPromises()
      expect(wrapper.text()).toContain('未找到相关提交')
      await button(wrapper, '清除筛选').trigger('click')
      await flushPromises()
      expect(wrapper.get('.history-entry').text()).toContain('found')
      expect(invoke).toHaveBeenLastCalledWith('read_git_history', {
        workspacePath: '/tmp/workspace',
        repositoryId: 'one',
        query: '',
        reference: '',
        skip: 0,
      })
      expect(actionCalls()).toHaveLength(0)
    } finally {
      wrapper.unmount()
    }
  })

  it('reloads history after switching branches instead of reusing the previous branch cache', async () => {
    let branch = 'main'
    invoke.mockImplementation(async (command) => {
      if (command === 'read_git_repository') return state({ branch })
      if (command === 'read_git_history') return { commits: [commit(branch)], hasMore: false }
      if (command === 'git_repository_action') {
        branch = 'feature'
        return '操作完成'
      }
    })
    const wrapper = mount(GitWorkbench, {
      attachTo: document.body,
      props: { workspacePath: '/tmp/workspace', repositories, disabled: false },
    })
    try {
      await flushPromises()
      await button(wrapper, '提交历史').trigger('click')
      await flushPromises()
      expect(wrapper.get('.history-entry').text()).toContain('main')
      await button(wrapper, '分支').trigger('click')
      const feature = wrapper.findAll('.branch-row').find((row) => row.text().includes('feature'))!
      await feature
        .findAll('button')
        .find((item) => item.text() === '切换')!
        .trigger('click')
      await flushPromises()
      await button(wrapper, '提交历史').trigger('click')
      await flushPromises()
      expect(wrapper.get('.history-entry').text()).toContain('feature')
      expect(invoke.mock.calls.filter(([command]) => command === 'read_git_history')).toHaveLength(2)
    } finally {
      wrapper.unmount()
    }
  })

  it('explains unavailable merge actions and navigates to Stash without saving automatically', async () => {
    const wrapper = await mountReady()
    try {
      await button(wrapper, '分支').trigger('click')
      const feature = wrapper.findAll('.branch-row').find((row) => row.text().includes('feature'))!
      const merge = feature.findAll('button').find((item) => item.text() === '合并')!
      expect(merge.attributes('disabled')).toBeDefined()
      expect(merge.attributes('title')).toContain('请先提交更改或保存到 Stash')
      await wrapper.get('.branch-notice button').trigger('click')
      expect(wrapper.find('.stash-view').exists()).toBe(true)
      expect(document.activeElement).toBe(wrapper.get('[aria-label="Stash 说明"]').element)
      expect(button(wrapper, '应用并保留').attributes('title')).toContain('保留此 Stash')
      expect(actionCalls()).toHaveLength(0)
    } finally {
      wrapper.unmount()
    }
  })
})

describe('Git workbench context menus', () => {
  it('stages and unstages the file from the correct area through the menu', async () => {
    const wrapper = await mountReady()
    try {
      await wrapper
        .get('[aria-label="查看工作区差异 partially staged.txt"]')
        .trigger('contextmenu', { clientX: 160, clientY: 200 })
      await flushPromises()
      expect(document.querySelector('[role="menu"]')).not.toBeNull()
      expect(actionCalls()).toHaveLength(0)
      await menuItem('暂存文件').trigger('click')
      await flushPromises()
      expect(actionCalls()[0]![1]).toMatchObject({ request: { operation: 'stage', paths: ['partially staged.txt'] } })
      expect(document.querySelector('.git-context-menu')).toBeNull()
      await wrapper.get('[aria-label="查看暂存差异 partially staged.txt"]').trigger('contextmenu')
      await flushPromises()
      await menuItem('取消暂存').trigger('click')
      await flushPromises()
      expect(actionCalls()[1]![1]).toMatchObject({ request: { operation: 'unstage', paths: ['partially staged.txt'] } })
    } finally {
      wrapper.unmount()
    }
  })

  it('keeps confirmation before discarding from the context menu', async () => {
    const wrapper = await mountReady()
    try {
      await wrapper.get('[aria-label="查看工作区差异 partially staged.txt"]').trigger('contextmenu')
      await flushPromises()
      await menuItem('丢弃未暂存的更改…').trigger('click')
      expect(wrapper.get('[role="alertdialog"]').text()).toContain('partially staged.txt')
      expect(actionCalls()).toHaveLength(0)
      await button(wrapper, '确认操作').trigger('click')
      await flushPromises()
      expect(actionCalls()[0]![1]).toMatchObject({ request: { operation: 'discard', paths: ['partially staged.txt'] } })
    } finally {
      wrapper.unmount()
    }
  })

  it('copies the context commit hash and opens its diff', async () => {
    const wrapper = await mountReady()
    try {
      await button(wrapper, '提交历史').trigger('click')
      await flushPromises()
      await wrapper.get('.history-entry').trigger('contextmenu')
      await flushPromises()
      await menuItem('复制提交哈希').trigger('click')
      await flushPromises()
      expect(invoke).toHaveBeenCalledWith('copy_content', { content: 'abc1234' })
      await wrapper.get('.history-entry').trigger('contextmenu')
      await flushPromises()
      await menuItem('查看提交差异').trigger('click')
      await flushPromises()
      expect(invoke).toHaveBeenCalledWith('read_git_diff', expect.objectContaining({ commit: 'abc1234', path: '' }))
      expect(wrapper.find('.commit-details').text()).toContain('提交 abc1234')
    } finally {
      wrapper.unmount()
    }
  })

  it('disables deleting the current branch and confirms branch and Stash deletion', async () => {
    const wrapper = await mountReady()
    try {
      await button(wrapper, '分支').trigger('click')
      await wrapper.findAll('.branch-row')[0]!.trigger('contextmenu')
      await flushPromises()
      expect(menuItem('删除本地分支…').attributes('disabled')).toBeDefined()
      expect(menuItem('切换分支').attributes('disabled')).toBeDefined()
      const feature = wrapper.findAll('.branch-row').find((row) => row.text().includes('feature'))!
      await feature.trigger('contextmenu')
      await flushPromises()
      await menuItem('删除本地分支…').trigger('click')
      expect(actionCalls()).toHaveLength(0)
      await button(wrapper, '确认操作').trigger('click')
      await flushPromises()
      expect(actionCalls()[0]![1]).toMatchObject({ request: { operation: 'deleteBranch', reference: 'feature' } })
      await button(wrapper, 'Stash').trigger('click')
      await wrapper.get('.stash-row').trigger('contextmenu')
      await flushPromises()
      await menuItem('删除 Stash…').trigger('click')
      expect(actionCalls()).toHaveLength(1)
      expect(wrapper.get('[role="alertdialog"]').text()).toContain('stash@{0}')
      await button(wrapper, '确认操作').trigger('click')
      await flushPromises()
      expect(actionCalls()[1]![1]).toMatchObject({ request: { operation: 'dropStash', reference: 'stash@{0}' } })
    } finally {
      wrapper.unmount()
    }
  })

  it('supports keyboard opening, navigation and Escape with focus restored to the file', async () => {
    const wrapper = await mountReady()
    try {
      const file = wrapper.get<HTMLButtonElement>('[aria-label="查看工作区差异 partially staged.txt"]')
      file.element.focus()
      await file.trigger('keydown', { key: 'F10', shiftKey: true })
      await flushPromises()
      expect(document.activeElement).toBe(menuItem('查看差异').element)
      await menuItem('查看差异').trigger('keydown', { key: 'ArrowDown' })
      expect(document.activeElement).toBe(menuItem('暂存文件').element)
      await menuItem('暂存文件').trigger('keydown', { key: 'Escape' })
      expect(document.querySelector('.git-context-menu')).toBeNull()
      expect(document.activeElement).toBe(file.element)
      expect(actionCalls()).toHaveLength(0)
    } finally {
      wrapper.unmount()
    }
  })

  it('dismisses menus on outside clicks, workspace changes and disabling the workbench', async () => {
    const wrapper = await mountReady()
    try {
      const open = async () => {
        await wrapper.get('[aria-label="查看工作区差异 partially staged.txt"]').trigger('contextmenu')
        await flushPromises()
        expect(document.querySelector('.git-context-menu')).not.toBeNull()
      }
      await open()
      document.body.dispatchEvent(new Event('pointerdown', { bubbles: true }))
      await flushPromises()
      expect(document.querySelector('.git-context-menu')).toBeNull()
      await open()
      await wrapper.setProps({ workspacePath: '/tmp/other-workspace' })
      await flushPromises()
      expect(document.querySelector('.git-context-menu')).toBeNull()
      await open()
      await wrapper.setProps({ disabled: true })
      expect(document.querySelector('.git-context-menu')).toBeNull()
      await wrapper.get('.file-row').trigger('contextmenu')
      expect(document.querySelector('.git-context-menu')).toBeNull()
      expect(actionCalls()).toHaveLength(0)
    } finally {
      wrapper.unmount()
    }
  })
})

describe('Git workbench requests', () => {
  it('drops an older repository refresh after a newer repository is selected', async () => {
    const first = deferred<GitRepositoryState>()
    const second = deferred<GitRepositoryState>()
    invoke.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise)
    let scope = { workspacePath: '/tmp/workspace', repositoryId: 'one' }
    const git = useGitWorkbench(() => scope)
    const oldRead = git.refresh()
    scope = { ...scope, repositoryId: 'two' }
    const newRead = git.refresh()
    second.resolve(state({ root: '/tmp/project-two', branch: 'feature' }))
    expect(await newRead).toBe(true)
    first.resolve(state())
    expect(await oldRead).toBe(false)
    expect(git.repository.value?.root).toBe('/tmp/project-two')
    expect(git.repository.value?.branch).toBe('feature')
    expect(git.loading.value).toBe(false)
  })

  it('invalidates pending diff and history responses when the workspace is cleared', async () => {
    const diff = deferred<{ text: string; truncated: boolean }>()
    const history = deferred<{ commits: GitHistoryCommit[]; hasMore: boolean }>()
    invoke.mockImplementation((command: string) => (command === 'read_git_diff' ? diff.promise : history.promise))
    const git = useGitWorkbench(() => ({ workspacePath: '/tmp/workspace', repositoryId: 'one' }))
    const diffRead = git.showDiff({ path: 'file.txt', staged: false, commit: '' })
    const historyRead = git.loadHistory({ query: '', reference: '', skip: 0 })
    git.clear()
    diff.resolve({ text: 'old workspace content', truncated: false })
    history.resolve({ commits: [commit('old')], hasMore: false })
    expect(await diffRead).toBe(false)
    expect(await historyRead).toBe(false)
    expect(git.diff.value).toBeNull()
    expect(git.history.value).toBeNull()
    expect(git.diffLoading.value).toBe(false)
    expect(git.historyLoading.value).toBe(false)
  })

  it('uses the newest selected file and passes staged and commit scope to the backend', async () => {
    const oldDiff = deferred<{ text: string; truncated: boolean }>()
    invoke.mockReturnValueOnce(oldDiff.promise).mockResolvedValueOnce({ text: '+staged', truncated: true })
    const git = useGitWorkbench(() => ({ workspacePath: '/tmp/workspace', repositoryId: 'one' }))
    const oldRead = git.showDiff({ path: 'old file.txt', staged: false, commit: '' })
    expect(await git.showDiff({ path: 'renamed file.txt', staged: true, commit: '' })).toBe(true)
    oldDiff.resolve({ text: '+old', truncated: false })
    expect(await oldRead).toBe(false)
    expect(git.diff.value).toEqual({ text: '+staged', truncated: true })
    expect(invoke).toHaveBeenLastCalledWith('read_git_diff', {
      workspacePath: '/tmp/workspace',
      repositoryId: 'one',
      path: 'renamed file.txt',
      staged: true,
      commit: '',
    })
  })

  it('resets history for a new query and ignores the late previous search', async () => {
    const old = deferred<{ commits: GitHistoryCommit[]; hasMore: boolean }>()
    invoke.mockReturnValueOnce(old.promise).mockResolvedValueOnce({ commits: [commit('new')], hasMore: false })
    const git = useGitWorkbench(() => ({ workspacePath: '/tmp/workspace', repositoryId: 'one' }))
    const oldRead = git.loadHistory({ query: 'old', reference: '__all__', skip: 0 })
    expect(await git.loadHistory({ query: 'new', reference: 'main', skip: 0 })).toBe(true)
    old.resolve({ commits: [commit('old')], hasMore: true })
    expect(await oldRead).toBe(false)
    expect(git.history.value?.commits.map((item) => item.hash)).toEqual(['new'])
    expect(git.history.value?.hasMore).toBe(false)
    expect(invoke).toHaveBeenLastCalledWith('read_git_history', {
      workspacePath: '/tmp/workspace',
      repositoryId: 'one',
      query: 'new',
      reference: 'main',
      skip: 0,
    })
  })

  it('appends the next page without replacing the previous history', async () => {
    invoke
      .mockResolvedValueOnce({ commits: [commit('first')], hasMore: true })
      .mockResolvedValueOnce({ commits: [commit('second')], hasMore: false })
    const git = useGitWorkbench(() => ({ workspacePath: '/tmp/workspace', repositoryId: 'one' }))
    await git.loadHistory({ query: '', reference: '__all__', skip: 0 })
    await git.loadHistory({ query: '', reference: '__all__', skip: 1 })
    expect(git.history.value?.commits.map((item) => item.hash)).toEqual(['first', 'second'])
  })

  it('refreshes after a failed mutation and preserves its error', async () => {
    invoke.mockImplementation(async (command: string) => {
      if (command === 'git_repository_action') throw new Error('stash apply 产生冲突')
      return state({
        files: [{ path: 'conflict.txt', originalPath: null, indexStatus: 'U', worktreeStatus: 'U', conflicted: true }],
      })
    })
    const git = useGitWorkbench(() => ({ workspacePath: '/tmp/workspace', repositoryId: 'one' }))
    expect(await git.runAction({ operation: 'applyStash', reference: 'stash@{0}' })).toBe(false)
    expect(git.error.value).toContain('stash apply 产生冲突')
    expect(git.repository.value?.files[0]?.conflicted).toBe(true)
    expect(git.busy.value).toBe(false)
  })

  it('prevents concurrent mutations and never reports an old workspace action as current', async () => {
    const action = deferred<string>()
    invoke.mockReturnValueOnce(action.promise)
    let scope = { workspacePath: '/tmp/workspace', repositoryId: 'one' }
    const git = useGitWorkbench(() => scope)
    const first = git.runAction({ operation: 'stage', paths: ['file.txt'] })
    expect(await git.runAction({ operation: 'commit', message: 'duplicate' })).toBe(false)
    expect(actionCalls()).toHaveLength(1)
    scope = { workspacePath: '/tmp/other-workspace', repositoryId: 'one' }
    git.clear()
    action.resolve('old success')
    expect(await first).toBe(false)
    expect(git.notice.value).toBe('')
    expect(git.repository.value).toBeNull()
  })

  it('explains desktop requirements without invoking local Git in browser preview', async () => {
    desktop.value = false
    const git = useGitWorkbench(() => ({ workspacePath: '/tmp/workspace', repositoryId: 'one' }))
    expect(await git.refresh()).toBe(false)
    expect(git.error.value).toContain('桌面')
    expect(invoke).not.toHaveBeenCalled()
  })
})

describe('Git workbench controls', () => {
  it('requests the correct side when selecting partially staged file differences', async () => {
    const wrapper = await mountReady()
    await wrapper.get('[aria-label="查看暂存差异 partially staged.txt"]').trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenLastCalledWith('read_git_diff', {
      workspacePath: '/tmp/workspace',
      repositoryId: 'one',
      path: 'partially staged.txt',
      staged: true,
      commit: '',
    })
    await wrapper.get('[aria-label="查看工作区差异 partially staged.txt"]').trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenLastCalledWith('read_git_diff', {
      workspacePath: '/tmp/workspace',
      repositoryId: 'one',
      path: 'partially staged.txt',
      staged: false,
      commit: '',
    })
    wrapper.unmount()
  })

  it('sends the destination of renamed files so the backend can expand both paths safely', async () => {
    const wrapper = await mountReady(
      state({
        files: [
          {
            path: 'new name.txt',
            originalPath: 'old name.txt',
            indexStatus: 'R',
            worktreeStatus: 'M',
            conflicted: false,
          },
        ],
      }),
    )
    await wrapper.get('[aria-label="取消暂存 new name.txt"]').trigger('click')
    await flushPromises()
    expect(actionCalls()[0]![1]).toMatchObject({ request: { operation: 'unstage', paths: ['new name.txt'] } })
    await wrapper.get('[aria-label="暂存 new name.txt"]').trigger('click')
    await flushPromises()
    expect(actionCalls()[1]![1]).toMatchObject({ request: { operation: 'stage', paths: ['new name.txt'] } })
    wrapper.unmount()
  })

  it('shows staged and unstaged versions separately and commits only the staged content', async () => {
    const wrapper = await mountReady()
    expect(wrapper.text()).toContain('partially staged.txt')
    expect(wrapper.text()).toContain('new file.txt')
    await wrapper.get('[aria-label="提交说明"]').setValue('只提交已经暂存的修改')
    await wrapper.get('.commit-form').trigger('submit')
    await flushPromises()
    expect(actionCalls()).toHaveLength(1)
    expect(actionCalls()[0]![1]).toMatchObject({
      workspacePath: '/tmp/workspace',
      repositoryId: 'one',
      request: { operation: 'commit', message: '只提交已经暂存的修改' },
    })
    expect(actionCalls().some(([, args]) => args.request.operation === 'stage')).toBe(false)
    wrapper.unmount()
  })

  it.each([
    [
      'unmerged conflicts',
      state({
        files: [{ path: 'conflict.txt', originalPath: null, indexStatus: 'U', worktreeStatus: 'U', conflicted: true }],
      }),
    ],
    [
      'no staged changes',
      state({
        files: [{ path: 'unstaged.txt', originalPath: null, indexStatus: ' ', worktreeStatus: 'M', conflicted: false }],
      }),
    ],
    ['a detached HEAD', state({ detached: true })],
  ])('disables committing with %s', async (_reason, repository) => {
    const wrapper = await mountReady(repository)
    await wrapper.get('[aria-label="提交说明"]').setValue('should not commit')
    expect(button(wrapper, '提交').attributes('disabled')).toBeDefined()
    expect(button(wrapper, '提交并推送').attributes('disabled')).toBeDefined()
    expect(actionCalls()).toHaveLength(0)
    wrapper.unmount()
  })

  it('requires confirmation before discarding a tracked file', async () => {
    const wrapper = await mountReady()
    await wrapper.get('[aria-label="丢弃 partially staged.txt"]').trigger('click')
    expect(wrapper.find('[role="alertdialog"]').exists()).toBe(true)
    expect(actionCalls()).toHaveLength(0)
    await button(wrapper, '取消').trigger('click')
    expect(actionCalls()).toHaveLength(0)
    await wrapper.get('[aria-label="丢弃 partially staged.txt"]').trigger('click')
    await button(wrapper, '确认操作').trigger('click')
    await flushPromises()
    expect(actionCalls()[0]![1]).toMatchObject({ request: { operation: 'discard', paths: ['partially staged.txt'] } })
    expect(wrapper.find('[aria-label="丢弃 new file.txt"]').exists()).toBe(false)
    wrapper.unmount()
  })

  it('retains the successful commit when the following push fails', async () => {
    const wrapper = await mountReady()
    invoke.mockImplementation(async (command: string, args: { request?: { operation: string } }) => {
      if (command === 'git_repository_action' && args.request?.operation === 'push') throw new Error('远程拒绝推送')
      if (command === 'git_repository_action') return 'created commit'
      return state({ files: [] })
    })
    await wrapper.get('[aria-label="提交说明"]').setValue('本地提交应该保留')
    await button(wrapper, '提交并推送').trigger('click')
    await flushPromises()
    expect(actionCalls().map(([, args]) => args.request.operation)).toEqual(['commit', 'push'])
    expect(wrapper.text()).toContain('远程拒绝推送')
    expect(button(wrapper, '提交').attributes('disabled')).toBeDefined()
    wrapper.unmount()
  })

  it('requires confirmation for branch and stash deletion and blocks unrelated actions while reviewing', async () => {
    const wrapper = await mountReady()
    await button(wrapper, '分支').trigger('click')
    const feature = wrapper.findAll('.branch-row').find((row) => row.text().includes('feature'))!
    await feature
      .findAll('button')
      .find((item) => item.text() === '删除')!
      .trigger('click')
    expect(wrapper.get('[role="alertdialog"]').text()).toContain('feature')
    expect(actionCalls()).toHaveLength(0)
    expect(button(wrapper, 'Push 推送').attributes('disabled')).toBeDefined()
    await button(wrapper, '确认操作').trigger('click')
    await flushPromises()
    expect(actionCalls()[0]![1]).toMatchObject({ request: { operation: 'deleteBranch', reference: 'feature' } })
    await button(wrapper, 'Stash').trigger('click')
    await button(wrapper, '删除').trigger('click')
    expect(actionCalls()).toHaveLength(1)
    await button(wrapper, '确认操作').trigger('click')
    await flushPromises()
    expect(actionCalls()[1]![1]).toMatchObject({ request: { operation: 'dropStash', reference: 'stash@{0}' } })
    wrapper.unmount()
  })

  it('drops a pending diff when switching workspaces', async () => {
    const wrapper = await mountReady()
    const previousDiff = deferred<{ text: string; truncated: boolean }>()
    invoke.mockImplementation(async (command: string, args: { workspacePath: string }) => {
      if (command === 'read_git_diff') return previousDiff.promise
      if (command === 'read_git_repository')
        return state({ root: args.workspacePath === '/tmp/new-workspace' ? '/tmp/new-project' : '/tmp/project-one' })
      return undefined
    })
    await wrapper.get('[aria-label="查看工作区差异 partially staged.txt"]').trigger('click')
    await wrapper.setProps({ workspacePath: '/tmp/new-workspace' })
    await flushPromises()
    previousDiff.resolve({ text: 'content from previous workspace', truncated: false })
    await flushPromises()
    expect(wrapper.text()).toContain('/tmp/new-project')
    expect(wrapper.text()).not.toContain('content from previous workspace')
    wrapper.unmount()
  })

  it('refreshes on reactivation while retaining the draft commit message', async () => {
    const wrapper = await mountReady()
    await wrapper.get('[aria-label="提交说明"]').setValue('保留我的提交草稿')
    await wrapper.setProps({ active: false })
    invoke.mockClear()
    await wrapper.setProps({ active: true })
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('read_git_repository', { workspacePath: '/tmp/workspace', repositoryId: 'one' })
    expect(wrapper.get<HTMLTextAreaElement>('[aria-label="提交说明"]').element.value).toBe('保留我的提交草稿')
    wrapper.unmount()
  })
})
