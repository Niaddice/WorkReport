<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, useId, type CSSProperties } from 'vue'
import { PhQuestion, PhX } from '@phosphor-icons/vue'
import { selectMenuPosition } from '../lib/select'

const props = defineProps<{ title: string; help: readonly string[]; anchor?: HTMLElement | null }>()
const popup = ref<HTMLElement>()
const opened = ref(false)
const popupId = useId()
const titleId = `${popupId}-title`
const placement = ref('bottom')
const style = ref<CSSProperties>({ position: 'fixed', visibility: 'hidden' })

function close(restoreFocus = false) {
  if (!opened.value) return
  opened.value = false
  listen(false)
  if (restoreFocus && props.anchor?.isConnected) props.anchor.focus({ preventScroll: true })
}
function onOutside(event: Event) {
  const target = event.target
  if (target instanceof Node && !popup.value?.contains(target) && !props.anchor?.contains(target)) close()
}
function onKeydown(event: KeyboardEvent) {
  if (event.key !== 'Escape' || event.defaultPrevented) return
  event.preventDefault()
  event.stopPropagation()
  close(true)
}
function onViewChange() {
  close()
}
function onScroll(event: Event) {
  if (event.target instanceof Node && popup.value?.contains(event.target)) return
  close()
}
function listen(active: boolean) {
  if (active) {
    document.addEventListener('pointerdown', onOutside, true)
    document.addEventListener('click', onOutside, true)
    document.addEventListener('focusin', onOutside, true)
    document.addEventListener('keydown', onKeydown, true)
    window.addEventListener('resize', onViewChange)
    window.addEventListener('blur', onViewChange)
    window.addEventListener('scroll', onScroll, true)
  } else {
    document.removeEventListener('pointerdown', onOutside, true)
    document.removeEventListener('click', onOutside, true)
    document.removeEventListener('focusin', onOutside, true)
    document.removeEventListener('keydown', onKeydown, true)
    window.removeEventListener('resize', onViewChange)
    window.removeEventListener('blur', onViewChange)
    window.removeEventListener('scroll', onScroll, true)
  }
}
async function open() {
  if (!props.anchor?.isConnected) return
  if (opened.value) return
  const viewport = { width: window.innerWidth, height: window.innerHeight }
  const width = Math.max(0, Math.min(360, viewport.width - 16))
  style.value = { position: 'fixed', width: `${width}px`, visibility: 'hidden' }
  opened.value = true
  listen(true)
  await nextTick()
  if (!opened.value || !popup.value || !props.anchor?.isConnected) return
  const anchor = props.anchor.getBoundingClientRect()
  const position = selectMenuPosition(
    { top: anchor.top, bottom: anchor.bottom, left: anchor.right - width, width },
    viewport,
    popup.value.scrollHeight,
  )
  placement.value = position.placement
  style.value = {
    position: 'fixed',
    top: `${position.top}px`,
    left: `${position.left}px`,
    width: `${position.width}px`,
    maxHeight: `${position.maxHeight}px`,
    visibility: 'visible',
  }
  await nextTick()
  if (opened.value) popup.value?.focus({ preventScroll: true })
}
function toggle() {
  if (opened.value) close()
  else void open()
}
defineExpose({ opened, id: popupId, open, close, toggle })
onBeforeUnmount(() => listen(false))
</script>

<template>
  <Teleport to="body">
    <Transition name="select-popover">
      <section
        v-if="opened"
        :id="popupId"
        ref="popup"
        class="module-help-popover"
        :data-placement="placement"
        :style="style"
        role="dialog"
        aria-modal="false"
        :aria-labelledby="titleId"
        tabindex="-1"
      >
        <header class="module-help-heading">
          <PhQuestion :size="17" aria-hidden="true" />
          <h2 :id="titleId">{{ title }} · 使用说明</h2>
          <button type="button" class="module-help-close" :aria-label="`关闭「${title}」说明`" @click="close(true)">
            <PhX :size="15" aria-hidden="true" />
          </button>
        </header>
        <ul class="module-help-list">
          <li v-for="(item, index) in help" :key="index">{{ item }}</li>
        </ul>
      </section>
    </Transition>
  </Teleport>
</template>
