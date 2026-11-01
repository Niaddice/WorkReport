import { onBeforeUnmount, onMounted, ref } from 'vue'
import type { ModuleId, ModuleType } from '../modules/types'
import { normalizeRailHoldMs } from './railHoldSettings'

interface Press {
  type: ModuleType
  id: ModuleId
  pointerId: number
  button: HTMLButtonElement
  started: boolean
}

export function useRailHoldClose(options: {
  blocked: () => boolean
  resolve: (type: ModuleType) => ModuleId | undefined
  delay: () => number
  duration: () => number
  close: (id: ModuleId) => void
}) {
  const holding = ref<ModuleType | null>(null)
  const progress = ref<number | null>(null)
  const position = ref({ x: 0, y: 0 })
  const label = ref('')
  let press: Press | null = null
  let suppressedClick: HTMLButtonElement | null = null
  let delayTimer: ReturnType<typeof setTimeout> | undefined
  let completionTimer: ReturnType<typeof setTimeout> | undefined
  let frame: number | undefined

  function cancel(suppress = true) {
    if (press && (suppress || press.started)) suppressedClick = press.button
    clearTimeout(delayTimer)
    clearTimeout(completionTimer)
    if (frame !== undefined) cancelAnimationFrame(frame)
    delayTimer = undefined
    completionTimer = undefined
    frame = undefined
    press = null
    holding.value = null
    progress.value = null
  }

  function valid(value: Press): boolean {
    return press === value && value.button.isConnected && !options.blocked() && options.resolve(value.type) === value.id
  }

  function start(event: PointerEvent, type: ModuleType, name: string) {
    if (event.button !== 0 || event.pointerType !== 'mouse') return
    cancel()
    // A new physical press starts a new click sequence.
    suppressedClick = null
    const id = options.resolve(type)
    if (!id || options.blocked()) return
    const current: Press = {
      type,
      id,
      pointerId: event.pointerId,
      button: event.currentTarget as HTMLButtonElement,
      started: false,
    }
    press = current
    holding.value = type
    position.value = { x: event.clientX, y: event.clientY }
    label.value = name
    delayTimer = setTimeout(() => {
      if (!valid(current)) return cancel()
      current.started = true
      progress.value = 0
      const startedAt = performance.now()
      const duration = normalizeRailHoldMs(options.duration())
      const tick = () => {
        if (!valid(current)) return cancel()
        progress.value = Math.min(1, (performance.now() - startedAt) / duration)
        frame = requestAnimationFrame(tick)
      }
      frame = requestAnimationFrame(tick)
      completionTimer = setTimeout(() => {
        if (!valid(current)) return cancel()
        cancel()
        options.close(current.id)
      }, duration)
    }, normalizeRailHoldMs(options.delay()))
  }

  function move(event: PointerEvent) {
    if (!press || event.pointerId !== press.pointerId) return
    if (!(event.buttons & 1)) return cancel()
    position.value = { x: event.clientX, y: event.clientY }
  }

  function release(event: PointerEvent) {
    if (press && event.pointerId === press.pointerId) cancel(false)
  }

  function shouldSuppressClick(event: MouseEvent): boolean {
    const suppress = suppressedClick === event.currentTarget && event.detail > 0
    suppressedClick = null
    if (suppress) {
      event.preventDefault()
      event.stopPropagation()
    }
    return suppress
  }

  const cancelGesture = () => cancel()
  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') cancel()
  }
  onMounted(() => {
    window.addEventListener('pointerup', release, true)
    window.addEventListener('pointercancel', cancelGesture, true)
    window.addEventListener('blur', cancelGesture)
    window.addEventListener('scroll', cancelGesture, true)
    window.addEventListener('keydown', onKeydown, true)
  })
  onBeforeUnmount(() => {
    cancel()
    window.removeEventListener('pointerup', release, true)
    window.removeEventListener('pointercancel', cancelGesture, true)
    window.removeEventListener('blur', cancelGesture)
    window.removeEventListener('scroll', cancelGesture, true)
    window.removeEventListener('keydown', onKeydown, true)
  })
  return { holding, progress, position, label, start, move, cancel, shouldSuppressClick }
}
