import { describe, expect, it } from 'vitest'
import {
  DEFAULT_UI_PREFERENCES,
  appendLayoutModule,
  clampSplitRatio,
  cloneUiPreferences,
  dropPreviewRect,
  dropSideForPoint,
  findDropSlot,
  flattenLayout,
  insertLayoutModuleAtGutter,
  layoutGeometry,
  layoutIds,
  moveLayoutModuleToEdge,
  normalizeUiPreferences,
  prependLayoutModule,
  removeLayoutModule,
  resizeLayoutLocally,
  setSplitRatio,
  splitLayout,
} from '../src/modules/layout'
import type { LayoutLeaf, LayoutNode, LayoutSlot, LayoutSplit, ModuleId, SplitDirection } from '../src/modules/types'

function leaf(id: ModuleId): LayoutLeaf {
  return { type: 'leaf', id }
}

function split(direction: SplitDirection, ratio: number, first: LayoutNode, second: LayoutNode): LayoutSplit {
  return { type: 'split', direction, ratio, first, second }
}

/** 故意写坏 id 的叶子，用来校验归一化时的丢弃逻辑。 */
function rawLeaf(id: string): LayoutNode {
  return { type: 'leaf', id } as unknown as LayoutNode
}

function ids(layout: LayoutNode): string[] {
  return layoutIds(layout)
}

function snapshot(value: unknown): string {
  return JSON.stringify(value)
}

describe('default preferences and cloning', () => {
  it('defaults to a row split with calendar on the left and editor on the right', () => {
    expect(DEFAULT_UI_PREFERENCES).toEqual({
      layout: split('row', 0.5, leaf('calendar'), leaf('editor')),
      skin: 'default',
      savedLayouts: [],
    })
  })

  it('deep-clones preferences so nested edits do not leak', () => {
    const clone = cloneUiPreferences(DEFAULT_UI_PREFERENCES)
    expect(clone).toEqual(DEFAULT_UI_PREFERENCES)
    expect(clone.layout).not.toBe(DEFAULT_UI_PREFERENCES.layout)
    const layout = clone.layout as LayoutSplit
    expect(layout.first).not.toBe((DEFAULT_UI_PREFERENCES.layout as LayoutSplit).first)
    ;(layout.first as LayoutLeaf).id = 'overview'
    layout.ratio = 0.7
    expect(DEFAULT_UI_PREFERENCES.layout).toEqual(split('row', 0.5, leaf('calendar'), leaf('editor')))
  })
})

describe('normalizeUiPreferences', () => {
  it('falls back to the default layout for missing or damaged values', () => {
    expect(normalizeUiPreferences(undefined)).toEqual(DEFAULT_UI_PREFERENCES)
    expect(normalizeUiPreferences(null)).toEqual(DEFAULT_UI_PREFERENCES)
    expect(normalizeUiPreferences('broken')).toEqual(DEFAULT_UI_PREFERENCES)
    expect(normalizeUiPreferences([])).toEqual(DEFAULT_UI_PREFERENCES)
    expect(normalizeUiPreferences({ layout: 'broken', skin: 'nope' })).toEqual(DEFAULT_UI_PREFERENCES)
  })

  it('keeps a valid nested layout and deep-clones it', () => {
    const layout = split('column', 0.3, split('row', 0.7, leaf('calendar'), leaf('overview')), leaf('editor'))
    const before = snapshot(layout)
    const normalized = normalizeUiPreferences({ layout, skin: 'default' })
    expect(normalized).toEqual({ layout, skin: 'default', savedLayouts: [] })
    expect(normalized.layout).not.toBe(layout)
    expect(snapshot(layout)).toBe(before)
  })

  it('clamps split ratios and defaults missing direction/ratio', () => {
    const clamped = normalizeUiPreferences({
      layout: split('row', 0.02, leaf('calendar'), split('column', 9, leaf('editor'), leaf('overview'))),
    })
    expect(clamped.layout).toEqual(
      split('row', 0.02, leaf('calendar'), split('column', 0.99, leaf('editor'), leaf('overview'))),
    )

    const fallback = normalizeUiPreferences({
      layout: {
        type: 'split',
        direction: 'diagonal',
        first: leaf('calendar'),
        second: leaf('editor'),
      } as unknown as LayoutNode,
    })
    expect(fallback.layout).toEqual(split('row', 0.5, leaf('calendar'), leaf('editor')))
  })

  it('drops invalid subtrees and collapses the remaining split', () => {
    const normalized = normalizeUiPreferences({
      layout: split('row', 0.5, split('column', 0.5, rawLeaf('bogus'), leaf('editor')), leaf('calendar')),
    })
    expect(normalized.layout).toEqual(split('row', 0.5, leaf('editor'), leaf('calendar')))
  })

  it('deduplicates repeated modules without re-adding other modules', () => {
    const layout = split('row', 0.5, leaf('calendar'), split('column', 0.5, rawLeaf('calendar'), leaf('overview')))
    const normalized = normalizeUiPreferences({ layout })
    // 重复的 calendar 被丢弃、父 split 折叠；不再自动补回核心模块。
    expect(ids(normalized.layout)).toEqual(['calendar', 'overview'])
    expect(normalized.layout).toEqual(split('row', 0.5, leaf('calendar'), leaf('overview')))
  })

  it('validates skins and prefers a layout over legacy fields', () => {
    const normalized = normalizeUiPreferences({
      layout: leaf('overview'),
      columns: 1,
      modules: [{ id: 'history' }],
      skin: 'neon',
    })
    expect(normalized.skin).toBe('default')
    expect(ids(normalized.layout)).toEqual(['overview'])
  })

  it('keeps valid saved layouts and filters damaged, empty or duplicate names', () => {
    const normalized = normalizeUiPreferences({
      layout: leaf('calendar'),
      skin: 'default',
      savedLayouts: [
        { name: '工作布局', layout: leaf('overview') },
        { name: '  每日  ', layout: split('row', 0.5, leaf('calendar'), leaf('editor')) },
        { name: '空布局', layout: null },
        { name: '重复', layout: leaf('history') },
        { name: '重复', layout: leaf('summary') },
        { name: '   ', layout: leaf('excel') },
        { name: '非法布局', layout: rawLeaf('bogus') },
      ],
    })
    expect(normalized.savedLayouts).toEqual([
      { name: '工作布局', layout: leaf('overview') },
      { name: '每日', layout: split('row', 0.5, leaf('calendar'), leaf('editor')) },
      { name: '重复', layout: leaf('history') },
    ])
  })
})

