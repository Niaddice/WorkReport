import { describe, expect, it } from 'vitest'
import { placePreviewBesideEditor } from '../src/lib/previewWindowPlacement'

const area = { x: 8, y: 44, width: 1264, height: 748 }
const preferred = { width: 520, height: 560 }

describe('journal preview docking', () => {
  it.each([
    { name: 'right side', x: 8, width: 500, expected: { x: 508, width: 520 } },
    { name: 'left side', x: 772, width: 500, expected: { x: 252, width: 520 } },
    { name: 'wider left side with a narrower preview', x: 448, width: 400, expected: { x: 8, width: 440 } },
    { name: 'right side when both sides have the same space', x: 440, width: 400, expected: { x: 840, width: 432 } },
  ])('attaches to the $name without covering the editor', ({ x, width, expected }) => {
    const editor = { x, y: 100, width, height: 600 }
    const preview = placePreviewBesideEditor(editor, area, preferred)
    expect(preview).toEqual({ ...expected, y: 100, height: 560 })
    expect(preview.x + preview.width <= editor.x || preview.x >= editor.x + editor.width).toBe(true)
  })

  it('keeps the preview readable and on screen when neither side can fit a normal window', () => {
    const preview = placePreviewBesideEditor({ x: 200, y: 760, width: 900, height: 100 }, area, preferred)
    expect(preview).toEqual({ x: 8, y: 232, width: 300, height: 560 })
    expect(preview.y).toBeGreaterThanOrEqual(area.y)
    expect(preview.y + preview.height).toBeLessThanOrEqual(area.y + area.height)
  })

  it('fits inside a viewport smaller than the normal minimum window size', () => {
    const smallArea = { x: 8, y: 44, width: 240, height: 160 }
    expect(placePreviewBesideEditor({ ...smallArea }, smallArea, preferred)).toEqual(smallArea)
  })
})
