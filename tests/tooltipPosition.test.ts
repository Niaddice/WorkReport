import { describe, expect, it } from 'vitest'
import { buttonTooltipPosition } from '../src/lib/tooltipPosition'

const viewport = { width: 960, height: 680 }
function anchor(left: number, top: number) {
  return { left, top, right: left + 26, bottom: top + 26, width: 26, height: 26 }
}

describe('button tooltip placement', () => {
  it('keeps right and left rail hints beside their source controls', () => {
    const right = buttonTooltipPosition(anchor(926, 180), { width: 150, height: 34 }, viewport)
    expect(right.placement).toBe('left')
    expect(right.left + 150).toBeLessThan(926)
    const left = buttonTooltipPosition(anchor(8, 180), { width: 150, height: 34 }, viewport)
    expect(left.placement).toBe('right')
    expect(left.left).toBeGreaterThan(34)
  })

  it('flips below buttons near the top edge and clamps long hints into the viewport', () => {
    const top = buttonTooltipPosition(anchor(420, 8), { width: 220, height: 42 }, viewport)
    expect(top.placement).toBe('bottom')
    expect(top.top).toBeGreaterThan(34)
    const bottom = buttonTooltipPosition(anchor(440, 648), { width: 1200, height: 90 }, viewport)
    expect(bottom.placement).toBe('top')
    expect(bottom.left).toBe(8)
    expect(bottom.top).toBeGreaterThanOrEqual(8)
    expect(bottom.top + 90).toBeLessThanOrEqual(672)
  })
})