describe('legacy layout migration', () => {
  it('pairs modules into rows for two columns and stacks the rows vertically', () => {
    const normalized = normalizeUiPreferences({
      columns: 2,
      modules: [{ id: 'calendar' }, { id: 'editor' }, { id: 'overview' }],
      skin: 'default',
    })
    expect(normalized).toEqual({
      layout: split('column', 0.5, split('row', 0.5, leaf('calendar'), leaf('editor')), leaf('overview')),
      skin: 'default',
      savedLayouts: [],
    })
  })

  it('stacks every module vertically in a single column', () => {
    const normalized = normalizeUiPreferences({
      columns: 1,
      modules: [{ id: 'calendar' }, { id: 'editor' }, { id: 'overview' }],
      skin: 'default',
    })
    expect(normalized.layout).toEqual(
      split('column', 1 / 3, leaf('calendar'), split('column', 0.5, leaf('editor'), leaf('overview'))),
    )
  })

  it('migrates the older axis/order schema', () => {
    expect(normalizeUiPreferences({ axis: 'row', order: ['editor', 'calendar'], skin: 'default' })).toEqual({
      layout: split('row', 0.5, leaf('editor'), leaf('calendar')),
      skin: 'default',
      savedLayouts: [],
    })
    expect(normalizeUiPreferences({ axis: 'column', order: ['editor', 'calendar'], skin: 'default' })).toEqual({
      layout: split('column', 0.5, leaf('editor'), leaf('calendar')),
      skin: 'default',
      savedLayouts: [],
    })
  })

  it('keeps a single migrated module without adding other modules', () => {
    expect(normalizeUiPreferences({ axis: 'column', order: ['calendar'], skin: 'default' }).layout).toEqual(
      leaf('calendar'),
    )
    const normalized = normalizeUiPreferences({ columns: 1, modules: [{ id: 'history', span: 'full' }] })
    expect(ids(normalized.layout)).toEqual(['history'])
  })

  it('keeps a valid layout with only one module', () => {
    expect(normalizeUiPreferences({ layout: leaf('overview'), skin: 'default' }).layout).toEqual(leaf('overview'))
  })

  it('drops duplicates, unknown ids and damaged module lists', () => {
    const normalized = normalizeUiPreferences({
      columns: 2,
      modules: [{ id: 'calendar' }, { id: 'calendar' }, { id: 'bogus' }, { id: 'editor', span: null }],
      skin: 'default',
    })
    expect(normalized.layout).toEqual(split('row', 0.5, leaf('calendar'), leaf('editor')))
    expect(normalizeUiPreferences({ columns: 9, modules: 'broken', skin: 'neon' })).toEqual(DEFAULT_UI_PREFERENCES)
  })

  it('keeps each module at most once and respects MAX_MODULES', () => {
    const modules = ['calendar', 'editor', 'overview', 'history', 'summary', 'excel', 'calendar', 'editor'].map(
      (id) => ({ id }),
    )
    expect(ids(normalizeUiPreferences({ columns: 1, modules }).layout)).toHaveLength(6)
  })

  it('removes the old embedded preview from saved layouts while preserving the editor', () => {
    expect(
      normalizeUiPreferences({
        layout: {
          type: 'split',
          direction: 'row',
          ratio: 0.5,
          first: leaf('editor'),
          second: { type: 'leaf', id: 'markdown-preview' },
        },
        skin: 'default',
      }).layout,
    ).toEqual(leaf('editor'))
    expect(normalizeUiPreferences({ layout: { type: 'leaf', id: 'markdown-preview' } })).toEqual(DEFAULT_UI_PREFERENCES)
  })
})

