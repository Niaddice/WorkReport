import type { LayoutNode } from './types'

export const LAYOUT_PRESETS: { id: string; name: string; description: string; layout: LayoutNode }[] = [
  {
    id: 'compact-writing',
    name: '紧凑月历',
    description: '左侧月历与概览，右侧留给日报编辑。',
    layout: {
      type: 'split',
      direction: 'row',
      ratio: 0.32,
      first: {
        type: 'split',
        direction: 'column',
        ratio: 0.62,
        first: { type: 'leaf', id: 'calendar' },
        second: { type: 'leaf', id: 'overview' },
      },
      second: { type: 'leaf', id: 'editor' },
    },
  },
]
