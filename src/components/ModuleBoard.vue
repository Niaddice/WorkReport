<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import ModulePanel from './ModulePanel.vue'
import { moduleById } from '../modules/registry'
import {
  clampSplitRatio,
  dropPreviewRect,
  dropSideForPoint,
  findDropSlot,
  flattenLayout,
  layoutGeometry,
  resizeLayoutLocally,
} from '../modules/layout'
import type { DropSide, LayoutGutter, LayoutNode, LayoutSlot, ModuleId, Rect } from '../modules/types'

const props = defineProps<{
  layout: LayoutNode
  /** 工作空间变化时关闭最大化浮层，避免把旧窗口状态带到新空间。 */
  workspacePath?: string
}>()
const emit = defineEmits<{
  remove: [id: ModuleId]
  split: [dragged: ModuleId, target: ModuleId, side: DropSide]
  insert: [dragged: ModuleId, path: number[]]
  resize: [path: number[], ratio: number]
}>()

/** 与 style.css 中 --app-gap 的默认值保持一致；几何计算需要像素值。 */
const GAP = 8
const board = ref<HTMLElement>()
const boardSize = ref({ width: 0, height: 0 })
/** 拖动分栏间隙时只在本组件内预览新布局，松手后才写回。 */
const previewLayout = ref<LayoutNode | null>(null)
const activeLayout = computed(() => previewLayout.value ?? props.layout)
const items = computed(() => flattenLayout(activeLayout.value))
const geometry = computed(() => layoutGeometry(activeLayout.value, boardSize.value.width, boardSize.value.height, GAP))
const slots = computed(() => geometry.value.slots)
const gutters = computed(() => geometry.value.gutters)
const layoutSignature = computed(() => JSON.stringify(props.layout))
const announcement = ref('')

interface DragState {
  id: ModuleId
  pointerId: number
  title: string
}
type DropTarget = { kind: 'module'; slot: LayoutSlot; side: DropSide } | { kind: 'gutter'; gutter: LayoutGutter }
interface GutterDrag {
  pointerId: number
  path: number[]
  direction: 'row' | 'column'
  parent: Rect
  ratio: number
}
const drag = ref<DragState | null>(null)
const gutterDrag = ref<GutterDrag | null>(null)
/** 浮层最大化的模块 id；只影响当前视图，不写入工作空间布局。 */
const maximized = ref<ModuleId | null>(null)
const pointer = ref({ x: 0, y: 0 })
const dropTarget = ref<DropTarget | null>(null)
let pending: { id: ModuleId; pointerId: number; x: number; y: number } | null = null

const draggedTitle = computed(() => drag.value?.title ?? '')
const moduleDropTarget = computed(() => (dropTarget.value?.kind === 'module' ? dropTarget.value : null))
const gutterDropTarget = computed(() => (dropTarget.value?.kind === 'gutter' ? dropTarget.value : null))
const dropPreviewStyle = computed(() => {
  const target = moduleDropTarget.value
  if (!target) return { display: 'none' }
  const rect = dropPreviewRect(target.slot, target.side)
  return {
    left: `${rect.left}px`,
    top: `${rect.top}px`,
    width: `${rect.width}px`,
    height: `${rect.height}px`,
  }
})
const dropLabel = computed(() => {
  const side = moduleDropTarget.value?.side
  if (side === 'left') return '左侧分栏'
  if (side === 'right') return '右侧分栏'
  if (side === 'top') return '上方分栏'
  if (side === 'bottom') return '下方分栏'
  return ''
})
const insertPreviewStyle = computed(() => {
  const target = gutterDropTarget.value
  if (!target) return { display: 'none' }
  const gutter = target.gutter
  if (gutter.direction === 'row') {
    return {
      left: `${gutter.left + gutter.width / 2 - 9}px`,
      top: `${gutter.top}px`,
      width: '18px',
      height: `${gutter.height}px`,
    }
  }
  return {
    left: `${gutter.left}px`,
    top: `${gutter.top + gutter.height / 2 - 9}px`,
    width: `${gutter.width}px`,
    height: '18px',
  }
})
const ghostStyle = computed(() => ({
  left: `${pointer.value.x + 16}px`,
  top: `${pointer.value.y + 12}px`,
}))