describe('flattenLayout and layoutIds', () => {
  it('returns modules in visual order for nested splits', () => {
    const layout = split('row', 0.5, split('column', 0.3, leaf('calendar'), leaf('overview')), leaf('editor'))
    expect(flattenLayout(layout)).toEqual([{ id: 'calendar' }, { id: 'overview' }, { id: 'editor' }])
    expect(layoutIds(layout)).toEqual(['calendar', 'overview', 'editor'])
  })
})

describe('removeLayoutModule', () => {
  it('collapses the parent split when a leaf is removed', () => {
    const layout = split('row', 0.5, split('column', 0.4, leaf('calendar'), leaf('overview')), leaf('editor'))
    const before = snapshot(layout)
    expect(removeLayoutModule(layout, 'overview')).toEqual(split('row', 0.5, leaf('calendar'), leaf('editor')))
    expect(snapshot(layout)).toBe(before)
  })

  it('replaces a split with its sibling subtree when a whole child is removed', () => {
    const layout = split('column', 0.3, split('row', 0.5, leaf('calendar'), leaf('overview')), leaf('editor'))
    expect(removeLayoutModule(layout, 'calendar')).toEqual(split('column', 0.3, leaf('overview'), leaf('editor')))
    expect(removeLayoutModule(layout, 'editor')).toEqual(split('row', 0.5, leaf('calendar'), leaf('overview')))
  })

  it('returns null for the only leaf and keeps the tree when the module is missing', () => {
    expect(removeLayoutModule(leaf('calendar'), 'calendar')).toBeNull()
    const layout = split('row', 0.5, leaf('calendar'), leaf('editor'))
    expect(removeLayoutModule(layout, 'history')).toBe(layout)
    expect(removeLayoutModule(layout, 'editor')).toEqual(leaf('calendar'))
  })
})

describe('appendLayoutModule', () => {
  it('wraps the root in a row split and keeps the new module rightmost', () => {
    expect(appendLayoutModule(leaf('calendar'), 'editor')).toEqual(split('row', 0.5, leaf('calendar'), leaf('editor')))

    const base = split('row', 0.5, leaf('calendar'), leaf('editor'))
    const before = snapshot(base)
    const appended = appendLayoutModule(base, 'overview')
    expect(appended).toEqual(split('row', 2 / 3, base, leaf('overview')))
    expect(ids(appended)).toEqual(['calendar', 'editor', 'overview'])
    expect(snapshot(base)).toBe(before)

    const nested = split('column', 0.5, split('row', 0.5, leaf('calendar'), leaf('editor')), leaf('overview'))
    expect(appendLayoutModule(nested, 'history')).toEqual(split('row', 3 / 4, nested, leaf('history')))
  })

  it('returns a single leaf for null and ignores duplicates', () => {
    expect(appendLayoutModule(null, 'history')).toEqual(leaf('history'))
    const layout = leaf('calendar')
    expect(appendLayoutModule(layout, 'calendar')).toBe(layout)
  })
})

describe('prependLayoutModule', () => {
  it('wraps the root in a row split and keeps the new module leftmost', () => {
    expect(prependLayoutModule(leaf('calendar'), 'editor')).toEqual(split('row', 0.5, leaf('editor'), leaf('calendar')))

    const base = split('row', 0.5, leaf('calendar'), leaf('editor'))
    expect(prependLayoutModule(base, 'overview')).toEqual(split('row', 1 / 3, leaf('overview'), base))
    expect(ids(prependLayoutModule(base, 'overview'))).toEqual(['overview', 'calendar', 'editor'])

    const nested = split('column', 0.5, split('row', 0.5, leaf('calendar'), leaf('editor')), leaf('overview'))
    expect(prependLayoutModule(nested, 'history')).toEqual(split('row', 1 / 4, leaf('history'), nested))
  })

  it('returns a single leaf for null and ignores duplicates', () => {
    expect(prependLayoutModule(null, 'history')).toEqual(leaf('history'))
    const layout = leaf('calendar')
    expect(prependLayoutModule(layout, 'calendar')).toBe(layout)
  })
})

