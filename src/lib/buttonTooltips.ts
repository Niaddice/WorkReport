export const BUTTON_TOOLTIP_DELAY = 1000

export interface ButtonTooltipTarget {
  anchor: HTMLElement
  text: string
  portal: HTMLElement
}

interface TooltipOptions {
  show: (target: ButtonTooltipTarget) => void
  hide: () => void
  document?: Document
}

const controls =
  'button, md-icon-button, md-filled-button, md-filled-tonal-button, md-outlined-button, md-text-button, md-elevated-button, [role="button"], [role="menuitem"], [role="menuitemcheckbox"], [role="menuitemradio"], [role="option"], [role="tab"], [role="combobox"]'

function isControl(element: HTMLElement): boolean {
  return element.matches(controls) || (element.tagName.startsWith('MD-') && element.tagName.endsWith('-BUTTON'))
}

function ancestry(node: Node | null): HTMLElement[] {
  const result: HTMLElement[] = []
  while (node) {
    if (node instanceof HTMLElement) result.push(node)
    if (node.parentNode) node = node.parentNode
    else node = node instanceof ShadowRoot ? node.host : null
  }
  return result
}

function containsComposed(ancestor: HTMLElement, node: EventTarget | null): boolean {
  return node instanceof Node && ancestry(node).includes(ancestor)
}

