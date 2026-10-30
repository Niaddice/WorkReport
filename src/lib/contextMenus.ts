// Only the app's contextual editing and Git menus are exposed.
export const CONTEXT_MENUS_ENABLED = true

export interface ContextMenuItem {
  id: string
  label: string
  disabled?: boolean
  danger?: boolean
  run: () => unknown
}

export function installContextMenuGuard(target: Window = window): () => void {
  const blockContextMenu = (event: Event) => {
    event.preventDefault()
    if (!CONTEXT_MENUS_ENABLED) event.stopImmediatePropagation()
  }
  // Suppress native WebView menus but let contextual handlers choose allowed actions.
  target.addEventListener('contextmenu', blockContextMenu, { capture: true })
  return () => target.removeEventListener('contextmenu', blockContextMenu, { capture: true })
}

export interface ContextEditor {
  scope: () => unknown
  canUndo: () => boolean
  canRedo: () => boolean
  undo: () => unknown
  redo: () => unknown
  replace: (text: string, start: number, end: number) => unknown
}

const editors = new WeakMap<HTMLElement, ContextEditor>()
export function registerContextEditor(element: HTMLElement, editor: ContextEditor): () => void {
  editors.set(element, editor)
  return () => {
    if (editors.get(element) === editor) editors.delete(element)
  }
}
export function contextEditor(element: HTMLElement): ContextEditor | undefined {
  return editors.get(element)
}