describe('moveLayoutModuleToEdge', () => {
  const base = split('row', 0.5, leaf('calendar'), split('row', 0.5, leaf('editor'), leaf('overview')))

  it('moves a module to the far left column', () => {
    const moved = moveLayoutModuleToEdge(base, 'overview', 'left')
    expect(ids(moved)).toEqual(['overview', 'calendar', 'editor'])
  })

  it('moves a module to the far right column', () => {
    const moved = moveLayoutModuleToEdge(base, 'calendar', 'right')
    expect(ids(moved)).toEqual(['editor', 'overview', 'calendar'])
  })

  it('returns the original tree when the module is absent or already at the edge', () => {
    expect(moveLayoutModuleToEdge(base, 'history', 'left')).toBe(base)
  })
})

describe('splitLayout', () => {
  const base = split('row', 0.5, leaf('calendar'), leaf('editor'))

  it('moves the dragged module to each side of the target', () => {
    expect(splitLayout(base, 'editor', 'calendar', 'left')).toEqual(split('row', 0.5, leaf('editor'), leaf('calendar')))
    expect(splitLayout(base, 'editor', 'calendar', 'right')).toEqual(
      split('row', 0.5, leaf('calendar'), leaf('editor')),
    )
    expect(splitLayout(base, 'editor', 'calendar', 'top')).toEqual(
      split('column', 0.5, leaf('editor'), leaf('calendar')),
    )
    expect(splitLayout(base, 'editor', 'calendar', 'bottom')).toEqual(
      split('column', 0.5, leaf('calendar'), leaf('editor')),
    )
  })

  it('replaces a nested target leaf with a new split', () => {
    const layout = split('column', 0.5, split('row', 0.5, leaf('calendar'), leaf('editor')), leaf('overview'))
    const before = snapshot(layout)
    const result = splitLayout(layout, 'overview', 'editor', 'bottom')
    expect(result).toEqual(split('row', 0.5, leaf('calendar'), split('column', 0.5, leaf('editor'), leaf('overview'))))
    expect(ids(result)).toEqual(['calendar', 'editor', 'overview'])
    expect(snapshot(layout)).toBe(before)
  })

  it('re-inserts a dragged module that was removed from a nested split', () => {
    const layout = split('column', 0.5, split('row', 0.5, leaf('calendar'), leaf('editor')), leaf('overview'))
    const result = splitLayout(layout, 'calendar', 'overview', 'right')
    expect(result).toEqual(split('column', 0.5, leaf('editor'), split('row', 0.5, leaf('overview'), leaf('calendar'))))
    expect(ids(result)).toEqual(['editor', 'overview', 'calendar'])
  })

  it('ignores invalid drags and does not mutate the input', () => {
    const before = snapshot(base)
    expect(splitLayout(base, 'calendar', 'calendar', 'left')).toBe(base)
    expect(splitLayout(base, 'calendar', 'overview', 'left')).toBe(base)
    expect(snapshot(base)).toBe(before)
  })
})

describe('setSplitRatio', () => {
  const layout = split('row', 0.5, split('column', 0.4, leaf('calendar'), leaf('editor')), leaf('overview'))

  it('updates the split at the given 0/1 path', () => {
    const before = snapshot(layout)
    expect(setSplitRatio(layout, [], 0.7)).toEqual(
      split('row', 0.7, split('column', 0.4, leaf('calendar'), leaf('editor')), leaf('overview')),
    )
    expect(setSplitRatio(layout, [0], 0.65)).toEqual(
      split('row', 0.5, split('column', 0.65, leaf('calendar'), leaf('editor')), leaf('overview')),
    )
    expect(snapshot(layout)).toBe(before)
  })

  it('clamps ratios and ignores invalid paths', () => {
    expect((setSplitRatio(layout, [], 0.005) as LayoutSplit).ratio).toBe(0.01)
    expect((setSplitRatio(layout, [], 9) as LayoutSplit).ratio).toBe(0.99)
    expect(setSplitRatio(layout, [0, 0], 0.6)).toBe(layout)
    expect(setSplitRatio(layout, [2], 0.6)).toBe(layout)
    expect(setSplitRatio(layout, [], 0.5)).toBe(layout)
    expect(setSplitRatio(leaf('calendar'), [], 0.6)).toEqual(leaf('calendar'))
  })
})

