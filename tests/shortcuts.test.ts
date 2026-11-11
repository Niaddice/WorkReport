import { describe, expect, it } from 'vitest'
import { cheatsheetShortcut, historyShortcut, saveShortcut, SHORTCUT_DEFINITIONS } from '../src/lib/shortcuts'

const key = (value: string, extra: Partial<KeyboardEvent> = {}) => ({
  key: value,
  altKey: false,
  ctrlKey: false,
  metaKey: false,
  shiftKey: false,
  ...extra,
})

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

  it('detects save shortcut (Cmd+S / Ctrl+S)', () => {
    expect(saveShortcut(key('s', { metaKey: true }), true)).toBe(true)
    expect(saveShortcut(key('s', { ctrlKey: true }), false)).toBe(true)
    expect(saveShortcut(key('s', { ctrlKey: true }), true)).toBe(false)
    expect(saveShortcut(key('s'), true)).toBe(false)
    expect(saveShortcut(key('s', { metaKey: true, altKey: true }), true)).toBe(false)
  })

  it('detects cheatsheet shortcut (Cmd+/ or Ctrl+/)', () => {
    expect(cheatsheetShortcut(key('/', { metaKey: true }), true)).toBe(true)
    expect(cheatsheetShortcut(key('/', { ctrlKey: true }), false)).toBe(true)
    expect(cheatsheetShortcut(key('?', { metaKey: true }), true)).toBe(true)
    expect(cheatsheetShortcut(key('/'), true)).toBe(false)
  })

  it('provides structured shortcut definitions for all categories', () => {
    expect(SHORTCUT_DEFINITIONS.length).toBeGreaterThan(5)
    const categories = new Set(SHORTCUT_DEFINITIONS.map((s) => s.category))
    expect(categories.has('global')).toBe(true)
    expect(categories.has('editor')).toBe(true)
    expect(categories.has('ai')).toBe(true)
    expect(categories.has('git')).toBe(true)
  })
})
