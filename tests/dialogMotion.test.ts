import { afterEach, describe, expect, it, vi } from 'vitest'
import { createDialogMotion } from '../src/lib/dialogMotion'

function mockDialog({ reduced = false, duration = '0.18s', animations = [] as { finished: Promise<unknown> }[] } = {}) {
  const classes = new Set<string>()
  const dialog = {
    open: false,
    classList: {
      add: (name: string) => classes.add(name),
      remove: (name: string) => classes.delete(name),
      contains: (name: string) => classes.has(name),
    },
    ownerDocument: {
      defaultView: {
        matchMedia: () => ({ matches: reduced }),
        getComputedStyle: () => ({ animationDuration: duration, animationDelay: '0s' }),
      },
    },
    showModal: vi.fn(() => {
      dialog.open = true
    }),
    close: vi.fn(() => {
      dialog.open = false
    }),
    focus: vi.fn(),
    getAnimations: () => animations,
  }
  return { element: dialog as unknown as HTMLDialogElement, dialog }
}

afterEach(() => vi.useRealTimers())

describe('native dialog motion', () => {
  it('opens and focuses a modal, retaining the native focus restoration lifecycle', () => {
    const { element, dialog } = mockDialog()
    const motion = createDialogMotion(() => element)
    motion.open()
    motion.open()
    expect(dialog.showModal).toHaveBeenCalledTimes(1)
    expect(dialog.focus).toHaveBeenCalledWith({ preventScroll: true })
    motion.dispose()
    expect(dialog.close).toHaveBeenCalledTimes(1)
  })

  it('keeps the modal open until its exit animation finishes', async () => {
    let finishAnimation!: () => void
    const finished = new Promise<void>((resolve) => {
      finishAnimation = resolve
    })
    const { element, dialog } = mockDialog({ animations: [{ finished }] })
    const motion = createDialogMotion(() => element)
    motion.open()
    const closing = motion.close()
    const settled = vi.fn()
    void closing.then(settled)
    await Promise.resolve()
    expect(settled).not.toHaveBeenCalled()
    expect(dialog.open).toBe(true)
    expect(element.classList.contains('is-closing')).toBe(true)
    finishAnimation()
    expect(await closing).toBe(true)
    expect(dialog.open).toBe(false)
    expect(element.classList.contains('is-closing')).toBe(false)
  })

  it('does not close a quickly reopened modal when the previous animation finishes', async () => {
    vi.useFakeTimers()
    let finishAnimation!: () => void
    const finished = new Promise<void>((resolve) => {
      finishAnimation = resolve
    })
    const { element, dialog } = mockDialog({ animations: [{ finished }] })
    const motion = createDialogMotion(() => element)
    motion.open()
    const closing = motion.close()
    motion.open()
    expect(await closing).toBe(false)
    finishAnimation()
    await Promise.resolve()
    await Promise.resolve()
    await vi.runAllTimersAsync()
    expect(dialog.open).toBe(true)
    expect(dialog.close).not.toHaveBeenCalled()
    expect(element.classList.contains('is-closing')).toBe(false)
    motion.dispose()
  })

  it('shares the pending close promise and has a deadline when animation completion is unavailable', async () => {
    vi.useFakeTimers()
    const { element, dialog } = mockDialog({ duration: '180ms' })
    const motion = createDialogMotion(() => element)
    motion.open()
    const closing = motion.close()
    expect(motion.close()).toBe(closing)
    vi.advanceTimersByTime(229)
    expect(dialog.open).toBe(true)
    vi.advanceTimersByTime(1)
    expect(await closing).toBe(true)
    expect(dialog.close).toHaveBeenCalledTimes(1)
  })

  it.each([{ reduced: true }, { duration: '0s' }])(
    'closes immediately when motion is unavailable: %j',
    async (options) => {
      const { element, dialog } = mockDialog(options)
      const motion = createDialogMotion(() => element)
      motion.open()
      expect(await motion.close()).toBe(true)
      expect(dialog.open).toBe(false)
      expect(element.classList.contains('is-closing')).toBe(false)
    },
  )

  it('cancels the pending close and removes the modal from the top layer when disposed', async () => {
    vi.useFakeTimers()
    const { element, dialog } = mockDialog()
    const motion = createDialogMotion(() => element)
    motion.open()
    const closing = motion.close()
    motion.dispose()
    expect(await closing).toBe(false)
    vi.runAllTimers()
    expect(dialog.open).toBe(false)
    expect(dialog.close).toHaveBeenCalledTimes(1)
    expect(element.classList.contains('is-closing')).toBe(false)
  })

  it('resolves immediately if the modal is already closed', async () => {
    const { element, dialog } = mockDialog()
    const motion = createDialogMotion(() => element)
    expect(await motion.close()).toBe(true)
    expect(dialog.close).not.toHaveBeenCalled()
  })
})