describe('layoutGeometry', () => {
  it('splits a row layout into left/right slots with a gutter over the gap', () => {
    const geometry = layoutGeometry(split('row', 0.5, leaf('calendar'), leaf('editor')), 1000, 400, 10)
    expect(geometry.slots).toEqual([
      { id: 'calendar', left: 0, top: 0, width: 495, height: 400 },
      { id: 'editor', left: 505, top: 0, width: 495, height: 400 },
    ])
    expect(geometry.gutters).toEqual([
      {
        left: 495,
        top: 0,
        width: 10,
        height: 400,
        path: [],
        direction: 'row',
        ratio: 0.5,
        parent: { left: 0, top: 0, width: 1000, height: 400 },
      },
    ])
  })

  it('splits a column layout from top to bottom', () => {
    const geometry = layoutGeometry(split('column', 0.3, leaf('calendar'), leaf('editor')), 400, 1000, 20)
    expect(geometry.slots).toEqual([
      { id: 'calendar', left: 0, top: 0, width: 400, height: 294 },
      { id: 'editor', left: 0, top: 314, width: 400, height: 686 },
    ])
    expect(geometry.gutters).toEqual([
      {
        left: 0,
        top: 294,
        width: 400,
        height: 20,
        path: [],
        direction: 'column',
        ratio: 0.3,
        parent: { left: 0, top: 0, width: 400, height: 1000 },
      },
    ])
  })

  it('recursively cuts nested splits and records 0/1 paths', () => {
    const layout = split('row', 0.5, split('column', 0.4, leaf('calendar'), leaf('overview')), leaf('editor'))
    const geometry = layoutGeometry(layout, 1000, 500, 10)
    expect(geometry.slots).toEqual([
      { id: 'calendar', left: 0, top: 0, width: 495, height: 196 },
      { id: 'overview', left: 0, top: 206, width: 495, height: 294 },
      { id: 'editor', left: 505, top: 0, width: 495, height: 500 },
    ])
    expect(geometry.gutters).toEqual([
      {
        left: 495,
        top: 0,
        width: 10,
        height: 500,
        path: [],
        direction: 'row',
        ratio: 0.5,
        parent: { left: 0, top: 0, width: 1000, height: 500 },
      },
      {
        left: 0,
        top: 196,
        width: 495,
        height: 10,
        path: [0],
        direction: 'column',
        ratio: 0.4,
        parent: { left: 0, top: 0, width: 495, height: 500 },
      },
    ])
  })

  it('applies ratio overrides by path for live resizing', () => {
    const layout = split('row', 0.5, split('column', 0.4, leaf('calendar'), leaf('overview')), leaf('editor'))
    const overrides = new Map([
      ['', 0.25],
      ['0', 0.75],
    ])
    const geometry = layoutGeometry(layout, 1000, 500, 10, overrides)
    expect(geometry.slots).toEqual([
      { id: 'calendar', left: 0, top: 0, width: 247.5, height: 367.5 },
      { id: 'overview', left: 0, top: 377.5, width: 247.5, height: 122.5 },
      { id: 'editor', left: 257.5, top: 0, width: 742.5, height: 500 },
    ])
    expect(geometry.gutters[0]!.ratio).toBe(0.25)
    expect(geometry.gutters[0]!.left).toBe(247.5)
    expect(geometry.gutters[1]!.ratio).toBe(0.75)
    expect(geometry.gutters[1]!.top).toBe(367.5)
  })

  it('returns empty geometry for non-positive extents and invalid ratios', () => {
    const layout = split('row', 0.5, leaf('calendar'), leaf('editor'))
    expect(layoutGeometry(layout, 0, 400, 10)).toEqual({ slots: [], gutters: [] })
    expect(layoutGeometry(layout, 1000, -1, 10)).toEqual({ slots: [], gutters: [] })
    expect(layoutGeometry(layout, 1000, 400, -4)).toEqual({ slots: [], gutters: [] })
    expect(layoutGeometry(layout, 10, 400, 10)).toEqual({ slots: [], gutters: [] })
    expect(layoutGeometry(split('row', 0, leaf('calendar'), leaf('editor')), 1000, 400, 10)).toEqual({
      slots: [],
      gutters: [],
    })
    expect(layoutGeometry(split('column', -0.2, leaf('calendar'), leaf('editor')), 1000, 400, 10)).toEqual({
      slots: [],
      gutters: [],
    })
  })

  it('handles a leaf root and a zero gap', () => {
    expect(layoutGeometry(leaf('calendar'), 120, 60, 10)).toEqual({
      slots: [{ id: 'calendar', left: 0, top: 0, width: 120, height: 60 }],
      gutters: [],
    })
    const geometry = layoutGeometry(split('row', 0.5, leaf('calendar'), leaf('editor')), 100, 80, 0)
    expect(geometry.slots.map((slot) => slot.width)).toEqual([50, 50])
    expect(geometry.gutters).toHaveLength(1)
    expect(geometry.gutters[0]!.width).toBe(0)
  })
})

