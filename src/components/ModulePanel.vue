<script setup lang="ts">
import { ref, type Component } from 'vue'
import { PhCornersIn, PhCornersOut, PhQuestion, PhX } from '@phosphor-icons/vue'
import ModuleHelpPopover from './ModuleHelpPopover.vue'
import type { ModuleId } from '../modules/types'

const props = withDefaults(
  defineProps<{
    id: ModuleId
    title: string
    icon: Component
    /** 只剩一个模块时不允许关闭。 */
    canRemove: boolean
    dragging: boolean
    /** 当前正在操作的模块。 */
    active?: boolean
    /** 当前是否以浮层最大化显示。 */
    maximized?: boolean
    /** 使用说明，标题栏的帮助按钮据此展示。 */
    help?: readonly string[]
  }>(),
  { active: false, maximized: false, help: () => [] },
)
const emit = defineEmits<{
  activate: []
  'drag-start': [event: PointerEvent]
  remove: []
  'toggle-maximize': []
}>()

const helpPopover = ref<InstanceType<typeof ModuleHelpPopover>>()
const helpButton = ref<HTMLButtonElement>()

function activateFromPointer(event: Event) {
  const panel = event.currentTarget as HTMLElement
  const focused = document.activeElement
  // WebKit 点击按钮或留白不一定转移焦点，不能让旧模块的输入框继续接收打字。
  if (focused instanceof HTMLElement && focused.closest('.module-panel') && !panel.contains(focused)) focused.blur()
  emit('activate')
}
function onPointerDown(event: PointerEvent) {
  if (props.maximized || event.button !== 0 || event.defaultPrevented) return
  const target = event.target
  if (
    !(target instanceof Element) ||
    target.closest(
      '.module-panel-title, .module-panel-subtitle, button, a, input, select, textarea, label, [role="button"], [contenteditable]:not([contenteditable="false"])',
    )
  )
    return
  emit('drag-start', event)
}
function openHelp() {
  helpPopover.value?.toggle()
}
</script>

<template>
  <section
    class="module-panel"
    :class="{ 'is-active': active, 'is-dragging': dragging, 'is-maximized': maximized }"
    :data-module-id="id"
    :role="maximized ? 'dialog' : undefined"
    :aria-modal="maximized ? 'true' : undefined"
    :aria-label="title"
    tabindex="-1"
    @pointerdown.capture="activateFromPointer"
    @click.capture="activateFromPointer"
    @focusin="emit('activate')"
  >
    <header class="module-panel-header" @pointerdown="onPointerDown">
      <component :is="icon" class="module-panel-icon" :size="15" aria-hidden="true" />
      <h2 class="module-panel-title">{{ title }}</h2>
      <slot name="subtitle" />
      <div class="module-panel-actions">
        <button
          v-if="help.length"
          ref="helpButton"
          type="button"
          class="module-panel-action module-help-button"
          :aria-label="`查看「${title}」使用说明`"
          :title="helpPopover?.opened ? undefined : '使用说明'"
          aria-haspopup="dialog"
          :aria-expanded="helpPopover?.opened ?? false"
          :aria-controls="helpPopover?.opened ? helpPopover.id : undefined"
          @click="openHelp"
        >
          <PhQuestion :size="15" aria-hidden="true" />
        </button>
        <button
          type="button"
          class="module-panel-action module-maximize-button"
          :aria-label="maximized ? `还原「${title}」模块大小` : `放大「${title}」模块`"
          :title="maximized ? '还原模块大小（Esc）' : '放大到整个窗口（Esc 退出）'"
          @click="emit('toggle-maximize')"
        >
          <PhCornersIn v-if="maximized" :size="15" aria-hidden="true" />
          <PhCornersOut v-else :size="15" aria-hidden="true" />
        </button>
        <button
          v-if="canRemove && !maximized"
          type="button"
          class="module-panel-action module-panel-remove"
          :aria-label="`从首页移除「${title}」模块`"
          title="移除模块"
          @click="emit('remove')"
        >
          <PhX :size="15" aria-hidden="true" />
        </button>
      </div>
    </header>
    <div class="module-panel-body">
      <slot />
    </div>
    <ModuleHelpPopover ref="helpPopover" :title="title" :help="help" :anchor="helpButton" />
  </section>
</template>
