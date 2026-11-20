// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import ExcelSettings from '../src/components/ExcelSettings.vue'
import {
  RECOMMENDED_EXCEL_CONTENT_PROMPT,
  RECOMMENDED_EXCEL_PROMPT,
  type ExcelSettings as ExcelSettingsData,
} from '../src/lib/useExcel'

describe('Excel settings with workspace output rules', () => {
  it('fills the recommended rules only when requested and keeps the settings editable', async () => {
    const original = {
      enabled: true,
      templatePath: '/templates/report.xlsx',
      prompt: '原有提示词',
      contentPrompt: '原有整理规则',
    }
    const wrapper = mount(ExcelSettings, {
      props: { modelValue: original, busy: false, aiReady: true, workspacePath: '/workspace' },
    })
    expect(wrapper.get<HTMLTextAreaElement>('#excel-output-prompt').element.value).toBe(original.prompt)
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
    await wrapper.get('button[aria-label="使用推荐 Excel 填充提示词"]').trigger('click')
    const updated = wrapper.emitted('update:modelValue')![0]![0] as ExcelSettingsData
    expect(updated).toEqual({ ...original, prompt: RECOMMENDED_EXCEL_PROMPT })
    expect(original.prompt).toBe('原有提示词')
    await wrapper.setProps({ modelValue: updated })
    expect(wrapper.get<HTMLTextAreaElement>('#excel-output-prompt').element.value).toBe(RECOMMENDED_EXCEL_PROMPT)
    expect(wrapper.vm.validate()).toBe(true)
    await wrapper.get('#excel-output-prompt').setValue('调整后的规则')
    expect(wrapper.emitted('update:modelValue')!.at(-1)![0]).toEqual({ ...original, prompt: '调整后的规则' })
    wrapper.unmount()
  })
  it('keeps content generation and Excel filling rules independent and migrates old settings without saving', async () => {
    const original = { enabled: true, templatePath: '/templates/report.xlsx', prompt: 'F2 填写日期，A14 开始填明细' }
    const wrapper = mount(ExcelSettings, {
      props: { modelValue: original, busy: false, aiReady: true, workspacePath: '/workspace' },
    })
    expect(wrapper.get<HTMLTextAreaElement>('#excel-content-prompt').element.value).toBe(
      RECOMMENDED_EXCEL_CONTENT_PROMPT,
    )
    expect(wrapper.get<HTMLTextAreaElement>('#excel-output-prompt').element.value).toBe(original.prompt)
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
    await wrapper.get('#excel-content-prompt').setValue('按项目整理交付成果，不添加日报之外的事实')
    const changed = wrapper.emitted('update:modelValue')!.at(-1)![0] as ExcelSettingsData
    expect(changed.contentPrompt).toBe('按项目整理交付成果，不添加日报之外的事实')
    expect(changed.prompt).toBe(original.prompt)
    await wrapper.setProps({ modelValue: changed })
    await wrapper.get('button[aria-label="使用推荐内容生成提示词"]').trigger('click')
    const recommended = wrapper.emitted('update:modelValue')!.at(-1)![0] as ExcelSettingsData
    expect(recommended.contentPrompt).toBe(RECOMMENDED_EXCEL_CONTENT_PROMPT)
    expect(recommended.prompt).toBe(original.prompt)
    await wrapper.setProps({ modelValue: { ...recommended, contentPrompt: '' } })
    expect(wrapper.vm.validate()).toBe(true)
    await wrapper.setProps({ modelValue: { ...recommended, contentPrompt: '字'.repeat(6001) } })
    expect(wrapper.vm.validate()).toBe(false)
    await wrapper.vm.$nextTick()
    expect(wrapper.text()).toContain('内容生成提示词不能超过 6000 字')
    wrapper.unmount()
  })
  it('validates template and prompt without requesting an output directory', () => {
    const wrapper = mount(ExcelSettings, {
      props: {
        modelValue: { enabled: true, templatePath: '/templates/report.xlsx', prompt: '整理工作明细' },
        busy: false,
        aiReady: true,
        workspacePath: '/workspace',
      },
    })
    expect(wrapper.find('#excel-output-directory').exists()).toBe(false)
    expect(wrapper.text()).not.toContain('选择目录')
    expect(wrapper.text()).toContain('当前工作空间下的 excel 目录')
    expect(wrapper.vm.validate()).toBe(true)
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
    wrapper.unmount()
  })
  it('shows the draft directory and opens the shared directory settings', async () => {
    const wrapper = mount(ExcelSettings, {
      props: {
        modelValue: { enabled: false, templatePath: '', prompt: '' },
        busy: false,
        aiReady: true,
        workspacePath: '/workspace',
        outputDirectory: '输出/Excel',
      },
    })
    expect(wrapper.text()).toContain('当前工作空间下的 输出/Excel 目录')
    const button = wrapper.findAll('button').find((item) => item.text() === '设置目录和文件名')!
    await button.trigger('click')
    expect(wrapper.emitted('configureDirectories')).toHaveLength(1)
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
    wrapper.unmount()
  })
})
