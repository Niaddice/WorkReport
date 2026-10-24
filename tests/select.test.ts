import { describe, expect, it } from 'vitest'
import { boundarySelectOption, matchSelectOption, nextSelectOption, selectMenuPosition } from '../src/lib/select'

const options = [
  { value: '', label: '请选择', disabled: true },
  { value: 'a', label: 'Alpha' },
  { value: 'b', label: 'Beta' },
  { value: 'disabled', label: 'Archived', disabled: true },
  { value: 'c', label: 'Apple' },
]

describe('select keyboard navigation', () => {
  it('skips disabled options, wraps and handles unavailable options', () => {
    expect(boundarySelectOption(options, 'first')).toBe(1)
    expect(boundarySelectOption(options, 'last')).toBe(4)
    expect(nextSelectOption(options, 2, 1)).toBe(4)
    expect(nextSelectOption(options, 4, 1)).toBe(1)
    expect(nextSelectOption(options, 1, -1)).toBe(4)
    expect(nextSelectOption([], 0, 1)).toBe(-1)
    expect(boundarySelectOption([{ value: '', label: 'None', disabled: true }], 'first')).toBe(-1)
  })

  it('allows an empty value when that option is enabled', () => {
    expect(boundarySelectOption([{ value: '', label: 'CLI 默认模型' }, ...options], 'first')).toBe(0)
  })

  it('supports case-insensitive prefixes and repeated-letter cycling', () => {
    expect(matchSelectOption(options, 'a', 1)).toBe(4)
    expect(matchSelectOption(options, 'aa', 4)).toBe(1)
    expect(matchSelectOption(options, 'AP', 3)).toBe(4)
    expect(matchSelectOption(options, 'Archived', 0)).toBe(-1)
    expect(matchSelectOption(options, 'missing', 0)).toBe(-1)
    expect(matchSelectOption(options, ' ', 0)).toBe(-1)
  })
})

describe('select popup viewport placement', () => {
  it('places a short list below and caps tall menus', () => {
    const anchor = { top: 100, bottom: 144, left: 20, width: 240 }
    expect(selectMenuPosition(anchor, { width: 800, height: 600 }, 150)).toEqual({
      placement: 'bottom',
      top: 150,
      left: 20,
      width: 240,
      maxHeight: 288,
    })
    expect(selectMenuPosition(anchor, { width: 800, height: 600 }, 1200).maxHeight).toBe(288)
  })

  it('flips upward near the bottom and stays within horizontal viewport bounds', () => {
    const result = selectMenuPosition(
      { top: 500, bottom: 544, left: 700, width: 240 },
      { width: 800, height: 600 },
      200,
    )
    expect(result).toEqual({ placement: 'top', top: 294, left: 552, width: 240, maxHeight: 288 })
  })

  it('constrains a large trigger and list in a small viewport', () => {
    const result = selectMenuPosition(
      { top: 100, bottom: 144, left: -20, width: 1000 },
      { width: 320, height: 240 },
      600,
    )
    expect(result.width).toBe(304)
    expect(result.left).toBe(8)
    expect(result.placement).toBe('top')
    expect(result.maxHeight).toBe(86)
    expect(result.top + Math.min(result.maxHeight, 600)).toBeLessThanOrEqual(232)
  })
})