function panelElement(id: ModuleId): HTMLElement | null {
  return board.value?.querySelector<HTMLElement>(`[data-module-id="${id}"]`) ?? null
}
function backdropElement(): HTMLElement | null {
  return board.value?.querySelector<HTMLElement>('.module-maximize-backdrop') ?? null
}
function prefersReducedMotion() {
  return window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false
}
function slotStyle(slot: LayoutSlot) {
  return {
    left: `${slot.left}px`,
    top: `${slot.top}px`,
    width: `${slot.width}px`,
    height: `${slot.height}px`,
  }
}
function gutterStyle(gutter: LayoutGutter) {
  return {
    left: `${gutter.left}px`,
    top: `${gutter.top}px`,
    width: `${gutter.width}px`,
    height: `${gutter.height}px`,
  }
}
function gutterLabel(gutter: LayoutGutter) {
  return gutter.direction === 'row' ? '左右拖动调整两侧模块宽度' : '上下拖动调整两侧模块高度'
}

const layoutAnimations = new Set<Animation>()
function capturePanelRects(): Map<ModuleId, DOMRect> {
  const rects = new Map<ModuleId, DOMRect>()
  for (const placement of items.value) {
    const panel = panelElement(placement.id)
    if (panel) rects.set(placement.id, panel.getBoundingClientRect())
  }
  return rects
}
function cancelLayoutAnimations() {
  for (const animation of layoutAnimations) animation.cancel()
  layoutAnimations.clear()
}
/** 布局树变化（分栏、增删、比例提交）后让模块从旧矩形平滑过渡到新矩形。 */
async function animateLayoutChange(previous: Map<ModuleId, DOMRect>, immediate = false, duration = 280) {
  if (!immediate) await nextTick()
  cancelLayoutAnimations()
  if (!previous.size || prefersReducedMotion()) return
  for (const [id, oldRect] of previous) {
    const panel = panelElement(id)
    if (!panel || typeof panel.animate !== 'function') continue
    const nextRect = panel.getBoundingClientRect()
    if (!oldRect.width || !oldRect.height || !nextRect.width || !nextRect.height) continue
    if (
      Math.abs(oldRect.width - nextRect.width) < 1 &&
      Math.abs(oldRect.height - nextRect.height) < 1 &&
      Math.abs(oldRect.left - nextRect.left) < 1 &&
      Math.abs(oldRect.top - nextRect.top) < 1
    )
      continue
    const animation = panel.animate(
      [
        {
          transformOrigin: 'top left',
          transform: `translate(${oldRect.left - nextRect.left}px, ${oldRect.top - nextRect.top}px) scale(${
            oldRect.width / nextRect.width
          }, ${oldRect.height / nextRect.height})`,
        },
        { transformOrigin: 'top left', transform: 'translate(0px, 0px) scale(1, 1)' },
      ],
      { duration, easing: 'cubic-bezier(0.2, 0.8, 0.2, 1)', fill: 'none' },
    )
    layoutAnimations.add(animation)
    animation.finished.catch(() => {}).finally(() => layoutAnimations.delete(animation))
  }
}
/** 右侧功能栏添加/定位模块时，把焦点移到对应模块的拖动手柄。 */
function revealModule(id: ModuleId) {
  panelElement(id)?.querySelector<HTMLElement>('.module-drag-handle')?.focus({ preventScroll: true })
}
defineExpose({ reveal: revealModule })

/** 窗口尺寸变化时，从当前画面矩形平滑缩放到新的布局矩形。 */
function animateBoardResize(previous: Map<ModuleId, DOMRect>) {
  void animateLayoutChange(previous, false, 160)
}

watch(
  layoutSignature,
  () => {
    // 父级布局已经更新，结束拖动间隙时的本地预览，避免下一帧闪回旧布局。
    previewLayout.value = null
    // flush: 'pre' 时 DOM 仍是旧布局，先记录旧外框，再等新布局渲染后做 FLIP。
    void animateLayoutChange(capturePanelRects())
  },
  { flush: 'pre' },
)

