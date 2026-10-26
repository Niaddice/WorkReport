export interface PreviewWindowRect {
  x: number
  y: number
  width: number
  height: number
}

export const PREVIEW_MIN_WIDTH = 300
export const PREVIEW_MIN_HEIGHT = 200

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(maximum, Math.max(minimum, value))
}

/** 选择编辑模块旁空间较大的一侧，边缘贴合；空间不足时收窄并限制在可视区域内。 */
export function placePreviewBesideEditor(
  editor: PreviewWindowRect,
  area: PreviewWindowRect,
  preferred: Pick<PreviewWindowRect, 'width' | 'height'>,
): PreviewWindowRect {
  const right = area.x + area.width
  const leftSpace = clamp(editor.x - area.x, 0, area.width)
  const rightSpace = clamp(right - editor.x - editor.width, 0, area.width)
  const useRight = rightSpace >= leftSpace
  const available = useRight ? rightSpace : leftSpace
  const minimumWidth = Math.min(PREVIEW_MIN_WIDTH, area.width)
  const width = Math.min(clamp(preferred.width, minimumWidth, area.width), Math.max(minimumWidth, available))
  const height = clamp(preferred.height, Math.min(PREVIEW_MIN_HEIGHT, area.height), area.height)
  return {
    x: clamp(useRight ? editor.x + editor.width : editor.x - width, area.x, right - width),
    y: clamp(editor.y, area.y, area.y + area.height - height),
    width,
    height,
  }
}
