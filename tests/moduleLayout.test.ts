import { beforeEach, describe, expect, it, vi } from 'vitest'

const { load, save } = vi.hoisted(() => ({ load: vi.fn(), save: vi.fn() }))
vi.mock('../src/lib/uiPreferences', () => ({ uiPreferencesApi: { load, save } }))

import { useModuleLayout } from '../src/modules/useModuleLayout'
import { layoutGeometry } from '../src/modules/layout'

function makeLayout() {
  return useModuleLayout({ workspacePath: () => '/tmp/workspace' })
}

beforeEach(() => {
  load.mockReset()
  save.mockReset()
  save.mockResolvedValue(undefined)
})

describe('useModuleLayout saved layouts', () => {
  it('persists pixel-aware resizing without changing non-adjacent modules', async () => {
    load.mockResolvedValue({
      layout: {
        type: 'split',
        direction: 'row',
        ratio: 0.3,
        first: { type: 'leaf', id: 'calendar' },
        second: {
          type: 'split',
          direction: 'row',
          ratio: 0.6,
          first: {
            type: 'split',
            direction: 'column',
            ratio: 0.5,
            first: { type: 'leaf', id: 'editor' },
            second: { type: 'leaf', id: 'history' },
          },
          second: { type: 'leaf', id: 'overview' },
        },
      },
      skin: 'default',
      savedLayouts: [],
    })
    const state = makeLayout()
    await state.load()
    const far = () => layoutGeometry(state.layout.value, 1000, 900, 3.2).slots.find((slot) => slot.id === 'overview')!
    const before = far()
    state.resize([], 0.6, { width: 1000, height: 900, gap: 3.2 })
    for (const key of ['left', 'top', 'width', 'height'] as const) expect(far()[key]).toBeCloseTo(before[key], 9)
    await vi.waitFor(() => expect(save).toHaveBeenCalledOnce())
    expect(save.mock.calls[0]![0].layout).toEqual(state.layout.value)
    expect(save.mock.calls[0]![1]).toBe('/tmp/workspace')
  })

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