// ===== 模块拖放：拖到其他模块上生成左右/上下分栏 =====
function beginDrag(id: ModuleId, event: PointerEvent) {
  const definition = moduleById(id)
  drag.value = { id, pointerId: event.pointerId, title: definition.title }
  pointer.value = { x: event.clientX, y: event.clientY }
  dropTarget.value = null
  document.body.classList.add('is-module-dragging')
  announcement.value = `正在移动「${definition.title}」模块，放到其他模块上可分栏，Esc 取消。`
}
function finishDrag() {
  drag.value = null
  pending = null
  dropTarget.value = null
  document.body.classList.remove('is-module-dragging')
}
function cancelDrag(event?: PointerEvent) {
  if (event && drag.value && event.pointerId !== drag.value.pointerId) return
  if (event && !drag.value && pending && event.pointerId !== pending.pointerId) return
  if (!drag.value) {
    pending = null
    return
  }
  const title = drag.value.title
  finishDrag()
  announcement.value = `已取消移动「${title}」模块。`
  if (gutterDrag.value) cancelGutterDrag()
}
function updateDropTarget(event: PointerEvent) {
  const state = drag.value
  const element = board.value
  if (!state || !element) return
  const rect = element.getBoundingClientRect()
  const x = event.clientX - rect.left
  const y = event.clientY - rect.top
  // 滑块本身只有 8px 宽，向两侧各扩 3px 方便把模块塞进两个模块之间。
  const gutter = gutters.value.find(
    (item) =>
      x >= item.left - 3 && x <= item.left + item.width + 3 && y >= item.top - 3 && y <= item.top + item.height + 3,
  )
  if (gutter) {
    dropTarget.value = { kind: 'gutter', gutter }
    return
  }
  const slot = findDropSlot(slots.value, x, y, state.id)
  dropTarget.value = slot ? { kind: 'module', slot, side: dropSideForPoint(slot, x, y) } : null
}
function onDragMove(event: PointerEvent) {
  if (gutterDrag.value) return
  if (pending) {
    if (event.pointerId !== pending.pointerId) return
    if (event.pointerType === 'mouse' && event.buttons === 0) {
      pending = null
      return
    }
    if (Math.hypot(event.clientX - pending.x, event.clientY - pending.y) < 4) return
    beginDrag(pending.id, event)
    if (!drag.value) {
      pending = null
      return
    }
    pending = null
  }
  const state = drag.value
  if (!state) return
  if (event.pointerId !== state.pointerId) return
  if (event.pointerType === 'mouse' && event.buttons === 0) {
    finishDrag()
    return
  }
  pointer.value = { x: event.clientX, y: event.clientY }
  updateDropTarget(event)
}
function onDragEnd(event?: PointerEvent) {
  if (gutterDrag.value) return
  if (pending) {
    if (!event || event.pointerId === pending.pointerId) pending = null
    return
  }
  const state = drag.value
  if (!state) return
  if (event && event.pointerId !== state.pointerId) return
  const element = board.value
  const rect = element?.getBoundingClientRect()
  const outside =
    !rect ||
    pointer.value.x < rect.left - 60 ||
    pointer.value.x > rect.right + 60 ||
    pointer.value.y < rect.top - 60 ||
    pointer.value.y > rect.bottom + 60
  const target = outside ? null : dropTarget.value
  finishDrag()
  if (outside) {
    announcement.value = `已取消移动「${state.title}」模块。`
    return
  }
  if (!target || (target.kind === 'module' && target.slot.id === state.id)) {
    announcement.value = '未放入其他模块，布局未改变。'
    return
  }
  if (target.kind === 'gutter') {
    emit('insert', state.id, [...target.gutter.path])
    announcement.value = `已将「${state.title}」插入到两个模块之间。`
    return
  }
  emit('split', state.id, target.slot.id, target.side)
  const targetTitle = moduleById(target.slot.id).title
  const label =
    target.side === 'left' ? '左侧' : target.side === 'right' ? '右侧' : target.side === 'top' ? '上方' : '下方'
  announcement.value = `已将「${state.title}」放到「${targetTitle}」${label}，形成分栏。`
}
function startDrag(id: ModuleId, event: PointerEvent) {
  if (drag.value || gutterDrag.value) return
  event.preventDefault()
  const target = event.target
  if (target instanceof HTMLElement && typeof target.setPointerCapture === 'function') {
    try {
      target.setPointerCapture(event.pointerId)
    } catch {
      /* 某些 WebView/测试环境不支持指针捕获，仍可依赖 window 事件。 */
    }
  }
  pending = { id, pointerId: event.pointerId, x: event.clientX, y: event.clientY }
}

