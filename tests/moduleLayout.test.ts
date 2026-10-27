import { beforeEach, describe, expect, it, vi } from 'vitest'

const { load, save } = vi.hoisted(() => ({ load: vi.fn(), save: vi.fn() }))
vi.mock('../src/lib/uiPreferences', () => ({ uiPreferencesApi: { load, save } }))

import { useModuleLayout } from '../src/modules/useModuleLayout'

function makeLayout() {
  return useModuleLayout({ workspacePath: () => '/tmp/workspace' })
}

beforeEach(() => {
  load.mockReset()
  save.mockReset()
  save.mockResolvedValue(undefined)
})

describe('useModuleLayout saved layouts', () => {
  it('saves, applies and removes a named layout', () => {
    const layout = makeLayout()
    layout.add('overview')
    expect(layout.items.value.map((item) => item.id)).toEqual(['calendar', 'editor', 'overview'])

    expect(layout.saveLayout('工作布局')).toBe(true)
    expect(layout.savedLayouts.value.map((item) => item.name)).toEqual(['工作布局'])

    // 继续改动当前布局，再应用保存的快照应还原到保存时的样子。
    layout.add('history')
    expect(layout.items.value.map((item) => item.id)).toEqual(['calendar', 'editor', 'overview', 'history'])

    expect(layout.applySavedLayout('工作布局')).toBe(true)
    expect(layout.items.value.map((item) => item.id)).toEqual(['calendar', 'editor', 'overview'])

    expect(layout.removeSavedLayout('工作布局')).toBe(true)
    expect(layout.savedLayouts.value).toEqual([])
  })

  it('overwrites the same name, ignores blank names and missing snapshots', () => {
    const layout = makeLayout()
    expect(layout.saveLayout('   ')).toBe(false)

    layout.add('overview')
    expect(layout.saveLayout('日常')).toBe(true)
    layout.add('history')
    expect(layout.saveLayout('日常')).toBe(true)

    expect(layout.savedLayouts.value).toHaveLength(1)
    expect(layout.savedLayouts.value[0]!.name).toBe('日常')
    expect(layout.items.value.map((item) => item.id)).toEqual(['calendar', 'editor', 'overview', 'history'])

    expect(layout.applySavedLayout('不存在')).toBe(false)
    expect(layout.removeSavedLayout('不存在')).toBe(false)
  })
})
