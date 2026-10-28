// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { computed, ref } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'
import dayjs from 'dayjs'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke, isTauri: () => true }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: async () => null }))

import { cloneGitSettings, gitDailyReport, useGit, validateGitSettings, type GitDailyLog } from '../src/lib/useGit'
import GitModule from '../src/components/modules/GitModule.vue'
import GitSettings from '../src/components/GitSettings.vue'
import { MODULE_CONTEXT, type ModuleContext } from '../src/modules/context'

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

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

beforeEach(() => invoke.mockReset())

describe('Git daily reports', () => {
  it('groups full commit messages by repository and leaves failed or empty repositories out of the report', () => {
    expect(gitDailyReport(log)).toBe('日报项目\n1、完成 Git 配置\n   支持多仓库\n   保留已有日报')
    expect(gitDailyReport({ date: log.date, repositories: [] })).toBe('')
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
    expect(git.settings.value).toEqual(configuration)
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
      openGitSettings: vi.fn(),
      selectReport: vi.fn(),
    },
  }
}

describe('Git module', () => {
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
    await wrapper.get('.git-report-actions .text-action').trigger('click')
    expect(invoke).toHaveBeenCalledWith('copy_content', { content: '人工调整后的工作记录' })
    await wrapper.get('.git-report-actions .git-fetch').trigger('click')
    await flushPromises()
    expect(context.actions.appendGitReport).toHaveBeenCalledExactlyOnceWith(
      '/tmp/workspace',
      log.date,
      '人工调整后的工作记录',
    )
    expect(wrapper.get('.git-report-actions .git-fetch').attributes('disabled')).toBeDefined()
    context.journal.selectedDate.value = '20261029'
    await flushPromises()
    expect(wrapper.find('textarea').exists()).toBe(false)
    wrapper.unmount()
  })

  it('disables reads for a selected range and guides unconfigured workspaces to Git settings', async () => {
    const context = moduleContext()
    context.rangeDates.value = ['20261027', log.date]
    const wrapper = mount(GitModule, {
      global: { provide: { [MODULE_CONTEXT as symbol]: context as unknown as ModuleContext } },
    })
    expect(wrapper.get('.git-toolbar .git-fetch').attributes('disabled')).toBeDefined()
    expect(wrapper.text()).toContain('每次读取一天')
    context.git.acceptSaved(cloneGitSettings())
    await flushPromises()
    await wrapper.get('.git-empty button').trigger('click')
    expect(context.actions.openGitSettings).toHaveBeenCalledOnce()
    wrapper.unmount()
  })
})

describe('Git settings form', () => {
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
