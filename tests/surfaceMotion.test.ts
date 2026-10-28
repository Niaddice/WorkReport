// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest'
import { flushPromises } from '@vue/test-utils'
import { createSurfaceMotion } from '../src/lib/surfaceMotion'
import { createDialogMotion } from '../src/lib/dialogMotion'
import { MAXIMIZE_DURATION, RESTORE_DURATION } from '../src/lib/windowMotion'

const cleanup: (() => void)[] = []
afterEach(() => {
  cleanup
    .splice(0)
    .reverse()
    .forEach((dispose) => dispose())
  document.body.replaceChildren()
  vi.restoreAllMocks()
  vi.useRealTimers()
})

function click(element: Element) {
  element.dispatchEvent(new MouseEvent('click', { bubbles: true, composed: true }))
}
function fixedRect(element: Element, rect: DOMRect) {
  element.getBoundingClientRect = () => rect
}
function scene(tag: 'section' | 'dialog' = 'section') {
  const opener = document.createElement('button')
  fixedRect(opener, new DOMRect(20, 30, 40, 30))
  const panel = document.createElement(tag)
  let visual = new DOMRect(100, 100, 400, 300)
  panel.getBoundingClientRect = () => visual
  const close = document.createElement('button')
  close.dataset.windowClose = 'default'
  fixedRect(close, new DOMRect(450, 110, 20, 20))
  panel.append(close)
  document.body.append(opener, panel)
  const animations: {
    frames: Keyframe[]
    options: KeyframeAnimationOptions
    finish: () => void
    cancel: ReturnType<typeof vi.fn>
  }[] = []
  panel.animate = ((frames: Keyframe[], options: KeyframeAnimationOptions) => {
    let finish!: () => void
    let reject!: (cause: unknown) => void
    const finished = new Promise<void>((resolve, fail) => {
      finish = resolve
      reject = fail
    })
    const cancel = vi.fn(() => {
      visual = new DOMRect(100, 100, 400, 300)
      reject(new DOMException('Cancelled', 'AbortError'))
    })
    animations.push({ frames, options, finish, cancel })
    return { finished, cancel } as unknown as Animation
  }) as typeof panel.animate
  const motion = createSurfaceMotion()
  cleanup.push(motion.dispose)
  return {
    opener,
    panel,
    close,
    motion,
    animations,
    setVisual: (rect: DOMRect) => {
      visual = rect
    },
  }
}

