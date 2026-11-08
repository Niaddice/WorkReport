// @vitest-environment happy-dom
import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { SUMMARY_PRESETS, getSummaryPreset, matchSummaryPreset } from '../src/lib/summaryPresets'
import SummaryGenerationDialog from '../src/components/SummaryGenerationDialog.vue'

describe('summary presets library', () => {
  it('defines structured summary presets with valid IDs, labels and non-empty prompts', () => {
    expect(SUMMARY_PRESETS.length).toBeGreaterThanOrEqual(5)
    for (const preset of SUMMARY_PRESETS) {
      expect(preset.id).toBeTruthy()
      expect(preset.label).toBeTruthy()
      expect(preset.prompt.trim().length).toBeGreaterThan(10)
    }
  })

  it('matches presets by exact or trimmed prompt text', () => {
    const weekly = getSummaryPreset('weekly')
    expect(weekly).toBeDefined()
    expect(matchSummaryPreset(weekly!.prompt)?.id).toBe('weekly')
    expect(matchSummaryPreset(`  ${weekly!.prompt}  \n`)?.id).toBe('weekly')
    expect(matchSummaryPreset('这是一段完全自定义的提示词')).toBeUndefined()
    expect(matchSummaryPreset('')).toBeUndefined()
  })

  it('finds preset by ID', () => {
    expect(getSummaryPreset('monthly')?.label).toBe('月度复盘')
    expect(getSummaryPreset('non-existent')).toBeUndefined()
  })
})

describe('SummaryGenerationDialog presets integration', () => {
  const defaultProps = {
    visible: true,
    configuring: true,
    prompt: SUMMARY_PRESETS[0]!.prompt,
    provider: 'DeepSeek',
    dates: ['20261101', '20261102'],
    reportCount: 2,
    generating: false,
    elapsedSeconds: 0,
    result: '',
    error: '',
    blockedReason: '',
    exporting: false,
    outputDirectory: 'summary',
    outputError: '',
  }

  it('renders all preset pills and marks the matching preset as active', () => {
    const wrapper = mount(SummaryGenerationDialog, { props: defaultProps, attachTo: document.body })
    const pills = document.querySelectorAll('.summary-preset-pill')
    expect(pills.length).toBeGreaterThanOrEqual(SUMMARY_PRESETS.length)

    // The first preset (default) should be active
    const activePill = document.querySelector('.summary-preset-pill.active')
    expect(activePill).not.toBeNull()
    expect(activePill!.textContent?.trim()).toBe('综合归纳')
    wrapper.unmount()
  })

  it('emits update:prompt when a preset pill is clicked', async () => {
    const wrapper = mount(SummaryGenerationDialog, { props: defaultProps, attachTo: document.body })
    const pills = Array.from(document.querySelectorAll<HTMLButtonElement>('button.summary-preset-pill'))
    const weeklyBtn = pills.find((item) => item.textContent?.trim() === '项目周报')
    expect(weeklyBtn).toBeDefined()

    weeklyBtn!.click()
    const emitted = wrapper.emitted('update:prompt')
    expect(emitted).toBeDefined()
    expect(emitted![0]![0]).toBe(getSummaryPreset('weekly')!.prompt)
    wrapper.unmount()
  })

  it('displays the custom pill indicator when prompt does not match any preset', () => {
    const wrapper = mount(SummaryGenerationDialog, {
      props: { ...defaultProps, prompt: '自定义总结要求：只列出 Bug' },
      attachTo: document.body,
    })
    const customPill = document.querySelector('.custom-pill')
    expect(customPill).not.toBeNull()
    expect(customPill!.textContent?.trim()).toBe('自定义')
    wrapper.unmount()
  })
})
