<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, useId, watch, type CSSProperties } from 'vue'
import { PhArrowLineDown, PhCircleNotch } from '@phosphor-icons/vue'
import type { Document } from '../lib/workspace'
import { selectMenuPosition } from '../lib/select'

const props = defineProps<{
  disabled: boolean
  hasContent: boolean
  sourceKey: string
  load: () => Promise<Document | null>
}>()
const emit = defineEmits<{ confirm: [content: string] }>()
const opened = ref(false)
const loading = ref(false)
const report = ref<Document | null>(null)
const error = ref('')
const trigger = ref<HTMLButtonElement>()
const popover = ref<HTMLElement>()
const dismiss = ref<HTMLButtonElement>()
const id = useId()
const style = ref<CSSProperties>({ position: 'fixed', visibility: 'hidden' })
const placement = ref('bottom')
const reportDate = computed(() => report.value?.date.replace(/^(\d{4})(\d{2})(\d{2})$/, '$1-$2-$3'))
let request = 0

function close(restoreFocus = false) {
  opened.value = false
  request++
  if (restoreFocus) trigger.value?.focus({ preventScroll: true })
}
function position() {
  if (!opened.value || !trigger.value || !popover.value) return
  const rect = trigger.value.getBoundingClientRect()
  const viewport = { width: document.documentElement.clientWidth || window.innerWidth, height: window.innerHeight }
  const anchor = { top: rect.top, bottom: rect.bottom, left: rect.right - 300, width: 300 }
  popover.value.style.width = `${selectMenuPosition(anchor, viewport, 0).width}px`
  const result = selectMenuPosition(anchor, viewport, popover.value.scrollHeight)
  placement.value = result.placement
  style.value = {
    position: 'fixed',
    visibility: 'visible',
    left: `${result.left}px`,
    top: `${result.top}px`,
    width: `${result.width}px`,
    maxHeight: `${result.maxHeight}px`,
  }
}
async function open() {
  if (props.disabled) return
  if (opened.value) return close(true)
  report.value = null
  error.value = ''
  loading.value = true
  style.value = { position: 'fixed', visibility: 'hidden' }
  opened.value = true
  const token = ++request
  await nextTick()
  if (!opened.value || token !== request) return
  position()
  await nextTick()
  if (!opened.value || token !== request) return
  dismiss.value?.focus({ preventScroll: true })
  try {
    const value = await props.load()
    if (opened.value && token === request) report.value = value
  } catch {
    if (opened.value && token === request) error.value = '读取上次日报失败，请重试。'
  } finally {
    if (opened.value && token === request) {
      loading.value = false
      await nextTick()
      position()
    }
  }
}
function retry() {
  close()
  void open()
}
function confirm() {
  if (props.disabled || loading.value || !report.value?.content.trim()) return
  const content = report.value.content
  close()
  emit('confirm', content)
}
function onOutside(event: Event) {
  const target = event.target
  if (target instanceof Node && !trigger.value?.contains(target) && !popover.value?.contains(target)) close()
}
function onScroll(event: Event) {
  if (!(event.target instanceof Node && popover.value?.contains(event.target))) close()
}
function onKeydown(event: KeyboardEvent) {
  if (event.key !== 'Escape' || event.isComposing) return
  event.preventDefault()
  event.stopPropagation()
  close(true)
}
watch(opened, (value, _previous, onCleanup) => {
  if (!value) return
  document.addEventListener('pointerdown', onOutside, true)
  document.addEventListener('focusin', onOutside, true)
  document.addEventListener('keydown', onKeydown, true)
  window.addEventListener('resize', position)
  window.addEventListener('scroll', onScroll, true)
  onCleanup(() => {
    document.removeEventListener('pointerdown', onOutside, true)
    document.removeEventListener('focusin', onOutside, true)
    document.removeEventListener('keydown', onKeydown, true)
    window.removeEventListener('resize', position)
    window.removeEventListener('scroll', onScroll, true)
  })
})
watch([() => props.disabled, () => props.sourceKey], () => close(), { flush: 'sync' })
onBeforeUnmount(() => close())
</script>

<template>
  <button
    ref="trigger"
    type="button"
    class="editor-tool-button previous-journal-trigger"
    aria-label="带入上次日报"
    title="带入上次日报"
    aria-haspopup="dialog"
    :aria-expanded="opened"
    :aria-controls="opened ? id : undefined"
    :disabled="disabled"
    @click="open"
  >
    <PhArrowLineDown :size="15" aria-hidden="true" />
  </button>
  <Teleport to="body">
    <Transition name="select-popover">
      <section
        v-if="opened"
        :id="id"
        ref="popover"
        class="previous-journal-popover"
        role="dialog"
        aria-label="带入上次日报"
        :aria-describedby="`${id}-description`"
        :aria-busy="loading"
        :data-placement="placement"
        :style="style"
      >
        <h3>带入上次日报</h3>
        <div :id="`${id}-description`" aria-live="polite">
          <p v-if="loading" class="previous-journal-status">
            <PhCircleNotch class="spin" :size="15" />正在查找上次日报…
          </p>
          <p v-else-if="error" class="error-text" role="alert">{{ error }}</p>
          <template v-else-if="report">
            <p>带入 {{ reportDate }} 的日报内容？</p>
            <pre class="previous-journal-excerpt">{{ report.content.slice(0, 300) }}</pre>
            <p class="previous-journal-hint">
              {{ hasContent ? '将替换当前内容，可通过撤销恢复。' : '确认后填入当前日报并自动保存。' }}
            </p>
          </template>
          <p v-else>未找到当前日期之前有内容的日报。</p>
        </div>
        <div class="previous-journal-actions">
          <button ref="dismiss" type="button" class="text-action muted-action" @click="close(true)">
            {{ report || loading || error ? '取消' : '关闭' }}
          </button>
          <button v-if="error" type="button" class="text-action" @click="retry">重试</button>
          <button v-else-if="report" type="button" class="text-action" :disabled="disabled || loading" @click="confirm">
            确认带入
          </button>
        </div>
      </section>
    </Transition>
  </Teleport>
</template>

<style scoped>
.previous-journal-trigger[aria-expanded='true'] {
  background: var(--skin-accent-soft);
  color: var(--skin-accent);
}
.previous-journal-popover {
  z-index: 500;
  padding: 14px;
  overflow-y: auto;
  overscroll-behavior: contain;
  border: 1px solid var(--skin-border);
  border-radius: 12px;
  background: var(--skin-surface);
  color: var(--skin-text-body);
  box-shadow: 0 8px 24px rgba(34, 57, 93, 0.14);
  font-size: var(--text-body);
}
.previous-journal-popover h3 {
  margin: 0 0 10px;
  font-size: inherit;
  font-weight: 600;
}
.previous-journal-popover p {
  margin: 0;
  line-height: 1.6;
}
.previous-journal-status {
  display: flex;
  align-items: center;
  gap: 6px;
}
.previous-journal-excerpt {
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 3;
  margin: 10px 0;
  padding: 8px;
  overflow: hidden;
  border-radius: 6px;
  background: var(--skin-canvas);
  color: var(--skin-text-secondary);
  font: inherit;
  line-height: 1.6;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
.previous-journal-hint {
  color: var(--skin-text-muted);
  font-size: var(--text-small);
}
.previous-journal-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 12px;
}
</style>
