<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, useId, watch } from 'vue'
import { PhGearSix, PhX } from '@phosphor-icons/vue'
import { createSurfaceMotion } from '../lib/surfaceMotion'
import type { WindowMotionRect } from '../lib/windowMotion'

const props = withDefaults(
  defineProps<{
    visible: boolean
    busy: boolean
    buttonRect?: () => WindowMotionRect | null
    /** 兼容关闭路由；窗口动效统一收向实际关闭按钮。 */
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
const motion = createSurfaceMotion({
  openingRect: () => props.buttonRect?.(),
  closingRect: () => closeButton.value?.getBoundingClientRect(),
  backdrop: () => backdrop.value,
})
function enter(_element: Element, done: () => void) {
  if (panel.value) motion.enter(panel.value, done)
  else done()
}
function leave(_element: Element, done: () => void) {
  if (panel.value) motion.leave(panel.value, done)
  else done()
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
  motion.dispose()
})
</script>

<template>
  <Transition
    :css="false"
    appear
    @enter="enter"
    @leave="leave"
    @enter-cancelled="motion.cancel()"
    @leave-cancelled="motion.cancel()"
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
            data-window-close="default"
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
