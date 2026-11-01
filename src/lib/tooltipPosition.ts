export type TooltipPlacement = 'top' | 'bottom' | 'left' | 'right'

interface TooltipAnchor {
  left: number
  right: number
  top: number
  bottom: number
  width: number
  height: number
}

/** Keep the hint in the viewport and put rail hints beside their buttons. */
export function buttonTooltipPosition(
  anchor: TooltipAnchor,
  size: { width: number; height: number },
  viewport: { width: number; height: number },
) {
  const edge = 8
  const gap = 9
  const width = Math.min(size.width, Math.max(0, viewport.width - edge * 2))
  const height = Math.min(size.height, Math.max(0, viewport.height - edge * 2))
  const clamp = (value: number, max: number) => Math.max(edge, Math.min(value, Math.max(edge, max)))
  const x = anchor.left + anchor.width / 2
  const y = anchor.top + anchor.height / 2
  let placement: TooltipPlacement
  if (anchor.right >= viewport.width - 72 && anchor.left >= width + gap + edge) placement = 'left'
  else if (anchor.left <= 72 && viewport.width - anchor.right >= width + gap + edge) placement = 'right'
  else if (anchor.top >= height + gap + edge) placement = 'top'
  else if (viewport.height - anchor.bottom >= height + gap + edge) placement = 'bottom'
  else placement = anchor.top > viewport.height - anchor.bottom ? 'top' : 'bottom'
  const horizontal = placement === 'top' || placement === 'bottom'
  const left = clamp(
    horizontal ? x - width / 2 : placement === 'left' ? anchor.left - width - gap : anchor.right + gap,
    viewport.width - width - edge,
  )
  const top = clamp(
    horizontal ? (placement === 'top' ? anchor.top - height - gap : anchor.bottom + gap) : y - height / 2,
    viewport.height - height - edge,
  )
  const arrow = Math.max(10, Math.min(horizontal ? x - left : y - top, (horizontal ? width : height) - 10))
  return { left, top, placement, arrow }
}
