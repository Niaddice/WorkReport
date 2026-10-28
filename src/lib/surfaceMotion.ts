import {
  MAXIMIZE_DURATION,
  RESTORE_DURATION,
  MAXIMIZE_EASING,
  RESTORE_EASING,
  windowRectTransform,
  type WindowMotionRect,
} from './windowMotion'

interface Anchor {
  element: Element
  rect: WindowMotionRect
  sequence: number
}
interface TriggerTracker {
  users: number
  sequence: number
  latest: Anchor | null
  remember: (event: Event) => void
}
const trackers = new WeakMap<Document, TriggerTracker>()
const triggers =
  'button, [role="button"], a[href], md-icon-button, md-filled-button, md-filled-tonal-button, md-outlined-button, md-text-button'

function rectOf(element: Element | null | undefined): WindowMotionRect | null {
  const rect = element?.getBoundingClientRect?.()
  if (
    !rect ||
    ![rect.left, rect.top, rect.width, rect.height].every(Number.isFinite) ||
    rect.width <= 0 ||
    rect.height <= 0
  )
    return null
  return { left: rect.left, top: rect.top, width: rect.width, height: rect.height }
}

/** 在 WebKit 不聚焦点击按钮、或异步操作移除按钮前记录真实触发位置。 */
function trackTriggers(ownerDocument?: Document) {
  if (!ownerDocument?.addEventListener) return { current: () => null as Anchor | null, release() {} }
  let tracker = trackers.get(ownerDocument)
  if (!tracker) {
    const state: TriggerTracker = {
      users: 0,
      sequence: 0,
      latest: null,
      remember(event) {
        if (event.type === 'keydown' && !['Enter', ' '].includes((event as KeyboardEvent).key)) {
          state.latest = null
          return
        }
        const target = event.target
        const element = target instanceof Element ? target.closest(triggers) : null
        const rect = rectOf(element)
        if (element && rect) state.latest = { element, rect, sequence: ++state.sequence }
      },
    }
    tracker = state
    trackers.set(ownerDocument, tracker)
    ownerDocument.addEventListener('pointerdown', tracker.remember, true)
    ownerDocument.addEventListener('click', tracker.remember, true)
    ownerDocument.addEventListener('keydown', tracker.remember, true)
  }
  tracker.users++
  const active = tracker
  let released = false
  return {
    current: () => active.latest,
    release() {
      if (released) return
      released = true
      if (--active.users) return
      ownerDocument.removeEventListener('pointerdown', active.remember, true)
      ownerDocument.removeEventListener('click', active.remember, true)
      ownerDocument.removeEventListener('keydown', active.remember, true)
      trackers.delete(ownerDocument)
    },
  }
}

interface SurfaceMotionOptions {
  openingRect?: () => WindowMotionRect | null | undefined
  closingRect?: () => WindowMotionRect | null | undefined
  backdrop?: () => HTMLElement | null | undefined
  /** 模块最大化/还原只变换几何，避免原位置出现时再次淡入。 */
  fade?: boolean
}

