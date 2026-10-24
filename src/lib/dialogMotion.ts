/** Keep a native modal in the top layer until its closing animation finishes. */
export function createDialogMotion(getDialog: () => HTMLDialogElement | null | undefined) {
  let revision = 0
  let closeTimer: ReturnType<typeof setTimeout> | undefined
  let activeDialog: HTMLDialogElement | undefined
  let pendingClose:
    { dialog: HTMLDialogElement; promise: Promise<boolean>; resolve: (closed: boolean) => void } | undefined

  function cancelPendingClose() {
    revision += 1
    clearTimeout(closeTimer)
    closeTimer = undefined
    pendingClose?.dialog.classList.remove('is-closing')
    pendingClose?.resolve(false)
    pendingClose = undefined
  }

  function open() {
    cancelPendingClose()
    const dialog = getDialog()
    if (!dialog) return
    if (activeDialog && activeDialog !== dialog) {
      activeDialog.classList.remove('is-closing')
      if (activeDialog.open) activeDialog.close()
    }
    activeDialog = dialog
    dialog.classList.remove('is-closing')
    if (!dialog.open) {
      dialog.showModal()
      dialog.focus({ preventScroll: true })
    }
  }

  function close(): Promise<boolean> {
    const dialog = getDialog()
    if (!dialog?.open) return Promise.resolve(true)
    if (pendingClose?.dialog === dialog) return pendingClose.promise
    cancelPendingClose()
    if (activeDialog && activeDialog !== dialog) {
      activeDialog.classList.remove('is-closing')
      if (activeDialog.open) activeDialog.close()
    }
    activeDialog = dialog
    const closingRevision = revision
    let resolveClose!: (closed: boolean) => void
    const completion = new Promise<boolean>((resolve) => {
      resolveClose = resolve
    })
    pendingClose = { dialog, promise: completion, resolve: resolveClose }
    const finish = () => {
      if (revision !== closingRevision) return
      revision += 1
      clearTimeout(closeTimer)
      closeTimer = undefined
      pendingClose = undefined
      dialog.classList.remove('is-closing')
      if (dialog.open) dialog.close()
      resolveClose(true)
    }
    if (dialog.ownerDocument.defaultView?.matchMedia?.('(prefers-reduced-motion: reduce)').matches) {
      finish()
      return completion
    }
    dialog.classList.add('is-closing')
    const style = dialog.ownerDocument.defaultView?.getComputedStyle(dialog)
    const durations = style?.animationDuration.split(',').map(toMilliseconds) ?? []
    const delays = style?.animationDelay.split(',').map(toMilliseconds) ?? []
    const duration = Math.max(0, ...durations.map((time, index) => time + (delays[index % delays.length] ?? 0)))
    if (!duration) {
      finish()
      return completion
    }
    // The timer also covers a stylesheet change, a cancelled animation, or a
    // backgrounded webview that never delivers an animation completion event.
    closeTimer = setTimeout(finish, duration + 50)
    const animations = dialog.getAnimations?.() ?? []
    if (animations.length) void Promise.allSettled(animations.map((animation) => animation.finished)).then(finish)
    return completion
  }

  function dispose() {
    cancelPendingClose()
    const dialog = activeDialog ?? getDialog()
    dialog?.classList.remove('is-closing')
    if (dialog?.open) dialog.close()
    activeDialog = undefined
  }

  return { open, close, dispose }
}

function toMilliseconds(value: string) {
  const time = Number.parseFloat(value)
  return Number.isFinite(time) ? time * (value.trim().endsWith('ms') ? 1 : 1000) : 0
}
