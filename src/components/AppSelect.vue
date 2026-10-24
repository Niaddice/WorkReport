<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useId, watch, type CSSProperties } from 'vue'
import { PhCaretDown, PhCheck } from '@phosphor-icons/vue'
import {
  boundarySelectOption,
  matchSelectOption,
  nextSelectOption,
  selectMenuPosition,
  type SelectOption,
} from '../lib/select'

defineOptions({ inheritAttrs: false })
const props = withDefaults(
  defineProps<{
    modelValue: string
    options: SelectOption[]
    disabled?: boolean
    id?: string
    placeholder?: string
  }>(),
  { disabled: false, placeholder: '请选择' },
)
const emit = defineEmits<{ 'update:modelValue': [value: string] }>()
const generatedId = useId()
const fieldId = computed(() => props.id || generatedId)
const listId = computed(() => `${fieldId.value}-options`)
const trigger = ref<HTMLButtonElement | null>(null)
const menu = ref<HTMLDivElement | null>(null)
const opened = ref(false)
const positioned = ref(false)
const activeIndex = ref(-1)
const placement = ref('bottom')
const menuStyle = ref<CSSProperties>({ position: 'fixed', visibility: 'hidden' })
const selectedIndex = computed(() => props.options.findIndex((option) => option.value === props.modelValue))
const selected = computed(() => props.options[selectedIndex.value])
const activeId = computed(() => (opened.value && activeIndex.value >= 0 ? optionId(activeIndex.value) : undefined))
let typeahead = ''
let typeaheadTimer: ReturnType<typeof setTimeout> | undefined
let frame = 0
let observer: ResizeObserver | undefined

function optionId(index: number) {
  return `${listId.value}-${index}`
}
function clearTypeahead() {
  typeahead = ''
  clearTimeout(typeaheadTimer)
}

function positionMenu() {
  if (!opened.value || !trigger.value || !menu.value) return
  const anchor = trigger.value.getBoundingClientRect()
  const viewport = { width: document.documentElement.clientWidth, height: window.innerHeight }
  if (anchor.bottom <= 0 || anchor.top >= viewport.height) {
    close()
    return
  }
  // Measure after constraining the width so wrapped model names contribute their
  // actual height to the placement decision, including on viewport resize.
  const { width } = selectMenuPosition(anchor, viewport, 0)
  menu.value.style.boxSizing = 'border-box'
  menu.value.style.width = `${width}px`
  const contentHeight = menu.value.scrollHeight + menu.value.offsetHeight - menu.value.clientHeight
  const position = selectMenuPosition(anchor, viewport, contentHeight)
  placement.value = position.placement
  menuStyle.value = {
    position: 'fixed',
    visibility: 'visible',
    boxSizing: 'border-box',
    top: `${position.top}px`,
    left: `${position.left}px`,
    width: `${position.width}px`,
    maxHeight: `${position.maxHeight}px`,
  }
  positioned.value = true
}

function schedulePosition() {
  if (!opened.value || frame) return
  frame = requestAnimationFrame(() => {
    frame = 0
    positionMenu()
  })
}

function onScroll(event: Event) {
  // A teleported menu must not remain over fixed headings when its field scrolls away.
  if (opened.value && event.target instanceof Element && trigger.value && event.target.contains(trigger.value)) {
    close()
    return
  }
  schedulePosition()
}

function scrollActiveIntoView() {
  const option = document.getElementById(optionId(activeIndex.value))
  if (!menu.value || !option) return
  const optionTop = option.offsetTop
  const optionBottom = optionTop + option.offsetHeight
  if (optionTop < menu.value.scrollTop) menu.value.scrollTop = optionTop
  else if (optionBottom > menu.value.scrollTop + menu.value.clientHeight)
    menu.value.scrollTop = optionBottom - menu.value.clientHeight
}

async function open(boundary?: 'first' | 'last') {
  if (props.disabled) return
  trigger.value?.focus({ preventScroll: true })
  clearTypeahead()
  activeIndex.value = boundary
    ? boundarySelectOption(props.options, boundary)
    : selected.value && !selected.value.disabled
      ? selectedIndex.value
      : boundarySelectOption(props.options, 'first')
  if (!opened.value) {
    positioned.value = false
    menuStyle.value = { ...menuStyle.value, visibility: 'hidden' }
  }
  opened.value = true
  await nextTick()
  if (!opened.value) return
  positionMenu()
  scrollActiveIntoView()
}

function close() {
  opened.value = false
  clearTypeahead()
}

function select(index: number) {
  const option = props.options[index]
  if (props.disabled || !option || option.disabled) return
  emit('update:modelValue', option.value)
  close()
  void nextTick(() => trigger.value?.focus({ preventScroll: true }))
}

function move(boundaryOrDirection: 'first' | 'last' | 1 | -1) {
  activeIndex.value =
    typeof boundaryOrDirection === 'string'
      ? boundarySelectOption(props.options, boundaryOrDirection)
      : nextSelectOption(
          props.options,
          activeIndex.value < 0 && boundaryOrDirection < 0 ? props.options.length : activeIndex.value,
          boundaryOrDirection,
        )
  void nextTick(scrollActiveIntoView)
}

