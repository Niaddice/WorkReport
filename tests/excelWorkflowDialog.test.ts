// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from 'vitest'
import { mount, type VueWrapper } from '@vue/test-utils'
import ExcelPreviewDialog from '../src/components/ExcelPreviewDialog.vue'
import type { ExcelPreview } from '../src/lib/useExcel'

const wrappers: VueWrapper[] = []
afterEach(() => wrappers.splice(0).forEach((wrapper) => wrapper.unmount()))

function dialog(overrides: Record<string, unknown> = {}) {
  const wrapper = mount(ExcelPreviewDialog, {
    props: {
      inline: true,
      visible: true,
      preview: null,
      configuring: true,
      contentPrompt: '按项目整理成果',
      prompt: '从 A14 开始填写确认的内容',
      content: '',
      exporting: false,
      error: '',
      provider: '本机 Codex',
      templateName: '工作周报.xlsx',
      ...overrides,
    },
  })
  wrappers.push(wrapper)
  return wrapper
}

describe('Excel two-step generation dialog', () => {
  it('generates readable content first and never fills the workbook automatically', async () => {
    const wrapper = dialog()
    expect(wrapper.findAll('.excel-workflow-steps li').map((item) => item.text())).toEqual(['1生成内容', '2填充 Excel'])
    expect(wrapper.find('#excel-session-content-prompt').exists()).toBe(true)
    expect(wrapper.find('#excel-session-prompt').exists()).toBe(false)
    await wrapper.get('#excel-session-content-prompt').setValue('突出已完成事项和风险')
    expect(wrapper.emitted('update:contentPrompt')).toEqual([['突出已完成事项和风险']])
    await wrapper.get('.excel-start-generation').trigger('click')
    expect(wrapper.emitted('generate')).toHaveLength(1)
    expect(wrapper.emitted('fill')).toBeUndefined()
    await wrapper.setProps({ configuring: false, reviewingContent: true, content: '项目 A：完成接口联调。' })
    expect(wrapper.get<HTMLTextAreaElement>('#excel-confirmed-content').element.value).toBe('项目 A：完成接口联调。')
    expect(wrapper.emitted('fill')).toBeUndefined()
    await wrapper.get('#excel-confirmed-content').setValue('项目 A：完成接口联调并通过验收。')
    expect(wrapper.emitted('update:content')).toEqual([['项目 A：完成接口联调并通过验收。']])
    await wrapper.get('#excel-session-prompt').setValue('将确认内容写入 B5，保留原样式')
    expect(wrapper.emitted('update:prompt')).toEqual([['将确认内容写入 B5，保留原样式']])
    await wrapper.get('.excel-confirm-content').trigger('click')
    expect(wrapper.emitted('fill')).toHaveLength(1)
    expect(wrapper.emitted('generate')).toHaveLength(1)
  })

  it('requires nonempty confirmed content and keeps failed filling editable in one place', async () => {
    const wrapper = dialog({ configuring: false, reviewingContent: true, content: '  ' })
    expect(wrapper.get('.excel-confirm-content').attributes('disabled')).toBeDefined()
    expect(wrapper.get('[role="alert"]').text()).toContain('请保留需要填入 Excel 的报告内容')
    await wrapper.setProps({ content: '项目 A：完成验收。', error: '模板填充失败，请重试' })
    expect(wrapper.findAll('[role="alert"]')).toHaveLength(1)
    expect(wrapper.get('[role="alert"]').text()).toBe('模板填充失败，请重试')
    expect(wrapper.find('.excel-generation-progress').exists()).toBe(false)
    expect(wrapper.get<HTMLTextAreaElement>('#excel-confirmed-content').element.value).toBe('项目 A：完成验收。')
    expect(wrapper.get('.excel-confirm-content').attributes('disabled')).not.toBe('true')
    await wrapper.get('.excel-confirm-content').trigger('click')
    expect(wrapper.emitted('fill')).toHaveLength(1)
    expect(wrapper.emitted('generate')).toBeUndefined()
    await wrapper.get('.excel-edit-content').trigger('click')
    expect(wrapper.emitted('edit-content')).toHaveLength(1)
  })

  it('separates AI progress for content and filling and returns previews to content review', async () => {
    const wrapper = dialog({ configuring: false, preparing: true, generatingContent: true })
    expect(wrapper.text()).toContain('AI 正在生成报告内容')
    expect(wrapper.text()).toContain('不会自动填充 Excel')
    await wrapper.setProps({ generatingContent: false })
    expect(wrapper.text()).toContain('AI 正在填充 Excel 模板')
    const preview: ExcelPreview = {
      id: 'report-1',
      templateName: '周报.xlsx',
      outputDirectory: '/workspace/excel',
      start: '20261101',
      end: '20261105',
      cells: [],
      styles: [],
      sheets: [],
    }
    await wrapper.setProps({ preparing: false, preview })
    expect(wrapper.text()).toContain('确认输出')
    await wrapper.get('.excel-preview-actions .text-action').trigger('click')
    expect(wrapper.emitted('regenerate')).toHaveLength(1)
    expect(wrapper.emitted('generate')).toBeUndefined()
    expect(wrapper.emitted('fill')).toBeUndefined()
  })
})
