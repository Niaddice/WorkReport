import { describe, expect, it } from 'vitest'
import { searchSettings, SETTING_SEARCH_ENTRIES } from '../src/lib/settingsSearch'

describe('settingsSearch', () => {
  it('returns all settings when query is empty', () => {
    const results = searchSettings('')
    expect(results).toHaveLength(SETTING_SEARCH_ENTRIES.length)
  })

  it('matches global settings by title and keywords', () => {
    const proxyResults = searchSettings('代理')
    expect(proxyResults.length).toBeGreaterThan(0)
    expect(proxyResults[0]!.entry.id).toBe('global-proxy')
    expect(proxyResults[0]!.entry.target).toEqual({ type: 'global', section: 'proxy' })

    const aiResults = searchSettings('deepseek')
    expect(aiResults.length).toBeGreaterThan(0)
    expect(aiResults[0]!.entry.id).toBe('global-ai')

    const fontResults = searchSettings('字体')
    expect(fontResults.length).toBeGreaterThan(0)
    expect(fontResults[0]!.entry.id).toBe('global-appearance')
  })

  it('matches module settings by title, category, and keywords', () => {
    const gitResults = searchSettings('本地仓库')
    expect(gitResults.length).toBeGreaterThan(0)
    expect(gitResults[0]!.entry.id).toBe('module-git-repositories')
    expect(gitResults[0]!.entry.target).toEqual({
      type: 'module',
      module: 'git',
      sectionId: 'git-repositories',
    })

    const excelResults = searchSettings('excel')
    expect(excelResults.length).toBeGreaterThan(0)
    expect(excelResults.some((r) => r.entry.id === 'module-excel-export')).toBe(true)

    const formatResults = searchSettings('默认格式')
    expect(formatResults.length).toBeGreaterThan(0)
    expect(formatResults[0]!.entry.id).toBe('module-editor-format')
  })

  it('matches general keyword "设置"', () => {
    const settingResults = searchSettings('设置')
    expect(settingResults.length).toBe(SETTING_SEARCH_ENTRIES.length)
  })
})
