// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { computed, ref } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'
import dayjs from 'dayjs'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke, isTauri: () => true }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: async () => null }))

import {
  cloneGitSettings,
  gitDailyReport,
  gitDailyReports,
  useGit,
  validateGitDateRange,
  validateGitSettings,
  type GitDailyLog,
} from '../src/lib/useGit'
import GitModule from '../src/components/modules/GitModule.vue'
import GitSettings from '../src/components/GitSettings.vue'
import { MODULE_CONTEXT, type ModuleContext } from '../src/modules/context'
import type { GitAppendResult } from '../src/lib/workspace'

const configuration = {
  executable: 'git',
  includeMerges: false,
  repositories: [{ id: 'one', name: '日报项目', path: '/tmp/project', author: 'me@example.test', branch: 'main' }],
}
const log: GitDailyLog = {
  date: '20261028',
  repositories: [
    {
      id: 'one',
      name: '日报项目',
      error: null,
      truncated: false,
      commits: [
        {
          hash: 'abc123456',
          author: '开发者',
          email: 'me@example.test',
          committedAt: '2026-10-28T12:00:00+08:00',
          message: '完成 Git 配置\n\n支持多仓库\n保留已有日报',
        },
      ],
    },
    { id: 'two', name: '失败仓库', error: '目录不存在', truncated: false, commits: [] },
  ],
}

function rangeLog(startDate: string, endDate: string): GitDailyLog {
  return { ...log, date: startDate, startDate, endDate }
}

function splitLog(): GitDailyLog {
  const repository = log.repositories[0]!
  const first = repository.commits[0]!
  return {
    ...rangeLog('20261027', '20261029'),
    repositories: [
      {
        ...repository,
        commits: [
          { ...first, hash: 'day-one', committedAt: dayjs('20261027').hour(12).toISOString(), message: '第一天工作' },
          { ...first, hash: 'day-two', committedAt: dayjs('20261028').hour(12).toISOString(), message: '第二天工作' },
        ],
      },
    ],
  }
}

function appendedDocument(date: string, content: string) {
  return { date, filename: `${date}.txt`, format: 'txt' as const, content, revision: `saved-${date}` }
}

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

beforeEach(() => invoke.mockReset())