describe('drop targeting helpers', () => {
  const slots: LayoutSlot[] = [
    { id: 'calendar', left: 0, top: 0, width: 495, height: 400 },
    { id: 'editor', left: 505, top: 0, width: 495, height: 400 },
  ]

  it('finds the first slot containing the point and skips the dragged module', () => {
    expect(findDropSlot(slots, 100, 100)?.id).toBe('calendar')
    expect(findDropSlot(slots, 900, 300)?.id).toBe('editor')
    expect(findDropSlot(slots, 100, 100, 'calendar')).toBeNull()
    expect(findDropSlot(slots, 900, 300, 'calendar')?.id).toBe('editor')
    expect(findDropSlot(slots, 500, 100)).toBeNull()
    expect(findDropSlot(slots, -10, 100)).toBeNull()
    expect(findDropSlot([], 10, 10)).toBeNull()
  })

  it('maps pointer offsets to a stable drop side', () => {
    const slot: LayoutSlot = { id: 'calendar', left: 0, top: 0, width: 100, height: 50 }
    expect(dropSideForPoint(slot, 10, 25)).toBe('left')
    expect(dropSideForPoint(slot, 90, 25)).toBe('right')
    expect(dropSideForPoint(slot, 50, 5)).toBe('top')
    expect(dropSideForPoint(slot, 50, 45)).toBe('bottom')
    // 中心点仍稳定取 right；斜向落点按较大的归一化偏移判断。
    expect(dropSideForPoint(slot, 50, 25)).toBe('right')
    expect(dropSideForPoint(slot, 90, 45)).toBe('right')
    expect(dropSideForPoint(slot, 10, 5)).toBe('left')
  })

  it('returns the matching half rect for each side', () => {
    const slot: LayoutSlot = { id: 'calendar', left: 10, top: 20, width: 100, height: 60 }
    expect(dropPreviewRect(slot, 'left')).toEqual({ left: 10, top: 20, width: 50, height: 60 })
    expect(dropPreviewRect(slot, 'right')).toEqual({ left: 60, top: 20, width: 50, height: 60 })
    expect(dropPreviewRect(slot, 'top')).toEqual({ left: 10, top: 20, width: 100, height: 30 })
    expect(dropPreviewRect(slot, 'bottom')).toEqual({ left: 10, top: 50, width: 100, height: 30 })
  })
})

describe('clampSplitRatio', () => {
  it('keeps ratios inside the safe range', () => {
    expect(clampSplitRatio(0.5)).toBe(0.5)
    expect(clampSplitRatio(0.01)).toBe(0.01)
    expect(clampSplitRatio(0.12)).toBe(0.12)
    expect(clampSplitRatio(0.99)).toBe(0.99)
    expect(clampSplitRatio(0)).toBe(0.01)
    expect(clampSplitRatio(-3)).toBe(0.01)
    expect(clampSplitRatio(1)).toBe(0.99)
    expect(clampSplitRatio(9)).toBe(0.99)
    expect(clampSplitRatio(Number.NaN)).toBe(0.5)
  })
})