describe('button anchored surface motion', () => {
  it('expands from a clicked icon button without relying on focus', async () => {
    const s = scene()
    const icon = document.createElementNS('http://www.w3.org/2000/svg', 'svg')
    s.opener.append(icon)
    click(icon)
    expect(document.activeElement).not.toBe(s.opener)
    const done = vi.fn()
    s.motion.enter(s.panel, done)
    expect(s.animations[0]!.frames[0]!.transform).toBe('translate(-80px, -70px) scale(0.1, 0.1)')
    expect(s.animations[0]!.options.duration).toBe(MAXIMIZE_DURATION)
    expect(done).not.toHaveBeenCalled()
    s.animations[0]!.finish()
    await flushPromises()
    expect(done).toHaveBeenCalledOnce()
  })

  it('retains the trigger rectangle when asynchronous work removes the opener', () => {
    const s = scene()
    click(s.opener)
    s.opener.remove()
    s.motion.enter(s.panel, vi.fn())
    expect(s.animations[0]!.frames[0]!.transform).toBe('translate(-80px, -70px) scale(0.1, 0.1)')
  })

  it('shrinks to the default close button on Escape', async () => {
    const s = scene()
    click(s.opener)
    s.motion.enter(s.panel, vi.fn())
    s.animations[0]!.finish()
    await flushPromises()
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    s.motion.leave(s.panel, vi.fn())
    expect(s.animations[1]!.frames[1]!.transform).toBe('translate(350px, 10px) scale(0.05, 0.06666666666666667)')
    expect(s.animations[1]!.options.duration).toBe(RESTORE_DURATION)
  })

  it('uses the clicked confirmation control even if it changes after an asynchronous operation', async () => {
    const s = scene()
    click(s.opener)
    s.motion.enter(s.panel, vi.fn())
    s.animations[0]!.finish()
    await flushPromises()
    const confirm = document.createElement('md-filled-button')
    confirm.dataset.windowClose = ''
    fixedRect(confirm, new DOMRect(150, 360, 80, 30))
    s.panel.append(confirm)
    click(confirm)
    fixedRect(confirm, new DOMRect(150, 360, 120, 30))
    s.motion.leave(s.panel, vi.fn())
    expect(s.animations[1]!.frames[1]!.transform).toBe('translate(50px, 260px) scale(0.2, 0.1)')
  })

  it('reverses from the current rectangle without completing an interrupted close', async () => {
    const s = scene()
    click(s.opener)
    s.motion.enter(s.panel, vi.fn())
    s.animations[0]!.finish()
    await flushPromises()
    const closed = vi.fn()
    click(s.close)
    s.motion.leave(s.panel, closed)
    const closing = s.animations[1]!
    s.setVisual(new DOMRect(300, 120, 200, 150))
    s.motion.cancel()
    const reopened = vi.fn()
    s.motion.enter(s.panel, reopened)
    expect(closing.cancel).toHaveBeenCalledOnce()
    expect(s.animations[2]!.frames[0]!.transform).toBe('translate(200px, 20px) scale(0.5, 0.5)')
    closing.finish()
    await flushPromises()
    expect(closed).not.toHaveBeenCalled()
    s.animations[2]!.finish()
    await flushPromises()
    expect(reopened).toHaveBeenCalledOnce()
  })

  it('keeps independent anchors for nested surfaces', async () => {
    const outer = scene()
    const inner = scene()
    click(outer.opener)
    outer.motion.enter(outer.panel, vi.fn())
    outer.animations[0]!.finish()
    await flushPromises()
    outer.panel.append(inner.opener)
    fixedRect(inner.opener, new DOMRect(140, 150, 80, 30))
    click(inner.opener)
    inner.motion.enter(inner.panel, vi.fn())
    expect(inner.animations[0]!.frames[0]!.transform).toBe('translate(40px, 50px) scale(0.2, 0.1)')
    outer.motion.leave(outer.panel, vi.fn())
    expect(outer.animations[1]!.frames[1]!.transform).toBe('translate(350px, 10px) scale(0.05, 0.06666666666666667)')
  })

  it('has a completion deadline and cancels all work on disposal', () => {
    vi.useFakeTimers()
    const s = scene()
    click(s.opener)
    s.motion.enter(s.panel, vi.fn())
    const closed = vi.fn()
    s.motion.leave(s.panel, closed)
    vi.advanceTimersByTime(RESTORE_DURATION + 80)
    expect(closed).toHaveBeenCalledOnce()
    s.motion.enter(s.panel, vi.fn())
    s.motion.dispose()
    expect(vi.getTimerCount()).toBe(0)
  })

  it('honors reduced motion and unavailable anchors without waiting', () => {
    const s = scene()
    vi.spyOn(window, 'matchMedia').mockReturnValue({ matches: true } as MediaQueryList)
    click(s.opener)
    const entered = vi.fn()
    s.motion.enter(s.panel, entered)
    s.motion.leave(s.panel, vi.fn())
    expect(entered).toHaveBeenCalledOnce()
    expect(s.animations).toHaveLength(0)
  })

  it('does not reuse unrelated focus or an old pointer trigger after keyboard navigation', () => {
    const s = scene()
    click(s.opener)
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }))
    const done = vi.fn()
    s.motion.enter(s.panel, done)
    expect(done).toHaveBeenCalledOnce()
    expect(s.animations).toHaveLength(0)
  })

  it('keeps a native modal open through shrink and cancels a stale close when reopened', async () => {
    const s = scene('dialog')
    const dialog = s.panel as HTMLDialogElement
    dialog.showModal = () => {
      dialog.open = true
    }
    dialog.close = () => {
      dialog.open = false
    }
    const motion = createDialogMotion(() => dialog)
    cleanup.push(motion.dispose)
    click(s.opener)
    motion.open()
    s.animations[0]!.finish()
    await flushPromises()
    click(s.close)
    const closing = motion.close()
    expect(dialog.open).toBe(true)
    motion.open()
    expect(await closing).toBe(false)
    s.animations[1]!.finish()
    s.animations[2]!.finish()
    await flushPromises()
    expect(dialog.open).toBe(true)
    const finalClose = motion.close()
    s.animations[3]!.finish()
    expect(await finalClose).toBe(true)
    expect(dialog.open).toBe(false)
  })
})
