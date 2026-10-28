// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import ExcelSettings from '../src/components/ExcelSettings.vue'
import { RECOMMENDED_EXCEL_PROMPT, type ExcelSettings as ExcelSettingsData } from '../src/lib/useExcel'

describe('Excel settings with a fixed workspace output directory', () => {
  it('fills the recommended rules only when requested and keeps the settings editable', async () => {
    const original = { enabled: true, templatePath: '/templates/report.xlsx', prompt: '原有提示词' }
    const wrapper = mount(ExcelSettings, {
      props: { modelValue: original, busy: false, aiReady: true, workspacePath: '/workspace' },
    })
    expect(wrapper.get<HTMLTextAreaElement>('#excel-output-prompt').element.value).toBe(original.prompt)
    expect(wrapper.emitted('update:modelValue')).toBeUndefined()
    await wrapper.get('.excel-prompt-heading button').trigger('click')
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
})
