<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch, useId } from 'vue'
import { PhBrowsers, PhCornersIn, PhCornersOut, PhX } from '@phosphor-icons/vue'
import { createSurfaceMotion } from '../lib/surfaceMotion'
import {
  placePreviewBesideEditor,
  PREVIEW_MIN_HEIGHT,
  PREVIEW_MIN_WIDTH,
  type PreviewWindowRect,
} from '../lib/previewWindowPlacement'

const props = withDefaults(
  defineProps<{ visible: boolean; topInset?: number; anchorRect?: () => PreviewWindowRect | null }>(),
  { topInset: 0 },
)
const emit = defineEmits<{ close: [] }>()
type Rect = PreviewWindowRect
const titleId = useId()
const motion = createSurfaceMotion()
const viewport = ref({ width: window.innerWidth, height: window.innerHeight })
const normal = ref<Rect | null>(null)
let preferredSize = { width: 520, height: 560 }
const maximized = ref(false)
const geometryMotion = ref(false)
const bounds = computed<Rect>(() => ({
  x: 8,
  y: props.topInset + 8,
  width: Math.max(0, viewport.value.width - 16),
  height: Math.max(0, viewport.value.height - props.topInset - 16),
}))
const rectangle = computed(() => (maximized.value ? bounds.value : (normal.value ?? bounds.value)))
const windowStyle = computed(() => ({
  left: `${rectangle.value.x}px`,
  top: `${rectangle.value.y}px`,
  width: `${rectangle.value.width}px`,
  height: `${rectangle.value.height}px`,
}))
let interaction: {
  kind: 'move' | 'resize'
  pointerId: number
  x: number
  y: number
  rect: Rect
  capture?: HTMLElement
} | null = null

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(maximum, Math.max(minimum, value))
}
function fit(rect: Rect): Rect {
  const area = bounds.value
  const width = clamp(rect.width, Math.min(PREVIEW_MIN_WIDTH, area.width), area.width)
  const height = clamp(rect.height, Math.min(PREVIEW_MIN_HEIGHT, area.height), area.height)
  return {
    width,
    height,
    x: clamp(rect.x, area.x, area.x + area.width - width),
    y: clamp(rect.y, area.y, area.y + area.height - height),
  }
}
function stopInteraction() {
  const previous = interaction
  interaction = null
  document.body.classList.remove('is-preview-window-moving', 'is-preview-window-resizing')
  window.removeEventListener('pointermove', onPointerMove)
  window.removeEventListener('pointerup', stopInteraction)
  window.removeEventListener('pointercancel', stopInteraction)
  window.removeEventListener('lostpointercapture', stopInteraction)
  window.removeEventListener('blur', stopInteraction)
  if (previous?.capture?.hasPointerCapture?.(previous.pointerId))
    previous.capture.releasePointerCapture(previous.pointerId)
}
function startInteraction(event: PointerEvent, kind: 'move' | 'resize') {
  if (event.button !== 0 || maximized.value || !normal.value) return
  if (kind === 'move' && event.target instanceof Element && event.target.closest('button')) return
  event.preventDefault()
  stopInteraction()
  geometryMotion.value = false
  interaction = { kind, pointerId: event.pointerId, x: event.clientX, y: event.clientY, rect: { ...normal.value } }
  if (event.currentTarget instanceof HTMLElement && typeof event.currentTarget.setPointerCapture === 'function') {
    try {
      event.currentTarget.setPointerCapture(event.pointerId)
      interaction.capture = event.currentTarget
    } catch {
      /* 合成事件或已结束的指针可能无法捕获，仍使用窗口事件完成拖动。 */
    }
  }
  document.body.classList.add(kind === 'move' ? 'is-preview-window-moving' : 'is-preview-window-resizing')
  window.addEventListener('pointermove', onPointerMove)
  window.addEventListener('pointerup', stopInteraction)
  window.addEventListener('pointercancel', stopInteraction)
  window.addEventListener('lostpointercapture', stopInteraction)
  window.addEventListener('blur', stopInteraction)
}
function resize(rect: Rect, width: number, height: number) {
  normal.value = fit({
    ...rect,
    width: clamp(width, Math.min(PREVIEW_MIN_WIDTH, bounds.value.width), bounds.value.x + bounds.value.width - rect.x),
    height: clamp(
      height,
      Math.min(PREVIEW_MIN_HEIGHT, bounds.value.height),
      bounds.value.y + bounds.value.height - rect.y,
    ),
  })
  preferredSize = { width: normal.value.width, height: normal.value.height }
}
function onPointerMove(event: PointerEvent) {
  const state = interaction
  if (!state || event.pointerId !== state.pointerId) return
  const dx = event.clientX - state.x
  const dy = event.clientY - state.y
  if (state.kind === 'move') normal.value = fit({ ...state.rect, x: state.rect.x + dx, y: state.rect.y + dy })
  else resize(state.rect, state.rect.width + dx, state.rect.height + dy)
}
function toggleMaximize() {
  stopInteraction()
  geometryMotion.value = true
  maximized.value = !maximized.value
}
function onHeaderDoubleClick(event: MouseEvent) {
  if (event.target instanceof Element && event.target.closest('button')) return
  toggleMaximize()
}
function onResizeKeydown(event: KeyboardEvent) {
  if (!normal.value || !['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return
  event.preventDefault()
  geometryMotion.value = false
  const step = event.shiftKey ? 40 : 10
  resize(
    normal.value,
    normal.value.width + (event.key === 'ArrowRight' ? step : event.key === 'ArrowLeft' ? -step : 0),
    normal.value.height + (event.key === 'ArrowDown' ? step : event.key === 'ArrowUp' ? -step : 0),
  )
}
function onKeydown(event: KeyboardEvent) {
  if (event.key !== 'Escape') return
  event.preventDefault()
  event.stopPropagation()
  if (maximized.value) toggleMaximize()
  else emit('close')
}
function measureViewport() {
  stopInteraction()
  geometryMotion.value = false
  viewport.value = { width: window.innerWidth, height: window.innerHeight }
  if (normal.value) normal.value = fit(normal.value)
}
watch(
  () => props.visible,
  (visible) => {
    if (!visible) {
      stopInteraction()
      return
    }
    measureViewport()
    maximized.value = false
    const anchor = props.anchorRect?.()
    if (anchor && anchor.width > 0 && anchor.height > 0) {
      normal.value = placePreviewBesideEditor(anchor, bounds.value, preferredSize)
    } else {
      normal.value ??= fit({
        x: viewport.value.width - 584,
        y: bounds.value.y + 32,
        width: preferredSize.width,
        height: Math.min(preferredSize.height, bounds.value.height - 32),
      })
    }
  },
  { immediate: true },
)
watch(() => props.topInset, measureViewport)
onMounted(() => window.addEventListener('resize', measureViewport))
onBeforeUnmount(() => {
  stopInteraction()
  motion.dispose()
  window.removeEventListener('resize', measureViewport)
})
</script>

<template>
  <Teleport to="body">
    <Transition
      :css="false"
      appear
      @enter="motion.enter"
      @leave="motion.leave"
      @enter-cancelled="motion.cancel()"
      @leave-cancelled="motion.cancel()"
    >
      <section
        v-if="visible"
        class="journal-preview-window"
        :class="{ 'is-maximized': maximized, 'is-animating-geometry': geometryMotion }"
        :style="windowStyle"
        role="dialog"
        aria-modal="false"
        :aria-labelledby="titleId"
        @keydown="onKeydown"
      >
        <header
          class="journal-preview-window-header"
          @pointerdown="startInteraction($event, 'move')"
          @dblclick="onHeaderDoubleClick"
        >
          <PhBrowsers :size="16" aria-hidden="true" />
          <h2 :id="titleId">日报预览</h2>
          <div class="journal-preview-window-actions">
            <button
              type="button"
              :aria-label="maximized ? '还原日报预览' : '最大化日报预览'"
              :title="maximized ? '还原' : '最大化'"
              @click="toggleMaximize()"
            >
              <PhCornersIn v-if="maximized" :size="16" aria-hidden="true" /><PhCornersOut
                v-else
                :size="16"
                aria-hidden="true"
              />
            </button>
            <button
              type="button"
              data-window-close="default"
              aria-label="关闭日报预览"
              title="关闭"
              @click="emit('close')"
            >
              <PhX :size="16" aria-hidden="true" />
            </button>
          </div>
        </header>
        <div class="journal-preview-window-body"><slot /></div>
        <button
          v-if="!maximized"
          type="button"
          class="journal-preview-window-resize"
          aria-label="调整日报预览窗口大小"
          title="拖动调整大小，也可使用方向键"
          @pointerdown.stop="startInteraction($event, 'resize')"
          @keydown="onResizeKeydown"
        ></button>
      </section>
    </Transition>
  </Teleport>
</template>
