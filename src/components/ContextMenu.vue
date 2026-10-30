<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import type { ContextMenuItem } from '../lib/contextMenus'

const props = withDefaults(
  defineProps<{
    x: number
    y: number
    items: ContextMenuItem[]
    kind?: 'git' | 'text'
    teleportTo?: HTMLElement | string
  }>(),
  { kind: 'text', teleportTo: 'body' },
)
const emit = defineEmits<{ close: [restoreFocus: boolean]; choose: [id: string] }>()
const panel = ref<HTMLElement>()
const left = ref(props.x)
const top = ref(props.y)
const ready = ref(false)

watch(
  () => [props.x, props.y, props.items],
  async () => {
    ready.value = false
    await nextTick()
    const element = panel.value
    if (!element?.isConnected) return
    const bounds = element.getBoundingClientRect()
    left.value = Math.max(8, Math.min(props.x, window.innerWidth - bounds.width - 8))
    top.value = Math.max(8, Math.min(props.y, window.innerHeight - bounds.height - 8))
    ready.value = true
    await nextTick()
    if (panel.value !== element || !element.isConnected) return
    element.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus({ preventScroll: true })
  },
  { immediate: true, flush: 'post' },
)

function outside(event: Event) {
  if (panel.value && !event.composedPath().includes(panel.value)) emit('close', false)
}
function dismiss() {
  emit('close', false)
}
function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' || event.key === 'Tab') {
    if (event.key === 'Escape') {
      event.preventDefault()
      event.stopPropagation()
    }
    emit('close', true)
    return
  }
  if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return
  event.preventDefault()
  event.stopPropagation()
  const buttons = [...(panel.value?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])]
  if (!buttons.length) return
  const current = buttons.indexOf(document.activeElement as HTMLButtonElement)
  const index =
    event.key === 'Home'
      ? 0
      : event.key === 'End'
        ? buttons.length - 1
        : (current + (event.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length
  buttons[index]?.focus({ preventScroll: true })
}
onMounted(() => {
  window.addEventListener('pointerdown', outside, true)
  window.addEventListener('scroll', outside, true)
  window.addEventListener('resize', dismiss)
  window.addEventListener('blur', dismiss)
})
onBeforeUnmount(() => {
  window.removeEventListener('pointerdown', outside, true)
  window.removeEventListener('scroll', outside, true)
  window.removeEventListener('resize', dismiss)
  window.removeEventListener('blur', dismiss)
})
</script>

<template>
  <Teleport :to="teleportTo">
    <div
      ref="panel"
      class="app-context-menu"
      :class="kind === 'git' ? 'git-context-menu' : 'text-context-menu'"
      role="menu"
      :aria-label="kind === 'git' ? 'Git 操作' : '文本操作'"
      :style="{ left: `${left}px`, top: `${top}px`, visibility: ready ? 'visible' : 'hidden' }"
      @keydown="onKeydown"
      @contextmenu.prevent
    >
      <button
        v-for="item in items"
        :key="item.id"
        type="button"
        role="menuitem"
        :class="{ 'is-danger': item.danger }"
        :disabled="item.disabled"
        tabindex="-1"
        @click="emit('choose', item.id)"
      >
        {{ item.label }}
      </button>
    </div>
  </Teleport>
</template>

<style scoped>
.app-context-menu {
  position: fixed;
  z-index: 200;
  display: grid;
  min-width: 168px;
  max-width: calc(100vw - 16px);
  max-height: calc(100vh - 16px);
  overflow: auto;
  padding: 5px;
  border: 1px solid var(--skin-border);
  border-radius: 10px;
  background: var(--skin-surface);
  box-shadow: 0 8px 28px var(--skin-shadow-color, #172b4d26);
  color: var(--skin-text);
}
.app-context-menu button {
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: inherit;
  font: inherit;
  font-size: var(--text-body);
  padding: 9px 12px;
  text-align: left;
  cursor: pointer;
}
.app-context-menu button:hover:not(:disabled),
.app-context-menu button:focus-visible {
  outline: none;
  background: var(--skin-accent-soft);
  color: var(--skin-accent);
}
.app-context-menu button.is-danger {
  color: var(--skin-danger);
}
.app-context-menu button.is-danger:hover:not(:disabled),
.app-context-menu button.is-danger:focus-visible {
  color: var(--skin-danger);
  background: var(--skin-danger-soft);
}
.app-context-menu button:disabled {
  opacity: 0.45;
  cursor: default;
}
</style>