describe('Git daily reports', () => {
  it('splits commits into their own local dates and never creates reports for empty or unselected days', () => {
    const value = splitLog()
    const source = JSON.stringify(value)
    expect(gitDailyReports(value, ['20261027', '20261028', '20261029'])).toEqual([
      { date: '20261027', content: '日报项目\n1、第一天工作' },
      { date: '20261028', content: '日报项目\n1、第二天工作' },
    ])
    expect(gitDailyReports(value, ['20261028', '20261029'])).toEqual([
      { date: '20261028', content: '日报项目\n1、第二天工作' },
    ])
    expect(JSON.stringify(value)).toBe(source)
  })

  it('assigns offset timestamps by the local commit date and preserves repository grouping within that day', () => {
    const committedAt = '2026-10-28T23:30:00-07:00'
    const date = dayjs(committedAt).format('YYYYMMDD')
    const commit = { ...log.repositories[0]!.commits[0]!, committedAt, message: '午夜后的本地工作\n\n详细说明' }
    const value: GitDailyLog = {
      date,
      repositories: [
        { ...log.repositories[0]!, commits: [commit] },
        {
          ...log.repositories[0]!,
          id: 'another',
          name: '第二项目',
          commits: [{ ...commit, hash: 'other', message: '另一项目工作' }],
        },
      ],
    }
    expect(gitDailyReports(value)).toEqual([
      { date, content: '日报项目\n1、午夜后的本地工作\n   详细说明\n\n第二项目\n1、另一项目工作' },
    ])
  })

  it('groups full commit messages by repository and leaves failed or empty repositories out of the report', () => {
    expect(gitDailyReport(log)).toBe('日报项目\n1、完成 Git 配置\n   支持多仓库\n   保留已有日报')
    expect(gitDailyReport({ date: log.date, repositories: [] })).toBe('')
  })

  it('adds the selected period to multi-day reports while preserving the single-day report format', () => {
    const report = gitDailyReport(rangeLog('20261027', '20261029'))
    expect(report).toContain('Git 工作记录（2026-10-27 ~ 2026-10-29）')
    expect(report).toContain('日报项目\n1、完成 Git 配置\n   支持多仓库\n   保留已有日报')
    expect(report).not.toContain('失败仓库')
    expect(gitDailyReport(rangeLog(log.date, log.date))).toBe(gitDailyReport(log))
    expect(gitDailyReport({ ...rangeLog('20261027', '20261029'), repositories: [] })).toBe('')
  })

  it.each([
    ['20261028', '20261028', true],
    ['20261231', '20270102', true],
    ['20240228', '20240301', true],
    ['20260229', '20260301', false],
    ['20261029', '20261028', false],
    ['2026-10-28', '20261029', false],
    ['', '20261029', false],
    ['20261028', '', false],
  ])('validates actual ordered range endpoints %s through %s', (start, end, valid) => {
    expect(validateGitDateRange(start, end) === '').toBe(valid)
  })

  it('passes both inclusive endpoints for range reads and retains the existing single-day IPC contract', async () => {
    const git = useGit()
    invoke.mockResolvedValueOnce(rangeLog('20261231', '20270102'))
    expect(await git.read('/tmp/workspace', '20261231', '20270102')).toBe(true)
    expect(invoke).toHaveBeenLastCalledWith('read_git_daily_log', {
      workspacePath: '/tmp/workspace',
      date: '20261231',
      endDate: '20270102',
    })
    expect(git.result.value?.endDate).toBe('20270102')
    invoke.mockResolvedValueOnce(log)
    expect(await git.read('/tmp/workspace', log.date, log.date)).toBe(true)
    expect(invoke).toHaveBeenLastCalledWith('read_git_daily_log', {
      workspacePath: '/tmp/workspace',
      date: log.date,
    })
  })

  it('rejects reversed dates before IPC and prevents an older range response from replacing a newer range', async () => {
    const git = useGit()
    expect(await git.read('/tmp/workspace', '20261029', '20261028')).toBe(false)
    expect(invoke).not.toHaveBeenCalled()
    expect(git.readError.value).not.toBe('')
    const previous = deferred<GitDailyLog>()
    invoke.mockReturnValueOnce(previous.promise).mockResolvedValueOnce(rangeLog('20261027', '20261029'))
    const first = git.read('/tmp/workspace', '20261027', '20261028')
    expect(await git.read('/tmp/workspace', '20261027', '20261029')).toBe(true)
    previous.resolve(rangeLog('20261027', '20261028'))
    expect(await first).toBe(false)
    expect(git.result.value?.endDate).toBe('20261029')
    expect(git.reading.value).toBe(false)
  })

  it('validates local paths and branch options while isolating nested drafts', () => {
    const value = cloneGitSettings(configuration)
    expect(validateGitSettings(value)).toBe('')
    value.repositories[0]!.branch = '--all'
    expect(validateGitSettings(value)).toContain('分支配置无效')
    expect(configuration.repositories[0]!.branch).toBe('main')
    value.repositories[0]!.branch = ''
    value.repositories[0]!.path = 'relative/path'
    expect(validateGitSettings(value)).toContain('绝对路径')
  })

  it('discards pending reads after a date or workspace change', async () => {
    const pending = deferred<GitDailyLog>()
    invoke.mockReturnValueOnce(pending.promise)
    const git = useGit()
    const reading = git.read('/tmp/workspace', log.date)
    git.clear()
    pending.resolve(log)
    expect(await reading).toBe(false)
    expect(git.result.value).toBeNull()
    expect(git.reading.value).toBe(false)
  })

  it('keeps accepted settings when a previous settings load arrives late', async () => {
    const pending = deferred<typeof configuration>()
    invoke.mockReturnValueOnce(pending.promise)
    const git = useGit()
    const loading = git.load()
    git.acceptSaved(configuration)
    pending.resolve({ ...configuration, repositories: [] })
    expect(await loading).toBe(false)
    expect(git.settings.value).toEqual(cloneGitSettings(configuration))
    expect(git.loading.value).toBe(false)
  })

  it('surfaces errors and allows a subsequent retry', async () => {
    const git = useGit()
    invoke.mockRejectedValueOnce(new Error('工作空间已切换'))
    expect(await git.read('/tmp/workspace', log.date)).toBe(false)
    expect(git.readError.value).toContain('工作空间已切换')
    invoke.mockResolvedValueOnce(log)
    expect(await git.read('/tmp/workspace', log.date)).toBe(true)
    expect(git.readError.value).toBe('')
    expect(git.result.value).toEqual(log)
  })
})