describe('resizeLayoutLocally', () => {
  const nested: LayoutNode = {
    type: 'split',
    direction: 'row',
    ratio: 0.5,
    first: leaf('calendar'),
    second: {
      type: 'split',
      direction: 'row',
      ratio: 0.5,
      first: leaf('editor'),
      second: leaf('overview'),
    },
  }
  function widths(layout: LayoutNode) {
    return layoutGeometry(layout, 1000, 1000, 0).slots.map((slot) => Math.round(slot.width))
  }

  it('moves only the module on each side of an outer gutter', () => {
    const resized = resizeLayoutLocally(nested, [], 0.6)
    // 日历和紧邻的编辑器变化，最右侧的数据概览宽度保持不变。
    expect(widths(resized)).toEqual([600, 150, 250])
    expect(widths(nested)).toEqual([500, 250, 250])
  })

  it('moves only the two modules adjacent to an inner gutter', () => {
    const resized = resizeLayoutLocally(nested, [1], 0.7)
    // 左侧日历不变，编辑器与数据概览之间重新分配。
    expect(widths(resized)).toEqual([500, 350, 150])
  })

  it.each(['left', 'right'] as const)('keeps the far module fixed when resizing the %s side of a stack', (side) => {
    const stack = split('column', 0.5, leaf('editor'), leaf('history'))
    const layout =
      side === 'left'
        ? split('row', 0.3, leaf('calendar'), split('row', 0.6, stack, leaf('overview')))
        : split('row', 0.7, split('row', 0.4, leaf('calendar'), stack), leaf('overview'))
    const before = new Map(layoutGeometry(layout, 1000, 1000, 0).slots.map((slot) => [slot.id, slot]))
    const after = new Map(
      layoutGeometry(resizeLayoutLocally(layout, [], 0.6), 1000, 1000, 0).slots.map((slot) => [slot.id, slot]),
    )
    const far = side === 'left' ? 'overview' : 'calendar'
    for (const key of ['left', 'top', 'width', 'height'] as const) {
      expect(after.get(far)![key]).toBeCloseTo(before.get(far)![key], 9)
    }
    for (const id of ['editor', 'history'] as const) {
      expect(after.get(id)!.width).not.toBeCloseTo(before.get(id)!.width)
      expect(after.get(id)!.height).toBe(before.get(id)!.height)
    }
  })
})

describe('resizeLayoutLocally with a perpendicular ancestor', () => {
  it('keeps remote siblings fixed when a parent splits along the other axis', () => {
    const layout: LayoutNode = {
      type: 'split',
      direction: 'column',
      ratio: 0.5,
      first: {
        type: 'split',
        direction: 'row',
        ratio: 0.5,
        first: leaf('calendar'),
        second: split('row', 0.5, leaf('editor'), leaf('overview')),
      },
      second: leaf('summary'),
    }
    const metrics = { width: 1000, height: 900, gap: 3.2 }
    const resized = resizeLayoutLocally(layout, [0], 0.7, metrics)
    const before = new Map(layoutGeometry(layout, 1000, 900, 3.2).slots.map((slot) => [slot.id, slot]))
    const after = new Map(layoutGeometry(resized, 1000, 900, 3.2).slots.map((slot) => [slot.id, slot]))
    for (const id of ['overview', 'summary'] as const) {
      for (const key of ['left', 'top', 'width', 'height'] as const) {
        expect(after.get(id)![key]).toBeCloseTo(before.get(id)![key], 9)
      }
    }
  })
})

describe('resizeLayoutLocally at pixel dimensions', () => {
  const metrics = { width: 1000, height: 900, gap: 3.2 }
  function rotated(node: LayoutNode): LayoutNode {
    return node.type === 'leaf'
      ? node
      : {
          ...node,
          direction: node.direction === 'row' ? 'column' : 'row',
          first: rotated(node.first),
          second: rotated(node.second),
        }
  }
  function slots(layout: LayoutNode) {
    return new Map(
      layoutGeometry(layout, metrics.width, metrics.height, metrics.gap).slots.map((slot) => [slot.id, slot]),
    )
  }

  it.each([
    ['left', 'row', 0.01],
    ['left', 'row', 0.99],
    ['right', 'row', 0.01],
    ['right', 'row', 0.99],
    ['left', 'column', 0.01],
    ['left', 'column', 0.99],
    ['right', 'column', 0.01],
    ['right', 'column', 0.99],
  ] as const)(
    'keeps non-adjacent modules fixed at the %s edge of a %s stack, requesting %s',
    (side, direction, ratio) => {
      const stack = split('column', 0.4, leaf('editor'), leaf('history'))
      const row =
        side === 'left'
          ? split('row', 0.3, leaf('calendar'), split('row', 0.6, stack, leaf('overview')))
          : split('row', 0.7, split('row', 0.4, leaf('calendar'), stack), leaf('overview'))
      const layout = direction === 'row' ? row : rotated(row)
      const before = slots(layout)
      const resized = resizeLayoutLocally(layout, [], ratio, metrics)
      const after = slots(resized)
      expect(after.size).toBe(4)
      const far = side === 'left' ? 'overview' : 'calendar'
      for (const key of ['left', 'top', 'width', 'height'] as const) {
        expect(after.get(far)![key]).toBeCloseTo(before.get(far)![key], 9)
      }
      const axis = direction === 'row' ? 'width' : 'height'
      const cross = direction === 'row' ? 'height' : 'width'
      for (const id of ['editor', 'history'] as const) {
        expect(after.get(id)![axis]).not.toBeCloseTo(before.get(id)![axis])
        expect(after.get(id)![cross]).toBeCloseTo(before.get(id)![cross], 9)
      }
      for (const slot of after.values()) {
        expect(slot[axis]).toBeGreaterThanOrEqual(metrics[axis] * 0.01 - 1e-8)
        expect(slot.width).toBeGreaterThan(0)
        expect(slot.height).toBeGreaterThan(0)
      }
      expect(normalizeUiPreferences({ layout: resized }).layout).toEqual(resized)
    },
  )

  it.each(['row', 'column'] as const)(
    'only adjusts the touching edges of both nested branches in a %s split',
    (direction) => {
      const row = split(
        'row',
        0.7,
        split(
          'column',
          0.4,
          split('row', 0.4, leaf('calendar'), leaf('editor')),
          split('row', 0.3, leaf('overview'), leaf('history')),
        ),
        leaf('summary'),
      )
      const layout = direction === 'row' ? row : rotated(row)
      const before = slots(layout)
      const after = slots(resizeLayoutLocally(layout, [], 0.6, metrics))
      for (const id of ['calendar', 'overview'] as const) {
        for (const key of ['left', 'top', 'width', 'height'] as const) {
          expect(after.get(id)![key]).toBeCloseTo(before.get(id)![key], 9)
        }
      }
      const axis = direction === 'row' ? 'width' : 'height'
      for (const id of ['editor', 'history', 'summary'] as const) {
        expect(after.get(id)![axis]).not.toBeCloseTo(before.get(id)![axis])
      }
    },
  )
})

