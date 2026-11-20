// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { JOURNAL_TEMPLATE_PRESETS, interpolateTemplate, getJournalTemplateContent } from '../src/lib/journalTemplates'
import JournalTemplateButton from '../src/components/JournalTemplateButton.vue'
import { useJournal } from '../src/lib/useJournal'
import { api, type Document } from '../src/lib/workspace'
import { DEFAULT_EDITOR_SETTINGS } from '../src/lib/settingsDraft'

vi.mock('../src/lib/workspace', () => ({
  api: {
    backupWarning: vi.fn(),
    retryBackup: vi.fn(),
    open: vi.fn(),
    list: vi.fn(),
    previews: vi.fn(),
    read: vi.fn(),
    create: vi.fn(),
    save: vi.fn(),
    checkpointHistory: vi.fn(),
    switchFormat: vi.fn(),
  },
}))

describe('journal templates logic', () => {
  it('interpolates date variables correctly into template string', () => {
    const raw = '# 日报 {date} ({weekday})\n年份：{year} 月份：{month} 日期：{day} raw：{rawdate}'
    const result = interpolateTemplate(raw, '2026-11-08')
    expect(result).toBe('# 日报 2026-11-08 (星期日)\n年份：2026 月份：11 日期：08 raw：20261108')
  })

  it('handles case-insensitive and 8-digit date formatting', () => {
    const raw = '{YYYY}-{MM}-{DD} {WEEKDAY} {DATE} {RAWDATE}'
    const result = interpolateTemplate(raw, '20261108')
    expect(result).toBe('2026-11-08 星期日 2026-11-08 20261108')
  })

  it('returns empty string on empty input or none template id', () => {
    expect(interpolateTemplate('')).toBe('')
    expect(getJournalTemplateContent('none')).toBe('')
    expect(getJournalTemplateContent('')).toBe('')
    expect(getJournalTemplateContent('non-existent')).toBe('')
  })

  it('provides all standard presets with both markdown and text format', () => {
    const ids = JOURNAL_TEMPLATE_PRESETS.map((item) => item.id)
    expect(ids).toContain('standard')
    expect(ids).toContain('agile')
    expect(ids).toContain('project')
    expect(ids).toContain('minimal')

    for (const preset of JOURNAL_TEMPLATE_PRESETS) {
      const md = getJournalTemplateContent(preset.id, 'md', '', '2026-11-08')
      const txt = getJournalTemplateContent(preset.id, 'txt', '', '2026-11-08')
      expect(md.length).toBeGreaterThan(0)
      expect(txt.length).toBeGreaterThan(0)
      expect(md).not.toBe(txt)
    }
  })

  it('renders custom template with variable interpolation', () => {
    const custom = '### {date} 自定义工作项\n1. 推进项目'
    const rendered = getJournalTemplateContent('custom', 'md', custom, '2026-11-08')
    expect(rendered).toBe('### 2026-11-08 自定义工作项\n1. 推进项目')
  })
})

