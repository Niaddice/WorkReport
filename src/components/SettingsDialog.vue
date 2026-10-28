<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, useId, watch } from 'vue'
import { PhGearSix, PhX } from '@phosphor-icons/vue'
import {
  MAXIMIZE_DURATION,
  RESTORE_DURATION,
  MAXIMIZE_EASING,
  RESTORE_EASING,
  windowRectTransform,
  type WindowMotionRect,
} from '../lib/windowMotion'

const props = withDefaults(
  defineProps<{
    visible: boolean
    busy: boolean
    buttonRect?: () => WindowMotionRect | null
    closeTarget?: 'settings' | 'close'
  }>(),
  { closeTarget: 'settings' },
)
const emit = defineEmits<{ close: [target: 'settings' | 'close'] }>()
const panel = ref<HTMLElement>()
const backdrop = ref<HTMLElement>()
const closeButton = ref<HTMLButtonElement>()
const titleId = useId()
let returnFocus: HTMLElement | null = null
let motion: { animations: Animation[]; timer?: ReturnType<typeof setTimeout> } | null = null
let interrupted: { rect: DOMRect; opacity: string; backdropOpacity: string } | null = null

function cancelMotion(preserve = true) {
  if (!motion) return
  if (preserve && panel.value)
    interrupted = {
      rect: panel.value.getBoundingClientRect(),
      opacity: getComputedStyle(panel.value).opacity,
      backdropOpacity: backdrop.value ? getComputedStyle(backdrop.value).opacity : '1',
    }
  const previous = motion
  motion = null
  clearTimeout(previous.timer)
  previous.animations.forEach((animation) => animation.cancel())
}
function animateWindow(opening: boolean, done: () => void) {
  cancelMotion()
  const current = interrupted
  interrupted = null
  const element = panel.value
  const reduced = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches
  if (!element || reduced || typeof element.animate !== 'function') {
    done()
    return
  }
  const frame = element.getBoundingClientRect()
  const button = props.buttonRect?.()
  const anchor = !opening && props.closeTarget === 'close' ? closeButton.value?.getBoundingClientRect() : button
  const anchorTransform = anchor ? windowRectTransform(anchor, frame) : null
  const currentTransform = current ? windowRectTransform(current.rect, frame) : null
  if (!anchorTransform) {
    done()
    return
  }
  const full = { transform: 'translate(0px, 0px) scale(1, 1)', opacity: 1 }
  const start = current
    ? { transform: currentTransform ?? full.transform, opacity: current.opacity }
    : opening
      ? { transform: anchorTransform, opacity: 0.72 }
      : full
  const end = opening ? full : { transform: anchorTransform, opacity: 0.6 }
  const duration = opening ? MAXIMIZE_DURATION : RESTORE_DURATION
  const state: NonNullable<typeof motion> = { animations: [] }
  motion = state
  const finish = () => {
    if (motion !== state) return
    motion = null
    clearTimeout(state.timer)
    done()
    state.animations.forEach((animation) => animation.cancel())
  }
  state.animations.push(
    element.animate([start, end], {
      duration,
      easing: opening ? MAXIMIZE_EASING : RESTORE_EASING,
      fill: 'both',
    }),
  )
  if (backdrop.value && typeof backdrop.value.animate === 'function')
    state.animations.push(
      backdrop.value.animate(
        [{ opacity: current?.backdropOpacity ?? (opening ? 0 : 1) }, { opacity: opening ? 1 : 0 }],
        { duration, easing: opening ? 'ease-out' : 'ease-in', fill: 'both' },
      ),
    )
  // 后台 WebView 可能不及时交付动画完成事件，保留兜底以免窗口卡在退出状态。
  state.timer = setTimeout(finish, duration + 80)
  void Promise.allSettled(state.animations.map((animation) => animation.finished)).then(finish)
}
function enter(_element: Element, done: () => void) {
  animateWindow(true, done)
}
function leave(_element: Element, done: () => void) {
  animateWindow(false, done)
}

function nativeDialogOpen() {
  return !!document.querySelector('dialog[open]:not(.is-inline)')
}
function requestClose(target: 'settings' | 'close' = 'settings') {
  if (props.visible && !props.busy) emit('close', target)
}
function onKeydown(event: KeyboardEvent) {
  if (event.key !== 'Escape' || !props.visible || event.defaultPrevented || nativeDialogOpen()) return
  event.preventDefault()
  event.stopPropagation()
  requestClose()
}
watch(
  () => props.visible,
  async (visible) => {
    if (visible) {
      const focused = document.activeElement
      if (focused instanceof HTMLElement && !panel.value?.contains(focused)) returnFocus = focused
      await nextTick()
      if (props.visible && !nativeDialogOpen()) panel.value?.focus({ preventScroll: true })
    } else {
      await nextTick()
      const focused = document.activeElement
      if (
        !props.visible &&
        !nativeDialogOpen() &&
        returnFocus?.isConnected &&
        (focused === document.body || (focused && panel.value?.contains(focused)))
      )
        returnFocus.focus({ preventScroll: true })
    }
  },
  { immediate: true, flush: 'post' },
)
onMounted(() => window.addEventListener('keydown', onKeydown))
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown)
  cancelMotion(false)
})
</script>

<template>
  <Transition
    :css="false"
    appear
    @enter="enter"
    @leave="leave"
    @enter-cancelled="cancelMotion()"
    @leave-cancelled="cancelMotion()"
  >
    <div v-show="visible" class="settings-dialog-layer" :inert="!visible">
      <div ref="backdrop" class="settings-dialog-backdrop" aria-hidden="true" @click.self="requestClose()"></div>
      <section
        ref="panel"
        class="settings-dialog"
        role="dialog"
        aria-modal="false"
        :aria-labelledby="titleId"
        :inert="!visible"
        tabindex="-1"
      >
        <header class="settings-dialog-heading">
          <PhGearSix :size="18" aria-hidden="true" />
          <h2 :id="titleId">设置</h2>
          <button
            ref="closeButton"
            type="button"
            class="settings-dialog-close"
            aria-label="关闭设置"
            title="关闭设置（Esc）"
            :disabled="busy"
            @click="requestClose('close')"
          >
            <PhX :size="18" aria-hidden="true" />
          </button>
        </header>
        <div class="settings-dialog-body"><slot /></div>
      </section>
    </div>
  </Transition>
</template>
