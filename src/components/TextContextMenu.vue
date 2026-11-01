<script setup lang="ts">
import { onBeforeUnmount, onMounted, shallowRef, watch } from 'vue'
import ContextMenu from './ContextMenu.vue'
import { textMenuSession, type TextMenuSession } from '../lib/textContextMenu'
import { pushMessage } from '../lib/messages'

const props = defineProps<{ scope: string }>()
const menu = shallowRef<(TextMenuSession & { x: number; y: number; portal: HTMLElement | string }) | null>(null)
watch(
  () => props.scope,
  () => {
    menu.value = null
  },
  { flush: 'sync' },
)

function open(event: MouseEvent | KeyboardEvent) {
  if (event instanceof KeyboardEvent && event.key !== 'ContextMenu' && !(event.shiftKey && event.key === 'F10')) return
  close()
  const target = event.composedPath().find((node): node is HTMLElement => node instanceof HTMLElement)
  if (!target) return
  const session = textMenuSession(target)
  if (!session) return
  event.preventDefault()
  event.stopPropagation()
  const point = target.getBoundingClientRect()
  const mouse = event instanceof MouseEvent && (event.clientX !== 0 || event.clientY !== 0)
  menu.value = {
    ...session,
    x: mouse ? event.clientX : point.left,
    y: mouse ? event.clientY : point.bottom,
    portal: target.closest<HTMLDialogElement>('dialog[open]') ?? 'body',
  }
}
function close(restoreFocus = false) {
  const previous = menu.value
  menu.value = null
  if (restoreFocus) previous?.restore()
}
async function choose(id: string) {
  const session = menu.value
  const item = session?.items.find((item) => item.id === id)
  if (!item || item.disabled) return
  close(true)
  try {
    await item.run()
  } catch (cause) {
    pushMessage({ tone: 'error', text: `${item.label}失败：${String(cause)}` })
  }
}
function input() {
  close()
}
onMounted(() => {
  window.addEventListener('contextmenu', open)
  window.addEventListener('keydown', open)
  window.addEventListener('input', input)
})
onBeforeUnmount(() => {
  window.removeEventListener('contextmenu', open)
  window.removeEventListener('keydown', open)
  window.removeEventListener('input', input)
})
</script>

<template>
  <ContextMenu
    v-if="menu"
    :x="menu.x"
    :y="menu.y"
    :items="menu.items"
    :teleport-to="menu.portal"
    @close="close"
    @choose="choose"
  />
</template>