describe('JournalTemplateButton component', () => {
  const defaultSettings = {
    ...DEFAULT_EDITOR_SETTINGS,
    customTemplate: '### {date} 自定义模板\n- 重点任务',
  }

  afterEach(() => {
    document.body.innerHTML = ''
  })

  it('renders template trigger button and toggles menu popover', async () => {
    const wrapper = mount(JournalTemplateButton, {
      props: {
        disabled: false,
        hasContent: false,
        currentFormat: 'md',
        date: '2026-11-08',
        editorSettings: defaultSettings,
      },
      attachTo: document.body,
    })

    const button = wrapper.find('.journal-template-trigger')
    expect(button.exists()).toBe(true)
    expect(button.attributes('aria-label')).toContain('模板')

    expect(document.body.querySelector('.journal-template-popover')).toBeNull()

    await button.trigger('click')
    await nextTick()

    const popover = document.body.querySelector('.journal-template-popover')
    expect(popover).not.toBeNull()

    // Presets + custom template should appear
    const items = document.body.querySelectorAll('.template-card')
    expect(items.length).toBe(JOURNAL_TEMPLATE_PRESETS.length + 1)
    expect(popover?.textContent).toContain('标准工作日报')
    expect(popover?.textContent).toContain('自定义模板')

    wrapper.unmount()
  })

  it('applies template with replace mode directly when editor is empty', async () => {
    const wrapper = mount(JournalTemplateButton, {
      props: {
        disabled: false,
        hasContent: false,
        currentFormat: 'md',
        date: '2026-11-08',
        editorSettings: defaultSettings,
      },
      attachTo: document.body,
    })

    await wrapper.find('.journal-template-trigger').trigger('click')
    await nextTick()

    const firstCard = document.body.querySelector('.template-card')
    const applyBtn = firstCard?.querySelector('.template-action-btn.primary') as HTMLButtonElement | null
    expect(applyBtn).not.toBeNull()
    applyBtn?.click()
    await nextTick()

    expect(wrapper.emitted('apply')).toBeTruthy()
    const [content, mode] = wrapper.emitted('apply')![0] as [string, string]
    expect(content).toContain('今日工作产出')
    expect(mode).toBe('replace')

    wrapper.unmount()
  })

  it('provides append and replace options when editor already has content', async () => {
    const wrapper = mount(JournalTemplateButton, {
      props: {
        disabled: false,
        hasContent: true,
        currentFormat: 'md',
        date: '2026-11-08',
        editorSettings: defaultSettings,
      },
      attachTo: document.body,
    })

    await wrapper.find('.journal-template-trigger').trigger('click')
    await nextTick()

    const firstCard = document.body.querySelector('.template-card')
    const appendBtn = firstCard?.querySelector('.template-action-btn.secondary') as HTMLButtonElement | null
    const replaceBtn = firstCard?.querySelector('.template-action-btn.warning') as HTMLButtonElement | null
    expect(appendBtn).not.toBeNull()
    expect(replaceBtn).not.toBeNull()

    appendBtn?.click()
    await nextTick()
    expect(wrapper.emitted('apply')).toBeTruthy()
    expect(wrapper.emitted('apply')![0][1]).toBe('append')

    // Reopen and test replace
    await wrapper.find('.journal-template-trigger').trigger('click')
    await nextTick()
    const firstCardReopened = document.body.querySelector('.template-card')
    const replaceBtnReopened = firstCardReopened?.querySelector(
      '.template-action-btn.warning',
    ) as HTMLButtonElement | null
    replaceBtnReopened?.click()
    await nextTick()
    expect(wrapper.emitted('apply')![1][1]).toBe('replace')

    wrapper.unmount()
  })

  it('emits openSettings when clicking the settings button', async () => {
    const wrapper = mount(JournalTemplateButton, {
      props: {
        disabled: false,
        hasContent: false,
        currentFormat: 'md',
        date: '2026-11-08',
        editorSettings: defaultSettings,
      },
      attachTo: document.body,
    })

    await wrapper.find('.journal-template-trigger').trigger('click')
    await nextTick()

    const settingsBtn = document.body.querySelector('.template-settings-shortcut') as HTMLButtonElement | null
    expect(settingsBtn).not.toBeNull()
    settingsBtn?.click()
    await nextTick()

    expect(wrapper.emitted('openSettings')).toBeTruthy()

    wrapper.unmount()
  })
})

describe('useJournal template auto-apply and draft flush guard', () => {
  beforeEach(() => {
    vi.resetAllMocks()
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('auto-applies skeleton on empty date but does not save if unedited', async () => {
    vi.mocked(api.open).mockResolvedValue({ path: '/workspace', name: 'workspace' })
    vi.mocked(api.list).mockResolvedValue([])
    vi.mocked(api.previews).mockResolvedValue([])

    const journal = useJournal()
    journal.setDefaultFormat('md')
    journal.setEditorSettings({
      defaultTemplateId: 'standard',
      customTemplate: '',
      autoApplyTemplate: true,
    })

    journal.selectedDate.value = '20261108'
    await journal.open('/workspace')

    // Document is a new draft, populated with the standard template
    expect(journal.isNewDraft.value).toBe(true)
    expect(journal.draft.value).toContain('今日工作产出')

    // Flushing unedited auto-applied template should NOT trigger api.create
    const flushResult = await journal.flush()
    expect(flushResult).toBe(true)
    expect(api.create).not.toHaveBeenCalled()

    // Now edit the content
    const createdDoc: Document = {
      date: '20261108',
      filename: '20261108.md',
      format: 'md',
      content: journal.draft.value + '\n今日完成了模板系统',
      revision: 'r1',
    }
    vi.mocked(api.create).mockResolvedValue(createdDoc)
    journal.edit(journal.draft.value + '\n今日完成了模板系统')

    // Flushing edited template SHOULD trigger api.create
    const saved = await journal.flush()
    expect(saved).toBe(true)
    expect(api.create).toHaveBeenCalledOnce()
    expect(api.create).toHaveBeenCalledWith('20261108', 'md', expect.stringContaining('今日完成了模板系统'))
  })
})
