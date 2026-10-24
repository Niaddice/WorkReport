import { computed, onBeforeUnmount, ref, watch, type Ref } from 'vue'

const CALENDAR_MIN_WIDTH = 420
const EDITOR_MIN_WIDTH = 340

/**
 * 首页日历与编辑区之间的分隔条：拖动、键盘微调、双击复位，
 * 并在窗口尺寸变化时把日历宽度夹回合法区间。
 */
export function useSplitter(layoutEl: Ref<HTMLElement | undefined>) {
  const calendarWidth = ref<number | null>(null)
  const resizing = ref(false)
  let observer: ResizeObserver | undefined
  let startX = 0
  let startWidth = 0

  const layoutStyle = computed(() =>
    calendarWidth.value === null ? undefined : { '--calendar-width': `${calendarWidth.value}px` },
  )

  function splitterPixelWidth() {
    return layoutEl.value?.querySelector<HTMLElement>('.journal-splitter')?.offsetWidth || 18
  }

  function calendarPanelWidth() {
    return layoutEl.value?.querySelector<HTMLElement>('.calendar-panel')?.offsetWidth ?? 0
  }

  function isStackedLayout() {
    const splitter = layoutEl.value?.querySelector<HTMLElement>('.journal-splitter')
    return !splitter || getComputedStyle(splitter).display === 'none'
  }

  function clampCalendarWidth(width: number) {
    const layout = layoutEl.value
    if (!layout || !layout.clientWidth) return width
    const max = Math.max(CALENDAR_MIN_WIDTH, layout.clientWidth - splitterPixelWidth() - EDITOR_MIN_WIDTH)
    return Math.min(Math.max(width, CALENDAR_MIN_WIDTH), max)
  }

  function setCalendarWidth(width: number) {
    if (!Number.isFinite(width)) return
    const next = Math.round(clampCalendarWidth(width))
    if (next !== calendarWidth.value) calendarWidth.value = next
  }

  function onSplitterMove(event: PointerEvent) {
    if (!resizing.value) return
    setCalendarWidth(startWidth + event.clientX - startX)
  }

  function stopSplitterResize() {
    resizing.value = false
    document.body.classList.remove('is-resizing')
    window.removeEventListener('pointermove', onSplitterMove)
    window.removeEventListener('pointerup', stopSplitterResize)
    window.removeEventListener('pointercancel', stopSplitterResize)
    window.removeEventListener('blur', stopSplitterResize)
  }

  function startSplitterResize(event: PointerEvent) {
    if (event.button !== 0 || !layoutEl.value || isStackedLayout()) return
    const width = calendarPanelWidth()
    if (!width) return
    event.preventDefault()
    resizing.value = true
    startX = event.clientX
    startWidth = width
    document.body.classList.add('is-resizing')
    window.addEventListener('pointermove', onSplitterMove)
    window.addEventListener('pointerup', stopSplitterResize)
    window.addEventListener('pointercancel', stopSplitterResize)
    window.addEventListener('blur', stopSplitterResize)
  }

  function onSplitterKeydown(event: KeyboardEvent) {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return
    const width = calendarPanelWidth()
    if (!width) return
    event.preventDefault()
    const step = event.shiftKey ? 48 : 16
    setCalendarWidth(width + (event.key === 'ArrowLeft' ? -step : step))
  }

  function resetSplitterWidth() {
    calendarWidth.value = null
  }

  watch(
    layoutEl,
    (element) => {
      observer?.disconnect()
      if (!element || typeof ResizeObserver === 'undefined') return
      observer = new ResizeObserver(() => {
        if (calendarWidth.value === null || isStackedLayout()) return
        setCalendarWidth(calendarWidth.value)
      })
      observer.observe(element)
    },
    { flush: 'post' },
  )

  onBeforeUnmount(() => {
    stopSplitterResize()
    observer?.disconnect()
  })

  return {
    calendarWidth,
    resizing,
    layoutStyle,
    startSplitterResize,
    onSplitterKeydown,
    resetSplitterWidth,
    stopSplitterResize,
  }
}
