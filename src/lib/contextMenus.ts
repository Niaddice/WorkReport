// Temporary app-wide policy. Set this to true to restore the existing context menus.
export const CONTEXT_MENUS_ENABLED = false

export function installContextMenuGuard(target: Window = window): () => void {
  if (CONTEXT_MENUS_ENABLED) return () => {}

  const blockContextMenu = (event: Event) => {
    event.preventDefault()
    event.stopImmediatePropagation()
  }
  // Window capture also covers teleported dialogs and stops custom menu handlers.
  target.addEventListener('contextmenu', blockContextMenu, { capture: true })
  return () => target.removeEventListener('contextmenu', blockContextMenu, { capture: true })
}