// ===== 分栏间隙拖动：调整左右/上下比例 =====
function startGutterDrag(event: PointerEvent, gutter: LayoutGutter) {
  if (drag.value || gutterDrag.value) return
  event.preventDefault()
  event.stopPropagation()
  if (event.target instanceof HTMLElement) {
    try {
      event.target.setPointerCapture(event.pointerId)
    } catch {
      /* 忽略不支持指针捕获的环境 */
    }
  }
  previewLayout.value = null
  gutterDrag.value = {
    pointerId: event.pointerId,
    path: [...gutter.path],
    direction: gutter.direction,
    parent: { ...gutter.parent },
    ratio: gutter.ratio,
  }
  document.body.classList.add('is-module-resizing')
  announcement.value = gutter.direction === 'row' ? '正在调整左右分栏比例。' : '正在调整上下分栏比例。'
}
function onGutterMove(event: PointerEvent) {
  const state = gutterDrag.value
  const element = board.value
  if (!state || !element || event.pointerId !== state.pointerId) return
  if (event.pointerType === 'mouse' && event.buttons === 0) {
    finishGutterDrag()
    return
  }
  const rect = element.getBoundingClientRect()
  const clientPosition = state.direction === 'row' ? event.clientX - rect.left : event.clientY - rect.top
  const origin = state.direction === 'row' ? state.parent.left : state.parent.top
  const span = state.direction === 'row' ? state.parent.width - GAP : state.parent.height - GAP
  if (span <= 0) return
  const ratio = clampSplitRatio((clientPosition - origin) / span)
  if (Math.abs(ratio - state.ratio) < 1e-4) return
  state.ratio = ratio
  previewLayout.value = resizeLayoutLocally(props.layout, state.path, ratio)
}
function finishGutterDrag() {
  const state = gutterDrag.value
  if (!state) return
  gutterDrag.value = null
  document.body.classList.remove('is-module-resizing')
  emit('resize', [...state.path], state.ratio)
  // 等父级应用新布局后再清掉预览，避免中间帧回跳。
  void nextTick(() => {
    previewLayout.value = null
  })
  announcement.value = '已调整分栏比例。'
}
function cancelGutterDrag() {
  if (!gutterDrag.value) return
  gutterDrag.value = null
  previewLayout.value = null
  document.body.classList.remove('is-module-resizing')
  announcement.value = '已取消调整分栏比例。'
}
function onGutterKeydown(event: KeyboardEvent, gutter: LayoutGutter) {
  const decrease =
    (gutter.direction === 'row' && event.key === 'ArrowLeft') ||
    (gutter.direction === 'column' && event.key === 'ArrowUp')
  const increase =
    (gutter.direction === 'row' && event.key === 'ArrowRight') ||
    (gutter.direction === 'column' && event.key === 'ArrowDown')
  if (!decrease && !increase) return
  event.preventDefault()
  emit('resize', [...gutter.path], clampSplitRatio(gutter.ratio + (increase ? 0.03 : -0.03)))
}

