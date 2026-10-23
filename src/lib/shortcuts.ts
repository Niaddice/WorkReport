export type HistoryAction = 'undo' | 'redo'

type HistoryKeyEvent = Pick<KeyboardEvent, 'key' | 'altKey' | 'ctrlKey' | 'metaKey' | 'shiftKey'>

export function historyShortcut(event: HistoryKeyEvent, isMac: boolean): HistoryAction | null {
  if (event.altKey) return null
  const modifier = isMac ? event.metaKey : event.ctrlKey
  if (!modifier) return null
  const key = event.key.toLowerCase()
  if (key === 'z') return event.shiftKey ? 'redo' : 'undo'
  if (!isMac && key === 'y') return 'redo'
  return null
}
