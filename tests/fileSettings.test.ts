import { describe, expect, it } from 'vitest'
import {
  cloneFileSettings,
  fileDirectoryError,
  filenameTemplateError,
  fileSettingsError,
  formatFileName,
} from '../src/lib/fileSettings'

describe('workspace file rules', () => {
  it('formats literal text and date placeholders for journals and reports', () => {
    const context = { date: '20261101', startDate: '20261001', endDate: '20261031', time: '093045' }
    expect(formatFileName('日报_{YYYY}年{MM}月{DD}日', context, 'md')).toBe('日报_2026年11月01日.md')
    expect(formatFileName('总结_{startDate}-{endDate}_{date}_{HH}{mm}{ss}', context, 'txt')).toBe(
      '总结_20261001-20261031_20261101_093045.txt',
    )
  })
  it('preserves legacy summary directories and clones each feature independently', () => {
    const files = cloneFileSettings(undefined, '输出/总结')
    files.journal.directory = '日报'
    expect(files.summary.directory).toBe('输出/总结')
    expect(cloneFileSettings().journal.directory).toBe('')
  })
  it.each([
    '/tmp/outside',
    '../outside',
    'a/../b',
    'a//b',
    '.history',
    '2026',
    '2026/reports',
    'a\\b',
    'workreport.json',
    'CON',
  ])('rejects unsafe directory %s', (directory) => {
    expect(fileDirectoryError('excel', directory)).not.toBe('')
  })
  it('allows only journals to use root, and prevents directories from overlapping', () => {
    expect(fileDirectoryError('journal', '')).toBe('')
    expect(fileDirectoryError('summary', '')).not.toBe('')
    expect(fileDirectoryError('journal', '工作/日报')).toBe('')
    const files = cloneFileSettings()
    files.journal.directory = '输出'
    files.summary.directory = '输出/总结'
    expect(fileSettingsError(files)?.message).toContain('互相包含')
  })
  it.each(['日报_{YYYY}', '{startDate}', '{unknown}', '../{date}', '{date}.md', '{date'])(
    'rejects invalid journal template %s',
    (template) => {
      expect(filenameTemplateError('journal', template)).not.toBe('')
    },
  )
  it('allows a fixed report name and requires a full journal date', () => {
    expect(filenameTemplateError('summary', '项目总结')).toBe('')
    expect(filenameTemplateError('journal', '日报_{YYYY}-{MM}-{DD}')).toBe('')
  })
})
