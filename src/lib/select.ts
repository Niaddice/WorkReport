export interface SelectOption {
  value: string
  label: string
  disabled?: boolean
}

export function nextSelectOption(options: SelectOption[], current: number, direction: 1 | -1): number {
  for (let step = 1; step <= options.length; step++) {
    const index = (((current + step * direction) % options.length) + options.length) % options.length
    if (!options[index]?.disabled) return index
  }
  return -1
}

export function boundarySelectOption(options: SelectOption[], boundary: 'first' | 'last'): number {
  return nextSelectOption(options, boundary === 'first' ? -1 : options.length, boundary === 'first' ? 1 : -1)
}

export function matchSelectOption(options: SelectOption[], query: string, current: number): number {
  const normalized = query.trim().toLocaleLowerCase()
  if (!normalized) return -1
  // Repeated letters cycle through matching options, like a native select.
  const search = [...normalized].every((character) => character === normalized[0]) ? normalized[0]! : normalized
  for (let step = 1; step <= options.length; step++) {
    const index = (((current + step) % options.length) + options.length) % options.length
    if (!options[index]?.disabled && options[index]!.label.trim().toLocaleLowerCase().startsWith(search)) return index
  }
  return -1
}

export function selectMenuPosition(
  anchor: { top: number; bottom: number; left: number; width: number },
  viewport: { width: number; height: number },
  contentHeight: number,
) {
  const margin = 8
  const gap = 6
  const desiredHeight = Math.min(288, contentHeight)
  const below = Math.max(0, viewport.height - margin - anchor.bottom - gap)
  const above = Math.max(0, anchor.top - margin - gap)
  const placement = below >= desiredHeight || below >= above ? 'bottom' : 'top'
  const maxHeight = Math.min(288, placement === 'bottom' ? below : above)
  const height = Math.min(desiredHeight, maxHeight)
  const width = Math.max(0, Math.min(anchor.width, viewport.width - margin * 2))
  const left = Math.max(margin, Math.min(anchor.left, viewport.width - width - margin))
  const top = placement === 'bottom' ? anchor.bottom + gap : anchor.top - gap - height
  return { placement, maxHeight, width, left, top }
}