function moduleContext() {
  const git = useGit()
  git.acceptSaved(configuration)
  const draft = ref('')
  return {
    git,
    ai: { defaultProvider: computed(() => undefined) },
    journal: {
      workspace: ref({ path: '/tmp/workspace', name: 'workspace' }),
      selectedDate: ref(log.date),
      saving: ref(false),
      error: ref(''),
      draft,
    },
    disabled: computed(() => false),
    today: computed(() => log.date),
    now: ref(dayjs(log.date)),
    rangeDates: ref<string[]>([]),
    actions: {
      appendGitReport: vi.fn(async (_path: string, _date: string, content: string) => {
        draft.value = content
        return true
      }),
      appendGitReports: vi.fn(
        async (_path: string, reports: { date: string; content: string }[]): Promise<GitAppendResult> => ({
          documents: reports.map(({ date, content }) => appendedDocument(date, content)),
          skippedDates: [],
          failed: [],
        }),
      ),
      openGitSettings: vi.fn(),
      selectReport: vi.fn(),
    },
  }
}

describe('Git module', () => {
  it('keeps icon-only fetch, copy and append actions together in the top toolbar', async () => {
    const context = moduleContext()
    invoke.mockResolvedValueOnce(log)
    const wrapper = mount(GitModule, {
      global: { provide: { [MODULE_CONTEXT as symbol]: context as unknown as ModuleContext } },
    })
    expect(wrapper.find('.git-toolbar [aria-label="Git 配置"]').exists()).toBe(false)
    expect(wrapper.get('.git-toolbar-actions').text()).toBe('')
    expect(wrapper.get('.git-fetch').attributes('aria-label')).toBe('获取提交')
    expect(wrapper.find('.git-copy').exists()).toBe(false)
    await wrapper.get('.git-fetch').trigger('click')
    await flushPromises()
    expect(wrapper.findAll('.git-toolbar-actions .icon-button')).toHaveLength(3)
    expect(wrapper.get('.git-toolbar-actions').text()).toBe('')
    expect(wrapper.get('.git-copy').attributes('title')).toBe('复制日报')
    expect(wrapper.get('.git-append').attributes('title')).toBe('追加到当日日报')
    expect(wrapper.find('.git-report-actions').exists()).toBe(false)
    context.journal.selectedDate.value = '20261029'
    await flushPromises()
    expect(wrapper.find('.git-copy').exists()).toBe(false)
    expect(wrapper.find('.git-append').exists()).toBe(false)
    wrapper.unmount()
  })

  it('opens the workbench lazily, retains it between views, and invalidates reports after repository actions', async () => {
    const context = moduleContext()
    invoke.mockResolvedValueOnce(log)
    const wrapper = mount(GitModule, {
      global: {
        provide: { [MODULE_CONTEXT as symbol]: context as unknown as ModuleContext },
        stubs: {
          GitWorkbench: {
            name: 'GitWorkbench',
            props: ['workspacePath', 'repositories', 'disabled', 'active'],
            emits: ['changed'],
            template: '<button class="workbench-probe" @click="$emit(\'changed\')">仓库操作</button>',
          },
        },
      },
    })
    expect(wrapper.find('.workbench-probe').exists()).toBe(false)
    await wrapper.get('.git-toolbar .git-fetch').trigger('click')
    await flushPromises()
    expect(wrapper.find('textarea').exists()).toBe(true)
    await wrapper.findAll('.git-views button')[1]!.trigger('click')
    const workbench = wrapper.findComponent({ name: 'GitWorkbench' })
    expect(workbench.props()).toMatchObject({ workspacePath: '/tmp/workspace', active: true })
    await wrapper.get('.workbench-probe').trigger('click')
    expect(context.git.result.value).toBeNull()
    await wrapper.findAll('.git-views button')[0]!.trigger('click')
    expect(wrapper.find('.workbench-probe').exists()).toBe(true)
    expect(workbench.props('active')).toBe(false)
    expect(wrapper.find('textarea').exists()).toBe(false)
    wrapper.unmount()
  })

  it('shows skipped duplicate commits without labelling them as an empty day', async () => {
    const context = moduleContext()
    const deduplicated: GitDailyLog = {
      date: log.date,
      repositories: [
        log.repositories[0]!,
        {
          id: 'clone',
          name: '同项目克隆',
          error: null,
          truncated: false,
          commits: [],
          duplicateCommits: 1,
        },
      ],
    }
    invoke.mockResolvedValueOnce(deduplicated)
    const wrapper = mount(GitModule, {
      global: { provide: { [MODULE_CONTEXT as symbol]: context as unknown as ModuleContext } },
    })
    await wrapper.get('.git-toolbar .git-fetch').trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('已跳过 1 条与其他来源重复的提交')
    expect(wrapper.findAll('.git-repository-log')[1]!.text()).not.toContain('当天没有符合')
    expect(wrapper.get<HTMLTextAreaElement>('textarea').element.value).not.toContain('同项目克隆')
    wrapper.unmount()
  })

  it('previews partial results, copies edited content, and appends once to the selected day', async () => {
    const context = moduleContext()
    invoke.mockImplementation(async (command) => (command === 'read_git_daily_log' ? log : undefined))
    const wrapper = mount(GitModule, {
      global: { provide: { [MODULE_CONTEXT as symbol]: context as unknown as ModuleContext } },
    })
    await wrapper.get('.git-toolbar .git-fetch').trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('read_git_daily_log', { workspacePath: '/tmp/workspace', date: log.date })
    expect(wrapper.text()).toContain('1 个仓库读取失败')
    expect(wrapper.text()).toContain('目录不存在')
    await wrapper.get('textarea').setValue('人工调整后的工作记录')
    await wrapper.get('.git-toolbar .git-copy').trigger('click')
    expect(invoke).toHaveBeenCalledWith('copy_content', { content: '人工调整后的工作记录' })
    await wrapper.get('.git-toolbar .git-append').trigger('click')
    await flushPromises()
    expect(context.actions.appendGitReport).toHaveBeenCalledExactlyOnceWith(
      '/tmp/workspace',
      log.date,
      '人工调整后的工作记录',
    )
    expect(wrapper.get('.git-toolbar .git-append').attributes('disabled')).toBeDefined()
    context.journal.selectedDate.value = '20261029'
    await flushPromises()
    expect(wrapper.find('textarea').exists()).toBe(false)
    wrapper.unmount()
  })

  it('reads calendar ranges in date order and appends range results to their matching journal dates', async () => {
    const context = moduleContext()
    context.rangeDates.value = ['20261029', log.date, '20261027']
    invoke.mockResolvedValueOnce(rangeLog('20261027', '20261029'))
    const wrapper = mount(GitModule, {
      global: { provide: { [MODULE_CONTEXT as symbol]: context as unknown as ModuleContext } },
    })
    expect(wrapper.findAll('input[type="date"]')).toHaveLength(0)
    expect(wrapper.get('.git-date').text()).toContain('2026年10月27日')
    expect(wrapper.get('.git-date').text()).toContain('2026年10月29日')
    expect(wrapper.get('.git-toolbar .git-fetch').attributes('disabled')).toBeUndefined()
    await wrapper.get('.git-toolbar .git-fetch').trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenCalledWith('read_git_daily_log', {
      workspacePath: '/tmp/workspace',
      date: '20261027',
      endDate: '20261029',
    })
    expect(wrapper.get<HTMLTextAreaElement>(`[aria-label="Git 日报预览 ${log.date}"]`).element.value).toContain(
      '完成 Git 配置',
    )
    expect(wrapper.get('.git-toolbar .git-copy').attributes('disabled')).toBeUndefined()
    expect(wrapper.get('.git-toolbar .git-append').attributes('disabled')).toBeUndefined()
    await wrapper.get('.git-toolbar .git-append').trigger('click')
    await flushPromises()
    expect(context.actions.appendGitReports).toHaveBeenCalledExactlyOnceWith('/tmp/workspace', [
      { date: log.date, content: gitDailyReport(log) },
    ])
    expect(context.actions.appendGitReport).not.toHaveBeenCalled()
    expect(wrapper.find('.git-commit-meta').text()).toContain('2026-10-28')
    wrapper.unmount()
  })

  it('keeps edited per-day previews mapped to their own dates and omits a selected day without commits', async () => {
    const context = moduleContext()
    context.rangeDates.value = ['20261027', '20261028', '20261029']
    invoke.mockResolvedValueOnce(splitLog())
    const wrapper = mount(GitModule, {
      global: { provide: { [MODULE_CONTEXT as symbol]: context as unknown as ModuleContext } },
    })
    await wrapper.get('.git-toolbar .git-fetch').trigger('click')
    await flushPromises()
    expect(wrapper.findAll('textarea')).toHaveLength(2)
    await wrapper.get('[aria-label="Git 日报预览 20261027"]').setValue('第一天人工补充')
    await wrapper.get('[aria-label="Git 日报预览 20261028"]').setValue('第二天人工补充')
    await wrapper.get('.git-toolbar .git-copy').trigger('click')
    expect(invoke).toHaveBeenCalledWith('copy_content', {
      content: expect.stringMatching(/2026-10-27[\s\S]*第一天人工补充[\s\S]*2026-10-28[\s\S]*第二天人工补充/),
    })
    await wrapper.get('.git-toolbar .git-append').trigger('click')
    await flushPromises()
    expect(context.actions.appendGitReports).toHaveBeenCalledExactlyOnceWith('/tmp/workspace', [
      { date: '20261027', content: '第一天人工补充' },
      { date: '20261028', content: '第二天人工补充' },
    ])
    expect(wrapper.get('.git-toolbar .git-append').attributes('disabled')).toBeDefined()
    wrapper.unmount()
  })

  it('retries only failed dates after a partially successful range append', async () => {
    const context = moduleContext()
    context.rangeDates.value = ['20261027', '20261028', '20261029']
    invoke.mockResolvedValueOnce(splitLog())
    context.actions.appendGitReports
      .mockResolvedValueOnce({
        documents: [appendedDocument('20261027', '日报项目\n1、第一天工作')],
        skippedDates: [],
        failed: [{ date: '20261028', error: '磁盘暂时不可写' }],
      })
      .mockResolvedValueOnce({
        documents: [appendedDocument('20261028', '日报项目\n1、第二天工作')],
        skippedDates: [],
        failed: [],
      })
    const wrapper = mount(GitModule, {
      global: { provide: { [MODULE_CONTEXT as symbol]: context as unknown as ModuleContext } },
    })
    await wrapper.get('.git-toolbar .git-fetch').trigger('click')
    await flushPromises()
    await wrapper.get('.git-toolbar .git-append').trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('磁盘暂时不可写')
    expect(wrapper.get('.git-toolbar .git-append').attributes('disabled')).toBeUndefined()
    await wrapper.get('.git-toolbar .git-append').trigger('click')
    await flushPromises()
    expect(context.actions.appendGitReports).toHaveBeenNthCalledWith(2, '/tmp/workspace', [
      { date: '20261028', content: '日报项目\n1、第二天工作' },
    ])
    expect(wrapper.get('.git-toolbar .git-append').attributes('disabled')).toBeDefined()
    wrapper.unmount()
  })

  it('does not display an old append failure after the calendar selection changes', async () => {
    const context = moduleContext()
    context.rangeDates.value = ['20261027', '20261028', '20261029']
    invoke.mockResolvedValueOnce(splitLog())
    const pending = deferred<Awaited<ReturnType<typeof context.actions.appendGitReports>>>()
    context.actions.appendGitReports.mockReturnValueOnce(pending.promise)
    const wrapper = mount(GitModule, {
      global: { provide: { [MODULE_CONTEXT as symbol]: context as unknown as ModuleContext } },
    })
    await wrapper.get('.git-toolbar .git-fetch').trigger('click')
    await flushPromises()
    await wrapper.get('.git-toolbar .git-append').trigger('click')
    context.rangeDates.value = []
    await flushPromises()
    pending.resolve({ documents: [], skippedDates: [], failed: [{ date: '20261027', error: '旧范围错误' }] })
    await flushPromises()
    expect(wrapper.text()).not.toContain('旧范围错误')
    expect(wrapper.find('textarea').exists()).toBe(false)
    wrapper.unmount()
  })

  it('marks already included dates as completed without appending them again', async () => {
    const context = moduleContext()
    context.rangeDates.value = ['20261027', '20261028', '20261029']
    invoke.mockResolvedValueOnce(splitLog())
    context.actions.appendGitReports.mockResolvedValueOnce({
      documents: [],
      skippedDates: ['20261027', '20261028'],
      failed: [],
    })
    const wrapper = mount(GitModule, {
      global: { provide: { [MODULE_CONTEXT as symbol]: context as unknown as ModuleContext } },
    })
    await wrapper.get('.git-toolbar .git-fetch').trigger('click')
    await flushPromises()
    await wrapper.get('.git-toolbar .git-append').trigger('click')
    await flushPromises()
    expect(wrapper.get('.git-toolbar .git-append').attributes('disabled')).toBeDefined()
    await wrapper.get('.git-toolbar .git-append').trigger('click')
    expect(context.actions.appendGitReports).toHaveBeenCalledTimes(1)
    wrapper.unmount()
  })

  it('clears a previous result when the calendar range endpoint changes', async () => {
    const context = moduleContext()
    context.rangeDates.value = ['20261026', '20261027', log.date, '20261029']
    invoke.mockResolvedValueOnce(rangeLog('20261026', '20261029'))
    const wrapper = mount(GitModule, {
      global: { provide: { [MODULE_CONTEXT as symbol]: context as unknown as ModuleContext } },
    })
    await wrapper.get('.git-toolbar .git-fetch').trigger('click')
    await flushPromises()
    expect(wrapper.find('textarea').exists()).toBe(true)
    context.rangeDates.value = ['20261026', '20261027', log.date, '20261029', '20261030']
    await flushPromises()
    expect(context.git.result.value).toBeNull()
    expect(wrapper.find('textarea').exists()).toBe(false)
    expect(wrapper.get('.git-date').text()).toContain('2026年10月30日')
    expect(context.journal.selectedDate.value).toBe(log.date)
    wrapper.unmount()
  })

  it('drops an in-flight result when only the calendar range changes with the selected day unchanged', async () => {
    const context = moduleContext()
    const pending = deferred<GitDailyLog>()
    invoke.mockReturnValueOnce(pending.promise)
    const wrapper = mount(GitModule, {
      global: { provide: { [MODULE_CONTEXT as symbol]: context as unknown as ModuleContext } },
    })
    await wrapper.get('.git-toolbar .git-fetch').trigger('click')
    context.rangeDates.value = ['20261027', log.date]
    await flushPromises()
    pending.resolve(log)
    await flushPromises()
    expect(context.journal.selectedDate.value).toBe(log.date)
    expect(context.git.result.value).toBeNull()
    expect(context.git.reading.value).toBe(false)
    expect(wrapper.find('textarea').exists()).toBe(false)
    expect(wrapper.get('.git-date').text()).toContain('2026年10月27日')
    wrapper.unmount()
  })

  it('drops a pending range response after the workspace changes', async () => {
    const context = moduleContext()
    context.rangeDates.value = ['20261026', '20261029']
    const pending = deferred<GitDailyLog>()
    invoke.mockReturnValueOnce(pending.promise)
    const wrapper = mount(GitModule, {
      global: { provide: { [MODULE_CONTEXT as symbol]: context as unknown as ModuleContext } },
    })
    await wrapper.get('.git-toolbar .git-fetch').trigger('click')
    context.journal.workspace.value = { path: '/tmp/other-workspace', name: 'other' }
    await flushPromises()
    pending.resolve(rangeLog('20261026', '20261029'))
    await flushPromises()
    expect(context.git.result.value).toBeNull()
    expect(context.git.reading.value).toBe(false)
    expect(wrapper.find('textarea').exists()).toBe(false)
    wrapper.unmount()
  })

  it('returns to the single-day request and append workflow after clearing the calendar range', async () => {
    const context = moduleContext()
    context.rangeDates.value = ['20261027', log.date, '20261029']
    invoke.mockResolvedValueOnce(rangeLog('20261027', '20261029')).mockResolvedValueOnce(log)
    const wrapper = mount(GitModule, {
      global: { provide: { [MODULE_CONTEXT as symbol]: context as unknown as ModuleContext } },
    })
    await wrapper.get('.git-toolbar .git-fetch').trigger('click')
    await flushPromises()
    expect(wrapper.get('.git-toolbar .git-append').attributes('disabled')).toBeUndefined()
    context.rangeDates.value = []
    await flushPromises()
    expect(context.git.result.value).toBeNull()
    await wrapper.get('.git-toolbar .git-fetch').trigger('click')
    await flushPromises()
    expect(invoke).toHaveBeenLastCalledWith('read_git_daily_log', { workspacePath: '/tmp/workspace', date: log.date })
    expect(wrapper.get('.git-toolbar .git-append').attributes('disabled')).toBeUndefined()
    expect(wrapper.findAll('input[type="date"]')).toHaveLength(0)
    wrapper.unmount()
  })

  it('guides unconfigured workspaces to Git settings', async () => {
    const context = moduleContext()
    context.git.acceptSaved(cloneGitSettings())
    const wrapper = mount(GitModule, {
      global: { provide: { [MODULE_CONTEXT as symbol]: context as unknown as ModuleContext } },
    })
    await wrapper.get('.git-empty button').trigger('click')
    expect(context.actions.openGitSettings).toHaveBeenCalledOnce()
    wrapper.unmount()
  })
})