describe('resizeLayoutLocally on the real workspace tree', () => {
  it('keeps non-adjacent modules at the same size in a mixed split layout', () => {
    // 当前使用中的工作空间布局：左边日历/编辑上下分栏，中间 AI 总结，右边历史/Excel 左右分栏。
    const layout: LayoutNode = {
      type: 'split',
      direction: 'row',
      ratio: 0.5395,
      first: {
        type: 'split',
        direction: 'row',
        ratio: 0.5562,
        first: {
          type: 'split',
          direction: 'column',
          ratio: 0.5,
          first: leaf('calendar'),
          second: leaf('editor'),
        },
        second: leaf('summary'),
      },
      second: {
        type: 'split',
        direction: 'row',
        ratio: 0.4836,
        first: leaf('history'),
        second: leaf('excel'),
      },
    }
    const before = layoutGeometry(layout, 1448, 805, 8).slots
    const resized = resizeLayoutLocally(layout, [], 0.62, { width: 1448, height: 805, gap: 8 })
    const after = layoutGeometry(resized, 1448, 805, 8).slots
    const byId = (slots: LayoutSlot[]) => new Map(slots.map((slot) => [slot.id, slot]))
    const beforeMap = byId(before)
    const afterMap = byId(after)
    // 间隙左右相邻的 Summary/History 变化。
    expect(afterMap.get('summary')!.width).not.toBeCloseTo(beforeMap.get('summary')!.width, 1)
    expect(afterMap.get('history')!.width).not.toBeCloseTo(beforeMap.get('history')!.width, 1)
    // 其余模块位置和尺寸均保持不变。
    for (const id of ['calendar', 'editor', 'excel'] as const) {
      for (const key of ['left', 'top', 'width', 'height'] as const) {
        expect(afterMap.get(id)![key]).toBeCloseTo(beforeMap.get(id)![key], 9)
      }
    }
  })
})

describe('insertLayoutModuleAtGutter', () => {
  const row: LayoutNode = {
    type: 'split',
    direction: 'row',
    ratio: 0.5,
    first: leaf('calendar'),
    second: leaf('editor'),
  }

  it('inserts a dragged module between the two modules around the gutter', () => {
    const tree: LayoutNode = {
      type: 'split',
      direction: 'row',
      ratio: 0.5,
      first: leaf('calendar'),
      second: {
        type: 'split',
        direction: 'row',
        ratio: 0.5,
        first: leaf('editor'),
        second: leaf('overview'),
      },
    }
    const inserted = insertLayoutModuleAtGutter(tree, 'calendar', [1])
    expect(flattenLayout(inserted).map((item) => item.id)).toEqual(['editor', 'calendar', 'overview'])
    const widths = layoutGeometry(inserted, 1200, 800, 0).slots.map((slot) => Math.round(slot.width))
    expect(widths).toEqual([400, 400, 400])
  })

  it('returns the tree when the dragged module is one of the adjacent leaves', () => {
    expect(insertLayoutModuleAtGutter(row, 'calendar', [])).toEqual(row)
    expect(insertLayoutModuleAtGutter(row, 'editor', [])).toEqual(row)
  })
})
