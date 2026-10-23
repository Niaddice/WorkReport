import { describe, expect, it } from 'vitest'
import { historyShortcut } from '../src/lib/shortcuts'

const key = (value: string, extra: Partial<KeyboardEvent> = {}) => ({ key: value, altKey: false, ctrlKey: false, metaKey: false, shiftKey: false, ...extra })

describe('history shortcuts', () => {
  it('uses Command on macOS', () => {
    expect(historyShortcut(key('z', { metaKey: true }), true)).toBe('undo')
    expect(historyShortcut(key('z', { metaKey: true, shiftKey: true }), true)).toBe('redo')
    expect(historyShortcut(key('y', { metaKey: true }), true)).toBeNull()
    expect(historyShortcut(key('z', { ctrlKey: true }), true)).toBeNull()
  })

  it('uses Control on Windows and supports both redo shortcuts', () => {
    expect(historyShortcut(key('z', { ctrlKey: true }), false)).toBe('undo')
    expect(historyShortcut(key('z', { ctrlKey: true, shiftKey: true }), false)).toBe('redo')
    expect(historyShortcut(key('y', { ctrlKey: true }), false)).toBe('redo')
    expect(historyShortcut(key('z', { metaKey: true }), false)).toBeNull()
  })

  it('ignores Alt combinations', () => {
    expect(historyShortcut(key('z', { ctrlKey: true, altKey: true }), false)).toBeNull()
  })
})