/** 模态窗口、浮窗和说明面板共用的按钮展开/收缩动画。 */
export function createSurfaceMotion(options: SurfaceMotionOptions = {}) {
  const ownerDocument = typeof document === 'undefined' ? undefined : document
  const tracker = trackTriggers(ownerDocument)
  let opener: WindowMotionRect | null = null
  let openingSequence = 0
  let interrupted: { rect: WindowMotionRect; opacity: string; backdropOpacity: string } | null = null
  let motion: { element: HTMLElement; animations: Animation[]; timer?: ReturnType<typeof setTimeout> } | null = null

  function captureOpening(element?: HTMLElement) {
    const trigger = tracker.current()
    openingSequence = trigger?.sequence ?? 0
    const next =
      options.openingRect?.() ??
      (trigger && !element?.contains(trigger.element) ? trigger.rect : null) ??
      (ownerDocument?.activeElement?.matches(triggers) && !element?.contains(ownerDocument.activeElement)
        ? rectOf(ownerDocument.activeElement)
        : null)
    if (next || (!motion && !interrupted)) opener = next
  }
  function closingRect(element: HTMLElement) {
    const trigger = tracker.current()
    if (
      trigger &&
      trigger.sequence > openingSequence &&
      element.contains(trigger.element) &&
      trigger.element.matches('[data-window-close]')
    )
      return trigger.rect
    return (
      options.closingRect?.() ??
      rectOf(
        element.querySelector?.('[data-window-close="default"]') ?? element.querySelector?.('[data-window-close]'),
      ) ??
      opener
    )
  }
  function cancel(preserve = true) {
    if (!preserve) interrupted = null
    if (!motion) return
    if (preserve) {
      const rect = rectOf(motion.element)
      const view = motion.element.ownerDocument.defaultView
      const backdrop = options.backdrop?.()
      if (rect)
        interrupted = {
          rect,
          opacity: view?.getComputedStyle(motion.element).opacity || '1',
          backdropOpacity: backdrop ? view?.getComputedStyle(backdrop).opacity || '1' : '1',
        }
    }
    const previous = motion
    motion = null
    clearTimeout(previous.timer)
    previous.element.classList.remove('is-surface-closing')
    previous.animations.forEach((animation) => animation.cancel())
  }
  function animate(element: HTMLElement, opening: boolean, done: () => void): boolean {
    const anchor = opening ? opener : closingRect(element)
    cancel()
    const current = interrupted
    interrupted = null
    if (
      element.ownerDocument.defaultView?.matchMedia?.('(prefers-reduced-motion: reduce)').matches ||
      typeof element.animate !== 'function' ||
      !anchor
    )
      return false
    element.classList.add('has-window-motion')
    const frame = rectOf(element)
    const anchorTransform = frame && windowRectTransform(anchor, frame)
    if (!frame || !anchorTransform) {
      element.classList.remove('has-window-motion')
      return false
    }
    const full = { transform: 'translate(0px, 0px) scale(1, 1)', opacity: 1 }
    const start = current
      ? { transform: windowRectTransform(current.rect, frame) ?? full.transform, opacity: current.opacity }
      : opening
        ? { transform: anchorTransform, opacity: options.fade === false ? 1 : 0.2 }
        : full
    const end = opening ? full : { transform: anchorTransform, opacity: options.fade === false ? 1 : 0 }
    const duration = opening ? MAXIMIZE_DURATION : RESTORE_DURATION
    const state: NonNullable<typeof motion> = { element, animations: [] }
    motion = state
    const finish = () => {
      if (motion !== state) return
      motion = null
      clearTimeout(state.timer)
      element.classList.remove('is-surface-closing')
      done()
      state.animations.forEach((animation) => animation.cancel())
    }
    element.style.transformOrigin = 'top left'
    element.classList.toggle('is-surface-closing', !opening)
    state.animations.push(
      element.animate([start, end], {
        duration,
        easing: opening ? MAXIMIZE_EASING : RESTORE_EASING,
        fill: 'both',
      }),
    )
    const backdrop = options.backdrop?.()
    if (backdrop && typeof backdrop.animate === 'function')
      state.animations.push(
        backdrop.animate([{ opacity: current?.backdropOpacity ?? (opening ? 0 : 1) }, { opacity: opening ? 1 : 0 }], {
          duration,
          easing: opening ? 'ease-out' : 'ease-in',
          fill: 'both',
        }),
      )
    state.timer = setTimeout(finish, duration + 80)
    void Promise.allSettled(state.animations.map((animation) => animation.finished)).then(finish)
    return true
  }
  function enter(element: Element, done: () => void) {
    captureOpening(element as HTMLElement)
    if (!animate(element as HTMLElement, true, done)) done()
  }
  function leave(element: Element, done: () => void) {
    if (!animate(element as HTMLElement, false, done)) done()
  }
  function dispose() {
    cancel(false)
    tracker.release()
  }
  return { captureOpening, animate, enter, leave, cancel, dispose }
}