/** Delegated hints for dynamic controls, including controls in Material shadow roots. */
export function installButtonTooltips(options: TooltipOptions): () => void {
  const doc = options.document ?? document
  const win = doc.defaultView
  if (!win) return () => {}

  interface ActiveTooltip {
    anchor: HTMLElement
    source: HTMLElement
    path: HTMLElement[]
    text: string
    portal: HTMLElement
    mode: 'pointer' | 'keyboard'
    dismissed: boolean
    titles: Map<HTMLElement, string | null>
  }

  let active: ActiveTooltip | null = null
  let timer: ReturnType<typeof setTimeout> | undefined
  let observer: MutationObserver | undefined
  let visible = false
  let keyboard = false
  let disposed = false

  function originalTitle(element: HTMLElement): string | null {
    return active?.titles.has(element) ? active.titles.get(element)! : element.getAttribute('title')
  }

  function explicitHint(element: HTMLElement): string | null {
    return element.hasAttribute('data-tooltip') ? element.getAttribute('data-tooltip') : originalTitle(element)
  }

  function excluded(anchor: HTMLElement): boolean {
    if (!anchor.isConnected || doc.hidden) return true
    return ancestry(anchor).some((element) => {
      if (element.matches('[inert], [hidden], [data-tooltip-disabled], [aria-hidden="true"]')) return true
      const style = win!.getComputedStyle(element)
      return style.display === 'none' || style.visibility === 'hidden' || style.visibility === 'collapse'
    })
  }

  function foreignTooltip(anchor: HTMLElement): boolean {
    return (anchor.getAttribute('aria-describedby') ?? '').split(/\s+/).some((id) => {
      const description = id && doc.getElementById(id)
      return (
        description &&
        description.getAttribute('role') === 'tooltip' &&
        !description.classList.contains('button-tooltip')
      )
    })
  }

  function mayShow(anchor: HTMLElement): boolean {
    return (
      !ancestry(anchor).some((element) => element.getAttribute('aria-expanded') === 'true') && !foreignTooltip(anchor)
    )
  }

  function resolve(path: HTMLElement[]): (ButtonTooltipTarget & { source: HTMLElement }) | null {
    if (path.some((element) => element.hasAttribute('data-tooltip-disabled'))) return null
    const candidates = path.filter(isControl)
    // Prefer the public Material host to its internal native button.
    let anchor: HTMLElement | undefined =
      candidates.find((element) => element.tagName.startsWith('MD-')) ?? candidates[0]
    if (!anchor) {
      anchor = path.find((element) => explicitHint(element) !== null && !!element.querySelector(controls))
    }
    if (!anchor || excluded(anchor)) return null
    const related = path.filter((element) => containsComposed(anchor!, element) || containsComposed(element, anchor!))
    let source = related.find((element) => element.hasAttribute('data-tooltip'))
    source ??= related.find((element) => originalTitle(element) !== null)
    source ??= Array.from(anchor.querySelectorAll<HTMLElement>('[data-tooltip], [title]')).find(
      (element) => explicitHint(element) !== null,
    )
    let text = source ? explicitHint(source)?.trim() : undefined
    if (!source) {
      source = anchor
      text = anchor.getAttribute('aria-label')?.trim()
      if (!text && anchor.matches('[role^="menuitem"], [role="option"]')) text = anchor.textContent?.trim()
    }
    if (!text || excluded(source)) return null
    const portal = ancestry(anchor).find((element) => element.matches('dialog[open]')) ?? doc.body
    return { anchor, source, text, portal }
  }

  function clearTimer() {
    clearTimeout(timer)
    timer = undefined
  }

  function hide() {
    clearTimer()
    if (visible) {
      visible = false
      options.hide()
    }
  }

  function recordExternalTitles(records: MutationRecord[]) {
    if (!active) return
    for (const record of records) {
      if (record.type !== 'attributes' || record.attributeName !== 'title') continue
      const element = record.target
      if (element instanceof HTMLElement && active.titles.has(element)) {
        active.titles.set(element, element.getAttribute('title'))
      }
    }
  }

  function release() {
    hide()
    // Apply queued Vue updates before restoring attributes; never restore an
    // older title over a label that changed in the same turn as dismissal.
    if (observer) recordExternalTitles(observer.takeRecords())
    observer?.disconnect()
    observer = undefined
    const previous = active
    active = null
    for (const [element, title] of previous?.titles ?? []) {
      if (title === null) element.removeAttribute('title')
      else element.setAttribute('title', title)
    }
  }

  function suppressTitles(path: HTMLElement[], source: HTMLElement, anchor: HTMLElement) {
    if (!active) return
    for (const element of new Set([...path, source, anchor])) {
      if (!active.titles.has(element) && (element.hasAttribute('title') || element === anchor)) {
        active.titles.set(element, element.getAttribute('title'))
      }
    }
    for (const element of active.titles.keys()) element.setAttribute('title', '')
    // These writes belong to the controller, not to Vue or an async label.
    observer?.takeRecords()
  }

  function dismiss() {
    hide()
    if (active) {
      // Keep native titles blank until pointer exit: otherwise the browser can
      // display a second, unstyled hint after a click dismisses our hint.
      active.dismissed = true
    }
  }

  function refresh() {
    if (!active) return
    if (excluded(active.anchor)) return release()
    const path = active.path.filter((element) => element.isConnected)
    const target = resolve(path)
    if (!target || target.anchor !== active.anchor) return dismiss()
    const changed = target.text !== active.text || target.portal !== active.portal
    active.source = target.source
    active.text = target.text
    active.portal = target.portal
    suppressTitles(path, target.source, target.anchor)
    if (!mayShow(target.anchor)) return dismiss()
    if (visible && changed) options.show({ anchor: target.anchor, text: target.text, portal: target.portal })
  }

  function onMutations(records: MutationRecord[]) {
    if (!active) return
    recordExternalTitles(records)
    refresh()
  }

  function begin(path: HTMLElement[], mode: 'pointer' | 'keyboard') {
    if (disposed) return
    if (observer) recordExternalTitles(observer.takeRecords())
    const target = resolve(path)
    if (!target) return release()
    if (active?.anchor === target.anchor && active.mode === mode) {
      active.path = path
      refresh()
      return
    }
    release()
    active = { ...target, mode, path, dismissed: !mayShow(target.anchor), titles: new Map() }
    suppressTitles(path, target.source, target.anchor)
    observer = new MutationObserver(onMutations)
    const observation: MutationObserverInit = {
      subtree: true,
      childList: true,
      attributes: true,
      attributeFilter: [
        'title',
        'data-tooltip',
        'aria-label',
        'aria-describedby',
        'aria-expanded',
        'aria-hidden',
        'hidden',
        'inert',
        'open',
        'class',
        'style',
      ],
    }
    observer.observe(doc.documentElement, observation)
    for (const element of path) {
      const root = element.getRootNode()
      if (root instanceof ShadowRoot) observer.observe(root, observation)
    }
    if (active.dismissed) return
    timer = setTimeout(() => {
      timer = undefined
      refresh()
      if (!active || active.dismissed || !mayShow(active.anchor)) return
      visible = true
      options.show({ anchor: active.anchor, text: active.text, portal: active.portal })
    }, BUTTON_TOOLTIP_DELAY)
  }

  function eventPath(event: Event): HTMLElement[] {
    return event.composedPath().filter((target): target is HTMLElement => target instanceof HTMLElement)
  }

  function onPointerOver(event: Event) {
    if ((event as PointerEvent).pointerType === 'touch') return
    keyboard = false
    begin(eventPath(event), 'pointer')
  }

  function onPointerOut(event: Event) {
    if (active?.mode === 'pointer' && !containsComposed(active.anchor, (event as MouseEvent).relatedTarget)) release()
  }

  function onFocusIn(event: Event) {
    if (keyboard) begin(eventPath(event), 'keyboard')
  }

  function onFocusOut(event: Event) {
    if (active?.mode === 'keyboard' && !containsComposed(active.anchor, (event as FocusEvent).relatedTarget)) release()
  }

  function onKeydown(event: KeyboardEvent) {
    keyboard = true
    if (event.key === 'Tab') release()
    else if (!['Shift', 'Control', 'Alt', 'Meta'].includes(event.key)) dismiss()
  }

  function onPointerDown() {
    keyboard = false
    dismiss()
  }

  function onVisibility() {
    if (doc.hidden) release()
  }

  doc.addEventListener('pointerover', onPointerOver, true)
  doc.addEventListener('pointerout', onPointerOut, true)
  doc.addEventListener('pointerdown', onPointerDown, true)
  doc.addEventListener('pointercancel', release, true)
  doc.addEventListener('click', dismiss, true)
  doc.addEventListener('contextmenu', dismiss, true)
  doc.addEventListener('focusin', onFocusIn, true)
  doc.addEventListener('focusout', onFocusOut, true)
  doc.addEventListener('keydown', onKeydown, true)
  doc.addEventListener('scroll', dismiss, true)
  doc.addEventListener('visibilitychange', onVisibility)
  win.addEventListener('resize', dismiss)
  win.addEventListener('blur', release)

  return () => {
    disposed = true
    release()
    doc.removeEventListener('pointerover', onPointerOver, true)
    doc.removeEventListener('pointerout', onPointerOut, true)
    doc.removeEventListener('pointerdown', onPointerDown, true)
    doc.removeEventListener('pointercancel', release, true)
    doc.removeEventListener('click', dismiss, true)
    doc.removeEventListener('contextmenu', dismiss, true)
    doc.removeEventListener('focusin', onFocusIn, true)
    doc.removeEventListener('focusout', onFocusOut, true)
    doc.removeEventListener('keydown', onKeydown, true)
    doc.removeEventListener('scroll', dismiss, true)
    doc.removeEventListener('visibilitychange', onVisibility)
    win.removeEventListener('resize', dismiss)
    win.removeEventListener('blur', release)
  }
}