// ===== 浮层最大化 =====
const MAXIMIZE_DURATION = 360
const RESTORE_DURATION = 280
const MAXIMIZE_EASING = 'cubic-bezier(0.2, 0.8, 0.2, 1)'
const RESTORE_EASING = 'cubic-bezier(0.4, 0, 0.2, 1)'
let maximizeAnimation: Animation | null = null
let restoring = false
function stopMaximizeAnimation() {
  if (!maximizeAnimation) return
  maximizeAnimation.cancel()
  maximizeAnimation = null
}
async function maximizeModule(id: ModuleId) {
  if (drag.value || gutterDrag.value || restoring) return
  const source = panelElement(id)?.getBoundingClientRect()
  stopMaximizeAnimation()
  maximized.value = id
  document.body.classList.add('is-module-maximized')
  announcement.value = `已放大「${moduleById(id).title}」模块，按 Esc 或点击遮罩还原。`
  await nextTick()
  const panel = panelElement(id)
  const backdrop = backdropElement()
  if (!panel || !source || prefersReducedMotion() || typeof panel.animate !== 'function') return
  const target = panel.getBoundingClientRect()
  if (!target.width || !target.height) return
  panel.style.transformOrigin = 'top left'
  const animation = panel.animate(
    [
      {
        transform: `translate(${source.left - target.left}px, ${source.top - target.top}px) scale(${
          source.width / target.width
        }, ${source.height / target.height})`,
        opacity: 0.72,
      },
      { transform: 'translate(0px, 0px) scale(1, 1)', opacity: 1 },
    ],
    { duration: MAXIMIZE_DURATION, easing: MAXIMIZE_EASING, fill: 'none' },
  )
  maximizeAnimation = animation
  animation.finished
    .catch(() => {})
    .finally(() => {
      if (maximizeAnimation === animation) maximizeAnimation = null
    })
  backdrop?.animate([{ opacity: 0 }, { opacity: 1 }], {
    duration: MAXIMIZE_DURATION,
    easing: 'ease-out',
    fill: 'none',
  })
}
async function restoreMaximized(immediate = false) {
  const id = maximized.value
  if (!id) return
  if (restoring && !immediate) return
  const title = moduleById(id).title
  const panel = panelElement(id)
  const slot = panel?.closest<HTMLElement>('.module-slot') ?? null
  const backdrop = backdropElement()
  const finish = () => {
    maximized.value = null
    restoring = false
    document.body.classList.remove('is-module-maximized')
    announcement.value = `已还原「${title}」模块大小。`
  }
  if (immediate || !panel || !slot || prefersReducedMotion() || typeof panel.animate !== 'function') {
    stopMaximizeAnimation()
    finish()
    return
  }
  const current = panel.getBoundingClientRect()
  const target = slot.getBoundingClientRect()
  if (!current.width || !current.height || !target.width || !target.height) {
    stopMaximizeAnimation()
    finish()
    return
  }
  restoring = true
  stopMaximizeAnimation()
  panel.style.transformOrigin = 'top left'
  const animation = panel.animate(
    [
      { transform: 'translate(0px, 0px) scale(1, 1)', opacity: 1 },
      {
        transform: `translate(${target.left - current.left}px, ${target.top - current.top}px) scale(${
          target.width / current.width
        }, ${target.height / current.height})`,
        opacity: 0.6,
      },
    ],
    { duration: RESTORE_DURATION, easing: RESTORE_EASING, fill: 'none' },
  )
  maximizeAnimation = animation
  backdrop?.animate([{ opacity: 1 }, { opacity: 0 }], {
    duration: RESTORE_DURATION,
    easing: 'ease-in',
    fill: 'none',
  })
  try {
    await animation.finished
  } catch {
    /* 被新的最大化或卸载取消 */
  }
  if (maximizeAnimation === animation) maximizeAnimation = null
  if (maximized.value === id) finish()
}
function toggleMaximize(id: ModuleId) {
  if (restoring) return
  if (maximized.value === id) void restoreMaximized()
  else void maximizeModule(id)
}
watch(
  () => items.value.map((placement) => placement.id),
  (ids) => {
    if (maximized.value && !ids.includes(maximized.value)) void restoreMaximized(true)
  },
)
watch(
  () => props.workspacePath,
  () => {
    void restoreMaximized(true)
  },
)

function onWindowBlur() {
  cancelDrag()
  cancelGutterDrag()
}
function onWindowKeydown(event: KeyboardEvent) {
  if (event.key !== 'Escape') return
  if (maximized.value) {
    event.preventDefault()
    event.stopImmediatePropagation()
    void restoreMaximized()
    return
  }
  if (!drag.value && !gutterDrag.value) return
  event.preventDefault()
  event.stopImmediatePropagation()
  if (gutterDrag.value) cancelGutterDrag()
  else cancelDrag()
}
function onPointerMove(event: PointerEvent) {
  if (gutterDrag.value) onGutterMove(event)
  else onDragMove(event)
}
function onPointerUp(event: PointerEvent) {
  if (gutterDrag.value) {
    if (event.pointerId === gutterDrag.value.pointerId) finishGutterDrag()
    return
  }
  onDragEnd(event)
}
function onPointerCancel(event: PointerEvent) {
  if (gutterDrag.value) {
    if (event.pointerId === gutterDrag.value.pointerId) cancelGutterDrag()
    return
  }
  cancelDrag(event)
}

