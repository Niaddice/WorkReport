// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { computed, ref } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke, isTauri: () => true }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: async () => null }))

import { cloneGitSettings, useGit, validateGitSettings } from '../src/lib/useGit'
import GitModule from '../src/components/modules/GitModule.vue'
import GitSettings from '../src/components/GitSettings.vue'
import { MODULE_CONTEXT, type ModuleContext } from '../src/modules/context'

const configuration = {
  executable: 'git',
  includeMerges: false,
  repositories: [{ id: 'one', name: '项目', path: '/tmp/project', author: '', branch: '' }],
}
function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}
beforeEach(() => invoke.mockReset())

describe('Git settings store', () => {
  it('loads saved repositories and reports errors without starting a daily log request', async () => {
    const git = useGit()
    invoke.mockRejectedValueOnce(new Error('读取失败')).mockResolvedValueOnce(configuration)
    expect(await git.load()).toBe(false)
    expect(git.error.value).toContain('读取失败')
    expect(await git.load()).toBe(true)
    expect(git.settings.value).toEqual(cloneGitSettings(configuration))
    expect(invoke.mock.calls.map(([command]) => command)).toEqual(['load_git_settings', 'load_git_settings'])
  })
  it('keeps accepted settings when a previous load arrives late', async () => {
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
  it('validates repository paths and keeps older saved fields in detached settings copies', () => {
    const value = cloneGitSettings({
      ...configuration,
      includeMerges: true,
      repositories: [{ ...configuration.repositories[0]!, author: 'previous author', branch: 'main' }],
    })
    expect(validateGitSettings(value)).toBe('')
    expect(value.includeMerges).toBe(true)
    expect(value.repositories[0]!.author).toBe('previous author')
    value.repositories[0]!.path = 'relative/path'
    expect(validateGitSettings(value)).toContain('绝对路径')
    expect(configuration.repositories[0]!.path).toBe('/tmp/project')
  })
})

function moduleContext() {
  const git = useGit()
  git.acceptSaved(configuration)
  return {
    git,
    ai: { defaultProvider: computed(() => undefined) },
    journal: {
      workspace: ref({ path: '/tmp/workspace', name: 'workspace' }),
      selectedDate: ref('20261101'),
      saving: ref(false),
    },
    disabled: ref(false),
    rangeDates: ref<string[]>([]),
    actions: { appendGitReport: vi.fn().mockResolvedValue(true), openGitSettings: vi.fn(), openAiSettings: vi.fn() },
  }
}
function mountWorkbenchModule(context: ReturnType<typeof moduleContext>) {
  return mount(GitModule, {
    global: {
      provide: { [MODULE_CONTEXT as symbol]: context as unknown as ModuleContext },
      stubs: { GitWorkbench: true },
    },
  })
}

describe('Git workbench module', () => {
  it('only renders the workbench and forwards configuration actions', async () => {
    const context = moduleContext()
    const wrapper = mountWorkbenchModule(context)
    try {
      const workbench = wrapper.getComponent({ name: 'GitWorkbench' })
      expect(wrapper.find('.git-views').exists()).toBe(false)
      expect(wrapper.find('.git-toolbar').exists()).toBe(false)
      expect(wrapper.find('.git-report-preview').exists()).toBe(false)
      expect(wrapper.text()).not.toContain('提交日报')
      expect(workbench.props()).toMatchObject({
        workspacePath: '/tmp/workspace',
        disabled: false,
        repositories: configuration.repositories,
      })
      workbench.vm.$emit('configureRepositories')
      workbench.vm.$emit('configureAi')
      expect(context.actions.openGitSettings).toHaveBeenCalledOnce()
      expect(context.actions.openAiSettings).toHaveBeenCalledOnce()
      expect(invoke).not.toHaveBeenCalled()
    } finally {
      wrapper.unmount()
    }
  })
  it('retains explicit append of selected work records to the currently selected single day', async () => {
    const context = moduleContext()
    const wrapper = mountWorkbenchModule(context)
    try {
      const workbench = wrapper.getComponent({ name: 'GitWorkbench' })
      expect(await workbench.props('appendReport')('已选的工作记录')).toBe(true)
      expect(context.actions.appendGitReport).toHaveBeenCalledExactlyOnceWith(
        '/tmp/workspace',
        '20261101',
        '已选的工作记录',
      )
      context.rangeDates.value = ['20261101', '20261102']
      await flushPromises()
      expect(workbench.props('appendDateLabel')).toBeUndefined()
      expect(await workbench.props('appendReport')('不应追加')).toBe(false)
      expect(context.actions.appendGitReport).toHaveBeenCalledOnce()
    } finally {
      wrapper.unmount()
    }
  })
  it('shows settings read errors and disables operations while settings are loading', async () => {
    const context = moduleContext()
    context.git.error.value = '读取 Git 设置失败'
    context.git.loading.value = true
    const wrapper = mountWorkbenchModule(context)
    try {
      expect(wrapper.get('[role="alert"]').text()).toContain('读取 Git 设置失败')
      expect(wrapper.getComponent({ name: 'GitWorkbench' }).props('disabled')).toBe(true)
    } finally {
      wrapper.unmount()
    }
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

  it('switches commit prompt when clicking preset pills', async () => {
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
    const pills = wrapper.findAll('.prompt-preset-pill')
    expect(pills.length).toBeGreaterThanOrEqual(4)
    const gitmojiPill = pills.find((p) => p.text().includes('Gitmoji'))
    expect(gitmojiPill).toBeDefined()
    await gitmojiPill!.trigger('click')
    expect(wrapper.props('modelValue').commitPrompt).toContain('Gitmoji')
    expect(gitmojiPill!.classes()).toContain('is-active')
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
    expect(wrapper.vm.validate()).toBe(true)
    expect(wrapper.props('modelValue').repositories[0]).toMatchObject({
      name: '项目',
      path: '/tmp/project',
      author: '',
      branch: '',
    })
    expect(invoke).not.toHaveBeenCalled()
    wrapper.unmount()
  })
})