describe('Git settings form', () => {
  it('reveals invalid collapsed repositories and opens newly added repositories for editing', async () => {
    const wrapper = mount(GitSettings, {
      attachTo: document.body,
      props: {
        modelValue: {
          ...cloneGitSettings(configuration),
          repositories: [...configuration.repositories, { id: 'invalid', name: '', path: '', author: '', branch: '' }],
        },
        busy: false,
        workspacePath: '/tmp/workspace',
        'onUpdate:modelValue': (value) => {
          void wrapper.setProps({ modelValue: value })
        },
      },
    })
    try {
      expect(wrapper.get<HTMLDetailsElement>('[data-repository-id="invalid"]').element.open).toBe(false)
      expect(wrapper.vm.validate()).toBe(false)
      expect(wrapper.get<HTMLDetailsElement>('[data-repository-id="invalid"]').element.open).toBe(true)
      expect(document.activeElement).toBe(wrapper.get('[data-repository-id="invalid"] input').element)
      await wrapper.get('.git-repositories-heading button').trigger('click')
      await flushPromises()
      const latest = wrapper.findAll<HTMLDetailsElement>('.git-repository-form').at(-1)!
      expect(latest.element.open).toBe(true)
      expect(document.activeElement).toBe(latest.get('input').element)
    } finally {
      wrapper.unmount()
    }
  })

  it('opens advanced settings when the saved Git executable needs correction', () => {
    const wrapper = mount(GitSettings, {
      attachTo: document.body,
      props: {
        modelValue: { ...cloneGitSettings(), executable: 'relative/path' },
        busy: false,
        workspacePath: '/tmp/workspace',
      },
    })
    try {
      expect(wrapper.get<HTMLDetailsElement>('.git-advanced').element.open).toBe(false)
      expect(wrapper.vm.validate()).toBe(false)
      expect(wrapper.get<HTMLDetailsElement>('.git-advanced').element.open).toBe(true)
      expect(document.activeElement).toBe(wrapper.get('.git-advanced input').element)
    } finally {
      wrapper.unmount()
    }
  })
  it('keeps the AI commit prompt as a settings draft and restores the recommended prompt', async () => {
    const wrapper = mount(GitSettings, {
      props: {
        modelValue: cloneGitSettings(),
        busy: false,
        workspacePath: '/tmp/workspace',
        'onUpdate:modelValue': (value) => {
          void wrapper.setProps({ modelValue: value })
        },
      },
    })
    await wrapper.get('[aria-label="Git 提交说明提示词"]').setValue('使用中文，只输出一行 fix: 说明')
    expect(wrapper.props('modelValue').commitPrompt).toBe('使用中文，只输出一行 fix: 说明')
    expect(wrapper.vm.validate()).toBe(true)
    expect(invoke).not.toHaveBeenCalled()
    await wrapper.get('.git-prompt-heading button').trigger('click')
    expect(wrapper.props('modelValue').commitPrompt).toBe(cloneGitSettings().commitPrompt)
    await wrapper.setProps({ modelValue: { ...cloneGitSettings(), commitPrompt: '字'.repeat(2001) } })
    expect(wrapper.vm.validate()).toBe(false)
    expect(wrapper.text()).toContain('2000 字')
    wrapper.unmount()
  })

  it('keeps changes in the parent draft and validates incomplete repositories before saving', async () => {
    const wrapper = mount(GitSettings, {
      props: {
        modelValue: cloneGitSettings(),
        busy: false,
        workspacePath: '/tmp/workspace',
        'onUpdate:modelValue': (value) => {
          void wrapper.setProps({ modelValue: value })
        },
      },
    })
    await wrapper.get('.git-repositories-heading button').trigger('click')
    expect(wrapper.findAll('.git-repository-form')).toHaveLength(1)
    expect(wrapper.vm.validate()).toBe(false)
    const fields = wrapper.findAll('.git-repository-form input')
    await fields[0]!.setValue('项目')
    await fields[1]!.setValue('/tmp/project')
    await fields[2]!.setValue('作者')
    await fields[3]!.setValue('main')
    expect(wrapper.vm.validate()).toBe(true)
    expect(wrapper.props('modelValue').repositories[0]).toMatchObject({
      name: '项目',
      path: '/tmp/project',
      author: '作者',
      branch: 'main',
    })
    expect(invoke).not.toHaveBeenCalled()
    wrapper.unmount()
  })
})