let resizeObserver: ResizeObserver | undefined
let hasMeasuredBoard = false
function measureBoard() {
  const width = board.value?.clientWidth ?? 0
  const height = board.value?.clientHeight ?? 0
  // 设置页用 v-show 隐藏首页时尺寸为 0，保留上一次有效尺寸避免布局跳回默认。
  if (!width || !height) return
  const previousSize = boardSize.value
  if (previousSize.width === width && previousSize.height === height) return
  // 绝对定位的模块不会随窗口自动重排，先记下当前屏幕矩形，再更新几何并做 FLIP，
  // 这样连续拖动窗口、最大化/还原时模块会平滑跟随缩放，而不是逐帧硬跳。
  const previous = hasMeasuredBoard ? capturePanelRects() : new Map<ModuleId, DOMRect>()
  hasMeasuredBoard = true
  boardSize.value = { width, height }
  if (previous.size) animateBoardResize(previous)
}
onMounted(() => {
  measureBoard()
  if (typeof ResizeObserver !== 'undefined' && board.value) {
    resizeObserver = new ResizeObserver(measureBoard)
    resizeObserver.observe(board.value)
  }
  window.addEventListener('pointermove', onPointerMove)
  window.addEventListener('pointerup', onPointerUp)
  window.addEventListener('pointercancel', onPointerCancel)
  window.addEventListener('blur', onWindowBlur)
  window.addEventListener('keydown', onWindowKeydown, true)
})
onBeforeUnmount(() => {
  cancelLayoutAnimations()
  resizeObserver?.disconnect()
  window.removeEventListener('pointermove', onPointerMove)
  window.removeEventListener('pointerup', onPointerUp)
  window.removeEventListener('pointercancel', onPointerCancel)
  window.removeEventListener('blur', onWindowBlur)
  window.removeEventListener('keydown', onWindowKeydown, true)
  stopMaximizeAnimation()
  document.body.classList.remove('is-module-dragging')
  document.body.classList.remove('is-module-resizing')
  document.body.classList.remove('is-module-maximized')
})
</script>

<template>
  <div
    ref="board"
    class="module-board"
    :class="{ 'is-dragging': !!drag, 'is-resizing': !!gutterDrag }"
    :data-module-count="items.length"
  >
    <div v-if="maximized" class="module-maximize-backdrop" aria-hidden="true" @click="restoreMaximized()"></div>
    <div
      v-for="slot in slots"
      :key="slot.id"
      class="module-slot"
      :inert="maximized !== null && maximized !== slot.id"
      :style="slotStyle(slot)"
    >
      <ModulePanel
        :id="slot.id"
        :title="moduleById(slot.id).title"
        :icon="moduleById(slot.id).icon"
        :can-remove="items.length > 1"
        :dragging="drag?.id === slot.id"
        :maximized="maximized === slot.id"
        @drag-start="startDrag(slot.id, $event)"
        @remove="emit('remove', slot.id)"
        @toggle-maximize="toggleMaximize(slot.id)"
      >
        <component :is="moduleById(slot.id).component" />
      </ModulePanel>
    </div>
    <div
      v-for="gutter in gutters"
      :key="`gutter-${gutter.path.join('-')}`"
      class="module-gutter"
      :class="`is-${gutter.direction}`"
      :style="gutterStyle(gutter)"
      role="separator"
      :aria-orientation="gutter.direction === 'row' ? 'vertical' : 'horizontal'"
      :aria-label="gutterLabel(gutter)"
      aria-valuemin="12"
      aria-valuemax="88"
      :aria-valuenow="Math.round(gutter.ratio * 100)"
      :aria-valuetext="
        gutter.direction === 'row'
          ? '左侧模块占比 ' + Math.round(gutter.ratio * 100) + '%'
          : '上方模块占比 ' + Math.round(gutter.ratio * 100) + '%'
      "
      tabindex="0"
      @pointerdown="startGutterDrag($event, gutter)"
      @keydown="onGutterKeydown($event, gutter)"
    ></div>
    <div
      v-if="moduleDropTarget"
      class="module-drop-preview"
      :data-side="moduleDropTarget.side"
      :style="dropPreviewStyle"
    >
      <span>{{ dropLabel }}</span>
    </div>
    <div
      v-if="gutterDropTarget"
      class="module-insert-preview"
      :class="`is-${gutterDropTarget.gutter.direction}`"
      :style="insertPreviewStyle"
    >
      <span>插入此处</span>
    </div>
    <div v-if="drag" class="module-drag-ghost" :style="ghostStyle" aria-hidden="true">
      {{ draggedTitle }}
    </div>
    <p class="sr-only" role="status" aria-live="polite">{{ announcement }}</p>
  </div>
</template>