function onKeydown(event: KeyboardEvent) {
  if (props.disabled || event.isComposing) return
  if (event.key === 'Escape' && opened.value) {
    event.preventDefault()
    event.stopPropagation()
    close()
    return
  }
  if (event.key === 'Tab') {
    close()
    return
  }
  if (event.altKey || event.ctrlKey || event.metaKey) return
  if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
    event.preventDefault()
    const direction = event.key === 'Home' ? 'first' : event.key === 'End' ? 'last' : event.key === 'ArrowDown' ? 1 : -1
    if (!opened.value) void open(typeof direction === 'string' ? direction : undefined)
    else move(direction)
    return
  }
  if (event.key === 'Enter' || event.key === ' ') {
    event.preventDefault()
    if (opened.value) select(activeIndex.value)
    else void open()
    return
  }
  if (event.key.length === 1) {
    event.preventDefault()
    const wasOpen = opened.value
    if (!wasOpen) void open()
    typeahead += event.key
    clearTimeout(typeaheadTimer)
    typeaheadTimer = setTimeout(clearTypeahead, 650)
    // Continue a multi-letter prefix from the current match; cycle single letters.
    const start =
      typeahead.length > 1 && new Set(typeahead.toLocaleLowerCase()).size > 1
        ? activeIndex.value - 1
        : activeIndex.value
    const match = matchSelectOption(props.options, typeahead, start)
    if (match >= 0) {
      activeIndex.value = match
      void nextTick(scrollActiveIntoView)
    }
  }
}

function onOutsidePointer(event: PointerEvent) {
  const target = event.target as Node | null
  if (opened.value && target && !trigger.value?.contains(target) && !menu.value?.contains(target)) close()
}

function onOutsideFocus(event: FocusEvent) {
  const target = event.target as Node | null
  if (opened.value && target && !trigger.value?.contains(target) && !menu.value?.contains(target)) close()
}

function onFocusOut(event: FocusEvent) {
  if (event.relatedTarget && menu.value?.contains(event.relatedTarget as Node)) return
  close()
}

watch(
  () => props.disabled,
  (disabled) => {
    if (disabled) close()
  },
)
watch(
  () => props.options,
  () => {
    if (!opened.value) return
    activeIndex.value =
      selected.value && !selected.value.disabled ? selectedIndex.value : boundarySelectOption(props.options, 'first')
    void nextTick(() => {
      positionMenu()
      scrollActiveIntoView()
    })
  },
  { deep: true },
)
watch(
  () => props.modelValue,
  () => {
    if (opened.value && selected.value && !selected.value.disabled) {
      activeIndex.value = selectedIndex.value
      void nextTick(scrollActiveIntoView)
    }
  },
)

onMounted(() => {
  document.addEventListener('pointerdown', onOutsidePointer, true)
  document.addEventListener('focusin', onOutsideFocus, true)
  window.addEventListener('resize', schedulePosition, { passive: true })
  window.addEventListener('scroll', onScroll, { capture: true, passive: true })
  observer = new ResizeObserver(schedulePosition)
  if (trigger.value) observer.observe(trigger.value)
})
onBeforeUnmount(() => {
  clearTypeahead()
  cancelAnimationFrame(frame)
  observer?.disconnect()
  document.removeEventListener('pointerdown', onOutsidePointer, true)
  document.removeEventListener('focusin', onOutsideFocus, true)
  window.removeEventListener('resize', schedulePosition)
  window.removeEventListener('scroll', onScroll, true)
})
</script>

<template>
  <div class="app-select" :class="{ 'is-open': opened, 'is-disabled': disabled }">
    <button
      v-bind="$attrs"
      :id="fieldId"
      ref="trigger"
      type="button"
      class="app-select-trigger"
      role="combobox"
      aria-haspopup="listbox"
      :aria-expanded="opened"
      :aria-controls="opened ? listId : undefined"
      :aria-activedescendant="activeId"
      :disabled="disabled"
      @click="opened ? close() : open()"
      @keydown="onKeydown"
      @blur="onFocusOut"
    >
      <span
        class="app-select-label"
        :class="{ 'is-placeholder': !selected || selected.disabled }"
        :title="selected?.label"
        >{{ selected?.label || placeholder }}</span
      >
      <PhCaretDown class="app-select-chevron" :size="15" weight="bold" aria-hidden="true" />
    </button>
    <Teleport to="body">
      <Transition name="select-popover">
        <div
          v-if="opened"
          :id="listId"
          ref="menu"
          class="app-select-menu"
          role="listbox"
          :aria-labelledby="fieldId"
          :style="menuStyle"
          :data-placement="placement"
          :data-positioned="positioned"
          @pointerdown.prevent
        >
          <div
            v-for="(option, index) in options"
            :id="optionId(index)"
            :key="option.value"
            class="app-select-option"
            :class="{
              'is-active': index === activeIndex,
              'is-selected': index === selectedIndex,
              'is-disabled': option.disabled,
            }"
            role="option"
            :aria-selected="index === selectedIndex"
            :aria-disabled="option.disabled || undefined"
            :title="option.label"
            @pointermove="!option.disabled && (activeIndex = index)"
            @click="select(index)"
          >
            <span class="app-select-option-label">{{ option.label }}</span>
            <PhCheck
              v-if="index === selectedIndex"
              class="app-select-option-check"
              :size="15"
              weight="bold"
              aria-hidden="true"
            />
          </div>
          <div v-if="!options.length" class="app-select-empty">暂无可选项</div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>
